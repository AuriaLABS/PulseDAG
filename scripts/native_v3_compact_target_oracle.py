#!/usr/bin/env python3
"""Independent arbitrary-precision 256-bit compact-target diagnostic for dormant v3.

NOT a KHeavyHash evaluator, proof of completed PoW, blue-work definition,
selected-parent/K choice, native-v3 admission or release authorization.
"""
MAX = (1 << 256) - 1
POW_LIMIT = 0x7FFFFF << (8 * 29)
VALID = {
    0x207FFFFF: "7fffff" + "00" * 29,
    0x1F7FFFFF: "007fffff" + "00" * 28,
    0x1E7FFFFF: "00007fffff" + "00" * 27,
    0x1D00FFFF: "00000000ffff" + "00" * 26,
    0x01010000: "00" * 31 + "01",
}
INVALID = (0, 0x03000001, 0x20007FFF, 0x2100FFFF, 0x22010101)


def expand(bits):
    if type(bits) is not int or not 0 <= bits < (1 << 32):
        raise ValueError("expected unsigned compact u32")
    exponent, mantissa = bits >> 24, bits & 0x7FFFFF
    if exponent == 0:  # Legacy difficulty format; invalid for native v3.
        return MAX
    if mantissa == 0:
        return 0
    offset = 8 * (exponent - 3)
    return min(mantissa << offset, MAX) if offset >= 0 else mantissa >> -offset


def compact(target):
    if not 0 < target <= MAX:
        return 0
    size = (target.bit_length() + 7) // 8
    word = (
        target >> (8 * (size - 3))
        if size >= 3
        else target << (8 * (3 - size))
    )
    if word & 0x800000:
        size += 1
        word >>= 8
    return (size << 24) | (word & 0x7FFFFF)


def valid_v3(bits):
    target = expand(bits)
    return 1 <= target <= POW_LIMIT and compact(target) == bits


def main():
    for bits, frozen_hex in VALID.items():
        target = expand(bits)
        assert len(frozen_hex) == 64
        assert target.to_bytes(32, "big").hex() == frozen_hex, hex(bits)
        assert compact(target) == bits and valid_v3(bits)
        assert target - 1 <= target and not target + 1 <= target
    for bits in INVALID:
        assert not valid_v3(bits), hex(bits)
    target = expand(0x1F7FFFFF)
    above = target + 1
    assert (target >> 192) == (above >> 192) and above > target
    assert expand(0x207FFFFF) == POW_LIMIT
    print("PASS: independent full-256-bit compact targets and low-tail rejection")


if __name__ == "__main__":
    main()
