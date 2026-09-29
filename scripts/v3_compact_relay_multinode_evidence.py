#!/usr/bin/env python3
"""Collect exact-candidate compact-relay evidence from a 5-node rehearsal bundle."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

TRANSPORT_SUM_KEYS = (
    "outbound_carriers_encoded_total",
    "inbound_carriers_accepted_total",
    "outbound_encoded_bytes_total",
    "inbound_accepted_bytes_total",
    "outbound_capability_messages_total",
    "inbound_capability_messages_total",
    "outbound_announcements_total",
    "inbound_announcements_total",
    "outbound_body_requests_total",
    "inbound_body_requests_total",
    "outbound_body_responses_total",
    "inbound_body_responses_total",
    "decode_failures_total",
)

CONTROLLER_SUM_KEYS = (
    "announcements_received_total",
    "reconstructed_blocks_ready_total",
    "body_requests_sent_total",
    "body_txids_requested_total",
    "body_requests_received_total",
    "body_responses_sent_total",
    "body_transactions_sent_total",
    "body_responses_received_total",
    "full_block_requests_total",
    "full_block_service_fallback_total",
    "invalid_response_total",
)

NODE_FILE = re.compile(r"^(n\d+)-p2p-status-final\.json$")


def load_data(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text())
    if not isinstance(payload, dict):
        raise ValueError(f"{path}: expected object payload")
    if payload.get("ok") is not True:
        raise ValueError(f"{path}: RPC envelope is not ok=true")
    data = payload.get("data")
    if not isinstance(data, dict):
        raise ValueError(f"{path}: expected object data")
    return data


def integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key, 0)
    if isinstance(value, bool) or not isinstance(value, int):
        return 0
    return value


def nonnegative_integer(mapping: dict[str, Any], key: str) -> bool:
    value = mapping.get(key)
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def protocol_capabilities_shape_ok(capabilities: dict[str, Any]) -> bool:
    identity = nested(capabilities, "protocol_identity")
    genesis_hash = identity.get("genesis_hash")
    return (
        nonnegative_integer(capabilities, "capabilities_version")
        and integer(capabilities, "capabilities_version") > 0
        and nonnegative_integer(capabilities, "consensus_metadata_schema_version")
        and integer(capabilities, "consensus_metadata_schema_version") > 0
        and capabilities.get("supports_dag_frontier") is True
        and capabilities.get("supports_consensus_metadata") is True
        and isinstance(identity.get("chain_id"), str)
        and bool(identity.get("chain_id"))
        and isinstance(genesis_hash, str)
        and len(genesis_hash) == 64
        and all(ch in "0123456789abcdefABCDEF" for ch in genesis_hash)
        and nonnegative_integer(identity, "transaction_protocol_version")
        and integer(identity, "transaction_protocol_version") > 0
        and nonnegative_integer(identity, "block_header_protocol_version")
        and integer(identity, "block_header_protocol_version") > 0
        and isinstance(identity.get("consensus_mode"), str)
        and bool(identity.get("consensus_mode"))
        and isinstance(identity.get("dag_ordering_version"), str)
        and bool(identity.get("dag_ordering_version"))
        and isinstance(capabilities.get("finality_policy_version"), str)
        and bool(capabilities.get("finality_policy_version"))
    )


def nested(mapping: dict[str, Any], key: str) -> dict[str, Any]:
    value = mapping.get(key, {})
    return value if isinstance(value, dict) else {}


def status_file_for(p2p_path: Path) -> Path:
    return p2p_path.with_name(
        p2p_path.name.replace("-p2p-status-final.json", "-status-final.json")
    )


def node_number(name: str) -> int:
    return int(name[1:])


def collect(
    artifact_root: Path,
    expected_nodes: int,
    rehearsal_exit_code: int,
    candidate_commit: str,
    candidate_tree: str,
) -> dict[str, Any]:
    discovered: dict[str, Path] = {}
    duplicates: list[str] = []
    for path in artifact_root.rglob("n*-p2p-status-final.json"):
        match = NODE_FILE.match(path.name)
        if not match:
            continue
        node = match.group(1)
        if node in discovered:
            duplicates.append(node)
        else:
            discovered[node] = path

    nodes: list[dict[str, Any]] = []
    transport_totals = {key: 0 for key in TRANSPORT_SUM_KEYS}
    controller_totals = {key: 0 for key in CONTROLLER_SUM_KEYS}
    failures: list[str] = []

    for node in sorted(discovered, key=node_number):
        p2p_path = discovered[node]
        p2p = load_data(p2p_path)
        status_path = status_file_for(p2p_path)
        status_capture_present = status_path.exists()
        status = load_data(status_path) if status_capture_present else {}
        transport = nested(p2p, "compact_relay_transport")
        controller = nested(p2p, "compact_relay_controller")

        for key in TRANSPORT_SUM_KEYS:
            transport_totals[key] += integer(transport, key)
        for key in CONTROLLER_SUM_KEYS:
            controller_totals[key] += integer(controller, key)

        selected_tip = status.get("selected_tip") or nested(status, "metrics").get("selected_tip") or ""
        best_height = integer(status, "best_height")
        chain_id = (
            p2p.get("chain_id")
            or status.get("chain_id")
            or status.get("network_id")
            or ""
        )
        protocol_capabilities = p2p.get("local_protocol_capabilities_v1")
        if not isinstance(protocol_capabilities, dict):
            protocol_capabilities = {}
        peer_count = integer(p2p, "peer_count")
        if peer_count == 0 and isinstance(p2p.get("connected_peers"), list):
            peer_count = len(p2p["connected_peers"])

        nodes.append(
            {
                "node": node,
                "p2p_status_path": str(p2p_path.relative_to(artifact_root)),
                "status_path": (
                    str(status_path.relative_to(artifact_root)) if status_capture_present else None
                ),
                "status_capture_present": status_capture_present,
                "mode": p2p.get("mode") or p2p.get("p2p_mode") or "",
                "connected_peers_are_real_network": (
                    p2p.get("connected_peers_are_real_network") is True
                ),
                "p2p_status_degraded": p2p.get("p2p_status_degraded") is not False,
                "p2p_status_stale": p2p.get("p2p_status_stale") is not False,
                "rpc_response_degraded": status.get("rpc_response_degraded") is not False,
                "rpc_response_stale": status.get("rpc_response_stale") is not False,
                "chain_id": chain_id,
                "local_protocol_capabilities_v1": protocol_capabilities,
                "peer_count": peer_count,
                "best_height": best_height,
                "selected_tip": selected_tip,
                "transport": transport,
                "controller": controller,
            }
        )

    if rehearsal_exit_code != 0:
        failures.append(f"rehearsal_exit_code={rehearsal_exit_code}")
    if duplicates:
        failures.append("duplicate_node_status=" + ",".join(sorted(set(duplicates))))
    if len(nodes) != expected_nodes:
        failures.append(f"node_count={len(nodes)} expected={expected_nodes}")

    modes = {node["mode"] for node in nodes}
    if nodes and modes != {"libp2p-real"}:
        failures.append("non_real_p2p_modes=" + ",".join(sorted(str(x) for x in modes)))
    if any(not node["connected_peers_are_real_network"] for node in nodes):
        failures.append("p2p_real_network_semantics_not_confirmed")
    if any(node["p2p_status_degraded"] or node["p2p_status_stale"] for node in nodes):
        failures.append("degraded_or_stale_p2p_status_capture")
    if any(node["rpc_response_degraded"] or node["rpc_response_stale"] for node in nodes):
        failures.append("degraded_or_stale_node_status_capture")

    chain_ids = {node["chain_id"] for node in nodes if node["chain_id"]}
    if len(chain_ids) != 1 or len(chain_ids) != len({node["chain_id"] for node in nodes}):
        failures.append("chain_id_convergence_failed")

    protocol_capability_encodings = {
        json.dumps(
            node["local_protocol_capabilities_v1"],
            sort_keys=True,
            separators=(",", ":"),
        )
        for node in nodes
        if node["local_protocol_capabilities_v1"]
    }
    if len(protocol_capability_encodings) != 1 or any(
        not node["local_protocol_capabilities_v1"] for node in nodes
    ):
        failures.append("protocol_capabilities_convergence_failed")
    if any(
        not protocol_capabilities_shape_ok(node["local_protocol_capabilities_v1"])
        for node in nodes
    ):
        failures.append("protocol_capabilities_shape_invalid")
    protocol_capabilities_v1 = (
        json.loads(next(iter(protocol_capability_encodings)))
        if len(protocol_capability_encodings) == 1
        else None
    )
    if protocol_capabilities_v1 is not None:
        protocol_chain_id = nested(
            protocol_capabilities_v1,
            "protocol_identity",
        ).get("chain_id", "")
        if len(chain_ids) != 1 or protocol_chain_id not in chain_ids:
            failures.append("protocol_identity_chain_id_mismatch")

    tips = {node["selected_tip"] for node in nodes if node["selected_tip"]}
    if len(tips) != 1 or any(not node["selected_tip"] for node in nodes):
        failures.append("selected_tip_convergence_failed")

    heights = {node["best_height"] for node in nodes}
    if len(heights) != 1 or any(node["best_height"] <= 0 for node in nodes):
        failures.append("best_height_convergence_failed")

    if any(node["peer_count"] <= 0 for node in nodes):
        failures.append("zero_peer_node_present")

    if any(not node["status_capture_present"] for node in nodes):
        failures.append("node_status_capture_missing")

    if any(not node["transport"] for node in nodes):
        failures.append("compact_transport_telemetry_missing")
    if any(not node["controller"] for node in nodes):
        failures.append("compact_controller_telemetry_missing")

    required_transport_integer_fields = (*TRANSPORT_SUM_KEYS, "max_carrier_bytes")
    required_controller_integer_fields = (
        *CONTROLLER_SUM_KEYS,
        "pending_announcements_current",
        "pending_announcements_peak",
        "max_inflight_per_peer",
    )
    if any(
        any(
            not nonnegative_integer(node["transport"], key)
            for key in required_transport_integer_fields
        )
        for node in nodes
    ):
        failures.append("compact_transport_telemetry_shape_invalid")
    if any(
        any(
            not nonnegative_integer(node["controller"], key)
            for key in required_controller_integer_fields
        )
        for node in nodes
    ):
        failures.append("compact_controller_telemetry_shape_invalid")
    if any(
        integer(node["controller"], "max_inflight_per_peer") != 64
        for node in nodes
    ):
        failures.append("compact_controller_inflight_bound_missing_or_unexpected")
    if any(
        integer(node["transport"], "max_carrier_bytes") != 60 * 1024
        for node in nodes
    ):
        failures.append("compact_carrier_bound_missing_or_unexpected")

    if transport_totals["outbound_carriers_encoded_total"] <= 0:
        failures.append("no_compact_outbound_carriers")
    if transport_totals["inbound_carriers_accepted_total"] <= 0:
        failures.append("no_compact_inbound_carriers")
    if transport_totals["outbound_encoded_bytes_total"] <= 0:
        failures.append("no_compact_outbound_bytes")
    if transport_totals["inbound_accepted_bytes_total"] <= 0:
        failures.append("no_compact_inbound_bytes")
    if transport_totals["outbound_announcements_total"] <= 0:
        failures.append("no_compact_outbound_announcements")
    if transport_totals["inbound_announcements_total"] <= 0:
        failures.append("no_compact_inbound_announcements")
    if controller_totals["announcements_received_total"] <= 0:
        failures.append("no_controller_announcements")
    if controller_totals["reconstructed_blocks_ready_total"] <= 0:
        failures.append("no_reconstructed_blocks_ready")
    if transport_totals["decode_failures_total"] != 0:
        failures.append(
            f"compact_decode_failures={transport_totals['decode_failures_total']}"
        )
    if controller_totals["invalid_response_total"] != 0:
        failures.append(
            f"compact_invalid_responses={controller_totals['invalid_response_total']}"
        )
    if any(integer(node["controller"], "pending_announcements_current") != 0 for node in nodes):
        failures.append("pending_compact_announcements_at_final_capture")

    return {
        "schema": "pulsedag-compact-relay-multinode-evidence-v1",
        "result": "PASS" if not failures else "FAIL",
        "candidate_commit": candidate_commit,
        "candidate_tree": candidate_tree,
        "artifact_root": str(artifact_root),
        "expected_nodes": expected_nodes,
        "observed_nodes": len(nodes),
        "rehearsal_exit_code": rehearsal_exit_code,
        "chain_ids": sorted(chain_ids),
        "local_protocol_capabilities_v1": protocol_capabilities_v1,
        "selected_tips": sorted(tips),
        "best_heights": sorted(heights),
        "transport_totals": transport_totals,
        "controller_totals": controller_totals,
        "full_block_fallback_observed": (
            controller_totals["full_block_requests_total"] > 0
            or controller_totals["full_block_service_fallback_total"] > 0
        ),
        "bandwidth_claim": (
            "encoded compact-relay carrier bytes only; this evidence does not claim socket-level "
            "delivery bytes or savings versus full-block relay"
        ),
        "nodes": nodes,
        "failure_reasons": failures,
    }


def render_markdown(report: dict[str, Any]) -> str:
    t = report["transport_totals"]
    c = report["controller_totals"]
    lines = [
        "# Compact DAG relay multi-node evidence",
        "",
        f"- result: **{report['result']}**",
        f"- candidate_commit: `{report['candidate_commit']}`",
        f"- candidate_tree: `{report['candidate_tree']}`",
        f"- nodes: {report['observed_nodes']}/{report['expected_nodes']}",
        f"- chain_ids: {', '.join(report['chain_ids']) or 'none'}",
        "- protocol_capabilities_v1: `"
        + json.dumps(
            report["local_protocol_capabilities_v1"],
            sort_keys=True,
            separators=(",", ":"),
        )
        + "`",
        f"- selected_tips: {', '.join(report['selected_tips']) or 'none'}",
        f"- best_heights: {', '.join(str(value) for value in report['best_heights']) or 'none'}",
        f"- outbound compact carriers: {t['outbound_carriers_encoded_total']}",
        f"- inbound accepted compact carriers: {t['inbound_carriers_accepted_total']}",
        f"- outbound encoded compact bytes: {t['outbound_encoded_bytes_total']}",
        f"- inbound accepted compact bytes: {t['inbound_accepted_bytes_total']}",
        f"- compact announcements received by controller: {c['announcements_received_total']}",
        f"- reconstructed blocks ready: {c['reconstructed_blocks_ready_total']}",
        f"- full-block request fallbacks: {c['full_block_requests_total']}",
        f"- full-block service fallbacks: {c['full_block_service_fallback_total']}",
        f"- invalid compact responses: {c['invalid_response_total']}",
        "",
        f"Bandwidth semantics: {report['bandwidth_claim']}.",
        "",
        "## Per-node final capture",
        "",
        "| node | mode | peers | height | selected tip | outbound compact bytes | reconstructed | pending |",
        "|---|---|---:|---:|---|---:|---:|---:|",
    ]
    for node in report["nodes"]:
        lines.append(
            "| {node} | {mode} | {peers} | {height} | {tip} | {bytes} | {reconstructed} | {pending} |".format(
                node=node["node"],
                mode=node["mode"],
                peers=node["peer_count"],
                height=node["best_height"],
                tip=node["selected_tip"] or "-",
                bytes=integer(node["transport"], "outbound_encoded_bytes_total"),
                reconstructed=integer(node["controller"], "reconstructed_blocks_ready_total"),
                pending=integer(node["controller"], "pending_announcements_current"),
            )
        )
    if report["failure_reasons"]:
        lines.extend(["", "## Failure reasons", ""])
        lines.extend(f"- {reason}" for reason in report["failure_reasons"])
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-md", type=Path, required=True)
    parser.add_argument("--expected-nodes", type=int, default=5)
    parser.add_argument("--rehearsal-exit-code", type=int, default=0)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--candidate-tree", required=True)
    args = parser.parse_args()

    report = collect(
        args.artifact_root,
        args.expected_nodes,
        args.rehearsal_exit_code,
        args.candidate_commit,
        args.candidate_tree,
    )
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_md.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    args.output_md.write_text(render_markdown(report))
    return 0 if report["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
