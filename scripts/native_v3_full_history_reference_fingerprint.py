#!/usr/bin/env python3
"""TEST-ONLY complete-genesis native-v3 header closure fingerprint oracle.

Not a checkpoint, monetary state root, selected-parent/work rule or permission
to validate pruned history. Requires every reachable native header to exist.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path

from native_v3_crosslang_ghostdag_vectors import generate
from native_v3_kheavyhash_oracle import canonical_header

CHAIN = "pulsedag-dormant-ghostdag-rust-policy-probe"
GENESIS_NS = 1_800_000_000_500_000_000
GENESIS = "8897ac04d1ace55caf6c8660f7b0fcff19179b2ea046fb80c9cd3b543c1d65ae"
DOMAIN = b"PulseDAG:dormant-v3-full-history-reference:v1"


def frame(value: bytes) -> bytes:
    return len(value).to_bytes(4, "little") + value


def full_history_digest(blocks: dict, tip: str, chain: str, ns: int, genesis: str) -> str:
    if chain != CHAIN or ns != GENESIS_NS or genesis != GENESIS:
        raise ValueError("not the independently pinned research network/genesis")
    nodes = {}
    for block in blocks.values():
        header = block["header"]
        block_hash = block["id"]
        committed_bytes = bytes.fromhex(block["header_bytes_hex"])
        if canonical_header(header, CHAIN, True) != committed_bytes:
            raise ValueError("header fields differ from committed canonical header bytes")
        if hashlib.sha256(committed_bytes).hexdigest() != block_hash:
            raise ValueError("hash differs from full canonical header bytes")
        if block_hash in nodes:
            raise ValueError("duplicate header identity")
        if header["parents"] != sorted(set(header["parents"])):
            raise ValueError("noncanonical header parents")
        nodes[block_hash] = header
    seen = set()
    pending = [tip]
    while pending:
        block_hash = pending.pop()
        if block_hash in seen:
            continue
        if block_hash not in nodes:
            raise ValueError("missing full header ancestry")
        seen.add(block_hash)
        if len(seen) > 128:
            raise ValueError("diagnostic full-history budget exceeded")
        header = nodes[block_hash]
        for parent in header["parents"]:
            if parent not in nodes:
                raise ValueError("missing side parent")
            if nodes[parent]["height"] >= header["height"]:
                raise ValueError("non-topological parent height")
            if nodes[parent]["timestamp_ns"] >= header["timestamp_ns"]:
                raise ValueError("non-topological parent timestamp")
            pending.append(parent)
    if GENESIS not in seen or nodes[GENESIS]["parents"] or nodes[GENESIS]["height"] != 0:
        raise ValueError("incomplete or invalid genesis ancestry")
    if len(seen) < 2:
        raise ValueError("non-genesis proof required")

    ordered = sorted(seen, key=lambda block_hash: (nodes[block_hash]["height"], block_hash))
    payload = bytearray(frame(DOMAIN) + frame(chain.encode("utf-8")))
    payload.extend(ns.to_bytes(8, "little"))
    payload.extend(bytes.fromhex(genesis))
    payload.extend(len(ordered).to_bytes(4, "little"))
    for block_hash in ordered:
        header = nodes[block_hash]
        payload.extend(bytes.fromhex(block_hash))
        payload.extend(header["height"].to_bytes(8, "little"))
        payload.extend(header["timestamp_ns"].to_bytes(8, "little"))
        payload.extend(len(header["parents"]).to_bytes(4, "little"))
        for parent in header["parents"]:
            payload.extend(bytes.fromhex(parent))
    return hashlib.sha256(payload).hexdigest()


def self_test(known: dict, golden: dict) -> None:
    # The existing independent Python native-v3 canonical SHA/KHeavyHash oracle
    # must recreate the frozen full-byte corpus before trusting any digest.
    if generate() != known:
        raise AssertionError("full-byte PoW corpus diverged from independent generator")
    if (golden["chain_id"], golden["genesis_ns"], golden["genesis_hash"]) != (
        CHAIN, GENESIS_NS, GENESIS
    ):
        raise AssertionError("research identity changed")
    if golden["status"] != "TEST_ONLY_NOT_CHECKPOINT_AUTHORITY":
        raise AssertionError("checkpoint authority must remain denied")
    if golden["domain"] != DOMAIN.decode("ascii"):
        raise AssertionError("domain changed")
    results = []
    for name in ("X0", "X1"):
        tip = known["blocks"][name]["id"]
        expected = golden["expected"][name]
        digest = full_history_digest(known["blocks"], tip, CHAIN, GENESIS_NS, GENESIS)
        if digest != expected:
            raise AssertionError(f"full-history fingerprint mismatch: {(name, digest, expected)}")
        results.append(digest)
        reverse = dict(reversed(list(known["blocks"].items())))
        if full_history_digest(reverse, tip, CHAIN, GENESIS_NS, GENESIS) != digest:
            raise AssertionError(f"block-map insertion order changed the {name} fingerprint")
        without_side = {key: value for key, value in known["blocks"].items() if key != "A"}
        try:
            full_history_digest(without_side, tip, CHAIN, GENESIS_NS, GENESIS)
        except ValueError:
            pass
        else:
            raise AssertionError("missing off-selected side history accepted")
    if results[0] == results[1]:
        raise AssertionError("distinct complete-DAG tips unexpectedly share a fingerprint")

    # The public digest helper must reject uncommitted JSON header metadata
    # even when the byte blob, its valid SHA-256 ID and the frozen golden
    # hashes are left unchanged. Otherwise a forged height, timestamp or
    # parent set could change the alleged complete-history fingerprint.
    for field, replacement in [
        ("height", known["blocks"]["A"]["header"]["height"] + 1),
        ("timestamp_ns", known["blocks"]["A"]["header"]["timestamp_ns"] + 1),
        ("parents", [known["blocks"]["B"]["id"]]),
        ("difficulty", known["blocks"]["A"]["header"]["difficulty"] - 1),
        ("nonce", known["blocks"]["A"]["header"]["nonce"] + 1),
        ("blue_score", known["blocks"]["A"]["header"]["blue_score"] + 1),
        ("state_root", "11" * 32),
    ]:
        forged = copy.deepcopy(known["blocks"])
        forged["A"]["header"][field] = replacement
        try:
            full_history_digest(
                forged, known["blocks"]["X0"]["id"], CHAIN, GENESIS_NS, GENESIS
            )
        except ValueError:
            pass
        else:
            raise AssertionError(f"uncommitted {field} metadata accepted")
    for chain, ns, genesis in [
        (CHAIN + "-foreign", GENESIS_NS, GENESIS),
        (CHAIN, GENESIS_NS + 1, GENESIS),
        (CHAIN, GENESIS_NS, "00" * 32),
    ]:
        try:
            full_history_digest(known["blocks"], known["blocks"]["X0"]["id"], chain, ns, genesis)
        except ValueError:
            pass
        else:
            raise AssertionError("foreign checkpoint source identity accepted")
    print("PASS: independently reproduced complete-DAG fingerprints, not checkpoint authority")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", type=Path, required=True)
    parser.add_argument("--golden", type=Path, required=True)
    args = parser.parse_args()
    known = json.loads(args.verify.read_text(encoding="utf-8"))
    golden = json.loads(args.golden.read_text(encoding="utf-8"))
    self_test(known, golden)


if __name__ == "__main__":
    main()
