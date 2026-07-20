#!/usr/bin/env python3
"""Write a fixed Task 12 namespace inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--candidate-sha", required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--state-root", type=Path, required=True)
    parser.add_argument("--fault-hook", type=Path, required=True)
    args = parser.parse_args()
    if len(args.candidate_sha) != 40 or any(char not in "0123456789abcdef" for char in args.candidate_sha):
        parser.error("--candidate-sha must be 40 lowercase hexadecimal characters")
    for value in (args.output, args.workspace, args.state_root, args.fault_hook):
        if not value.is_absolute():
            parser.error("all paths must be absolute")

    definitions = (
        ("seed-1", "seed", "pdg-s1"),
        ("node-1", "node", "pdg-n1"),
        ("node-2", "node", "pdg-n2"),
        ("node-3", "node", "pdg-n3"),
        ("node-4", "node", "pdg-n4"),
    )
    nodes = []
    for name, role, namespace in definitions:
        root = args.state_root / "nodes" / name
        nodes.append({
            "name": name,
            "role": role,
            "transport": ["sudo", "-n", "ip", "netns", "exec", namespace],
            "transport_mode": "argv",
            "repo_root": str(args.workspace),
            "env_file": str(root / "node.env"),
            "lifecycle_root": str(root / "lifecycle"),
            "rpc_url": "http://127.0.0.1:8280",
        })
    payload = {
        "schema_version": 1,
        "network_profile": "private-testnet-v2.3.0",
        "chain_id": "pulsedag-private-v2.3.0",
        "candidate_sha": args.candidate_sha,
        "nodes": nodes,
        "fault": {
            "target": "node-4",
            "isolate_command": [str(args.fault_hook), "isolate"],
            "restore_command": [str(args.fault_hook), "restore"],
        },
        "thresholds": {
            "max_height_spread": 2,
            "min_progress_blocks": 2,
            "poll_interval_seconds": 2,
            "phase_timeout_seconds": 300,
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
