#!/usr/bin/env python3
"""Manage PulseDAG private-testnet node releases and process lifecycle safely."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from lifecycle_core import (
    Layout,
    LifecycleError,
    activate_release,
    ensure_layout,
    install_release,
    lifecycle_lock,
)
from lifecycle_service import (
    perform_rollback,
    perform_upgrade,
    start_node,
    status,
    stop_node,
    verify,
)


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
