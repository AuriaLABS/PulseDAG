#!/usr/bin/env python3
"""Collect a redacted and checksummed private-testnet incident evidence bundle."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import socket
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

INCIDENT_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{2,63}$")
SEVERITIES = {"SEV-1", "SEV-2", "SEV-3", "SEV-4"}
SENSITIVE_KEY = re.compile(
    r"(?:token|secret|password|private[_-]?key|seed|mnemonic|authorization|cookie|credential)",
    re.IGNORECASE,
)
DEFAULT_ENDPOINTS = (
    "/health",
    "/readiness",
    "/release",
    "/status",
    "/sync/status",
    "/sync/verify",
    "/p2p/status",
    "/p2p/peers",
    "/tx/mempool",
    "/pow/health",
    "/snapshot",
    "/maintenance/report",
    "/runtime/events?limit=200",
    "/runtime/events/summary?limit=500",
)


class EvidenceError(RuntimeError):
    """Raised when evidence cannot be collected or persisted safely."""


def utc_now() -> str:
    """Return an RFC3339 UTC timestamp without fractional seconds."""

    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


def safe_filename(endpoint: str) -> str:
    """Convert an RPC endpoint into a stable evidence filename."""

    cleaned = (
        endpoint.strip("/")
        .replace("/", "-")
        .replace("?", "-")
        .replace("&", "-")
        .replace("=", "-")
    )
    return (cleaned or "root") + ".json"


def redact(value: Any) -> Any:
    """Recursively redact values whose keys look sensitive."""

    if isinstance(value, dict):
        return {
            str(key): "<redacted>" if SENSITIVE_KEY.search(str(key)) else redact(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [redact(item) for item in value]
    return value


def fetch_json(base_url: str, endpoint: str, timeout: float) -> tuple[int, Any]:
    """Fetch one read-only endpoint and return its HTTP status and parsed JSON body."""

    url = f"{base_url.rstrip('/')}{endpoint}"
    request = urllib.request.Request(url, headers={"accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as exc:
        try:
            payload = json.load(exc)
        except (json.JSONDecodeError, TypeError):
            payload = {"error": str(exc)}
        return exc.code, payload


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest for one evidence file."""

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_output_directory(path: Path) -> Path:
    """Create an operator-owned output directory with restrictive permissions."""

    path = path.resolve()
    path.mkdir(parents=True, exist_ok=True, mode=0o750)
    metadata = path.stat()
    if metadata.st_uid != os.geteuid():
        raise EvidenceError(f"output directory is not owned by the current operator: {path}")
    if metadata.st_mode & 0o022:
        raise EvidenceError(f"output directory must not be group/world writable: {path}")
    return path


def write_json(path: Path, payload: Any) -> None:
    """Write one JSON evidence document with operator-only write access."""

    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.chmod(path, 0o640)


def collect(args: argparse.Namespace) -> dict[str, Any]:
    """Collect endpoint responses and produce a checksummed incident manifest."""

    if not INCIDENT_ID.fullmatch(args.incident_id):
        raise EvidenceError(
            "incident id must be 3-64 characters using letters, digits, dot, dash, or underscore"
        )
    if args.severity not in SEVERITIES:
        raise EvidenceError(f"unsupported severity: {args.severity}")
    if not args.operator.strip():
        raise EvidenceError("operator is required")

    output = validate_output_directory(args.out_dir / args.incident_id)
    if (output / "manifest.json").exists():
        raise EvidenceError(
            f"incident evidence bundle already exists; use a unique incident/node identifier: {output}"
        )
    responses = output / "responses"
    responses.mkdir(exist_ok=False, mode=0o750)

    started_at = utc_now()
    records: list[dict[str, Any]] = []
    failures = 0
    for endpoint in args.endpoint or DEFAULT_ENDPOINTS:
        record: dict[str, Any] = {
            "endpoint": endpoint,
            "collected_at": utc_now(),
        }
        try:
            status, payload = fetch_json(args.node_url, endpoint, args.timeout)
            record["http_status"] = status
            record["payload"] = redact(payload)
            if status < 200 or status >= 300:
                failures += 1
        except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            failures += 1
            record["http_status"] = None
            record["collection_error"] = str(exc)

        target = responses / safe_filename(endpoint)
        write_json(target, record)
        records.append(
            {
                "endpoint": endpoint,
                "file": str(target.relative_to(output)),
                "sha256": sha256_file(target),
                "http_status": record.get("http_status"),
            }
        )

    manifest = {
        "schema_version": 1,
        "incident_id": args.incident_id,
        "severity": args.severity,
        "operator": args.operator.strip(),
        "node_url": args.node_url,
        "node_host": socket.gethostname(),
        "started_at": started_at,
        "completed_at": utc_now(),
        "endpoint_count": len(records),
        "collection_failure_count": failures,
        "records": records,
        "public_testnet_ready": False,
        "thirty_day_public_testnet_clock_started": False,
    }
    write_json(output / "manifest.json", manifest)

    checksum_lines = [
        f"{sha256_file(path)}  {path.relative_to(output)}"
        for path in sorted(output.rglob("*.json"))
    ]
    checksums = output / "SHA256SUMS"
    checksums.write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")
    os.chmod(checksums, 0o640)
    return manifest


def main() -> int:
    """Parse CLI arguments and collect one incident evidence bundle."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node-url", default="http://127.0.0.1:8280")
    parser.add_argument("--incident-id", required=True)
    parser.add_argument("--severity", choices=sorted(SEVERITIES), required=True)
    parser.add_argument("--operator", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=3.0)
    parser.add_argument(
        "--endpoint",
        action="append",
        help="Override the default endpoint list; repeat as needed.",
    )
    args = parser.parse_args()

    try:
        manifest = collect(args)
    except EvidenceError as exc:
        print(json.dumps({"result": "ERROR", "error": str(exc)}, indent=2), file=sys.stderr)
        return 1

    result = "PASS" if manifest["collection_failure_count"] == 0 else "PARTIAL"
    print(json.dumps({"result": result, **manifest}, indent=2, sort_keys=True))
    return 0 if result == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
