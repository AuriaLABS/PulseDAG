#!/usr/bin/env python3
"""Independent, frozen test-only native-v3 fork/rejoin vectors for Rust parity.

This generator uses the Python-stdlib KHeavyHash oracle and derives the exact
dormant-native-v3 genesis from the empty-UTXO and empty-Merkle commitment
domains. It is NOT a protocol-approved genesis, full monetary validity,
GHOSTDAG consensus implementation or permission to activate mining.
"""
import argparse
import hashlib
import json
from pathlib import Path

from native_v3_compact_target_oracle import expand, valid_v3
from native_v3_kheavyhash_oracle import canonical_header, kheavy_stages
from native_v3_work_policy_candidates import work_candidate

CHAIN = "pulsedag-dormant-ghostdag-rust-policy-probe"
GENESIS_NS = 1_800_000_000_500_000_000
BITS = 0x207FFFFF
SCHEMA = "pulsedag.dormant-v3-crosslang-fork-pow-work-v1"


def root(domain):
    return hashlib.sha256(domain).hexdigest()


def empty_utxo_root():
    domain = b"PulseDAG:utxo-state-root:v1"
    return root(len(domain).to_bytes(4, "little") + domain + bytes(8))


def header(parents, ns, height, score, merkle, state):
    return {
        "version": 3,
        "parents": sorted(parents),
        "timestamp_ns": ns,
        "difficulty": BITS,
        "nonce": 0,
        "merkle_root": merkle,
        "state_root": state,
        "blue_score": score,
        "height": height,
    }


def evaluate(h):
    full = canonical_header(h, CHAIN, True)
    pre = canonical_header(h, CHAIN, False)
    pow_hash = kheavy_stages(pre, h["nonce"])["kheavyhash"]
    target = expand(h["difficulty"])
    return {
        "id": hashlib.sha256(full).hexdigest(),
        "header_bytes_hex": full.hex(),
        "pre_pow_bytes_hex": pre.hex(),
        "pow_hash": pow_hash,
        "accepted": int(pow_hash, 16) <= target,
        "work": str(work_candidate(h["difficulty"], "full_2pow256")),
    }


def mine(h):
    assert valid_v3(h["difficulty"])
    for nonce in range(1024):
        h["nonce"] = nonce
        data = evaluate(h)
        if data["accepted"]:
            return {"header": dict(h), **data}
    raise RuntimeError("test-only easy target nonce budget exhausted")


def generate():
    merkle = root(b"PulseDAG:merkle-empty:v1")
    state = empty_utxo_root()
    genesis = header([], GENESIS_NS, 0, 0, merkle, state)
    genesis["nonce"] = 0
    g = {"header": genesis, **evaluate(genesis)}
    # Genesis is unmined by protocol. Its incidental digest is diagnostic only.
    g["work"] = "0"
    A = mine(header([g["id"]], GENESIS_NS + 500_000_000,
                    1, 1, merkle, "33" * 32))
    B = mine(header([g["id"]], GENESIS_NS + 600_000_000,
                    1, 1, "22" * 32, "33" * 32))
    parents = sorted([A["id"], B["id"]])
    X0 = mine(header(parents, GENESIS_NS + 1_000_000_000, 2, 2,
                     merkle, "33" * 32))
    X1 = mine(header(parents, GENESIS_NS + 1_000_000_000, 2, 3,
                     "44" * 32, "33" * 32))
    assert A["id"] != B["id"] and X0["id"] != X1["id"]
    for name, obj in [("A", A), ("B", B), ("X0", X0), ("X1", X1)]:
        assert obj["accepted"], name
        assert obj["work"] == "2", name
    return {
        "schema": SCHEMA,
        "status": "DORMANT_TEST_ONLY_UNAPPROVED",
        "chain_id": CHAIN,
        "genesis_ns": GENESIS_NS,
        "chosen_parent": parents[0],
        "other_parent": parents[1],
        "expected": {
            "K0": {"selected_parent": parents[0], "blue_score": 2,
                   "blue_work": "4", "merge_blues": [],
                   "merge_reds": [parents[1]]},
            "K1": {"selected_parent": parents[0], "blue_score": 3,
                   "blue_work": "6", "merge_blues": [parents[1]],
                   "merge_reds": []},
        },
        "blocks": {"G": g, "A": A, "B": B, "X0": X0, "X1": X1},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--verify", type=Path)
    opts = parser.parse_args()
    data = generate()
    rendered = json.dumps(data, sort_keys=True, separators=(",", ":"))
    if opts.verify:
        frozen = json.loads(opts.verify.read_text(encoding="utf-8"))
        if frozen != data:
            raise AssertionError("frozen independent KHeavyHash/native header fork differs")
        print("PASS: exact frozen native-v3 cross-language fork/PoW/work JSON")
    elif opts.emit:
        print("BEGIN_NATIVE_V3_FORK_JSON")
        print(rendered)
        print("END_NATIVE_V3_FORK_JSON")
    else:
        raise SystemExit("use --emit or --verify")


if __name__ == "__main__":
    main()
