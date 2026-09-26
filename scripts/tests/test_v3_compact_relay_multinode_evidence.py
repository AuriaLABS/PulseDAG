#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "v3_compact_relay_multinode_evidence.py"
SPEC = importlib.util.spec_from_file_location("compact_evidence", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class CompactRelayEvidenceTests(unittest.TestCase):
    def write_node(self, root: Path, index: int, active: bool = True) -> None:
        endpoints = root / "endpoints"
        endpoints.mkdir(parents=True, exist_ok=True)
        outbound = index if active else 0
        inbound = index if active else 0
        p2p = {
            "ok": True,
            "data": {
                "mode": "libp2p-real",
                "connected_peers_are_real_network": True,
                "p2p_status_degraded": False,
                "p2p_status_stale": False,
                "chain_id": "pulsedag-test",
                "local_protocol_capabilities_v1": {
                    "capabilities_version": 1,
                    "protocol_identity": {
                        "chain_id": "pulsedag-test",
                        "genesis_hash": "11" * 32,
                        "transaction_protocol_version": 2,
                        "block_header_protocol_version": 2,
                        "consensus_mode": "ghostdag_v1",
                        "dag_ordering_version": "ghostdag-v1-topological-v1",
                    },
                    "consensus_metadata_schema_version": 1,
                    "finality_policy_version": "ghostdag-v1-finality",
                    "supports_dag_frontier": True,
                    "supports_consensus_metadata": True,
                    "high_cadence_allowed": False,
                },
                "peer_count": 2,
                "connected_peers": ["peer-a", "peer-b"],
                "compact_relay_transport": {
                    "outbound_carriers_encoded_total": outbound,
                    "inbound_carriers_accepted_total": inbound,
                    "outbound_encoded_bytes_total": outbound * 200,
                    "inbound_accepted_bytes_total": inbound * 180,
                    "outbound_capability_messages_total": 1,
                    "inbound_capability_messages_total": 1,
                    "outbound_announcements_total": outbound,
                    "inbound_announcements_total": inbound,
                    "outbound_body_requests_total": 0,
                    "inbound_body_requests_total": 0,
                    "outbound_body_responses_total": 0,
                    "inbound_body_responses_total": 0,
                    "decode_failures_total": 0,
                    "max_carrier_bytes": 60 * 1024,
                },
                "compact_relay_controller": {
                    "announcements_received_total": inbound,
                    "reconstructed_blocks_ready_total": inbound,
                    "body_requests_sent_total": 0,
                    "body_txids_requested_total": 0,
                    "body_requests_received_total": 0,
                    "body_responses_sent_total": 0,
                    "body_transactions_sent_total": 0,
                    "body_responses_received_total": 0,
                    "full_block_requests_total": 0,
                    "full_block_service_fallback_total": 0,
                    "invalid_response_total": 0,
                    "pending_announcements_current": 0,
                    "pending_announcements_peak": 1,
                    "max_inflight_per_peer": 64,
                },
            },
        }
        status = {
            "ok": True,
            "data": {
                "rpc_response_degraded": False,
                "rpc_response_stale": False,
                "chain_id": "pulsedag-test",
                "best_height": 42,
                "selected_tip": "tip-final",
            },
        }
        (endpoints / f"n{index}-p2p-status-final.json").write_text(json.dumps(p2p))
        (endpoints / f"n{index}-status-final.json").write_text(json.dumps(status))

    def test_five_node_compact_activity_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "PASS")
            self.assertEqual(report["observed_nodes"], 5)
            self.assertGreater(report["transport_totals"]["outbound_encoded_bytes_total"], 0)
            self.assertGreater(report["controller_totals"]["reconstructed_blocks_ready_total"], 0)

    def test_missing_compact_activity_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index, active=False)
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn("no_compact_outbound_carriers", report["failure_reasons"])
            self.assertIn("no_reconstructed_blocks_ready", report["failure_reasons"])

    def test_protocol_identity_divergence_fails_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n5-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["local_protocol_capabilities_v1"]["protocol_identity"][
                "genesis_hash"
            ] = "22" * 32
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "protocol_capabilities_convergence_failed",
                report["failure_reasons"],
            )

    def test_stale_protocol_identity_field_names_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n5-p2p-status-final.json"
            payload = json.loads(path.read_text())
            identity = payload["data"]["local_protocol_capabilities_v1"][
                "protocol_identity"
            ]
            identity["block_header_version"] = identity.pop(
                "block_header_protocol_version"
            )
            identity["ordering_version"] = identity.pop("dag_ordering_version")
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "protocol_capabilities_shape_invalid",
                report["failure_reasons"],
            )

    def test_missing_rpc_envelope_status_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n2-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload.pop("ok")
            path.write_text(json.dumps(payload))
            with self.assertRaisesRegex(ValueError, "RPC envelope is not ok=true"):
                MODULE.collect(root, 5, 0, "commit", "tree")

    def test_unsuccessful_rpc_envelope_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n2-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["ok"] = False
            path.write_text(json.dumps(payload))
            with self.assertRaisesRegex(ValueError, "RPC envelope is not ok=true"):
                MODULE.collect(root, 5, 0, "commit", "tree")

    def test_missing_compact_telemetry_on_one_node_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n5-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"].pop("compact_relay_transport")
            payload["data"].pop("compact_relay_controller")
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "compact_transport_telemetry_missing",
                report["failure_reasons"],
            )
            self.assertIn(
                "compact_controller_telemetry_missing",
                report["failure_reasons"],
            )
            self.assertIn(
                "compact_carrier_bound_missing_or_unexpected",
                report["failure_reasons"],
            )

    def test_non_boolean_real_network_marker_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n3-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["connected_peers_are_real_network"] = "true"
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "p2p_real_network_semantics_not_confirmed",
                report["failure_reasons"],
            )

    def test_missing_freshness_marker_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n4-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"].pop("rpc_response_stale")
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "degraded_or_stale_node_status_capture",
                report["failure_reasons"],
            )

    def test_degraded_capture_fails_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n2-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["p2p_status_degraded"] = True
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "degraded_or_stale_p2p_status_capture",
                report["failure_reasons"],
            )

    def test_height_divergence_fails_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n4-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["best_height"] = 41
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "best_height_convergence_failed",
                report["failure_reasons"],
            )

    def test_missing_node_status_capture_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            (root / "endpoints" / "n4-status-final.json").unlink()
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn("node_status_capture_missing", report["failure_reasons"])

    def test_incomplete_protocol_identity_shape_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n3-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["local_protocol_capabilities_v1"]["protocol_identity"].pop(
                "genesis_hash"
            )
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "protocol_capabilities_shape_invalid",
                report["failure_reasons"],
            )

    def test_missing_required_telemetry_counter_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n2-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["compact_relay_transport"].pop(
                "outbound_capability_messages_total"
            )
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "compact_transport_telemetry_shape_invalid",
                report["failure_reasons"],
            )

    def test_unexpected_controller_inflight_bound_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n5-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["compact_relay_controller"]["max_inflight_per_peer"] = 65
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn(
                "compact_controller_inflight_bound_missing_or_unexpected",
                report["failure_reasons"],
            )

    def test_decode_failure_fails_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index in range(1, 6):
                self.write_node(root, index)
            path = root / "endpoints" / "n3-p2p-status-final.json"
            payload = json.loads(path.read_text())
            payload["data"]["compact_relay_transport"]["decode_failures_total"] = 1
            path.write_text(json.dumps(payload))
            report = MODULE.collect(root, 5, 0, "commit", "tree")
            self.assertEqual(report["result"], "FAIL")
            self.assertIn("compact_decode_failures=1", report["failure_reasons"])


if __name__ == "__main__":
    unittest.main()
