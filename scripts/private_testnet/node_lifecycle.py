#!/usr/bin/env python3
"""Manage PulseDAG private-testnet node releases and process lifecycle safely."""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

RELEASE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
ENV_KEY = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
DNS_MULTIADDR = re.compile(r"^/(dns4|dns6)/([^/]+)/tcp/([0-9]+)$")
IP_MULTIADDR = re.compile(r"^/(ip4|ip6)/([^/]+)/tcp/([0-9]+)$")


class LifecycleError(RuntimeError):
    """Raised when an operator action cannot be completed safely."""


@dataclass(frozen=True)
class Layout:
    """Filesystem layout for one managed PulseDAG node."""

    root: Path

    @property
    def releases(self) -> Path:
        return self.root / "releases"

    @property
    def current_link(self) -> Path:
        return self.root / "current"

    @property
    def previous_link(self) -> Path:
        return self.root / "previous"

    @property
    def run(self) -> Path:
        return self.root / "run"

    @property
    def logs(self) -> Path:
        return self.root / "logs"

    @property
    def state(self) -> Path:
        return self.root / "state"

    @property
    def pid_file(self) -> Path:
        return self.run / "pulsedagd.pid"

    @property
    def log_file(self) -> Path:
        return self.logs / "pulsedagd.log"

    @property
    def state_file(self) -> Path:
        return self.state / "lifecycle.json"

    @property
    def lock_file(self) -> Path:
        return self.run / "lifecycle.lock"


def utc_now() -> str:
    """Return an RFC3339 UTC timestamp suitable for operator evidence."""

    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest for a release binary."""

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_env_file(path: Path) -> dict[str, str]:
    """Parse a restricted KEY=VALUE environment file without executing shell code."""

    if not path.is_file():
        raise LifecycleError(f"environment file does not exist: {path}")

    values: dict[str, str] = {}
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].strip()
        if "=" not in line:
            raise LifecycleError(f"{path}:{line_number}: expected KEY=VALUE")
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not ENV_KEY.fullmatch(key):
            raise LifecycleError(f"{path}:{line_number}: invalid environment key: {key!r}")
        if value.startswith(("'", '"')) or value.endswith(("'", '"')):
            if len(value) < 2 or value[0] != value[-1]:
                raise LifecycleError(f"{path}:{line_number}: mismatched quotes")
            value = value[1:-1]
        if "$(" in value or "`" in value or "${" in value:
            raise LifecycleError(
                f"{path}:{line_number}: shell expansion is not allowed in lifecycle environment files"
            )
        values[key] = value
    return values


def ensure_layout(layout: Layout) -> None:
    """Create the managed directory layout and reject unsafe ownership or permissions."""

    layout.root.mkdir(parents=True, exist_ok=True, mode=0o750)
    for path in (layout.releases, layout.run, layout.logs, layout.state):
        path.mkdir(parents=True, exist_ok=True, mode=0o750)

    expected_uid = os.geteuid()
    for path in (layout.root, layout.releases, layout.run, layout.logs, layout.state):
        metadata = path.stat()
        if metadata.st_uid != expected_uid:
            raise LifecycleError(
                f"managed path is not owned by the current operator uid {expected_uid}: {path}"
            )
        if metadata.st_mode & 0o022:
            raise LifecycleError(f"managed path must not be group/world writable: {path}")


@contextlib.contextmanager
def lifecycle_lock(layout: Layout) -> Iterator[None]:
    """Serialize lifecycle mutations so concurrent operators cannot race symlink or PID updates."""

    with layout.lock_file.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        yield


def atomic_write_text(path: Path, content: str, mode: int = 0o640) -> None:
    """Replace a small state file atomically."""

    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    temporary.write_text(content, encoding="utf-8")
    os.chmod(temporary, mode)
    os.replace(temporary, path)


def atomic_symlink(link: Path, target: Path) -> None:
    """Replace a release symlink atomically using a path relative to the lifecycle root."""

    relative_target = os.path.relpath(target, start=link.parent)
    temporary = link.with_name(f".{link.name}.{uuid.uuid4().hex}.tmp")
    os.symlink(relative_target, temporary)
    os.replace(temporary, link)


def resolved_release(link: Path, releases: Path) -> Path | None:
    """Resolve a managed release symlink and ensure it cannot escape the releases directory."""

    if not link.is_symlink():
        return None
    target = link.resolve(strict=True)
    releases_root = releases.resolve(strict=True)
    try:
        target.relative_to(releases_root)
    except ValueError as exc:
        raise LifecycleError(f"release link escapes managed releases directory: {link}") from exc
    return target


def read_state(layout: Layout) -> dict[str, object]:
    """Read lifecycle state, returning an empty document when no state exists."""

    if not layout.state_file.is_file():
        return {}
    try:
        payload = json.loads(layout.state_file.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as exc:
        raise LifecycleError(f"invalid lifecycle state file: {layout.state_file}") from exc
    if not isinstance(payload, dict):
        raise LifecycleError(f"lifecycle state must be a JSON object: {layout.state_file}")
    return payload


def write_state(layout: Layout, payload: dict[str, object]) -> None:
    """Persist lifecycle state atomically."""

    atomic_write_text(layout.state_file, json.dumps(payload, indent=2, sort_keys=True) + "\n")


def read_pid(layout: Layout) -> int | None:
    """Read the managed PID or return None when no PID file exists."""

    if not layout.pid_file.is_file():
        return None
    raw = layout.pid_file.read_text(encoding="utf-8").strip()
    if not raw.isdigit() or int(raw) <= 1:
        raise LifecycleError(f"invalid PID file: {layout.pid_file}")
    return int(raw)


def process_start_ticks(pid: int) -> str | None:
    """Read Linux process start ticks to protect against PID reuse."""

    stat_path = Path(f"/proc/{pid}/stat")
    try:
        fields = stat_path.read_text(encoding="utf-8").split()
    except OSError:
        return None
    return fields[21] if len(fields) > 21 else None


def process_exists(pid: int) -> bool:
    """Return whether a process currently exists."""

    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def managed_process(layout: Layout) -> tuple[int, dict[str, object]] | None:
    """Return the managed live process while rejecting PID reuse."""

    pid = read_pid(layout)
    if pid is None:
        return None
    if not process_exists(pid):
        layout.pid_file.unlink(missing_ok=True)
        return None

    state = read_state(layout)
    expected_ticks = str(state.get("process_start_ticks", ""))
    actual_ticks = process_start_ticks(pid)
    if not expected_ticks or not actual_ticks or expected_ticks != actual_ticks:
        raise LifecycleError(
            f"PID {pid} is alive but does not match the recorded managed process; refusing to signal it"
        )
    return pid, state


def run_preflight(env_file: Path, preflight_script: Path) -> None:
    """Run the Task 07 fail-closed configuration contract."""

    result = subprocess.run(
        ["bash", str(preflight_script), str(env_file)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        details = (result.stderr or result.stdout).strip()
        raise LifecycleError(f"private-testnet preflight failed:\n{details}")


def resolve_bootnodes(values: dict[str, str], allow_unresolved: bool) -> list[dict[str, object]]:
    """Resolve DNS bootnodes before starting an ordinary node."""

    bootstrap = values.get("PULSEDAG_P2P_BOOTSTRAP", "").strip()
    results: list[dict[str, object]] = []
    for raw in (entry.strip() for entry in bootstrap.split(",")):
        if not raw:
            continue

        dns_match = DNS_MULTIADDR.fullmatch(raw)
        ip_match = IP_MULTIADDR.fullmatch(raw)
        if dns_match:
            family_name, host, port_raw = dns_match.groups()
            family = socket.AF_INET if family_name == "dns4" else socket.AF_INET6
            try:
                addresses = sorted(
                    {
                        entry[4][0]
                        for entry in socket.getaddrinfo(
                            host,
                            int(port_raw),
                            family=family,
                            type=socket.SOCK_STREAM,
                        )
                    }
                )
            except socket.gaierror as exc:
                if not allow_unresolved:
                    raise LifecycleError(f"bootnode DNS resolution failed for {raw}: {exc}") from exc
                addresses = []
            results.append({"multiaddr": raw, "resolved_addresses": addresses})
        elif ip_match:
            results.append({"multiaddr": raw, "resolved_addresses": [ip_match.group(2)]})
        else:
            raise LifecycleError(f"unsupported bootnode multiaddr: {raw}")
    return results


def health_url(values: dict[str, str]) -> str:
    """Build the loopback health endpoint from the validated RPC bind."""

    bind = values.get("PULSEDAG_RPC_BIND", "")
    if bind.startswith("[::1]:"):
        port = bind.rsplit(":", 1)[1]
        return f"http://[::1]:{port}/health"
    host, separator, port = bind.rpartition(":")
    if not separator or not port.isdigit():
        raise LifecycleError(f"invalid PULSEDAG_RPC_BIND: {bind!r}")
    if host in {"0.0.0.0", "localhost"}:
        host = "127.0.0.1"
    return f"http://{host}:{port}/health"


def wait_for_health(
    url: str,
    pid: int,
    timeout_seconds: float,
    process: subprocess.Popen[bytes] | None = None,
) -> None:
    """Wait for health while failing immediately when the managed process exits."""

    deadline = time.monotonic() + timeout_seconds
    last_error = "health endpoint not attempted"
    while time.monotonic() < deadline:
        if (process is not None and process.poll() is not None) or not process_exists(pid):
            raise LifecycleError(f"node process exited before health became available: {last_error}")
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                if 200 <= response.status < 300:
                    return
                last_error = f"HTTP {response.status}"
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            last_error = str(exc)
        time.sleep(0.25)
    raise LifecycleError(f"health endpoint did not become ready within {timeout_seconds}s: {last_error}")


def install_release(layout: Layout, binary: Path, release_id: str) -> Path:
    """Install an immutable release directory and return its path."""

    if not RELEASE_ID.fullmatch(release_id):
        raise LifecycleError(f"invalid release id: {release_id!r}")
    binary = binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise LifecycleError(f"release binary must be an executable file: {binary}")

    checksum = sha256_file(binary)
    destination = layout.releases / release_id
    if destination.exists():
        installed_binary = destination / "pulsedagd"
        if installed_binary.is_file() and sha256_file(installed_binary) == checksum:
            return destination
        raise LifecycleError(f"release id already exists with different content: {release_id}")

    staging = layout.releases / f".staging-{release_id}-{uuid.uuid4().hex}"
    staging.mkdir(mode=0o750)
    try:
        installed_binary = staging / "pulsedagd"
        shutil.copy2(binary, installed_binary)
        os.chmod(installed_binary, 0o750)
        manifest = {
            "release_id": release_id,
            "binary_sha256": checksum,
            "installed_at": utc_now(),
        }
        atomic_write_text(staging / "manifest.json", json.dumps(manifest, indent=2) + "\n")
        os.replace(staging, destination)
    finally:
        if staging.exists():
            shutil.rmtree(staging)
    return destination


def activate_release(layout: Layout, release: Path) -> Path | None:
    """Activate a release and retain the old release as the rollback target."""

    current = resolved_release(layout.current_link, layout.releases)
    if current == release:
        return current
    if current is not None:
        atomic_symlink(layout.previous_link, current)
    atomic_symlink(layout.current_link, release)
    return current


def swap_current_previous(layout: Layout) -> tuple[Path, Path]:
    """Swap current and previous releases inside one serialized local operation."""

    current = resolved_release(layout.current_link, layout.releases)
    previous = resolved_release(layout.previous_link, layout.releases)
    if current is None or previous is None:
        raise LifecycleError("rollback requires both current and previous releases")
    atomic_symlink(layout.previous_link, current)
    atomic_symlink(layout.current_link, previous)
    return previous, current


def current_binary(layout: Layout) -> tuple[Path, Path]:
    """Return the active release and executable."""

    release = resolved_release(layout.current_link, layout.releases)
    if release is None:
        raise LifecycleError("no active release; install or upgrade a release first")
    binary = release / "pulsedagd"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise LifecycleError(f"active release binary is missing or not executable: {binary}")
    return release, binary


def start_node(
    layout: Layout,
    env_file: Path,
    preflight_script: Path,
    health_timeout: float,
    allow_unresolved_bootnodes: bool,
) -> dict[str, object]:
    """Start the active release idempotently and wait for health."""

    existing = managed_process(layout)
    if existing is not None:
        pid, state = existing
        return {"action": "start", "changed": False, "status": "running", "pid": pid, **state}

    run_preflight(env_file, preflight_script)
    values = parse_env_file(env_file)
    bootnodes = resolve_bootnodes(values, allow_unresolved_bootnodes)
    release, binary = current_binary(layout)

    environment = os.environ.copy()
    environment.update(values)
    layout.log_file.parent.mkdir(parents=True, exist_ok=True)
    with layout.log_file.open("ab", buffering=0) as log_handle:
        process = subprocess.Popen(
            [str(binary)],
            cwd=str(layout.root),
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=log_handle,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    ticks = process_start_ticks(process.pid)
    if ticks is None:
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)
        raise LifecycleError("could not record process start identity")

    state: dict[str, object] = {
        "pid": process.pid,
        "process_start_ticks": ticks,
        "release_id": release.name,
        "binary_sha256": sha256_file(binary),
        "env_file": str(env_file.resolve()),
        "health_url": health_url(values),
        "bootnodes": bootnodes,
        "started_at": utc_now(),
        "status": "starting",
    }
    atomic_write_text(layout.pid_file, f"{process.pid}\n")
    write_state(layout, state)

    try:
        wait_for_health(str(state["health_url"]), process.pid, health_timeout, process)
    except Exception:
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGTERM)
        time.sleep(0.2)
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)
        with contextlib.suppress(subprocess.TimeoutExpired):
            process.wait(timeout=2)
        layout.pid_file.unlink(missing_ok=True)
        state["status"] = "failed"
        state["failed_at"] = utc_now()
        write_state(layout, state)
        raise

    state["status"] = "running"
    state["healthy_at"] = utc_now()
    write_state(layout, state)
    return {"action": "start", "changed": True, **state}


def stop_node(layout: Layout, timeout_seconds: float) -> dict[str, object]:
    """Stop the managed process idempotently without signaling a reused PID."""

    managed = managed_process(layout)
    if managed is None:
        return {"action": "stop", "changed": False, "status": "stopped"}

    pid, state = managed
    with contextlib.suppress(ProcessLookupError):
        os.killpg(pid, signal.SIGTERM)

    deadline = time.monotonic() + timeout_seconds
    while process_exists(pid) and time.monotonic() < deadline:
        time.sleep(0.1)
    if process_exists(pid):
        with contextlib.suppress(ProcessLookupError):
            os.killpg(pid, signal.SIGKILL)
        kill_deadline = time.monotonic() + 2
        while process_exists(pid) and time.monotonic() < kill_deadline:
            time.sleep(0.05)
    if process_exists(pid):
        raise LifecycleError(f"managed process did not stop: pid={pid}")

    layout.pid_file.unlink(missing_ok=True)
    state.update({"status": "stopped", "stopped_at": utc_now()})
    write_state(layout, state)
    return {"action": "stop", "changed": True, "pid": pid, **state}


def status(layout: Layout) -> dict[str, object]:
    """Return machine-readable lifecycle status."""

    managed = managed_process(layout)
    current = resolved_release(layout.current_link, layout.releases)
    previous = resolved_release(layout.previous_link, layout.releases)
    state = read_state(layout)
    return {
        "action": "status",
        "status": "running" if managed is not None else "stopped",
        "pid": managed[0] if managed is not None else None,
        "current_release": current.name if current is not None else None,
        "previous_release": previous.name if previous is not None else None,
        "state": state,
    }


def verify(
    layout: Layout,
    env_file: Path,
    preflight_script: Path,
    allow_unresolved_bootnodes: bool,
) -> dict[str, object]:
    """Validate configuration, release links, ownership, and bootnode resolution."""

    run_preflight(env_file, preflight_script)
    values = parse_env_file(env_file)
    bootnodes = resolve_bootnodes(values, allow_unresolved_bootnodes)
    release, binary = current_binary(layout)
    return {
        "action": "verify",
        "status": "valid",
        "release_id": release.name,
        "binary_sha256": sha256_file(binary),
        "health_url": health_url(values),
        "bootnodes": bootnodes,
    }


def perform_upgrade(
    layout: Layout,
    env_file: Path,
    preflight_script: Path,
    binary: Path,
    release_id: str,
    health_timeout: float,
    stop_timeout: float,
    allow_unresolved_bootnodes: bool,
    start_after: bool,
) -> dict[str, object]:
    """Install and activate a release, restoring the old release after a failed health check."""

    was_running = managed_process(layout) is not None
    old_release = resolved_release(layout.current_link, layout.releases)
    new_release = install_release(layout, binary, release_id)

    if was_running:
        stop_node(layout, stop_timeout)
    activate_release(layout, new_release)

    if start_after or was_running:
        try:
            start_node(
                layout,
                env_file,
                preflight_script,
                health_timeout,
                allow_unresolved_bootnodes,
            )
        except Exception as upgrade_error:
            if old_release is not None:
                activate_release(layout, old_release)
                if was_running:
                    try:
                        start_node(
                            layout,
                            env_file,
                            preflight_script,
                            health_timeout,
                            allow_unresolved_bootnodes,
                        )
                    except Exception as rollback_error:
                        raise LifecycleError(
                            "upgrade failed and automatic rollback could not restore health: "
                            f"upgrade={upgrade_error}; rollback={rollback_error}"
                        ) from rollback_error
            raise LifecycleError(f"upgrade failed; previous release restored: {upgrade_error}") from upgrade_error

    return {
        "action": "upgrade",
        "changed": old_release != new_release,
        "current_release": new_release.name,
        "previous_release": old_release.name if old_release is not None else None,
        "started": bool(start_after or was_running),
    }


def perform_rollback(
    layout: Layout,
    env_file: Path,
    preflight_script: Path,
    health_timeout: float,
    stop_timeout: float,
    allow_unresolved_bootnodes: bool,
    start_after: bool,
) -> dict[str, object]:
    """Swap current and previous releases and restore the prior running state."""

    was_running = managed_process(layout) is not None
    if was_running:
        stop_node(layout, stop_timeout)
    new_current, new_previous = swap_current_previous(layout)
    if start_after or was_running:
        start_node(
            layout,
            env_file,
            preflight_script,
            health_timeout,
            allow_unresolved_bootnodes,
        )
    return {
        "action": "rollback",
        "changed": True,
        "current_release": new_current.name,
        "previous_release": new_previous.name,
        "started": bool(start_after or was_running),
    }


def build_parser() -> argparse.ArgumentParser:
    """Create the command-line interface."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="Managed node lifecycle root.")
    parser.add_argument("--env-file", type=Path, required=True, help="Validated node environment file.")
    parser.add_argument(
        "--preflight-script",
        type=Path,
        default=Path("scripts/v2_3_0_private_testnet_preflight.sh"),
        help="Task 07 configuration preflight.",
    )
    parser.add_argument("--health-timeout", type=float, default=60.0)
    parser.add_argument("--stop-timeout", type=float, default=20.0)
    parser.add_argument(
        "--allow-unresolved-bootnodes",
        action="store_true",
        help="Allow an offline operator drill to continue when DNS bootnodes do not resolve.",
    )

    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("status")
    subparsers.add_parser("verify")
    subparsers.add_parser("start")
    subparsers.add_parser("stop")
    subparsers.add_parser("restart")

    install = subparsers.add_parser("install")
    install.add_argument("--binary", type=Path, required=True)
    install.add_argument("--release-id", required=True)

    upgrade = subparsers.add_parser("upgrade")
    upgrade.add_argument("--binary", type=Path, required=True)
    upgrade.add_argument("--release-id", required=True)
    upgrade.add_argument("--no-start", action="store_true")

    rollback = subparsers.add_parser("rollback")
    rollback.add_argument("--no-start", action="store_true")
    return parser


def main() -> int:
    """Run one serialized lifecycle operation."""

    args = build_parser().parse_args()
    layout = Layout(args.root.resolve())
    env_file = args.env_file.resolve()
    preflight_script = args.preflight_script.resolve()

    try:
        ensure_layout(layout)
        with lifecycle_lock(layout):
            if args.command == "status":
                result = status(layout)
            elif args.command == "verify":
                result = verify(
                    layout,
                    env_file,
                    preflight_script,
                    args.allow_unresolved_bootnodes,
                )
            elif args.command == "start":
                result = start_node(
                    layout,
                    env_file,
                    preflight_script,
                    args.health_timeout,
                    args.allow_unresolved_bootnodes,
                )
            elif args.command == "stop":
                result = stop_node(layout, args.stop_timeout)
            elif args.command == "restart":
                stop_node(layout, args.stop_timeout)
                result = start_node(
                    layout,
                    env_file,
                    preflight_script,
                    args.health_timeout,
                    args.allow_unresolved_bootnodes,
                )
                result["action"] = "restart"
            elif args.command == "install":
                release = install_release(layout, args.binary, args.release_id)
                old_release = activate_release(layout, release)
                result = {
                    "action": "install",
                    "changed": old_release != release,
                    "current_release": release.name,
                    "previous_release": old_release.name if old_release is not None else None,
                }
            elif args.command == "upgrade":
                result = perform_upgrade(
                    layout,
                    env_file,
                    preflight_script,
                    args.binary,
                    args.release_id,
                    args.health_timeout,
                    args.stop_timeout,
                    args.allow_unresolved_bootnodes,
                    not args.no_start,
                )
            elif args.command == "rollback":
                result = perform_rollback(
                    layout,
                    env_file,
                    preflight_script,
                    args.health_timeout,
                    args.stop_timeout,
                    args.allow_unresolved_bootnodes,
                    not args.no_start,
                )
            else:
                raise LifecycleError(f"unsupported command: {args.command}")
    except (LifecycleError, OSError) as exc:
        print(json.dumps({"result": "ERROR", "error": str(exc)}, indent=2), file=sys.stderr)
        return 1

    print(json.dumps({"result": "PASS", **result}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
