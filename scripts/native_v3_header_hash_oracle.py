#!/usr/bin/env python3
"""Standalone SHA-256 reference for dormant BlockHeaderV3 canonical bytes.

This independently implements the *header serialization* commitment only.
It neither evaluates KHeavyHash nor validates compact PoW targets, work,
selected-parent, frozen-genesis network identity or monetary consensus.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

DOMAIN = b"PulseDAG:block-header:v3:nanoseconds"
FIELDS = {
    "version", "parents", "timestamp_ns", "difficulty", "nonce",
    "merkle_root", "state_root", "blue_score", "height",
}
LIMIT_PARENTS = 64
LIMIT_CHAIN_BYTES = 128
LIMIT_ENCODED_BYTES = 8192


class InvalidHeader(ValueError):
    """Invalid dormant-v3 header shape/serialization input."""


def unsigned(value: object, bits: int) -> int:
    if type(value) is not int or not 0 <= value < (1 << bits):
        raise InvalidHeader(f"expected u{bits} integer")
    return value


def hex_root(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(c in "0123456789abcdef" for c in value)
    )


def len_prefixed(data: bytes) -> bytes:
    if len(data) >= (1 << 32):
        raise InvalidHeader("length exceeds u32")
    return struct.pack("<I", len(data)) + data


def canonical_material(header: dict, chain_id: str, include_nonce: bool) -> bytes:
    if not isinstance(header, dict) or set(header) != FIELDS:
        raise InvalidHeader("wrong header field set")
    if not isinstance(chain_id, str):
        raise InvalidHeader("chain ID must be text")
    try:
        chain = chain_id.encode("ascii")
    except UnicodeEncodeError as exc:
        raise InvalidHeader("chain ID must be ASCII") from exc
    if not 0 < len(chain) <= LIMIT_CHAIN_BYTES or any(
        not 33 <= c <= 126 for c in chain
    ):
        raise InvalidHeader("invalid printable chain ID")
    if unsigned(header["version"], 32) != 3:
        raise InvalidHeader("not BlockHeaderV3")
    if unsigned(header["timestamp_ns"], 64) == 0:
        raise InvalidHeader("zero nanosecond timestamp")
    parents = header["parents"]
    if not isinstance(parents, list) or len(parents) > LIMIT_PARENTS:
        raise InvalidHeader("too many / invalid parents")
    if not all(hex_root(parent) for parent in parents):
        raise InvalidHeader("noncanonical parent")
    if any(a >= b for a, b in zip(parents, parents[1:])):
        raise InvalidHeader("nonascending or duplicate parents")
    height = unsigned(header["height"], 64)
    if height == 0 and parents:
        raise InvalidHeader("height-zero block has parents")
    if height > 0 and not parents:
        raise InvalidHeader("non-genesis block has no parents")
    for key in ("merkle_root", "state_root"):
        if not hex_root(header[key]):
            raise InvalidHeader(f"noncanonical {key}")

    material = bytearray()
    material += len_prefixed(DOMAIN)
    material += len_prefixed(chain)
    material += struct.pack("<I", unsigned(header["version"], 32))
    material += struct.pack("<I", len(parents))
    for parent in parents:
        material += len_prefixed(parent.encode("ascii"))
    material += struct.pack("<Q", unsigned(header["timestamp_ns"], 64))
    material += struct.pack("<I", unsigned(header["difficulty"], 32))
    if include_nonce:
        material += struct.pack("<Q", unsigned(header["nonce"], 64))
    else:
        # Checked even when intentionally not committed in the PoW seed.
        unsigned(header["nonce"], 64)
    material += len_prefixed(header["merkle_root"].encode("ascii"))
    material += len_prefixed(header["state_root"].encode("ascii"))
    material += struct.pack("<Q", unsigned(header["blue_score"], 64))
    material += struct.pack("<Q", height)
    if len(material) > LIMIT_ENCODED_BYTES:
        raise InvalidHeader("header canonical material above size limit")
    return bytes(material)


def vectors_from_document(document: dict) -> int:
    if document.get("schema") != "pulsedag.dormant-v3-header-sha-v1":
        raise InvalidHeader("unknown vector schema")
    vectors = document.get("vectors")
    if not isinstance(vectors, list) or len(vectors) < 5:
        raise InvalidHeader("insufficient independent header vectors")
    names = set()
    seen = {}
    for vector in vectors:
        name = vector["name"]
        if name in names:
            raise InvalidHeader("duplicate vector name")
        names.add(name)
        chain, header = vector["chain_id"], vector["header"]
        full = canonical_material(header, chain, True)
        preimage = canonical_material(header, chain, False)
        actual = (
            hashlib.sha256(full).hexdigest(),
            hashlib.sha256(preimage).hexdigest(),
            len(full),
            len(preimage),
        )
        expected = (
            vector["expected_header_sha256"],
            vector["expected_pow_preimage_sha256"],
            vector["expected_header_bytes"],
            vector["expected_pow_preimage_bytes"],
        )
        if actual != expected:
            raise AssertionError(f"{name}: header/preimage golden mismatch")
        seen[name] = actual
    base = seen["single-parent-base"]
    nonce = seen["single-parent-nonce-changed"]
    nano = seen["single-parent-one-nanosecond-later"]
    chain = seen["single-parent-other-chain"]
    if base[0] == nonce[0] or base[1] != nonce[1]:
        raise AssertionError("nonce must change header hash, NOT preimage")
    if base[:2] == nano[:2] or base[:2] == chain[:2]:
        raise AssertionError("timestamp / chain domain must affect both hashes")
    for index in range(2):
        if base[index] in (nano[index], chain[index]):
            raise AssertionError("insufficient nanosecond / chain separation")
    return len(vectors)


def self_test() -> None:
    header = {
        "version": 3,
        "parents": ["11" * 32],
        "timestamp_ns": 1_800_000_000_000_000_123,
        "difficulty": 0x207FFFFF,
        "nonce": 14,
        "merkle_root": "22" * 32,
        "state_root": "33" * 32,
        "blue_score": 12,
        "height": 5,
    }

    def rejects(value: dict, chain: str = "v3") -> None:
        try:
            canonical_material(value, chain, True)
        except InvalidHeader:
            return
        raise AssertionError("expected malformed header rejection")

    rejects({**header, "version": 2})
    rejects({**header, "timestamp_ns": 0})
    rejects({**header, "timestamp_ns": 1 << 64})
    rejects({**header, "nonce": -1})
    rejects({**header, "parents": []})
    rejects({**header, "parents": ["44" * 32, "11" * 32]})
    rejects({**header, "parents": ["11" * 32] * 2})
    rejects({**header, "parents": ["11" * 32] * 65})
    rejects({**header, "merkle_root": "AA" * 32})
    rejects({**header, "blue_score": True})
    rejects({**header}, chain="v3 not printable")
    rejects({**header}, chain="é")
    rejects({**header}, chain="x" * 129)
    rejects({**header, "height": 0})
    rejects({**header, "extra": 1})
    assert canonical_material(header, "v3", True) != canonical_material(
        header, "v3", False
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("fixture", nargs="?", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: independent canonical header SHA self-tests")
    if args.fixture:
        count = vectors_from_document(
            json.loads(args.fixture.read_text(encoding="utf-8"))
        )
        print(f"PASS: {count} header SHA + nonce-free seed vectors")
    if not args.self_test and not args.fixture:
        parser.error("specify --self-test and/or a vector fixture path")


if __name__ == "__main__":
    main()
