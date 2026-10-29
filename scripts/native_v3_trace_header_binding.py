#!/usr/bin/env python3
"""TEST-ONLY native-v3 trace/header cryptographic binding (offline, fail closed).

Cross-check first-seen block IDs against canonical *complete* genesis-rooted
BlockHeaderV3 SHA256 identifiers and independent Python KHeavyHash PoW.
Does NOT authenticate actual peer observations, clock calibration, physical
hosts, transaction payloads, blue-score/K/GHOSTDAG, UTXO or mainnet readiness.
This deliberately requires complete ancestry: never trust a pruned shortcut.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
from pathlib import Path

from native_v3_compact_target_oracle import expand, valid_v3
from native_v3_kheavyhash_oracle import canonical_header, kheavy_stages
from native_v3_propagation_evidence import (
    analyze, parse_bounded_events, parse_bounded_manifest, require,
)

MAX_HEADERS = 256
MAX_HEADER_BYTES = 1024 * 1024
MAX_HEADER_LINE_BYTES = 4096


def verify_full_native_pow_ancestry(
    headers: list[dict], observed_block_ids: set[str], *,
    expected_chain_id: str, expected_genesis_hash: str,
    expected_genesis_ns: int,
) -> dict:
    """Return only verified *header* IDs; no claims about source provenance."""
    require(type(expected_chain_id) is str and
            re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", expected_chain_id) is not None,
            "independently frozen chain ID required")
    require(type(expected_genesis_hash) is str and
            re.fullmatch(r"[0-9a-f]{64}", expected_genesis_hash) is not None,
            "independently frozen genesis hash required")
    require(type(expected_genesis_ns) is int and 0 < expected_genesis_ns < 2**64,
            "independently frozen genesis timestamp required")
    require(type(headers) is list and 2 <= len(headers) <= MAX_HEADERS,
            "full genesis-rooted header corpus must fit 256-header test budget")
    require(type(observed_block_ids) is set and bool(observed_block_ids),
            "no independently observed mined block IDs provided")
    by_id: dict[str, dict] = {}
    for item in headers:
        require(type(item) is dict and set(item) == {"block_hash", "header"},
                "header corpus requires exact block_hash and header fields")
        stated = item["block_hash"]
        require(type(stated) is str and
                re.fullmatch(r"[0-9a-f]{64}", stated) is not None,
                "noncanonical stated native header ID")
        h = item["header"]
        # Independent oracle performs exact field/schema, byte, parent and
        # integer-width checks including canonical sorted unique parents.
        full = canonical_header(h, expected_chain_id, True)
        computed = hashlib.sha256(full).hexdigest()
        require(computed == stated, "header ID not bound to canonical bytes")
        require(stated not in by_id, "duplicate header in signed/unsigned corpus")
        by_id[stated] = h
    require(expected_genesis_hash in by_id,
            "complete corpus lacks independently pinned genesis")
    genesis = by_id[expected_genesis_hash]
    require(genesis["height"] == 0 and genesis["parents"] == [] and
            genesis["blue_score"] == 0 and
            genesis["timestamp_ns"] == expected_genesis_ns,
            "genesis identity, timestamp or zero-score mismatch")
    require(expected_genesis_hash not in observed_block_ids,
            "genesis cannot be externally mined")
    require(observed_block_ids.issubset(by_id.keys()),
            "one or more observed mined IDs have no independently verified header")
    # A 60-second observation window normally starts AFTER genesis: historical
    # parents may be necessary even though they were mined before capture.
    # Every supplied extra header MUST be an actual ancestor of an observed
    # mined block, never an unrelated fork or a self-asserted pruning boundary.
    ancestor_closure: set[str] = set()
    pending = list(observed_block_ids)
    while pending:
        current = pending.pop()
        if current in ancestor_closure:
            continue
        require(current in by_id, "missing complete parent ancestry")
        ancestor_closure.add(current)
        pending.extend(by_id[current]["parents"])
    require(expected_genesis_hash in ancestor_closure,
            "observed blocks are not rooted in pinned complete genesis")
    require(ancestor_closure == set(by_id),
            "unrelated header supplied outside observed-block ancestor closure")
    checked = 0
    for block, h in by_id.items():
        if block == expected_genesis_hash:
            continue
        parents = h["parents"]
        require(bool(parents), "non-genesis block has no parents")
        for parent in parents:
            require(parent in by_id, "missing referenced native ancestor")
            p = by_id[parent]
            require(p["height"] < h["height"] and
                    p["timestamp_ns"] < h["timestamp_ns"],
                    "noncausal native parent height/timestamp")
        require(h["height"] == 1 + max(by_id[p]["height"] for p in parents),
                "invalid canonical native height")
        require(valid_v3(h["difficulty"]), "invalid canonical compact target")
        # SHA256 header hash alone is NOT proof of work.
        preimage = canonical_header(h, expected_chain_id, False)
        digest = kheavy_stages(preimage, h["nonce"])["kheavyhash"]
        require(int(digest, 16) <= expand(h["difficulty"]),
                "native KHeavyHash digest fails full 256-bit compact target")
        checked += 1
    return {
        "status": "TEST_ONLY_CRYPTOGRAPHIC_HEADERS_NOT_NETWORK_EVIDENCE",
        "genesis_hash_externally_pinned": expected_genesis_hash,
        "chain_id_externally_pinned": expected_chain_id,
        "total_complete_headers": len(by_id),
        "non_genesis_full_pow_headers_checked": checked,
        "observed_mined_headers_checked": len(observed_block_ids),
        "historical_ancestor_headers_checked": checked - len(observed_block_ids),
        "no_unrelated_unobserved_header_accepted": True,
        "all_first_seen_mined_ids_match_headers": True,
        "genesis_rooted_no_pruning_allowed": True,
        "blue_score_is_untrusted_not_recomputed": True,
        "not_native_ghostdag_or_K_security": True,
        "not_authenticated_peer_events_or_miner_identity": True,
        "not_monetary_consensus_or_mainnet_go": True,
    }


def self_test() -> None:
    from native_v3_ghostdag_kheavyhash_bridge import CHAIN, EASY_BITS, header, mine
    genesis = header([], 1_800_000_000_000_000_000, 0, 0, "0")
    g_id = hashlib.sha256(canonical_header(genesis, CHAIN, True)).hexdigest()
    first, a_id, _ = mine(header([g_id], genesis["timestamp_ns"] + 500_000_000,
                                 1, 1, "1"))
    second, b_id, _ = mine(header([a_id], genesis["timestamp_ns"] + 1_000_000_000,
                                  2, 2, "2"))
    corpus = [{"block_hash": block, "header": h} for block, h in
              [(b_id, second), (g_id, genesis), (a_id, first)]]
    pins = {"expected_chain_id": CHAIN, "expected_genesis_hash": g_id,
            "expected_genesis_ns": genesis["timestamp_ns"]}
    good = verify_full_native_pow_ancestry(corpus, {a_id, b_id}, **pins)
    require(good["non_genesis_full_pow_headers_checked"] == 2,
            "PoW-authenticated ancestry cardinality wrong")
    require(good["historical_ancestor_headers_checked"] == 0,
            "both headers should be observed in complete-window case")
    # Correct case: observation begins AFTER first valid native-v3 mined
    # block, so the first block is historical but must still be authenticated.
    delayed_window = verify_full_native_pow_ancestry(
        corpus, {b_id}, **pins)
    require(delayed_window["observed_mined_headers_checked"] == 1 and
            delayed_window["historical_ancestor_headers_checked"] == 1 and
            delayed_window["non_genesis_full_pow_headers_checked"] == 2,
            "historical native header ancestry was rejected or left unauthenticated")
    # But a valid PoW header unrelated to ALL observed ancestors is NEVER
    # allowed to inflate or dilute the committed header capture.
    detached, detached_id, _ = mine(
        header([g_id], genesis["timestamp_ns"] + 500_000_005, 1, 1, "3"))
    try:
        verify_full_native_pow_ancestry(
            corpus + [{"block_hash": detached_id, "header": detached}],
            {b_id}, **pins)
    except ValueError as error:
        require("unrelated header" in str(error),
                "unexpected failure mode for detached valid-PoW header")
    else:
        raise AssertionError("unrelated actual-PoW header accepted")
    # A nonce can have a perfectly consistent SHA256 native header ID yet
    # INVALID KHeavyHash PoW. Ensure the verifier checks the actual digest,
    # not just structural identity or compact-target canonicality.
    invalid_tip = None
    invalid_tip_id = None
    for nonce in range(256):
        bad = {**second, "nonce": nonce}
        preimage = canonical_header(bad, CHAIN, False)
        if int(kheavy_stages(preimage, nonce)["kheavyhash"], 16) > expand(EASY_BITS):
            invalid_tip = bad
            invalid_tip_id = hashlib.sha256(
                canonical_header(bad, CHAIN, True)).hexdigest()
            break
    require(invalid_tip is not None and invalid_tip_id != b_id,
            "failed to construct a genuinely invalid full-PoW header")
    invalid_pow_corpus = [
        {"block_hash": invalid_tip_id, "header": invalid_tip},
        corpus[1], corpus[2],
    ]
    try:
        verify_full_native_pow_ancestry(
            invalid_pow_corpus, {a_id, invalid_tip_id}, **pins)
    except ValueError as error:
        require("KHeavyHash digest fails" in str(error),
                "invalid PoW must fail for actual full-target digest, not metadata")
    else:
        raise AssertionError("forged SHA-valid but PoW-invalid tip accepted")
    malformed = [
        ([corpus[0], corpus[1]], {a_id, b_id}, pins),
        (corpus + [corpus[0]], {a_id, b_id}, pins),
        (corpus, {a_id}, pins),
        (corpus, {a_id, b_id, "f" * 64}, pins),
        (corpus, {a_id, b_id}, {**pins, "expected_chain_id": "wrong.chain"}),
        (corpus, {a_id, b_id}, {**pins, "expected_genesis_ns": pins["expected_genesis_ns"] + 1}),
        ([{**corpus[0], "block_hash": "f" * 64}, *corpus[1:]],
         {a_id, b_id}, pins),
        ([{"block_hash": b_id, "header": {**second, "difficulty": 0}},
          *corpus[1:]], {a_id, b_id}, pins),
        (corpus, {a_id, b_id}, {**pins, "expected_genesis_hash": "f" * 64}),
    ]
    for records, seen, context in malformed:
        try:
            verify_full_native_pow_ancestry(records, seen, **context)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid native PoW/chain/header claim accepted")
    # Exact same bounded parser and field-duplicate/NaN rejection for headers.
    fixture = b"".join((json.dumps(record, separators=(",", ":")) + "\n").encode()
                       for record in corpus)
    decoded, raw_sha, nbytes = parse_bounded_events(
        io.BytesIO(fixture), max_rows=MAX_HEADERS,
        max_total_bytes=MAX_HEADER_BYTES, max_line_bytes=MAX_HEADER_LINE_BYTES)
    require(decoded == corpus and nbytes == len(fixture) and
            raw_sha == hashlib.sha256(fixture).hexdigest(),
            "header corpus lost canonical identity during bounded parsing")
    require(verify_full_native_pow_ancestry(decoded, {a_id, b_id}, **pins) == good,
            "header JSONL decoding changes authentication result")
    print("PASS: 2 genuine Python KHeavyHash observed/historical headers, "
          "pinned genesis, detached-PoW rejection and strict negative fixtures "
          "(TEST ONLY, not network evidence)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--events-jsonl", type=Path)
    parser.add_argument("--headers-jsonl", type=Path)
    parser.add_argument("--expected-sha")
    parser.add_argument("--expected-network")
    parser.add_argument("--expected-chain-id")
    parser.add_argument("--expected-genesis")
    parser.add_argument("--expected-genesis-ns", type=int)
    parser.add_argument("--output", type=Path)
    a = parser.parse_args()
    if a.self_test:
        self_test()
        return
    require(all((a.manifest, a.events_jsonl, a.headers_jsonl, a.expected_sha,
                 a.expected_network, a.expected_chain_id, a.expected_genesis,
                 a.expected_genesis_ns, a.output)),
            "all trace files and independent exact-SHA/network/chain/genesis "
            "hash/genesis timestamp pins are mandatory")
    with a.manifest.open("rb") as fd:
        manifest, manifest_sha = parse_bounded_manifest(fd)
    with a.events_jsonl.open("rb") as fd:
        events, event_sha, _ = parse_bounded_events(fd)
    with a.headers_jsonl.open("rb") as fd:
        headers, header_sha, _ = parse_bounded_events(
            fd, max_rows=MAX_HEADERS, max_total_bytes=MAX_HEADER_BYTES,
            max_line_bytes=MAX_HEADER_LINE_BYTES)
    trace = analyze(manifest, events, expected_sha=a.expected_sha,
                    expected_network_id=a.expected_network,
                    expected_genesis_hash=a.expected_genesis)
    seen = {r["block_hash"] for r in events if r["event"] == "mined"}
    verified = verify_full_native_pow_ancestry(
        headers, seen, expected_chain_id=a.expected_chain_id,
        expected_genesis_hash=a.expected_genesis,
        expected_genesis_ns=a.expected_genesis_ns)
    result = {
        "trace_statistics_untrusted": trace,
        "header_cryptographic_verification": verified,
        "raw_input_sha256_not_signatures": {
            "manifest": manifest_sha, "events": event_sha, "headers": header_sha
        },
        "external_physical_network_authentication_proven": False,
        "numeric_GHOSTDAG_K_or_mining_authorized": False,
    }
    a.output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n",
                        encoding="utf-8")
    print("TEST_ONLY full-PoW header/trace IDs bound; "
          "peer observations remain unauthenticated")


if __name__ == "__main__":
    main()
