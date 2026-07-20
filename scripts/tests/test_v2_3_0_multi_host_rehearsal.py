#!/usr/bin/env python3
"""Regression coverage for the v2.3.0 multi-host rehearsal contract."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
import sys
from pathlib import Path
from typing import Sequence

SCRIPT = Path(__file__).resolve().parents[1] / "private_testnet" / "multi_host_rehearsal.py"
spec = importlib.util.spec_from_file_location("multi_host_rehearsal", SCRIPT)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)


def inventory_payload() -> dict:
    nodes = []
    for index, name in enumerate(("seed-1", "node-1", "node-2", "node-3", "node-4")):
        nodes.append(
            {
                "name": name,
                "role": "seed" if index == 0 else "node",
                "transport": ["fake-ssh", name],
                "transport_mode": "argv",
                "repo_root": "/opt/pulsedag/source",
                "env_file": "/etc/pulsedag/private-testnet.env",
                "lifecycle_root": "/var/lib/pulsedag/lifecycle",
                "rpc_url": "http://127.0.0.1:8280",
            }
        )
    return {
        "schema_version": 1,
        "network_profile": module.EXPECTED_NETWORK_PROFILE,
        "chain_id": module.EXPECTED_CHAIN_ID,
        "candidate_sha": "a" * 40,
        "nodes": nodes,
        "fault": {
            "target": "node-4",
            "isolate_command": ["/usr/local/sbin/fault-hook", "isolate"],
            "restore_command": ["/usr/local/sbin/fault-hook", "restore"],
        },
        "thresholds": {
            "max_height_spread": 2,
            "min_progress_blocks": 2,
            "poll_interval_seconds": 0.1,
            "phase_timeout_seconds": 10,
        },
    }


class FakeExecutor:
    def __init__(self) -> None:
        self.status_calls = 0
        self.isolated = False

    def __call__(self, node: module.Node, argv: Sequence[str], _timeout: float) -> str:
        if tuple(argv[:2]) == ("/usr/local/sbin/fault-hook", "isolate"):
            self.isolated = True
            return "isolated\n"
        if tuple(argv[:2]) == ("/usr/local/sbin/fault-hook", "restore"):
            self.isolated = False
            return "restored\n"
        if len(argv) >= 2 and argv[0] == "python3" and argv[1] == "-c":
            url = argv[-2]
            endpoint = "/" + url.split("/", 3)[-1] if url.count("/") >= 3 else "/"
            if endpoint == "/status":
                round_index = self.status_calls // module.EXPECTED_NODE_COUNT
                self.status_calls += 1
                peer_count = 0 if self.isolated and node.name == "node-4" else 1
                data = {
                    "chain_id": module.EXPECTED_CHAIN_ID,
                    "best_height": 100 + (round_index * 2),
                    "p2p_mode": "libp2p-real",
                    "connected_peers_are_real_network": True,
                    "rpc_response_degraded": False,
                    "rpc_response_stale": False,
                    "p2p_status_degraded": False,
                    "peer_count": peer_count,
                }
            elif endpoint == "/sync/status":
                data = {
                    "consistency_ok": True,
                    "consistency_issue_count": 0,
                    "storage_replay_gap": 0,
                    "live_sync_error_active": False,
                }
            else:
                data = {"status": "ok"}
            return json.dumps(data)
        if len(argv) >= 5 and argv[:4] == ["git", "-C", node.repo_root, "rev-parse"]:
            return "a" * 40 + "\n"
        if len(argv) >= 5 and argv[:4] == ["git", "-C", node.repo_root, "status"]:
            return ""
        return "PASS\n"


class RehearsalTests(unittest.TestCase):
    def write_inventory(self, root: Path, payload: dict | None = None) -> Path:
        path = root / "inventory.json"
        path.write_text(json.dumps(payload or inventory_payload()), encoding="utf-8")
        return path

    def test_inventory_requires_five_nodes_and_loopback_rpc(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            payload = inventory_payload()
            payload["nodes"] = payload["nodes"][:4]
            path = self.write_inventory(root, payload)
            with self.assertRaisesRegex(module.RehearsalError, "exactly five nodes"):
                module.load_inventory(path)

            payload = inventory_payload()
            payload["nodes"][1]["rpc_url"] = "http://10.0.0.2:8280"
            path = self.write_inventory(root, payload)
            with self.assertRaisesRegex(module.RehearsalError, "loopback-only"):
                module.load_inventory(path)

    def test_fake_rehearsal_emits_verified_go_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            inventory = module.load_inventory(self.write_inventory(root))
            out_dir = root / "evidence"
            runner = module.RehearsalRunner(inventory, out_dir, executor=FakeExecutor())
            self.assertEqual(runner.run(), 0)
            manifest = json.loads((out_dir / "decision.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["decision"], "GO")
            self.assertFalse(manifest["public_testnet_ready"])
            self.assertFalse(manifest["thirty_day_public_testnet_clock_started"])
            module.verify_evidence(out_dir)

    def test_checksum_tampering_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            inventory = module.load_inventory(self.write_inventory(root))
            out_dir = root / "evidence"
            runner = module.RehearsalRunner(inventory, out_dir, executor=FakeExecutor())
            self.assertEqual(runner.run(), 0)
            decision = out_dir / "decision.json"
            decision.write_text(decision.read_text(encoding="utf-8") + " ", encoding="utf-8")
            with self.assertRaisesRegex(module.RehearsalError, "checksum mismatch"):
                module.verify_evidence(out_dir)

    def test_go_requires_all_mandatory_phases(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            decision = {
                "gate": "v2.3.0-multi-host-private-testnet-rehearsal",
                "node_count": 5,
                "candidate_sha": "a" * 40,
                "decision": "GO",
                "failure": None,
                "phases": [],
                "version_bump_authorized": False,
                "public_testnet_ready": False,
                "thirty_day_public_testnet_clock_started": False,
            }
            (root / "decision.json").write_text(json.dumps(decision), encoding="utf-8")
            import hashlib
            digest = hashlib.sha256((root / "decision.json").read_bytes()).hexdigest()
            (root / "SHA256SUMS").write_text(f"{digest}  decision.json\n", encoding="utf-8")
            with self.assertRaisesRegex(module.RehearsalError, "missing phases"):
                module.verify_evidence(root)


if __name__ == "__main__":
    unittest.main()
