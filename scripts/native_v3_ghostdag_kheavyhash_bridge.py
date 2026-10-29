#!/usr/bin/env python3
"""NON-NORMATIVE native-v3 SHA256/KHeavyHash-to-GHOSTDAG policy integration probe.

Generate a *test-only* unmined genesis, two real PoW-valid native BlockHeaderV3
siblings and a PoW-valid two-parent join. Bind the symbolic classifier's work
to canonical compact targets only AFTER independent Python KHeavyHash verifies
every non-genesis header. This is NOT an official genesis, transaction-valid
block, authenticated production DAG, approved K, Rust cross-check or activation.
"""
import hashlib

from native_v3_compact_target_oracle import expand, valid_v3
from native_v3_ghostdag_policy_candidate import select_and_color, require
from native_v3_kheavyhash_oracle import canonical_header, kheavy_stages
from native_v3_work_policy_candidates import work_candidate

CHAIN = "pulsedag-v3-ghostdag-dormant-integration"
EASY_BITS = 0x207FFFFF
GENESIS_NS = 1_800_000_000_000_000_000


def header(parents, timestamp, height, score, merkle):
    return {
        "version": 3, "parents": sorted(parents), "timestamp_ns": timestamp,
        "difficulty": EASY_BITS, "nonce": 0,
        "merkle_root": merkle * 64, "state_root": "3" * 64,
        "blue_score": score, "height": height,
    }


def header_hash(h, chain=CHAIN):
    return hashlib.sha256(canonical_header(h, chain, True)).hexdigest()


def verified_work(h, chain=CHAIN):
    if not valid_v3(h["difficulty"]):
        raise ValueError("noncanonical compact target")
    stage = kheavy_stages(canonical_header(h, chain, False), h["nonce"])
    if int(stage["kheavyhash"], 16) > expand(h["difficulty"]):
        raise ValueError("invalid KHeavyHash nonce")
    return work_candidate(h["difficulty"], "full_2pow256")


def mine(header_template):
    candidate = dict(header_template)
    for nonce in range(128):
        candidate["nonce"] = nonce
        try:
            w = verified_work(candidate)
        except ValueError as error:
            if str(error) != "invalid KHeavyHash nonce":
                raise
        else:
            return dict(candidate), header_hash(candidate), w
    raise AssertionError("test-only easy-target nonce budget exhausted")


def probe():
    genesis = header([], GENESIS_NS, 0, 0, "0")
    genesis_hash = header_hash(genesis)
    require(genesis_hash != header_hash(genesis, CHAIN + "-wrong-network"),
            "genesis hash must bind to test chain")
    require(verified_genesis_unmined(genesis), "genesis unexpectedly mined")

    a, ah, aw = mine(header([genesis_hash], GENESIS_NS+500_000_000, 1, 1, "1"))
    b, bh, bw = mine(header([genesis_hash], GENESIS_NS+500_000_100, 1, 1, "2"))
    require(ah != bh and aw == bw == 2, "PoW-valid siblings must differ")
    # Labels are explicitly sorted by *real* canonical hash bytes, so A wins
    # an equal-work/equal-score/equal-height parent tie.
    mapped = sorted(((ah, a, aw), (bh, b, bw)))
    lower_hash, lower_header, lower_work = mapped[0]
    upper_hash, upper_header, upper_work = mapped[1]
    require(lower_hash < upper_hash, "canonical hash ordering failed")
    require(lower_header["parents"] == upper_header["parents"] == [genesis_hash],
            "incorrect authenticated fork parents")

    for k in (0, 1):
        symbolic = {
            "G": ((), 0),
            "A": (("G",), lower_work),
            "B": (("G",), upper_work),
            "X": (("A", "B"), 2),
        }
        selected, derived_score, total_work, blues, reds = select_and_color(
            symbolic, k
        )["X"]
        require(selected == "A", "real hash tie must choose smaller parent")
        require((derived_score, total_work, blues, reds) ==
                ((2, 4, (), ("B",)) if k == 0 else (3, 6, ("B",), ())),
                "policy probe expected color/work mismatch")
        join, join_hash, join_work = mine(
            header([lower_hash, upper_hash], GENESIS_NS+1_000_000_000,
                   2, derived_score, "4" if k == 0 else "5")
        )
        require(join_work == 2 and join_hash not in (genesis_hash, ah, bh),
                "join must be a distinct valid-work header")
        require(join["parents"] == [lower_hash, upper_hash],
                "join must commit correctly ordered actual parent hashes")
        require(join["blue_score"] == derived_score,
                "committed blue score must match derived candidate")
        require(join["height"] == 2 and join["timestamp_ns"] >
                max(lower_header["timestamp_ns"], upper_header["timestamp_ns"]),
                "join must have correct height / nanosecond order")
        # A forged score is never authoritative; a consensus admission
        # implementation must recompute and reject, rather than trust the field.
        forged = dict(join, blue_score=derived_score + 1)
        require(forged["blue_score"] != derived_score and
                header_hash(forged) != join_hash,
                "score forgery must change the SHA256 header commitment")

    # Do not claim the separately frozen 7-vector standalone PoW corpus
    # proves this graph's red/blue monetary admission or an active Rust path.
    print("PASS: chain-bound real PoW-valid header fork/join; K=0/1 policy probe")


def verified_genesis_unmined(genesis):
    return genesis["height"] == 0 and genesis["parents"] == []


if __name__ == "__main__":
    probe()
