#!/usr/bin/env python3
"""Run and evidence the PulseDAG v2.3.0 five-node private-testnet rehearsal."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shlex
import subprocess
import sys
import time
import urllib.parse
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Sequence

EXPECTED_NETWORK_PROFILE = "private-testnet-v2.3.0"
EXPECTED_CHAIN_ID = "pulsedag-private-v2.3.0"
EXPECTED_NODE_COUNT = 5
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,31}$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
RPC_ENDPOINTS = ("/health", "/status", "/sync/status", "/p2p/status", "/checks")


class RehearsalError(RuntimeError):
    """Raised when the rehearsal contract or runtime evidence fails closed."""


@dataclass(frozen=True)
class Node:
    """Validated remote-node definition."""

    name: str
    role: str
    transport: tuple[str, ...]
    transport_mode: str
    repo_root: str
    env_file: str
    lifecycle_root: str
    rpc_url: str


@dataclass(frozen=True)
class Inventory:
    """Validated five-node rehearsal inventory."""

    candidate_sha: str
    nodes: tuple[Node, ...]
    fault_target: str
    isolate_command: tuple[str, ...]
    restore_command: tuple[str, ...]
    max_height_spread: int
    min_progress_blocks: int
    poll_interval_seconds: float
    phase_timeout_seconds: float


Executor = Callable[[Node, Sequence[str], float], str]


def utc_now() -> str:
    """Return an RFC3339 UTC timestamp."""

    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def require(condition: bool, message: str) -> None:
    """Raise a rehearsal error when a contract predicate is false."""

    if not condition:
        raise RehearsalError(message)


def absolute_path(value: object, field: str) -> str:
    """Validate and return an absolute remote path."""

    require(isinstance(value, str) and value.startswith("/"), f"{field} must be an absolute path")
    return value


def command_argv(value: object, field: str) -> tuple[str, ...]:
    """Validate a no-shell argv array."""

    require(isinstance(value, list) and value, f"{field} must be a non-empty argv array")
    require(all(isinstance(part, str) and part for part in value), f"{field} contains an invalid argv item")
    argv = tuple(value)
    require(argv[0].startswith("/"), f"{field} executable must be an absolute path")
    require("-c" not in argv[:2], f"{field} must not invoke a shell command string")
    return argv


def validate_loopback_url(value: object, field: str) -> str:
    """Require a loopback-only HTTP RPC URL."""

    require(isinstance(value, str), f"{field} must be a string")
    parsed = urllib.parse.urlparse(value)
    require(parsed.scheme == "http", f"{field} must use http")
    require(parsed.hostname in {"127.0.0.1", "localhost", "::1"}, f"{field} must be loopback-only")
    require(parsed.port is not None, f"{field} must include an explicit port")
    require(parsed.path in {"", "/"}, f"{field} must not include a path")
    return value.rstrip("/")


def load_inventory(path: Path) -> Inventory:
    """Load and fully validate the rehearsal inventory."""

    payload = json.loads(path.read_text(encoding="utf-8"))
    require(payload.get("schema_version") == 1, "inventory schema_version must be 1")
    require(payload.get("network_profile") == EXPECTED_NETWORK_PROFILE, "unexpected network profile")
    require(payload.get("chain_id") == EXPECTED_CHAIN_ID, "unexpected chain id")
    candidate_sha = payload.get("candidate_sha")
    require(
        isinstance(candidate_sha, str) and SHA_RE.fullmatch(candidate_sha),
        "candidate_sha must be 40 lowercase hex characters",
    )

    raw_nodes = payload.get("nodes")
    require(
        isinstance(raw_nodes, list) and len(raw_nodes) == EXPECTED_NODE_COUNT,
        "inventory must define exactly five nodes",
    )
    nodes: list[Node] = []
    names: set[str] = set()
    seed_count = 0
    for index, raw in enumerate(raw_nodes):
        require(isinstance(raw, dict), f"nodes[{index}] must be an object")
        name = raw.get("name")
        require(isinstance(name, str) and NAME_RE.fullmatch(name), f"nodes[{index}].name is invalid")
        require(name not in names, f"duplicate node name: {name}")
        names.add(name)
        role = raw.get("role")
        require(role in {"seed", "node"}, f"{name}.role must be seed or node")
        seed_count += int(role == "seed")
        transport_raw = raw.get("transport")
        require(
            isinstance(transport_raw, list) and transport_raw,
            f"{name}.transport must be a non-empty argv prefix",
        )
        require(
            all(isinstance(part, str) and part for part in transport_raw),
            f"{name}.transport contains an invalid item",
        )
        require(
            transport_raw[0] not in {"sh", "bash", "/bin/sh", "/bin/bash"},
            f"{name}.transport must not be a shell",
        )
        transport_mode = raw.get("transport_mode")
        require(transport_mode in {"ssh", "argv"}, f"{name}.transport_mode must be ssh or argv")
        nodes.append(
            Node(
                name=name,
                role=role,
                transport=tuple(transport_raw),
                transport_mode=transport_mode,
                repo_root=absolute_path(raw.get("repo_root"), f"{name}.repo_root"),
                env_file=absolute_path(raw.get("env_file"), f"{name}.env_file"),
                lifecycle_root=absolute_path(raw.get("lifecycle_root"), f"{name}.lifecycle_root"),
                rpc_url=validate_loopback_url(raw.get("rpc_url"), f"{name}.rpc_url"),
            )
        )
    require(seed_count == 1, "inventory must define exactly one seed")

    fault = payload.get("fault")
    require(isinstance(fault, dict), "fault must be an object")
    fault_target = fault.get("target")
    require(isinstance(fault_target, str) and fault_target in names, "fault target must name an inventory node")
    target = next(node for node in nodes if node.name == fault_target)
    require(target.role == "node", "fault target must be an ordinary node")

    thresholds = payload.get("thresholds")
    require(isinstance(thresholds, dict), "thresholds must be an object")
    max_height_spread = thresholds.get("max_height_spread", 2)
    min_progress_blocks = thresholds.get("min_progress_blocks", 3)
    poll_interval_seconds = thresholds.get("poll_interval_seconds", 5)
    phase_timeout_seconds = thresholds.get("phase_timeout_seconds", 180)
    require(
        isinstance(max_height_spread, int) and 0 <= max_height_spread <= 32,
        "max_height_spread must be an integer from 0 to 32",
    )
    require(
        isinstance(min_progress_blocks, int) and 1 <= min_progress_blocks <= 1000,
        "min_progress_blocks must be an integer from 1 to 1000",
    )
    require(
        isinstance(poll_interval_seconds, (int, float))
        and 0.1 <= poll_interval_seconds <= 60,
        "poll_interval_seconds must be between 0.1 and 60",
    )
    require(
        isinstance(phase_timeout_seconds, (int, float))
        and 10 <= phase_timeout_seconds <= 7200,
        "phase_timeout_seconds must be between 10 and 7200",
    )

    return Inventory(
        candidate_sha=candidate_sha,
        nodes=tuple(nodes),
        fault_target=fault_target,
        isolate_command=command_argv(fault.get("isolate_command"), "fault.isolate_command"),
        restore_command=command_argv(fault.get("restore_command"), "fault.restore_command"),
        max_height_spread=max_height_spread,
        min_progress_blocks=min_progress_blocks,
        poll_interval_seconds=float(poll_interval_seconds),
        phase_timeout_seconds=float(phase_timeout_seconds),
    )


def default_executor(node: Node, argv: Sequence[str], timeout: float) -> str:
    """Run one remote argv through the node transport without a shell."""

    command = [*node.transport, shlex.join(argv)] if node.transport_mode == "ssh" else [*node.transport, *argv]
    completed = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if completed.returncode != 0:
        stderr = completed.stderr.strip()
        raise RehearsalError(f"{node.name}: remote command failed ({completed.returncode}): {stderr}")
    return completed.stdout


class RehearsalRunner:
    """Execute the fail-closed multi-host rehearsal and write immutable evidence."""

    def __init__(self, inventory: Inventory, out_dir: Path, executor: Executor = default_executor) -> None:
        self.inventory = inventory
        self.out_dir = out_dir
        self.executor = executor
        self.phases: list[dict[str, Any]] = []
        self.started_at = utc_now()
        self.out_dir.mkdir(parents=True, exist_ok=False)

    def run_remote(self, node: Node, argv: Sequence[str], timeout: float | None = None) -> str:
        """Run one bounded remote command."""

        return self.executor(node, argv, timeout or self.inventory.phase_timeout_seconds)

    def lifecycle_argv(self, node: Node, action: str) -> list[str]:
        """Build a canonical Task 09 lifecycle argv."""

        return [
            "python3",
            f"{node.repo_root}/scripts/private_testnet/node_lifecycle.py",
            "--root",
            node.lifecycle_root,
            "--env-file",
            node.env_file,
            "--preflight-script",
            f"{node.repo_root}/scripts/v2_3_0_private_testnet_preflight.sh",
            action,
        ]

    def record_phase(self, name: str, result: str, details: dict[str, Any] | None = None) -> None:
        """Append one phase result to the decision record."""

        self.phases.append({"name": name, "result": result, "captured_at": utc_now(), "details": details or {}})

    def preflight(self) -> None:
        """Validate configuration and lifecycle state on every host."""

        revisions: dict[str, str] = {}
        for node in self.inventory.nodes:
            revision = self.run_remote(
                node,
                ["git", "-C", node.repo_root, "rev-parse", "HEAD"],
            ).strip()
            require(
                revision == self.inventory.candidate_sha,
                f"{node.name}: source checkout does not match candidate_sha",
            )
            dirty = self.run_remote(
                node,
                ["git", "-C", node.repo_root, "status", "--porcelain"],
            ).strip()
            require(not dirty, f"{node.name}: source checkout is not clean")
            revisions[node.name] = revision
            self.run_remote(
                node,
                [
                    "bash",
                    f"{node.repo_root}/scripts/v2_3_0_private_testnet_preflight.sh",
                    node.env_file,
                ],
            )
            self.run_remote(node, self.lifecycle_argv(node, "verify"))
        self.record_phase("preflight", "PASS", {"nodes": EXPECTED_NODE_COUNT, "revisions": revisions})

    def start_nodes(self) -> None:
        """Start the seed first and ordinary nodes second."""

        ordered = sorted(self.inventory.nodes, key=lambda node: node.role != "seed")
        for node in ordered:
            self.run_remote(node, self.lifecycle_argv(node, "start"))
        self.record_phase("start", "PASS", {"order": [node.name for node in ordered]})

    def fetch_rpc(self, node: Node, endpoint: str) -> dict[str, Any]:
        """Fetch and unwrap one loopback RPC endpoint on a remote host."""

        code = (
            "import json,sys,urllib.request;"
            "p=json.load(urllib.request.urlopen(sys.argv[1],timeout=float(sys.argv[2])));"
            "assert p.get('ok') is True and isinstance(p.get('data'),dict),p;"
            "print(json.dumps(p['data'],sort_keys=True))"
        )
        stdout = self.run_remote(
            node,
            ["python3", "-c", code, f"{node.rpc_url}{endpoint}", "5"],
            timeout=10,
        )
        try:
            payload = json.loads(stdout)
        except json.JSONDecodeError as exc:
            raise RehearsalError(f"{node.name}: invalid JSON from {endpoint}: {exc}") from exc
        require(isinstance(payload, dict), f"{node.name}: {endpoint} data must be an object")
        return payload

    def snapshot(self, phase: str) -> dict[str, dict[str, Any]]:
        """Collect the stable read-only RPC evidence set from all nodes."""

        captured: dict[str, dict[str, Any]] = {}
        phase_dir = self.out_dir / phase
        phase_dir.mkdir(parents=True, exist_ok=False)
        for node in self.inventory.nodes:
            endpoints: dict[str, Any] = {}
            for endpoint in RPC_ENDPOINTS:
                endpoints[endpoint] = self.fetch_rpc(node, endpoint)
            captured[node.name] = endpoints
            (phase_dir / f"{node.name}.json").write_text(
                json.dumps(endpoints, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
        return captured

    def validate_convergence(
        self,
        snapshot: dict[str, dict[str, Any]],
        *,
        allow_isolated_target: bool = False,
    ) -> dict[str, Any]:
        """Validate chain identity, real P2P, sync consistency, and height spread."""

        heights: dict[str, int] = {}
        peer_counts: dict[str, int] = {}
        for node in self.inventory.nodes:
            status = snapshot[node.name]["/status"]
            sync = snapshot[node.name]["/sync/status"]
            require(status.get("chain_id") == EXPECTED_CHAIN_ID, f"{node.name}: unexpected chain_id")
            require(status.get("p2p_mode") == "libp2p-real", f"{node.name}: p2p_mode is not libp2p-real")
            require(
                status.get("connected_peers_are_real_network") is True,
                f"{node.name}: peers are not reported as real network peers",
            )
            require(status.get("rpc_response_degraded") is False, f"{node.name}: degraded status response")
            require(status.get("rpc_response_stale") is False, f"{node.name}: stale status response")
            require(status.get("p2p_status_degraded") is False, f"{node.name}: degraded P2P status")
            require(sync.get("consistency_ok") is True, f"{node.name}: sync consistency failed")
            require(sync.get("consistency_issue_count") == 0, f"{node.name}: sync consistency issues are present")
            require(sync.get("storage_replay_gap") == 0, f"{node.name}: storage replay gap is non-zero")
            require(sync.get("live_sync_error_active") is False, f"{node.name}: live sync error is active")
            height = status.get("best_height")
            peer_count = status.get("peer_count")
            require(isinstance(height, int) and height >= 0, f"{node.name}: invalid best_height")
            require(isinstance(peer_count, int) and peer_count >= 0, f"{node.name}: invalid peer_count")
            heights[node.name] = height
            peer_counts[node.name] = peer_count
            if allow_isolated_target and node.name == self.inventory.fault_target:
                require(peer_count == 0, f"{node.name}: fault target still has peers")
            else:
                require(peer_count >= 1, f"{node.name}: expected at least one peer")
        considered = [
            height
            for name, height in heights.items()
            if not (allow_isolated_target and name == self.inventory.fault_target)
        ]
        spread = max(considered) - min(considered)
        require(
            spread <= self.inventory.max_height_spread,
            f"height spread {spread} exceeds {self.inventory.max_height_spread}",
        )
        return {"heights": heights, "peer_counts": peer_counts, "height_spread": spread}

    def wait_for_convergence(
        self,
        phase: str,
        *,
        allow_isolated_target: bool = False,
    ) -> tuple[dict[str, dict[str, Any]], dict[str, Any]]:
        """Poll until convergence succeeds or the phase times out."""

        deadline = time.monotonic() + self.inventory.phase_timeout_seconds
        attempt = 0
        last_error = "no attempt"
        while time.monotonic() < deadline:
            attempt += 1
            try:
                snapshot = self.snapshot(f"{phase}-attempt-{attempt:02d}")
                summary = self.validate_convergence(snapshot, allow_isolated_target=allow_isolated_target)
                return snapshot, {**summary, "attempts": attempt}
            except RehearsalError as exc:
                last_error = str(exc)
                time.sleep(self.inventory.poll_interval_seconds)
        raise RehearsalError(f"{phase}: convergence timeout: {last_error}")

    def prove_progress(self, baseline: dict[str, dict[str, Any]], phase: str) -> dict[str, Any]:
        """Require external mining to advance the network before fault injection."""

        baseline_max = max(node_data["/status"]["best_height"] for node_data in baseline.values())
        deadline = time.monotonic() + self.inventory.phase_timeout_seconds
        attempt = 0
        while time.monotonic() < deadline:
            attempt += 1
            snapshot = self.snapshot(f"{phase}-attempt-{attempt:02d}")
            summary = self.validate_convergence(snapshot)
            current_max = max(summary["heights"].values())
            progress = current_max - baseline_max
            if progress >= self.inventory.min_progress_blocks:
                return {**summary, "baseline_height": baseline_max, "progress_blocks": progress, "attempts": attempt}
            time.sleep(self.inventory.poll_interval_seconds)
        raise RehearsalError("external mining did not produce the required block progress")

    def restart_target(self) -> None:
        """Restart one ordinary node and require convergence afterward."""

        node = next(node for node in self.inventory.nodes if node.name == self.inventory.fault_target)
        self.run_remote(node, self.lifecycle_argv(node, "restart"))
        _, summary = self.wait_for_convergence("post-restart")
        self.record_phase("restart-rejoin", "PASS", summary)

    def partition_and_rejoin(self) -> None:
        """Isolate the target through explicit remote hooks, restore it, and require catch-up."""

        node = next(node for node in self.inventory.nodes if node.name == self.inventory.fault_target)
        isolate_attempted = False
        try:
            isolate_attempted = True
            self.run_remote(node, self.inventory.isolate_command)
            _, isolated_summary = self.wait_for_convergence("partition", allow_isolated_target=True)
            self.record_phase("partition", "PASS", isolated_summary)
        finally:
            if isolate_attempted:
                self.run_remote(node, self.inventory.restore_command)
        _, rejoin_summary = self.wait_for_convergence("post-partition-rejoin")
        self.record_phase("partition-rejoin", "PASS", rejoin_summary)

    def write_decision(self, decision: str, failure: str | None = None) -> None:
        """Write the fail-closed decision manifest and checksums."""

        manifest = {
            "gate": "v2.3.0-multi-host-private-testnet-rehearsal",
            "candidate_sha": self.inventory.candidate_sha,
            "network_profile": EXPECTED_NETWORK_PROFILE,
            "chain_id": EXPECTED_CHAIN_ID,
            "node_count": EXPECTED_NODE_COUNT,
            "fault_target": self.inventory.fault_target,
            "started_at": self.started_at,
            "completed_at": utc_now(),
            "decision": decision,
            "failure": failure,
            "phases": self.phases,
            "version_bump_authorized": False,
            "public_testnet_ready": False,
            "thirty_day_public_testnet_clock_started": False,
        }
        (self.out_dir / "decision.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        checksum_lines: list[str] = []
        for path in sorted(self.out_dir.rglob("*")):
            if path.is_file() and path.name != "SHA256SUMS":
                digest = hashlib.sha256(path.read_bytes()).hexdigest()
                checksum_lines.append(f"{digest}  {path.relative_to(self.out_dir).as_posix()}")
        (self.out_dir / "SHA256SUMS").write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")

    def run(self) -> int:
        """Run every required Task 12 phase and emit GO only after all pass."""

        try:
            self.preflight()
            self.start_nodes()
            baseline, baseline_summary = self.wait_for_convergence("baseline")
            self.record_phase("baseline-convergence", "PASS", baseline_summary)
            progress_summary = self.prove_progress(baseline, "pre-fault-progress")
            self.record_phase("external-mining-progress", "PASS", progress_summary)
            self.restart_target()
            self.partition_and_rejoin()
            _, final_summary = self.wait_for_convergence("final")
            self.record_phase("final-convergence", "PASS", final_summary)
            final_snapshot = {
                name: {"/status": {"best_height": height}}
                for name, height in final_summary["heights"].items()
            }
            post_fault_progress = self.prove_progress(final_snapshot, "post-fault-progress")
            self.record_phase("post-fault-mining-progress", "PASS", post_fault_progress)
            self.write_decision("GO")
            return 0
        except (RehearsalError, OSError, subprocess.SubprocessError, json.JSONDecodeError) as exc:
            self.record_phase("failure", "FAIL", {"error": str(exc)})
            self.write_decision("NO-GO", str(exc))
            print(f"NO-GO: {exc}", file=sys.stderr)
            return 1


def verify_evidence(path: Path) -> None:
    """Verify an existing evidence bundle and its mandatory guardrails."""

    decision_path = path / "decision.json"
    checksums_path = path / "SHA256SUMS"
    require(decision_path.is_file(), "decision.json is missing")
    require(checksums_path.is_file(), "SHA256SUMS is missing")
    manifest = json.loads(decision_path.read_text(encoding="utf-8"))
    require(
        manifest.get("gate") == "v2.3.0-multi-host-private-testnet-rehearsal",
        "unexpected evidence gate",
    )
    require(manifest.get("node_count") == EXPECTED_NODE_COUNT, "evidence must cover five nodes")
    candidate_sha = manifest.get("candidate_sha")
    require(isinstance(candidate_sha, str) and SHA_RE.fullmatch(candidate_sha), "evidence candidate_sha is invalid")
    require(manifest.get("decision") in {"GO", "NO-GO"}, "decision must be GO or NO-GO")
    require(manifest.get("version_bump_authorized") is False, "evidence must not authorize a version bump")
    require(manifest.get("public_testnet_ready") is False, "evidence must preserve public_testnet_ready=false")
    require(
        manifest.get("thirty_day_public_testnet_clock_started") is False,
        "evidence must preserve the 30-day clock guardrail",
    )
    if manifest.get("decision") == "GO":
        passed = {phase.get("name") for phase in manifest.get("phases", []) if phase.get("result") == "PASS"}
        required = {
            "preflight",
            "start",
            "baseline-convergence",
            "external-mining-progress",
            "restart-rejoin",
            "partition",
            "partition-rejoin",
            "final-convergence",
            "post-fault-mining-progress",
        }
        require(required <= passed, f"GO evidence is missing phases: {sorted(required - passed)}")
        require(
            not any(
                phase.get("result") == "FAIL"
                for phase in manifest.get("phases", [])
            ),
            "GO evidence must not contain a failed phase",
        )
        require(manifest.get("failure") is None, "GO evidence must not contain a failure")

    expected: dict[str, str] = {}
    for raw in checksums_path.read_text(encoding="utf-8").splitlines():
        digest, separator, relative = raw.partition("  ")
        require(
            separator == "  "
            and re.fullmatch(r"[0-9a-f]{64}", digest) is not None,
            "invalid SHA256SUMS line",
        )
        require(
            relative
            and not relative.startswith("/")
            and ".." not in Path(relative).parts,
            "invalid checksum path",
        )
        expected[relative] = digest
    require("decision.json" in expected, "decision.json checksum is missing")
    actual = {
        item.relative_to(path).as_posix()
        for item in path.rglob("*")
        if item.is_file() and item.name != "SHA256SUMS"
    }
    require(actual == set(expected), "SHA256SUMS must cover every evidence file exactly once")
    for relative, digest in expected.items():
        target = path / relative
        require(target.is_file(), f"checksummed file is missing: {relative}")
        require(hashlib.sha256(target.read_bytes()).hexdigest() == digest, f"checksum mismatch: {relative}")


def main() -> int:
    """Validate inventory, run the rehearsal, or verify an evidence bundle."""

    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate_parser = subparsers.add_parser("validate-inventory")
    validate_parser.add_argument("--inventory", type=Path, required=True)

    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--inventory", type=Path, required=True)
    run_parser.add_argument("--out-dir", type=Path, required=True)

    verify_parser = subparsers.add_parser("verify-evidence")
    verify_parser.add_argument("--evidence-dir", type=Path, required=True)

    args = parser.parse_args()
    try:
        if args.command == "validate-inventory":
            inventory = load_inventory(args.inventory)
            print(
                json.dumps(
                    {
                        "result": "PASS",
                        "candidate_sha": inventory.candidate_sha,
                        "nodes": [node.name for node in inventory.nodes],
                    }
                )
            )
            return 0
        if args.command == "verify-evidence":
            verify_evidence(args.evidence_dir)
            print(json.dumps({"result": "PASS", "evidence_dir": str(args.evidence_dir)}))
            return 0
        inventory = load_inventory(args.inventory)
        return RehearsalRunner(inventory, args.out_dir).run()
    except (RehearsalError, OSError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
