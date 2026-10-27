#!/usr/bin/env python3
"""Read-only native-v3 first-seen propagation evidence validator.

An OBSERVED trace must be produced by external, calibrated, native-v3 25-node
instrumentation. This never starts a node/miner, checks genuine PoW, derives
GHOSTDAG anticone/K, or grants release approval. Built-in examples are
synthetic unit tests ONLY, never empirical evidence.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
from pathlib import Path
from typing import BinaryIO

MIN_NODES = 25
MIN_MINERS = 16
MIN_BLOCKS = 120
MIN_DURATION_NS = 60_000_000_000
MAX_CLOCK_UNCERTAINTY_NS = 5_000_000
EXPECTED_INTERVAL_NS = 500_000_000

# Hard local parser budgets checked WHILE consuming bytes, before JSON
# decoding/allocating the potentially untrusted first-seen event rows.
# 200,000 rows permit 25-peer coverage for up to ~8,000 mined blocks; this
# is NOT a 500ms network performance assumption or consensus limit.
MAX_TRACE_ROWS = 200_000
MAX_TRACE_BYTES = 64 * 1024 * 1024
MAX_EVENT_LINE_BYTES = 4 * 1024
MAX_MANIFEST_BYTES = 128 * 1024


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def integer(value: object) -> bool:
    return type(value) is int


def parse_bounded_manifest(source: BinaryIO) -> tuple[dict, str]:
    # Do not trust Path.stat(): the input may change between stat and read.
    raw = source.read(MAX_MANIFEST_BYTES + 1)
    require(len(raw) <= MAX_MANIFEST_BYTES,
            "manifest exceeds bounded 128-KiB byte budget")
    manifest = json.loads(raw)
    require(type(manifest) is dict, "manifest must be a JSON object")
    return manifest, hashlib.sha256(raw).hexdigest()


def parse_bounded_events(
    source: BinaryIO, *, max_rows: int = MAX_TRACE_ROWS,
    max_total_bytes: int = MAX_TRACE_BYTES,
    max_line_bytes: int = MAX_EVENT_LINE_BYTES,
) -> tuple[list[dict], str, int]:
    # Every line must be one JSON object; reject blank lines and overlong
    # lines BEFORE allocating full untrusted input. These test-only optional
    # lower budgets exercise the same invariants without huge allocations.
    require(0 < max_rows <= MAX_TRACE_ROWS and
            0 < max_total_bytes <= MAX_TRACE_BYTES and
            0 < max_line_bytes <= MAX_EVENT_LINE_BYTES,
            "invalid trace parser resource budget")
    rows: list[dict] = []
    digest = hashlib.sha256()
    consumed = 0
    while True:
        line = source.readline(max_line_bytes + 1)
        if not line:
            break
        require(len(line) <= max_line_bytes,
                "JSONL record exceeds bounded 4-KiB line budget")
        consumed += len(line)
        require(consumed <= max_total_bytes,
                "JSONL trace exceeds bounded total byte budget")
        require(len(rows) < max_rows, "JSONL trace row budget exceeded")
        require(bool(line.strip()), "blank JSONL record is invalid")
        digest.update(line)
        row = json.loads(line)
        require(type(row) is dict, "JSONL record must be a JSON object")
        rows.append(row)
    return rows, digest.hexdigest(), consumed


def quantile(sorted_values: list[int], numerator: int, denominator: int) -> int:
    """Nearest-rank upper-order statistic (no floating point rank)."""
    require(bool(sorted_values), "empty quantile series")
    rank = (len(sorted_values) * numerator + denominator - 1) // denominator
    return sorted_values[max(0, rank - 1)]


def analyze(manifest: dict, rows: list[dict], *, expected_sha: str,
            expected_network_id: str, expected_genesis_hash: str) -> dict:
    # All three pins come from the independent operator, NEVER the trace.
    require(type(expected_sha) is str and
            re.fullmatch(r"[0-9a-f]{40}", expected_sha) is not None,
            "independent exact SHA pin required")
    require(type(expected_network_id) is str and
            re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", expected_network_id)
            is not None, "independent network ID pin required")
    require(type(expected_genesis_hash) is str and
            re.fullmatch(r"[0-9a-f]{64}", expected_genesis_hash) is not None,
            "independent frozen genesis hash pin required")
    require(type(manifest) is dict, "invalid manifest")
    require(manifest.get("schema") == "NATIVE_V3_FIRST_SEEN_V1", "wrong trace schema")
    sha = manifest.get("candidate_sha")
    require(type(sha) is str and sha == expected_sha,
            "trace SHA disagrees with independently pinned exact candidate")
    require(manifest.get("network_id") == expected_network_id,
            "trace network ID disagrees with independently pinned network")
    require(manifest.get("genesis_hash") == expected_genesis_hash,
            "trace genesis hash disagrees with independently pinned genesis")
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
    require(type(rows) is list and len(rows) <= MAX_TRACE_ROWS,
            "trace absent or exceeds 200,000-row bounded analysis budget")
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
    # In a partially observed network, observed-only percentiles are
    # selection-biased: a peer that never receives a block may be precisely
    # the slowest one. An absent reception is RIGHT-CENSORED (unknown), not 0
    # latency, not the observed p99, and not evidence of fast convergence.
    # Bound the full N-pair order statistic by placing all M missing samples
    # at 0 (best case) or at +infinity (worst case); no distribution model.
    censored_bounds: dict[str, dict[str, int | None]] = {}
    for name, permille in (("p50", 500), ("p95", 950),
                           ("p99", 990), ("p999", 999)):
        rank = (total_pairs * permille + 999) // 1000  # 1-based ceil
        minimum_possible = (delays[rank - missing - 1]
                            if rank > missing else 0)
        maximum_possible = (delays[rank - 1]
                            if rank <= len(delays) else None)
        censored_bounds[name] = {
            "lower_ns_missing_assumed_zero": minimum_possible,
            "upper_ns_missing_may_be_unbounded": maximum_possible,
        }
        require(maximum_possible is None or
                minimum_possible <= maximum_possible,
                f"invalid missingness quantile interval: {name}")
    return {
        "status": "OBSERVATIONS_NEED_INDEPENDENT_AUTHENTICATION_NOT_K_FREEZE",
        "run_id": run_id,
        "candidate_sha": sha,
        "network_id": expected_network_id,
        "genesis_hash": expected_genesis_hash,
        "trace_identifiers_independently_pinned": True,
        "independent_pins_do_not_authenticate_events_or_headers": True,
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
        "censored_full_population_quantile_bounds_ns": censored_bounds,
        "p999_full_population_finite_upper_bound_proven":
            censored_bounds["p999"]["upper_ns_missing_may_be_unbounded"] is not None,
        "all_peer_block_receptions_observed": missing == 0,
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
                "network_id": "TEST_ONLY_NATIVE_V3", "genesis_hash": "b" * 64,
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
    # The pins model trusted operator configuration, not a manifest override.
    pins = {"expected_sha": "a" * 40,
            "expected_network_id": "TEST_ONLY_NATIVE_V3",
            "expected_genesis_hash": "b" * 64}
    def check(source_manifest: dict, events: list[dict]) -> dict:
        return analyze(source_manifest, events, **pins)

    result = check(manifest, rows)
    require(result["observed_produced_blocks"] == MIN_BLOCKS + 1, "block count")
    require(result["missing_peer_block_pairs"] == 0, "unexpected missingness")
    require(result["observed_delay_ns_nearest_rank"]["p999"] > 0, "missing p999")
    wrong = [
        ({**manifest, "synthetic": True}, rows),
        ({**manifest, "nodes": nodes[:24]}, rows),
        ({**manifest, "candidate_sha": "insecure"}, rows),
        ({**manifest, "candidate_sha": "c" * 40}, rows),
        ({**manifest, "network_id": "OTHER_TEST_NETWORK"}, rows),
        ({**manifest, "genesis_hash": "c" * 64}, rows),
        ({**manifest, "nodes":
          [{**nodes[0], "clock_uncertainty_ns": 7_000_000}] + nodes[1:]}, rows),
        (manifest, rows[:-1] + [rows[0]]),
        (manifest, [r for r in rows if r["event"] != "mined"]),
        (manifest, rows[:100]),
    ]
    for bad_manifest, bad_rows in wrong:
        try:
            check(bad_manifest, bad_rows)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid empirical trace admitted")
    for changed_pin in (
        {"expected_sha": "c" * 40},
        {"expected_network_id": "OTHER_NETWORK"},
        {"expected_genesis_hash": "c" * 64},
        {"expected_sha": ""},
        {"expected_genesis_hash": "../" + "b" * 64},
    ):
        try:
            analyze(manifest, rows, **{**pins, **changed_pin})
        except ValueError:
            pass
        else:
            raise AssertionError("untrusted or mismatching external identity pin accepted")
    dropped = [r for r in rows if not (r["event"] == "received" and
                                      r["node_id"] == "n24")]
    incomplete = check(manifest, dropped)
    require(incomplete["missing_peer_block_pairs"] == MIN_BLOCKS + 1,
            "missing receiver observations were hidden")
    require(incomplete["p999_full_population_finite_upper_bound_proven"] is False,
            "high-tail missingness incorrectly presented as finite p99.9")
    require(incomplete["censored_full_population_quantile_bounds_ns"]
            ["p999"]["upper_ns_missing_may_be_unbounded"] is None,
            "unknown high-tail observations incorrectly imputed")
    require(incomplete["all_peer_block_receptions_observed"] is False,
            "missing peer count was lost")
    require(result["all_peer_block_receptions_observed"] is True,
            "complete case incorrectly marked censored")
    require(result["censored_full_population_quantile_bounds_ns"]
            ["p999"]["upper_ns_missing_may_be_unbounded"] ==
            result["observed_delay_ns_nearest_rank"]["p999"],
            "complete-population and observed p999 must coincide")
    # Bounds must survive regular Python and -O. Never parse unlimited data
    # first and reject only inside analyze() after memory was allocated.
    bounded = b'{"block_hash":"abc"}\\n'.replace(b"\\n", b"\n")
    parsed_rows, raw_digest, raw_size = parse_bounded_events(io.BytesIO(bounded))
    require(len(parsed_rows) == 1 and raw_size == len(bounded),
            "bounded JSONL event parsing failed")
    require(raw_digest == hashlib.sha256(bounded).hexdigest(),
            "raw trace digest was not byte-accurate")
    manifest_bytes = json.dumps(manifest).encode("utf-8")
    parsed_manifest, manifest_digest = parse_bounded_manifest(io.BytesIO(manifest_bytes))
    require(parsed_manifest == manifest and
            manifest_digest == hashlib.sha256(manifest_bytes).hexdigest(),
            "bounded manifest parser does not preserve exact input")
    malicious_lines = [
        (b" " * 33 + b"{}", {"max_line_bytes": 32}),
        (b"{}\n" * 4, {"max_rows": 3}),
        (b"{}\n" * 4, {"max_total_bytes": 11}),
        (b"{}\n\n", {}),
        (b"[]\n", {}),
        (b"null\n", {}),
        (b"{no-json}\n", {}),
    ]
    for malicious, lower_budget in malicious_lines:
        try:
            parse_bounded_events(io.BytesIO(malicious), **lower_budget)
        except ValueError:
            pass
        else:
            raise AssertionError("oversized/malformed JSONL admitted")
    try:
        parse_bounded_manifest(io.BytesIO(b"x" * (MAX_MANIFEST_BYTES + 1)))
    except ValueError:
        pass
    else:
        raise AssertionError("oversized manifest admitted")
    try:
        parse_bounded_events(io.BytesIO(b"{}\n"), max_rows=MAX_TRACE_ROWS + 1)
    except ValueError:
        pass
    else:
        raise AssertionError("unbounded configured row budget admitted")
    # Even a single unobserved extreme block-peer pair can destroy a finite
    # p99.9 bound on a modest sample; never silently assume it was fast.
    single_missing = check(manifest, rows[:-1])
    require(single_missing["missing_peer_block_pairs"] == 1,
            "single dropped reception was not counted")
    require(single_missing["censored_full_population_quantile_bounds_ns"]
            ["p999"]["upper_ns_missing_may_be_unbounded"] is not None,
            "finite p999 with one missing and 2904 pairs miscomputed")
    # At least 4 unobserved pairs out of 2904 make nearest-rank p99.9
    # mathematically unbounded in the worst case.
    four_missing = check(manifest, rows[:-4])
    require(four_missing["missing_peer_block_pairs"] == 4,
            "four missing observations were not counted")
    require(four_missing["censored_full_population_quantile_bounds_ns"]
            ["p999"]["upper_ns_missing_may_be_unbounded"] is None,
            "unbounded full-population p999 accidentally imputed")
    print("PASS: synthetic-only validator scenarios (NOT measured propagation)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--events-jsonl", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--expected-sha")
    parser.add_argument("--expected-network")
    parser.add_argument("--expected-genesis")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    require(args.manifest and args.events_jsonl and args.output and
            args.expected_sha and args.expected_network and args.expected_genesis,
            "--manifest, --events-jsonl, --output, and independently trusted "
            "--expected-sha, --expected-network, --expected-genesis are required")
    require(args.manifest.is_file() and args.events_jsonl.is_file(),
            "external measured trace files were not provided")
    with args.manifest.open("rb") as source:
        manifest, manifest_sha256 = parse_bounded_manifest(source)
    with args.events_jsonl.open("rb") as source:
        events, events_sha256, event_bytes = parse_bounded_events(source)
    result = analyze(manifest, events, expected_sha=args.expected_sha,
                     expected_network_id=args.expected_network,
                     expected_genesis_hash=args.expected_genesis)
    # Digests are reproducible byte checksums, NOT signatures, independent
    # collector authentication or proof of native header validity.
    result["manifest_raw_sha256_untrusted"] = manifest_sha256
    result["events_jsonl_raw_sha256_untrusted"] = events_sha256
    result["events_jsonl_bytes"] = event_bytes
    result["bounded_trace_reader_applied"] = True
    args.output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n",
                           encoding="utf-8")
    print("TRACE_ANALYZED_NOT_K_FREEZE: external authentication is still required")


if __name__ == "__main__":
    main()
