#!/usr/bin/env python3
"""Independent stdlib-only Keccak/cSHAKE256, Xoshiro256++ and KHeavyHash oracle.

DORMANT TEST ONLY. Not an approved genesis, production mining path, GHOSTDAG
parent/blue-work policy, network activation or authorization for mainnet.

Based on FIPS 202, NIST SP800-185 cSHAKE256 and the public Kaspa Go/Rust
kHeavyHash algorithm. This script does NOT import kaspa-pow, rusty-kaspa or
a packaged keccak implementation. It cross-checks published upstream vectors.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

MASK = (1 << 64) - 1
RATE = 136
DOMAIN = b"PulseDAG:block-header:v3:nanoseconds"
ROTATIONS = (
    (0, 36, 3, 41, 18),
    (1, 44, 10, 45, 2),
    (62, 6, 43, 15, 61),
    (28, 55, 25, 21, 56),
    (27, 20, 39, 8, 14),
)
RC = (
    0x0000000000000001, 0x0000000000008082, 0x800000000000808A,
    0x8000000080008000, 0x000000000000808B, 0x0000000080000001,
    0x8000000080008081, 0x8000000000008009, 0x000000000000008A,
    0x0000000000000088, 0x0000000080008009, 0x000000008000000A,
    0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
    0x8000000000008003, 0x8000000000008002, 0x8000000000000080,
    0x000000000000800A, 0x800000008000000A, 0x8000000080008081,
    0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
)


def rot(x: int, shift: int) -> int:
    return ((x << shift) | (x >> (64 - shift))) & MASK if shift else x


def keccak_f1600(a: list[int]) -> None:
    for constant in RC:
        columns = [a[x] ^ a[x+5] ^ a[x+10] ^ a[x+15] ^ a[x+20] for x in range(5)]
        for x in range(5):
            delta = columns[(x-1) % 5] ^ rot(columns[(x+1) % 5], 1)
            for y in range(5):
                a[x + 5*y] ^= delta
        moved = [0] * 25
        for x in range(5):
            for y in range(5):
                moved[y + 5*((2*x + 3*y) % 5)] = rot(a[x+5*y], ROTATIONS[x][y])
        for x in range(5):
            for y in range(5):
                a[x+5*y] = (moved[x+5*y] ^ ((~moved[(x+1)%5+5*y]) &
                                           moved[(x+2)%5+5*y])) & MASK
        a[0] ^= constant


def sponge(message: bytes, suffix: int, length: int = 32) -> bytes:
    state = [0] * 25
    padded = bytearray(message) + bytes([suffix])
    padded += bytes((-len(padded)) % RATE)
    padded[-1] ^= 0x80
    for offset in range(0, len(padded), RATE):
        for i, value in enumerate(padded[offset:offset+RATE]):
            state[i//8] ^= value << (8*(i%8))
        keccak_f1600(state)
    out = bytearray()
    while len(out) < length:
        for lane in state[:RATE//8]:
            out += lane.to_bytes(8, "little")
        if len(out) < length:
            keccak_f1600(state)
    return bytes(out[:length])


def left_encode(value: int) -> bytes:
    count = max(1, (value.bit_length() + 7) // 8)
    return bytes([count]) + value.to_bytes(count, "big")


def cshake256(message: bytes, custom: bytes) -> bytes:
    if not custom:
        return sponge(message, 0x1f)
    encode = lambda x: left_encode(len(x)*8) + x
    prefix = left_encode(RATE) + encode(b"") + encode(custom)
    prefix += bytes((-len(prefix)) % RATE)
    return sponge(prefix + message, 0x04)


def xoshiro256pp(seed: bytes):
    if len(seed) != 32:
        raise ValueError("seed must be 32 bytes")
    state = list(struct.unpack("<QQQQ", seed))
    while True:
        a,b,c,d = state
        word = (rot((a+d)&MASK,23) + a) & MASK
        tmp = (b << 17) & MASK
        c ^= a; d ^= b; b ^= c; a ^= d; c ^= tmp
        d = rot(d,45)
        state[:] = [a,b,c,d]
        yield word


def matrix_rank(matrix: list[list[int]]) -> int:
    numbers = [[float(x) for x in row] for row in matrix]
    selected = [False]*64
    rank = 0
    for col in range(64):
        pivot_row = next((r for r in range(64) if not selected[r]
                          and abs(numbers[r][col]) > 1e-9), None)
        if pivot_row is None:
            continue
        rank += 1
        selected[pivot_row] = True
        pivot = numbers[pivot_row][col]
        for k in range(col+1,64):
            numbers[pivot_row][k] /= pivot
        for other in range(64):
            if other != pivot_row and abs(numbers[other][col]) > 1e-9:
                factor = numbers[other][col]
                for k in range(col+1,64):
                    numbers[other][k] -= numbers[pivot_row][k] * factor
    return rank


def generate_matrix(seed: bytes):
    generator = xoshiro256pp(seed)
    for retries in range(32):
        matrix = []
        for _ in range(64):
            row = []
            for _ in range(4):
                word = next(generator)
                row.extend((word >> (4*i)) & 15 for i in range(16))
            matrix.append(row)
        if matrix_rank(matrix) == 64:
            return matrix, retries
    raise ValueError("matrix rank rejection ceiling exceeded")


def kheavy_stages(preimage: bytes, nonce: int):
    if type(nonce) is not int or not 0 <= nonce < 1<<64:
        raise ValueError("nonce out of u64 range")
    prehash = sponge(preimage,0x01)  # Keccak256, not SHA3-256
    initial = cshake256(prehash + bytes(8) + bytes(32) +
                        nonce.to_bytes(8,"little"), b"ProofOfWorkHash")
    matrix, retries = generate_matrix(prehash)
    nibbles = [v for byte in initial for v in (byte>>4, byte&15)]
    product = [(sum(a*b for a,b in zip(row,nibbles)) >> 10) & 15
               for row in matrix]
    xor = bytes(initial[i] ^ ((product[2*i]<<4) | product[2*i+1])
                for i in range(32))
    final = cshake256(xor,b"HeavyHash")
    return {
        "pre_pow_keccak256": prehash.hex(),
        "nonce_pow_hash": initial.hex(),
        "matrix_retries": retries,
        "matrix_xor": xor.hex(),
        "kheavyhash": final.hex(),
    }


def canonical_header(header: dict, chain_id: str, include_nonce: bool) -> bytes:
    fields = {"version","parents","timestamp_ns","difficulty","nonce",
              "merkle_root","state_root","blue_score","height"}
    if type(header) is not dict or set(header) != fields:
        raise ValueError("wrong header fields")
    try:
        chain = chain_id.encode("ascii")
    except (UnicodeError, AttributeError) as exc:
        raise ValueError("chain ID must be ASCII") from exc
    if not (0 < len(chain) <= 128 and all(33 <= v <= 126 for v in chain)):
        raise ValueError("noncanonical chain ID")
    parents = header["parents"]
    valid_root = lambda v: isinstance(v,str) and len(v)==64 and all(
        c in "0123456789abcdef" for c in v)
    if (type(parents) is not list or len(parents)>64
            or not all(valid_root(p) for p in parents)
            or any(a >= b for a,b in zip(parents, parents[1:]))):
        raise ValueError("invalid canonical parents")
    if not all(valid_root(header[k]) for k in ("merkle_root","state_root")):
        raise ValueError("invalid roots")
    def unsigned(v,bits):
        if type(v) is not int or not 0 <= v < 1<<bits:
            raise ValueError("unsigned integer width violation")
        return v
    if unsigned(header["version"],32) != 3:
        raise ValueError("not native-v3 header")
    if unsigned(header["timestamp_ns"],64)==0:
        raise ValueError("zero timestamp")
    height = unsigned(header["height"],64)
    if (height==0 and parents) or (height>0 and not parents):
        raise ValueError("genesis parent constraint")
    def field(v): return len(v).to_bytes(4,"little") + v
    out = bytearray(field(DOMAIN) + field(chain))
    out += struct.pack("<II",header["version"],len(parents))
    for parent in parents:
        out += field(parent.encode("ascii"))
    out += struct.pack("<QI",header["timestamp_ns"],unsigned(header["difficulty"],32))
    if include_nonce:
        out += struct.pack("<Q",unsigned(header["nonce"],64))
    out += field(header["merkle_root"].encode("ascii"))
    out += field(header["state_root"].encode("ascii"))
    out += struct.pack("<QQ",unsigned(header["blue_score"],64),height)
    if len(out)>8192:
        raise ValueError("oversized header")
    return bytes(out)


def self_test() -> None:
    if sponge(b"",0x01).hex() != (
        "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    ):
        raise AssertionError("independent empty Keccak-256 vector mismatch")
    for n in (0,2,135,136,137,272):
        if sponge(bytes(n),0x06) != hashlib.sha3_256(bytes(n)).digest():
            raise AssertionError(f"SHA3-256 boundary reference differs at n={n}")
    prehash = bytes([42])*32
    pow_input = prehash + struct.pack("<Q",5435345234) + bytes(32) + struct.pack("<Q",432432432)
    first = cshake256(pow_input,b"ProofOfWorkHash")
    if not first.hex().startswith("2fb72b63dd0dd0d82b00cd9f83d4eca0"):
        raise AssertionError("published Kaspa initial PowHash stage mismatch")
    matrix,_ = generate_matrix(prehash)
    nibs = [v for b in first for v in (b>>4,b&15)]
    mixed = [(sum(a*b for a,b in zip(row,nibs))>>10)&15 for row in matrix]
    xor = bytes(first[i]^((mixed[2*i]<<4)|mixed[2*i+1]) for i in range(32))
    if cshake256(xor,b"HeavyHash").hex() != (
        "5a5bcd6e352eb8c87c80d0f0574a45a5fcc3d5755660ac120dc9893684c19be6"
    ):
        raise AssertionError("published Kaspa final HeavyHash stage mismatch")


def verify_fixture(doc: dict) -> int:
    if doc.get("schema")!="pulsedag.dormant-v3-independent-kheavyhash-v1":
        raise ValueError("unknown golden schema")
    vectors=doc.get("vectors")
    if not isinstance(vectors,list) or len(vectors)<7:
        raise ValueError("missing adversarial KHeavyHash vectors")
    seen=set()
    for v in vectors:
        if v["name"] in seen:
            raise ValueError("duplicate vector")
        seen.add(v["name"])
        h,chain=v["header"],v["chain_id"]
        if h["height"]==0:
            raise ValueError("genesis must remain unmined and externally frozen")
        full=canonical_header(h,chain,True)
        pre=canonical_header(h,chain,False)
        # Raw canonical inputs are frozen, not just their SHA digests/lengths.
        # This comparison is explicit and cannot be skipped with python -O.
        if full.hex() != v["canonical_header_hex"]:
            raise AssertionError("canonical header bytes disagreement: "+v["name"])
        if pre.hex() != v["canonical_seed_hex"]:
            raise AssertionError("canonical nonce-free preimage bytes disagreement: "+v["name"])
        if (len(full),len(pre)) != (v["header_bytes"],v["seed_bytes"]):
            raise AssertionError("canonical header length disagreement: "+v["name"])
        if hashlib.sha256(full).hexdigest()!=v["expected_header_sha256"]:
            raise AssertionError("SHA header mismatch: "+v["name"])
        if hashlib.sha256(pre).hexdigest()!=v["expected_seed_sha256"]:
            raise AssertionError("SHA seed mismatch: "+v["name"])
        result=kheavy_stages(pre,h["nonce"])
        if result!=v["stages"]:
            raise AssertionError("independent KHeavyHash stages differ: "+v["name"])
        bits=h["difficulty"]
        exponent,mantissa=bits>>24,bits & 0x7fffff
        if exponent < 3 or exponent > 32 or mantissa==0:
            raise ValueError("oracle fixture compact target out of range")
        target=mantissa << (8*(exponent-3))
        if target.to_bytes(32,"big").hex()!=v["expected_target_256_hex"]:
            raise AssertionError("target mismatch: "+v["name"])
        accepted=int(result["kheavyhash"],16)<=target
        if accepted is not v["expected_meets_target"]:
            raise AssertionError("full-256-bit PoW verdict mismatch: "+v["name"])
    required_invalid = {"same-header-invalid-pow-nonce16", "harder-target-invalid-pow"}
    if not required_invalid.issubset(seen):
        raise AssertionError("mandatory invalid-PoW adversarial fixtures are missing")
    return len(vectors)


def fixture_byte_tamper_self_test(document: dict) -> None:
    """Reject mutated frozen raw inputs even when Python assertions are disabled.

    These cases fail at the first vector BEFORE expensive PoW evaluation; the
    untouched independent digest and full-target vectors still run separately.
    """
    for key, error_text in (
        ("canonical_header_hex", "canonical header bytes disagreement"),
        ("canonical_seed_hex", "canonical nonce-free preimage bytes disagreement"),
    ):
        malformed = {**document}
        vectors = [*document["vectors"]]
        corrupted = {**vectors[0]}
        original = corrupted[key]
        corrupted[key] = ("00" if original[:2] != "00" else "ff") + original[2:]
        vectors[0] = corrupted
        malformed["vectors"] = vectors
        try:
            verify_fixture(malformed)
        except AssertionError as error:
            if error_text not in str(error):
                raise
        else:
            raise AssertionError(f"mutated frozen fixture {key} was accepted")


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test",action="store_true")
    parser.add_argument("fixture",nargs="?",type=Path)
    args=parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: Keccak/SHA3 boundary and published independent Kaspa stages")
    if args.fixture:
        document=json.loads(args.fixture.read_text(encoding="utf-8"))
        count=verify_fixture(document)
        fixture_byte_tamper_self_test(document)
        print(f"PASS: {count} independent full KHeavyHash SHA/preimage/nonce/target vectors plus raw-byte tamper rejections")
    if not args.self_test and not args.fixture:
        parser.error("supply --self-test and/or fixture")


if __name__=="__main__":
    main()
