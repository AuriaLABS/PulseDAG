#!/usr/bin/env python3
"""Read-only Task30 per-process CPU PoW attempt accounting.

Only logs produced by the private Task30 prune/restart/rejoin miner adapter are
accepted (one <block>-attempt-<number>.log per process). The output is a lower
bound, NEVER a claim that missing logs, nonces, or backend searches were covered.
This script does not mine, validate/accept blocks or change any consensus rules.
"""
from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

ATTEMPT_NAME = re.compile(r"^(?P<label>[A-Za-z][A-Za-z0-9_-]*)-attempt-(?P<attempt>[1-9][0-9]*)\.log$")
KV = re.compile(r"([a-z_]+)=([^\s]+)")
HEX256 = re.compile(r"^[a-fA-F0-9]{64}$")
OFFLINE_LABEL = re.compile(r"^offline-([1-9][0-9]*)$")


class EvidenceError(ValueError):
    """A misleading or internally contradictory observation."""


def fields(line: str) -> dict[str, str]:
    return dict(KV.findall(line))


def natural_key(label: str) -> tuple[str, int, str]:
    match = re.match(r"^(.*?)-([0-9]+)$", label)
    return (match.group(1), int(match.group(2)), "") if match else (label, 0, label)


def nonnegative_int(value: object, field: str, *, positive: bool = False) -> int:
    if type(value) is not int or value < int(positive):
        raise EvidenceError(f"{field} must be {'positive' if positive else 'non-negative'} integer")
    return value


def decode_exhaustion(payload: dict[str, object]) -> tuple[int, dict[str, object]]:
    if payload.get("schema_version") != 1 or payload.get("event") != "nonce_search_exhausted":
        raise EvidenceError("unexpected exhaustion schema or event")
    if payload.get("reason") != "no_valid_pow_within_budget":
        raise EvidenceError("invalid exhaustion reason")
    if payload.get("backend_claimed_accepted") is not False or payload.get("canonical_accepted") is not False:
        raise EvidenceError("exhaustion must not claim accepted work")
    for field in ("backend", "template_id", "template_hash"):
        if not isinstance(payload.get(field), str) or not payload[field]:
            raise EvidenceError(f"missing {field}")
    target = payload.get("canonical_target_hex")
    if not isinstance(target, str) or not HEX256.fullmatch(target):
        raise EvidenceError("missing canonical 256-bit target")
    nonnegative_int(payload.get("block_height"), "block_height")
    nonnegative_int(payload.get("compact_target_bits"), "compact_target_bits")
    tries = nonnegative_int(payload.get("attempted_hashes"), "attempted_hashes", positive=True)
    budget = nonnegative_int(payload.get("configured_hash_budget"), "configured_hash_budget", positive=True)
    if tries > budget:
        raise EvidenceError("attempted hashes exceed configured budget")
    provenance = payload.get("nonce_domain")
    if provenance == "cpu_protocol_strided_full_budget":
        if (payload["backend"] != "cpu" or tries != budget
                or payload.get("nonce_start_inclusive") != 0
                or payload.get("nonce_end_exclusive") != budget):
            raise EvidenceError("invalid claim of complete CPU nonce-domain coverage")
    elif provenance == "backend_specific_or_partial_unattested":
        if payload.get("nonce_start_inclusive") is not None or payload.get("nonce_end_exclusive") is not None:
            raise EvidenceError("unattested backend cannot claim complete nonce coverage")
    else:
        raise EvidenceError("unknown nonce-domain provenance")
    return tries, {
        "template_id": payload["template_id"],
        "block_height": payload["block_height"],
        "target_hex": target.lower(),
        "backend": payload["backend"],
        "nonce_domain": provenance,
    }


def inspect_attempt(path: Path, label: str, attempt: int) -> dict[str, object]:
    exhaustions: list[dict[str, object]] = []
    mined: list[dict[str, str]] = []
    submits: list[dict[str, str]] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if line.startswith("nonce_search_evidence_json:"):
            try:
                payload = json.loads(line.split(":", 1)[1].strip())
            except (ValueError, TypeError) as exc:
                raise EvidenceError(f"{path.name}: malformed exhaustion JSON") from exc
            if not isinstance(payload, dict):
                raise EvidenceError(f"{path.name}: exhaustion must be an object")
            exhaustions.append(payload)
        elif line.startswith("mining: "):
            mined.append(fields(line))
        elif line.startswith("submit_result: "):
            submits.append(fields(line))
    if len(exhaustions) > 1 or len(mined) > 1 or len(submits) > 1:
        raise EvidenceError(f"{path.name}: duplicate evidence for a single process")
    if exhaustions and (mined or submits):
        raise EvidenceError(f"{path.name}: exhaustion contradicts mined/submitted work")
    row: dict[str, object] = {
        "schema_version": 1, "label": label, "attempt": attempt,
        "source_file": path.name, "hashes_observed_lower_bound": 0,
        "work_accounted": False, "outcome": "unattested", "node_accepted": False,
    }
    if exhaustions:
        tries, metadata = decode_exhaustion(exhaustions[0])
        row.update(metadata)
        row.update(
            hashes_observed_lower_bound=tries,
            work_accounted=True,
            outcome="nonce_search_exhausted",
            evidence_source="canonical_exhaustion_json",
        )
    elif mined:
        meta = mined[0]
        try:
            tries = int(meta["attempts"])
        except (KeyError, ValueError) as exc:
            raise EvidenceError(f"{path.name}: invalid mining attempts") from exc
        if tries < 1 or meta.get("accepted") != "true":
            raise EvidenceError(f"{path.name}: mining result is not a canonical solution")
        submitted = bool(submits and submits[0].get("accepted") == "true")
        row.update(
            hashes_observed_lower_bound=tries,
            work_accounted=True,
            outcome="node_submit_accepted" if submitted else "canonical_solution_not_accepted",
            node_accepted=submitted,
            evidence_source="miner_canonical_solution_log",
            template_id=meta.get("template_id"),
            block_height=int(meta["height"]) if meta.get("height", "").isdigit() else None,
            target_hex=meta.get("target_hex"),
            backend="unrecorded_in_mining_line",
            nonce_domain="not_attested_by_mining_line",
        )
    elif submits and submits[0].get("accepted") == "true":
        raise EvidenceError(f"{path.name}: submitted block without canonical mining evidence")
    return row


def summarize(directory: Path) -> tuple[list[dict[str, object]], dict[str, object]]:
    if not directory.is_dir():
        raise EvidenceError(f"missing miner log directory: {directory}")
    files: list[tuple[str, int, Path]] = []
    for path in directory.glob("*-attempt-*.log"):
        match = ATTEMPT_NAME.fullmatch(path.name)
        if not match:
            raise EvidenceError(f"unrecognized attempt log name: {path.name}")
        files.append((match.group("label"), int(match.group("attempt")), path))
    files.sort(key=lambda p: (natural_key(p[0]), p[1]))
    seen: set[tuple[str, int]] = set()
    counts: defaultdict[str, int] = defaultdict(int)
    per_block: dict[str, dict[str, object]] = {}
    rows: list[dict[str, object]] = []
    for label, attempt, path in files:
        key = (label, attempt)
        if key in seen:
            raise EvidenceError(f"duplicate attempt: {label}-{attempt}")
        seen.add(key)
        if attempt != counts[label] + 1:
            raise EvidenceError(f"non-contiguous attempt sequence for {label}: {attempt}")
        counts[label] += 1
        record = inspect_attempt(path, label, attempt)
        rows.append(record)
        group = per_block.setdefault(label, {
            "attempts": 0, "hashes_observed_lower_bound": 0,
            "unattested_attempts": 0, "node_accepted": False,
        })
        group["attempts"] += 1
        group["hashes_observed_lower_bound"] += record["hashes_observed_lower_bound"]
        group["unattested_attempts"] += int(not record["work_accounted"])
        if record["node_accepted"]:
            if group["node_accepted"]:
                raise EvidenceError(f"multiple accepted submissions for {label}")
            group["node_accepted"] = True
    offline_accepted = [
        label for label, group in per_block.items()
        if OFFLINE_LABEL.fullmatch(label) and group["node_accepted"]
    ]
    total_unattested = sum(group["unattested_attempts"] for group in per_block.values())
    summary = {
        "schema_version": 1,
        "kind": "TASK30_POW_CROSS_PROCESS_OBSERVATION_ONLY",
        "non_consensus": True,
        "source": "independent_per_process_miner_logs",
        "attempt_logs_seen": len(rows),
        "hashes_observed_lower_bound": sum(row["hashes_observed_lower_bound"] for row in rows),
        "unattested_attempts": total_unattested,
        "all_attempts_hashes_accounted": bool(rows) and total_unattested == 0,
        "offline_node_accepted_block_count": len(offline_accepted),
        "per_block": per_block,
        "cannot_prove_full_nonce_coverage_from_success_logs": True,
        "cannot_authorize_mining_or_mainnet": True,
    }
    return rows, summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--miner-dir", type=Path, required=True)
    parser.add_argument("--jsonl", type=Path, required=True)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--require-offline-accepted", type=int, default=0)
    args = parser.parse_args(argv)
    try:
        rows, summary = summarize(args.miner_dir)
    except (EvidenceError, OSError) as exc:
        parser.error(str(exc))
    args.jsonl.parent.mkdir(parents=True, exist_ok=True)
    args.summary.parent.mkdir(parents=True, exist_ok=True)
    args.jsonl.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8")
    args.summary.write_text(json.dumps(summary, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    if summary["offline_node_accepted_block_count"] < args.require_offline_accepted:
        parser.error("insufficient observed offline submitted blocks; no 64-block PASS may be inferred")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
