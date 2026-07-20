#!/usr/bin/env python3
"""High-level PulseDAG node lifecycle operations."""

from __future__ import annotations

import contextlib
import os
import signal
import subprocess
import time
from pathlib import Path

from lifecycle_bootnodes import resolve_bootnodes
from lifecycle_core import (
    Layout,
    LifecycleError,
    activate_release,
    atomic_write_text,
    current_binary,
    health_url,
    install_release,
    managed_process,
    parse_env_file,
    process_exists,
    process_start_ticks,
    read_state,
    resolved_release,
    run_preflight,
    sha256_file,
    swap_current_previous,
    utc_now,
    wait_for_health,
    write_state,
)


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

    values = parse_env_file(env_file)
    run_preflight(values, preflight_script)
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

    values = parse_env_file(env_file)
    run_preflight(values, preflight_script)
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
