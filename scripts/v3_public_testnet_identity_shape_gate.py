#!/usr/bin/env python3
"""Offline shape check for a PROPOSED native-v3 public-testnet identity.

This is not genesis authentication, artifact attestation, consensus validation,
monetary-v3 activation, GO authorization, or a deployable config renderer.
It NEVER contacts nodes, writes configuration, toggles env flags or grants GO.
"""
from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3.public-testnet-identity-draft.v1"
POLICY_SHA256 = "134009249c301682df78d0c950fc1a70604eeddb9396361e9594e6fac121680b"
CADENCE_SHA256 = "6c571a95c57b7987e6a65d744d2abc9ed83c2a064ddb53b42e50cf1d6d03277f"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA = re.compile(r"^[0-9a-f]{40}$")
TESTNET_CHAIN = re.compile(r"^pulsedag-public-testnet-v3(?:-[a-z0-9]+)*$")
TESTNET_DOMAIN = re.compile(r"^pulsedag:v3:public-testnet:[a-z0-9_-]{4,64}$")
MAINNET_CHAIN = re.compile(r"^pulsedag-mainnet-v3(?:-[a-z0-9]+)*$")
MAINNET_DOMAIN = re.compile(r"^pulsedag:v3:mainnet:[a-z0-9_-]{4,64}$")
# These are lexical checks only. Real libp2p multihash/CID decoding,
# public-key ownership and a signed chain-bound bootnode inventory are separate GO gates.
BASE58BTC_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
BASE58BTC_PEER = re.compile(r"^[1-9A-HJ-NP-Za-km-z]{16,128}$")
CIDV1_BASE32_PEER = re.compile(r"^baf[a-z2-7]{13,125}$")
PLACEHOLDERS = ("TBD", "PLACEHOLDER", "FREEZE_REQUIRED", "REPLACE_ME", "EXAMPLE")
MAX_MANIFEST_BYTES = 64 * 1024
REQUIRED = frozenset({
    "schema", "status", "network_profile", "chain_id", "genesis_hash",
    "genesis_timestamp_ns", "signing_domain", "consensus_candidate",
    "source_git_sha", "node_artifact_sha256", "miner_artifact_sha256",
    "config_sha256", "monetary_policy_sha256", "cadence_sha256",
    "bootnode_peer_ids", "monetary_runtime_enabled", "contracts_enabled",
    "admin_enabled", "public_testnet_ready",
    "thirty_day_clock_started", "allow_legacy_fallback",
    "future_mainnet_identity",
})


class CandidateError(ValueError):
    """The candidate manifest cannot be used even as an offline identity draft."""


def reject(reason: str) -> None:
    raise CandidateError(reason)


def _exact_keys(value: object, keys: frozenset[str], label: str) -> dict:
    if type(value) is not dict:
        reject(f"{label}: expected JSON object")
    provided = set(value)
    if provided != keys:
        reject(f"{label}: missing={sorted(keys - provided)} extra={sorted(provided - keys)}")
    return value


def _text(value: object, label: str) -> str:
    if type(value) is not str or not value or value != value.strip():
        reject(f"{label}: expected nonempty trimmed string")
    if any(token.lower() in value.lower() for token in PLACEHOLDERS):
        reject(f"{label}: unresolved placeholder")
    if any(ord(c) < 33 or ord(c) > 126 for c in value):
        reject(f"{label}: invalid or non-ASCII character")
    return value


def _hex(value: object, label: str, git: bool = False) -> str:
    value = _text(value, label)
    pattern = GIT_SHA if git else SHA256
    if not pattern.fullmatch(value) or value == "0" * len(value):
        reject(f"{label}: expected nonzero canonical lowercase hex digest")
    return value


def _false(value: object, label: str) -> None:
    # bool alone; JSON zero, empty strings and null do not satisfy this gate.
    if value is not False:
        reject(f"{label}: must be JSON false")


def _no_duplicates(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            reject(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def parse_manifest(raw: bytes) -> object:
    if not raw or len(raw) > MAX_MANIFEST_BYTES:
        reject("manifest missing or exceeds 64KiB")
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_no_duplicates)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        reject(f"invalid UTF-8 JSON: {exc}")


def _uvarint(raw: bytes, offset: int, label: str) -> tuple[int, int]:
    """Decode one bounded, minimally encoded unsigned protobuf/CID varint."""
    value = 0
    start = offset
    for shift in range(0, 70, 7):
        if offset >= len(raw):
            reject(f"{label}: truncated varint")
        byte = raw[offset]
        offset += 1
        if shift == 63 and byte > 1:
            reject(f"{label}: varint overflow")
        value |= (byte & 0x7f) << shift
        if byte < 0x80:
            if offset - start > 1 and byte == 0:
                reject(f"{label}: noncanonical varint")
            return value, offset
    reject(f"{label}: varint too long")


def _decode_bootnode_peer_id(peer: str) -> bytes:
    """Return canonical binary MULTIHASH identity, not a public-key proof.

    Only draft-level identity-multihash Ed25519 and sha2-256 are supported.
    CIDv1 must be version 1 / libp2p-key codec; no generic CID masquerade.
    """
    if peer.startswith("baf"):
        if not CIDV1_BASE32_PEER.fullmatch(peer):
            reject("bootnode CIDv1: invalid lowercase base32 alphabet or length")
        encoded = peer[1:].upper()
        try:
            binary = base64.b32decode(encoded + "=" * (-len(encoded) % 8))
        except binascii.Error:
            reject("bootnode CIDv1: invalid base32 bytes")
        if base64.b32encode(binary).decode("ascii").rstrip("=").lower() != peer[1:]:
            reject("bootnode CIDv1: noncanonical trailing base32 bits")
        version, offset = _uvarint(binary, 0, "bootnode CIDv1 version")
        codec, offset = _uvarint(binary, offset, "bootnode CIDv1 codec")
        if version != 1 or codec != 0x72:
            reject("bootnode CIDv1: require v1 libp2p-key codec")
        multihash = binary[offset:]
    else:
        if not BASE58BTC_PEER.fullmatch(peer):
            reject("bootnode Base58btc: invalid alphabet or length")
        number = 0
        for character in peer:
            number = number * 58 + BASE58BTC_ALPHABET.index(character)
        leading_zeroes = len(peer) - len(peer.lstrip("1"))
        decoded = number.to_bytes((number.bit_length() + 7) // 8, "big")
        multihash = b"\x00" * leading_zeroes + decoded
        if not multihash or len(multihash) > 128:
            reject("bootnode Base58btc: invalid decoded length")

    code, offset = _uvarint(multihash, 0, "bootnode multihash code")
    length, offset = _uvarint(multihash, offset, "bootnode multihash length")
    payload = multihash[offset:]
    if length != len(payload):
        reject("bootnode multihash: truncated or appended payload")
    if code == 0:
        if length != 36 or payload[:4] != bytes((8, 1, 18, 32)):
            reject("bootnode identity: expected Ed25519 public-key protobuf framing")
    elif code == 0x12:
        if length != 32:
            reject("bootnode sha2-256: expected exactly 32 digest bytes")
    else:
        reject("bootnode multihash: unapproved algorithm")

    # Does NOT establish that a provider controls this key or that the node exists.
    return multihash


def check_candidate(candidate: object) -> dict:
    m = _exact_keys(candidate, REQUIRED, "candidate")
    if _text(m["schema"], "schema") != SCHEMA:
        reject("unsupported schema")
    if _text(m["status"], "status") != "UNAPPROVED_DRAFT":
        reject("draft review must not claim a frozen or GO status")
    if _text(m["network_profile"], "network_profile") != "public-testnet-v3":
        reject("only the proposed v3 public testnet profile is supported")
    if _text(m["consensus_candidate"], "consensus_candidate") != "native-v3-unapproved":
        reject("cannot use legacy/ghostdag_v1 or claim live native-v3 consensus")
    chain_id = _text(m["chain_id"], "chain_id")
    if not TESTNET_CHAIN.fullmatch(chain_id):
        reject("chain_id is not in the v3 public testnet namespace")
    domain = _text(m["signing_domain"], "signing_domain")
    if not TESTNET_DOMAIN.fullmatch(domain) or domain == chain_id:
        reject("signing domain is not a distinct testnet-only namespace")

    genesis = _hex(m["genesis_hash"], "genesis_hash")
    if (type(m["genesis_timestamp_ns"]) is not int
            or not 0 < m["genesis_timestamp_ns"] < (1 << 64)):
        reject("genesis_timestamp_ns must be a positive u64 integer")

    _hex(m["source_git_sha"], "source_git_sha", git=True)
    node = _hex(m["node_artifact_sha256"], "node_artifact_sha256")
    miner = _hex(m["miner_artifact_sha256"], "miner_artifact_sha256")
    _hex(m["config_sha256"], "config_sha256")
    if node == miner:
        reject("node and miner cannot have the same artifact digest")
    if _hex(m["monetary_policy_sha256"], "monetary_policy_sha256") != POLICY_SHA256:
        reject("monetary policy fingerprint does not match the current candidate")
    if _hex(m["cadence_sha256"], "cadence_sha256") != CADENCE_SHA256:
        reject("cadence fingerprint does not match the current candidate")

    bootnodes = m["bootnode_peer_ids"]
    if type(bootnodes) is not list or not 2 <= len(bootnodes) <= 16:
        reject("require 2-16 distinct proposed public-testnet bootnode identities")
    peers = [_text(peer, "bootnode_peer_id") for peer in bootnodes]
    decoded_peer_ids = [_decode_bootnode_peer_id(peer) for peer in peers]
    if len(set(decoded_peer_ids)) != len(decoded_peer_ids):
        reject("duplicate bootnode multihash across Base58btc/CIDv1 encodings")

    for field in (
        "monetary_runtime_enabled", "contracts_enabled", "admin_enabled",
        "public_testnet_ready", "thirty_day_clock_started", "allow_legacy_fallback",
    ):
        _false(m[field], field)

    # Mainnet need not have a frozen identity until AFTER public testnet;
    # if one is supplied, collision is still prohibited immediately.
    other = m["future_mainnet_identity"]
    if other is not None:
        v = _exact_keys(other, frozenset({"chain_id", "genesis_hash", "signing_domain"}), "mainnet")
        other_chain = _text(v["chain_id"], "mainnet.chain_id")
        other_genesis = _hex(v["genesis_hash"], "mainnet.genesis_hash")
        other_domain = _text(v["signing_domain"], "mainnet.signing_domain")
        if not MAINNET_CHAIN.fullmatch(other_chain):
            reject("future mainnet chain_id must belong to the mainnet-v3 namespace")
        if not MAINNET_DOMAIN.fullmatch(other_domain):
            reject("future mainnet signing domain must be mainnet-v3 scoped")
        if other_chain == chain_id or other_genesis == genesis or other_domain == domain:
            reject("public testnet identity overlaps future mainnet identity")

    return {
        "schema_shape": "PASS_ONLY",
        "network": "public-testnet-v3",
        "source_and_artifacts_authenticated": False,
        "genesis_and_consensus_verified": False,
        "future_mainnet_identity_shape_checked": other is not None,
        "mainnet_cross_identity_frozen": False,  # A draft never authenticates or freezes mainnet.
        "public_testnet_launch_authorized": False,
        "mainnet_launch_authorized": False,
        "monetary_runtime_enabled": False,
        "contracts_enabled": False,
        "explanation": "UNAPPROVED offline JSON shape only; exact artifact/genesis/consensus checks, security, operators and explicit #781 GO still required",
    }


# Local byte-consistency evidence ONLY. This is never a release attestation,
# source-authenticity proof, bootnode proof, executable validation or launch GO.
MAX_LOCAL_BINARY_BYTES = 2 * 1024 * 1024 * 1024
MAX_LOCAL_CONFIG_BYTES = 1024 * 1024


def _hash_local_regular_file(path: Path, label: str, max_bytes: int) -> tuple[str, tuple[int, int]]:
    """Reject links, special files and mutable/oversized inputs before reporting a digest."""
    if path.is_symlink():
        reject(f"{label}: symbolic links are not accepted as frozen artifacts")
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    except OSError as exc:
        reject(f"{label}: cannot open local artifact: {exc}")
    try:
        with os.fdopen(fd, "rb") as source:
            before = os.fstat(source.fileno())
            if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= max_bytes:
                reject(f"{label}: require nonempty, bounded, regular file")
            digest = hashlib.sha256()
            total = 0
            while chunk := source.read(1024 * 1024):
                total += len(chunk)
                if total > max_bytes:
                    reject(f"{label}: size limit exceeded while hashing")
                digest.update(chunk)
            after = os.fstat(source.fileno())
            if (before.st_size, before.st_mtime_ns, before.st_dev, before.st_ino) != (
                total, after.st_mtime_ns, after.st_dev, after.st_ino
            ):
                reject(f"{label}: local file changed during hashing")
            return digest.hexdigest(), (before.st_dev, before.st_ino)
    except OSError as exc:
        reject(f"{label}: cannot hash local artifact: {exc}")


def check_local_artifact_bytes(
    candidate: object,
    node_file: Path,
    miner_file: Path,
    config_file: Path,
    local_checkout_sha: str,
) -> dict:
    """Compare a validated UNAPPROVED draft to locally supplied bytes only.

    An attacker can forge BOTH a manifest and matching bytes. Even success
    cannot authenticate builds, attest the source, verify genesis, or grant GO.
    """
    verdict = check_candidate(candidate)
    draft = candidate
    sha = _hex(local_checkout_sha, "local_checkout_sha", git=True)
    if sha != draft["source_git_sha"]:
        reject("source_git_sha does not match the local checkout HEAD")
    artifact_files = (
        ("node", node_file, "node_artifact_sha256", MAX_LOCAL_BINARY_BYTES),
        ("miner", miner_file, "miner_artifact_sha256", MAX_LOCAL_BINARY_BYTES),
        ("config", config_file, "config_sha256", MAX_LOCAL_CONFIG_BYTES),
    )
    identities = set()
    for label, path, digest_key, maximum in artifact_files:
        observed, identity = _hash_local_regular_file(path, label, maximum)
        if identity in identities:
            reject(f"{label}: same local inode reused for multiple distinct artifacts")
        identities.add(identity)
        if observed != draft[digest_key]:
            reject(f"{label}: local SHA256 bytes disagree with UNAPPROVED draft")
    return {
        **verdict,
        "local_artifact_byte_consistency": "PASS_ONLY",
        "local_checkout_head_matches_draft": True,
        "source_provenance_attested": False,
        "builder_and_artifacts_authenticated": False,
        "genesis_and_consensus_verified": False,
        "public_testnet_launch_authorized": False,
        "mainnet_launch_authorized": False,
        "monetary_runtime_enabled": False,
        "contracts_enabled": False,
    }


def _clean_local_checkout_head(repository: Path) -> str:
    """Read a local Git HEAD only; tracked dirty files fail closed."""
    if not repository.is_dir() or repository.is_symlink():
        reject("local source path must be a real directory")
    try:
        head = subprocess.run(
            ["git", "-C", str(repository), "rev-parse", "--verify", "HEAD"],
            capture_output=True, text=True, check=True, timeout=10,
        ).stdout.strip()
        status = subprocess.run(
            ["git", "-C", str(repository), "status", "--porcelain", "--untracked-files=no"],
            capture_output=True, text=True, check=True, timeout=10,
        ).stdout
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        reject(f"local source Git evidence unavailable: {exc}")
    if status.strip():
        reject("local checkout has modified tracked files")
    return _hex(head, "local_checkout_head", git=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_json", type=Path, help="offline candidate manifest; never a live env file")
    parser.add_argument("--node-file", type=Path, help="local node bytes to hash (not execute)")
    parser.add_argument("--miner-file", type=Path, help="local miner bytes to hash (not execute)")
    parser.add_argument("--config-file", type=Path, help="local config bytes to hash (not load)")
    parser.add_argument("--source-repo", type=Path, help="clean local source Git checkout")
    args = parser.parse_args()
    try:
        raw = args.candidate_json.read_bytes()
        candidate = parse_manifest(raw)
        requested = (args.node_file, args.miner_file, args.config_file, args.source_repo)
        if any(value is not None for value in requested):
            if not all(value is not None for value in requested):
                reject("local consistency requires all four --node/--miner/--config/--source arguments")
            result = check_local_artifact_bytes(
                candidate, args.node_file, args.miner_file, args.config_file,
                _clean_local_checkout_head(args.source_repo),
            )
        else:
            result = check_candidate(candidate)
    except (CandidateError, OSError) as exc:
        print(f"FAIL (NO-GO): {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
