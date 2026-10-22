#!/usr/bin/env python3
"""Unapproved native-v3 PoW target-work ALTERNATIVES. No consensus activation.

Compare exact-integer work numerators, not blue-work or a selected-parent rule.
"""
import argparse
import json

from native_v3_compact_target_oracle import MAX, VALID, expand, valid_v3

FULL = 1 << 256
NUMERATORS = {"full_2pow256": FULL, "max_u256": MAX}


def work_candidate(bits: int, policy: str) -> int:
    if policy not in NUMERATORS:
        raise ValueError("unknown, unfrozen numerator")
    if not valid_v3(bits):
        raise ValueError("invalid compact target")
    contribution = NUMERATORS[policy] // (expand(bits) + 1)
    if not 1 <= contribution <= MAX:
        raise ArithmeticError("candidate contribution exceeds positive u256")
    return contribution


def checked_u256_sum(contributions) -> int:
    total = 0
    for contribution in contributions:
        if type(contribution) is not int or not 0 <= contribution <= MAX:
            raise ValueError("invalid cumulative contribution")
        total += contribution
        if total > MAX:
            raise OverflowError("u256 cumulative work overflow; do not saturate")
    return total


def self_test() -> None:
    frozen = {
        0x207FFFFF: 2,
        0x1F7FFFFF: 512,
        0x1E7FFFFF: 131072,
        0x1D00FFFF: 4295032833,
        0x01010000: 1 << 255,
    }
    assert set(frozen) == set(VALID)
    for bits, expected in frozen.items():
        assert expand(bits).to_bytes(32, "big").hex() == VALID[bits]
        assert work_candidate(bits, "full_2pow256") == expected
    assert work_candidate(0x01010000, "max_u256") == (1 << 255) - 1
    assert work_candidate(0x207FFFFF, "max_u256") == 2
    assert work_candidate(0x1F7FFFFF, "max_u256") == 512

    easier_to_harder = [0x207FFFFF, 0x1F7FFFFF, 0x1E7FFFFF, 0x1D00FFFF, 0x01010000]
    weighted = [work_candidate(bits, "full_2pow256") for bits in easier_to_harder]
    assert all(left < right for left, right in zip(weighted, weighted[1:]))
    assert checked_u256_sum(weighted[:3]) == 131586
    assert checked_u256_sum([1, 1, 1]) == 3

    extreme = work_candidate(0x01010000, "full_2pow256")
    try:
        checked_u256_sum([extreme, extreme])
    except OverflowError:
        pass
    else:
        raise AssertionError("missing overflow rejection")

    for bits in [0, 0x03000001, 0x20007FFF, 0x2100FFFF, 0x22010101, -1, True]:
        try:
            work_candidate(bits, "full_2pow256")
        except ValueError:
            pass
        else:
            raise AssertionError(f"accepted invalid target {bits!r}")
    for amount in [-1, MAX + 1, True]:
        try:
            checked_u256_sum([amount])
        except ValueError:
            pass
        else:
            raise AssertionError(f"accepted invalid contribution {amount!r}")
    try:
        work_candidate(0x207FFFFF, "unknown")
    except ValueError:
        pass
    else:
        raise AssertionError("unapproved numerator accepted")
    print("PASS: pre-freeze target-work alternatives and checked overflow")


def report() -> dict:
    vectors = []
    for bits in sorted(VALID, key=expand, reverse=True):
        vectors.append({
            "bits": f"0x{bits:08x}",
            "target_hex": VALID[bits],
            "unit_work_option": "1",
            "floor_2pow256_over_target_plus_1_option": str(work_candidate(bits, "full_2pow256")),
            "floor_maxu256_over_target_plus_1_option": str(work_candidate(bits, "max_u256")),
        })
    return {
        "schema": "pulsedag.dormant-v3-work-policy-alternatives-v1",
        "status": "UNAPPROVED_DIAGNOSTIC_ONLY",
        "genesis_work": "UNDECIDED",
        "blue_red_k": "UNDECIDED",
        "selected_parent": "UNDECIDED",
        "cumulative_work_width": "UNDECIDED",
        "vectors": vectors,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        print(json.dumps(report(), indent=2, sort_keys=True))
