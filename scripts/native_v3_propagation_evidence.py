#!/usr/bin/env python3
"""Read-only native-v3 first-seen propagation evidence validator.

An OBSERVED trace must be produced by external, calibrated, native-v3 25-node
instrumentation. This never starts a node/miner, checks genuine PoW, derives
GHOSTDAG anticone/K, or grants release approval. Built-in examples are
synthetic unit tests ONLY, never empirical evidence.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

MIN_NODES = 25
MIN_MINERS = 16
MIN_BLOCKS = 120
MIN_DURATION_NS = 60_000_000_000
MAX_CLOCK_UNCERTAINTY_NS = 5_000_000
EXPECTED_INTERVAL_NS = 500_000_000


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def integer(value: object) -> bool:
    return type(value) is int


def quantile(sorted_values: list[int], numerator: int, denominator: int) -> int:
    """Nearest-rank upper-order statistic (no floating point rank)."""
    require(bool(sorted_values), "empty quantile series")
    rank = (len(sorted_values) * numerator + denominator - 1) // denominator
    return sorted_values[max(0, rank - 1)]


def analyze(manifest: dict, rows: list[dict]) -> dict:
    require(type(manifest) is dict, "invalid manifest")
    require(manifest.get("schema") == "NATIVE_V3_FIRST_SEEN_V1", "wrong trace schema")
    sha = manifest.get("candidate_sha")
    require(type(sha) is str and re.fullmatch(r"[0-9a-f]{40}", sha) is not None,
            "exact SHA required")
    require(manifest.get("instrumentation") == "external_native_v3_first_seen",
            "not native-v3 first-seen instrumentation")
    require(manifest.get("synthetic") is False, "synthetic trace is not observation")
    require(integer(manifest.get("slot_ns")) and
            manifest["slot_ns"] == EXPECTED_INTERVAL_NS,
            "unexpected 500ms trial cadence")
    run_id = manifest.get("run_id")
    require(type(run_id) is str and re.fullmatch(r"[A-Za-z0-9_.-]{8,100}", run_id)
            is not None, "run ID missing")
    nodes_raw = manifest.get("nodes")
    require(type(nodes_raw) is list and len(nodes_raw) >= MIN_NODES,
            "need 25+ declared nodes")
    require(len(nodes_raw) <= 512, "unbounded node list")
    nodes: dict[str, dict] = {}
    hosts: set[str] = set()
    miners = set()
    for node in nodes_raw:
        require(type(node) is dict and set(node) ==
                {"id", "host_id", "miner", "clock_offset_ns", "clock_uncertainty_ns"},
                "invalid node fields")
        nid, host = node["id"], node["host_id"]
        require(type(nid) is str and re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", nid)
                is not None, "bad node ID")
        require(type(host) is str and re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", host)
                is not None, "bad host ID")
        require(nid not in nodes and host not in hosts,
                "duplicate node/host identity")
        require(type(node["miner"]) is bool, "miner flag must be boolean")
        require(integer(node["clock_offset_ns"]) and
                abs(node["clock_offset_ns"]) <= 10_000_000_000,
                "calibration offset missing or extreme")
        uncertainty = node["clock_uncertainty_ns"]
        require(integer(uncertainty) and
                0 <= uncertainty <= MAX_CLOCK_UNCERTAINTY_NS,
                "clock uncertainty cannot be bounded to 5ms")
        nodes[nid] = node
        hosts.add(host)
        if node["miner"]:
            miners.add(nid)
    require(len(miners) >= MIN_MINERS, "need 16+ distinct miners")
    require(type(rows) is list and len(rows) <= 2_000_000,
            "trace absent or oversized")
    events: dict[tuple[str, str], dict] = {}
    mined: dict[str, tuple[str, int]] = {}
    for row in rows:
        require(type(row) is dict and set(row) ==
                {"block_hash", "node_id", "event", "observed_wall_ns"},
                "unexpected/missing trace event fields")
        block, node_id, event, t = (row[x] for x in
                                     ("block_hash", "node_id", "event", "observed_wall_ns"))
        require(type(block) is str and re.fullmatch(r"[0-9a-f]{64}", block)
                is not None, "invalid block identifier")
        require(type(node_id) is str and node_id in nodes, "unknown node ID")
        require(type(event) is str and event in ("mined", "received"),
                "invalid event kind")
        require(integer(t) and t > 0, "invalid wall clock event")
        key = (block, node_id)
        require(key not in events, "duplicate first-seen/mined observation")
        events[key] = row
        if event == "mined":
            require(node_id in miners and block not in mined,
                    "duplicate producer or unregistered miner")
            mined[block] = (node_id, t - nodes[node_id]["clock_offset_ns"])
    require(len(mined) >= MIN_BLOCKS, "need 120 distinct mined block identifiers")
    require(len({source for source, _ in mined.values()}) >= MIN_MINERS,
            "need observations from 16 active miners")
    producer_times = sorted(t for _, t in mined.values())
    require(producer_times[-1] - producer_times[0] >= MIN_DURATION_NS,
            "trace shorter than 60s")
    require(all(events[(b, source)]["event"] == "mined"
                for b, (source, _) in mined.items()), "missing mining origin")
    require(all(block in mined for block, _ in events),
            "received event without origin")

    total_pairs = len(mined) * (len(nodes) - 1)
    delays: list[int] = []
    upper_bounded: list[int] = []
    missing = 0
    ambiguous_early = 0
    for block, (source, produced) in mined.items():
        for receiver, cfg in nodes.items():
            if receiver == source:
                continue
            obs = events.get((block, receiver))
            if obs is None:
                missing += 1
                continue
            require(obs["event"] == "received",
                    "non-origin peer event is not received")
            delay = obs["observed_wall_ns"] - cfg["clock_offset_ns"] - produced
            uncertainty = (cfg["clock_uncertainty_ns"] +
                           nodes[source]["clock_uncertainty_ns"])
            require(delay + uncertainty >= 0,
                    "receive predates mining beyond clock uncertainty")
            if delay < 0:
                ambiguous_early += 1
            delays.append(max(0, delay))
            upper_bounded.append(max(0, delay + uncertainty))
    require(bool(delays), "no peer receptions")
    require(len(delays) + missing == total_pairs,
            "peer coverage accounting failed")
    delays.sort()
    upper_bounded.sort()
    percentiles = {name: quantile(delays, p, 1000)
                   for name, p in (("p50", 500), ("p95", 950),
                                   ("p99", 990), ("p999", 999))}
    return {
        "status": "OBSERVATIONS_NEED_INDEPENDENT_AUTHENTICATION_NOT_K_FREEZE",
        "run_id": run_id,
        "candidate_sha": sha,
        "declared_nodes": len(nodes),
        "declared_distinct_hosts": len(hosts),
        "active_mining_sources": len({s for s, _ in mined.values()}),
        "observed_produced_blocks": len(mined),
        "producer_span_ns": producer_times[-1] - producer_times[0],
        "received_peer_block_pairs": len(delays),
        "missing_peer_block_pairs": missing,
        "missing_pairs_fraction": f"{missing / total_pairs:.6f}",
        "negative_delays_within_clock_uncertainty": ambiguous_early,
        "observed_delay_ns_nearest_rank": percentiles,
        "p999_upper_bound_with_declared_clock_uncertainty_ns":
            quantile(upper_bounded, 999, 1000),
        "not_authenticated_native_headers_or_anticone": True,
        "not_verified_clock_calibration_or_physical_host_independence": True,
        "not_K_false_red_or_finality_probability": True,
        "not_convergence_sla_or_mainnet_go": True,
        "no_automatic_K_or_parent_limit_selection": True,
    }


def self_test() -> None:
    nodes = [{"id": f"n{i:02d}", "host_id": f"host{i:02d}",
              "miner": i < MIN_MINERS,
              "clock_offset_ns": i * 50_000,
              "clock_uncertainty_ns": 1_000_000} for i in range(MIN_NODES)]
    manifest = {"schema": "NATIVE_V3_FIRST_SEEN_V1", "candidate_sha": "a" * 40,
                "run_id": "SYNTHETIC-SELFTEST", "slot_ns": EXPECTED_INTERVAL_NS,
                "instrumentation": "external_native_v3_first_seen",
                "synthetic": False, "nodes": nodes}
    # Test-only generated samples in memory: not saved as empirical evidence.
    rows = []
    for block_index in range(MIN_BLOCKS + 1):
        origin = block_index % MIN_MINERS
        block = f"{block_index+1:064x}"
        produced = 1_800_000_000_000_000_000 + block_index * EXPECTED_INTERVAL_NS
        for i, node in enumerate(nodes):
            rows.append({"block_hash": block, "node_id": node["id"],
                         "event": "mined" if i == origin else "received",
                         "observed_wall_ns": produced + node["clock_offset_ns"] +
                         (0 if i == origin else 35_000_000 + i * 1_000_000)})
    result = analyze(manifest, rows)
    require(result["observed_produced_blocks"] == MIN_BLOCKS + 1, "block count")
    require(result["missing_peer_block_pairs"] == 0, "unexpected missingness")
    require(result["observed_delay_ns_nearest_rank"]["p999"] > 0, "missing p999")
    wrong = [
        ({**manifest, "synthetic": True}, rows),
        ({**manifest, "nodes": nodes[:24]}, rows),
        ({**manifest, "candidate_sha": "insecure"}, rows),
        ({**manifest, "nodes":
          [{**nodes[0], "clock_uncertainty_ns": 7_000_000}] + nodes[1:]}, rows),
        (manifest, rows[:-1] + [rows[0]]),
        (manifest, [r for r in rows if r["event"] != "mined"]),
        (manifest, rows[:100]),
    ]
    for bad_manifest, bad_rows in wrong:
        try:
            analyze(bad_manifest, bad_rows)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid empirical trace admitted")
    dropped = [r for r in rows if not (r["event"] == "received" and
                                      r["node_id"] == "n24")]
    require(analyze(manifest, dropped)["missing_peer_block_pairs"] ==
            MIN_BLOCKS + 1, "missing receiver observations were hidden")
    print("PASS: synthetic-only validator scenarios (NOT measured propagation)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--events-jsonl", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    require(args.manifest and args.events_jsonl and args.output,
            "--manifest, --events-jsonl and --output are required")
    require(args.manifest.is_file() and args.events_jsonl.is_file(),
            "external measured trace files were not provided")
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    with args.events_jsonl.open(encoding="utf-8") as source:
        events = [json.loads(line) for line in source if line.strip()]
    result = analyze(manifest, events)
    args.output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n",
                           encoding="utf-8")
    print("TRACE_ANALYZED_NOT_K_FREEZE: external authentication is still required")


if __name__ == "__main__":
    main()
