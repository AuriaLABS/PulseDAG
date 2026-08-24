#!/usr/bin/env python3
"""Fail-closed Task31 validator for exact lock-only RustSec exceptions."""

from __future__ import annotations

import datetime as dt
import json
import os
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REVIEW_DEADLINE = dt.date(2026, 8, 31)

EXPECTED_IGNORES = {
    "RUSTSEC-2025-0009",
    "RUSTSEC-2026-0098",
    "RUSTSEC-2026-0099",
    "RUSTSEC-2026-0104",
    "RUSTSEC-2026-0119",
    "RUSTSEC-2026-0258",
}
EXPECTED_LIBP2P_FEATURES = {
    "tokio", "gossipsub", "identify", "kad", "macros", "tcp", "noise", "yamux", "ping",
}
FORBIDDEN_FEATURES = {"dns", "mdns", "quic", "upnp"}
VULNERABLE_LOCKED = {
    ("ring", "0.16.20"),
    ("rustls-webpki", "0.101.7"),
    ("hickory-proto", "0.24.4"),
    ("h2", "0.3.27"),
}
FORBIDDEN_COMPILED_NAMES = {"libp2p-dns", "libp2p-mdns", "libp2p-quic", "libp2p-upnp"}
COMPILE_ROOTS = ("pulsedag-p2p", "pulsedagd", "pulsedag-miner")
REQUIRED_PATCHED = {
    ("crossbeam-epoch", "0.9.20"),
    ("anyhow", "1.0.103"),
    ("event-listener", "5.4.2"),
}
FORBIDDEN_OLD_REACHABLE = {
    ("crossbeam-epoch", "0.9.18"),
    ("anyhow", "1.0.102"),
    ("event-listener", "5.4.1"),
}


def fail(message: str) -> None:
    raise SystemExit(f"lock-only RustSec validation failed: {message}")


def run(command: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command, cwd=ROOT, env=env, check=False, text=True, encoding="utf-8",
        errors="replace", stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        fail(f"command failed ({result.returncode}): {' '.join(command)}")
    return result


def cargo_metadata() -> dict[str, Any]:
    return json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1"]).stdout)


def package_index(metadata: dict[str, Any]) -> dict[str, tuple[str, str]]:
    return {
        str(package["id"]): (str(package["name"]), str(package["version"]))
        for package in metadata["packages"]
    }


def compile_clean(root: str, evidence_dir: Path, by_id: dict[str, tuple[str, str]]) -> set[tuple[str, str]]:
    with tempfile.TemporaryDirectory(prefix=f"pulsedag-lock-only-{root}-") as target_dir:
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = target_dir
        result = run([
            "cargo", "check", "--locked", "-p", root,
            "--message-format=json-render-diagnostics",
        ], env=env)

    (evidence_dir / f"{root}-compiler-messages.jsonl").write_text(result.stdout, encoding="utf-8")
    (evidence_dir / f"{root}-compiler-stderr.txt").write_text(result.stderr, encoding="utf-8")

    compiled: set[tuple[str, str]] = set()
    for line_number, line in enumerate(result.stdout.splitlines(), start=1):
        try:
            message = json.loads(line)
        except json.JSONDecodeError as exc:
            fail(f"non-JSON cargo message for {root} at line {line_number}: {exc}")
        if message.get("reason") != "compiler-artifact":
            continue
        package = by_id.get(str(message.get("package_id", "")))
        if package is not None:
            compiled.add(package)

    (evidence_dir / f"{root}-compiled-packages.txt").write_text(
        "\n".join(f"{name}@{version}" for name, version in sorted(compiled)) + "\n",
        encoding="utf-8",
    )
    return compiled


def main() -> None:
    today = dt.datetime.now(dt.timezone.utc).date()
    if today > REVIEW_DEADLINE:
        fail(f"exception expired on {REVIEW_DEADLINE.isoformat()}; remove or re-review it before continuing")

    audit = tomllib.loads((ROOT / ".cargo" / "audit.toml").read_text(encoding="utf-8"))
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    if ignored != EXPECTED_IGNORES:
        fail(f"unexpected advisory ignore set: {sorted(ignored)}")

    p2p_manifest = tomllib.loads((ROOT / "crates" / "pulsedag-p2p" / "Cargo.toml").read_text(encoding="utf-8"))
    libp2p = p2p_manifest.get("dependencies", {}).get("libp2p")
    if not isinstance(libp2p, dict):
        fail("libp2p must remain an explicit dependency table")
    version = str(libp2p.get("version", ""))
    if not (version == "0.54" or version.startswith("0.54.")):
        fail(f"exception is frozen to libp2p 0.54.x, found {version!r}")
    if libp2p.get("default-features") is not False:
        fail("libp2p default features must remain disabled")
    features = set(libp2p.get("features", []))
    if features != EXPECTED_LIBP2P_FEATURES:
        fail(f"unexpected libp2p feature set: {sorted(features)}")
    if features & FORBIDDEN_FEATURES:
        fail(f"forbidden libp2p features selected: {sorted(features & FORBIDDEN_FEATURES)}")

    metadata = cargo_metadata()
    by_id = package_index(metadata)
    locked = set(by_id.values())
    missing_vulnerable = VULNERABLE_LOCKED - locked
    if missing_vulnerable:
        fail(f"expected lock-only package versions disappeared: {sorted(missing_vulnerable)}")
    missing_patched = REQUIRED_PATCHED - locked
    if missing_patched:
        fail(f"required patched package versions missing: {sorted(missing_patched)}")
    stale_reachable = FORBIDDEN_OLD_REACHABLE & locked
    if stale_reachable:
        fail(f"previous vulnerable reachable versions returned: {sorted(stale_reachable)}")

    for profile in (
        ROOT / "configs" / "private-testnet" / "seed.env.example",
        ROOT / "configs" / "private-testnet" / "node.env.example",
        ROOT / "configs" / "single-node" / "single-node.env.example",
    ):
        if "PULSEDAG_P2P_MDNS=false" not in profile.read_text(encoding="utf-8"):
            fail(f"release profile {profile.relative_to(ROOT)} does not keep mDNS disabled")

    evidence_dir = ROOT / "ci-evidence" / "dependency-lock-only-exceptions"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    compiled_by_root: dict[str, set[tuple[str, str]]] = {}
    for root in COMPILE_ROOTS:
        compiled = compile_clean(root, evidence_dir, by_id)
        compiled_by_root[root] = compiled
        vulnerable = sorted(compiled & VULNERABLE_LOCKED)
        if vulnerable:
            fail(f"{root} compiles lock-only vulnerable versions: {vulnerable}")
        compiled_names = {name for name, _version in compiled}
        forbidden_names = sorted(compiled_names & FORBIDDEN_COMPILED_NAMES)
        if forbidden_names:
            fail(f"{root} compiles forbidden optional libp2p packages: {forbidden_names}")

    document = (ROOT / "docs" / "security" / "V2_4_0_LOCK_ONLY_RUSTSEC_EXCEPTIONS.md").read_text(encoding="utf-8")
    for required in (
        "Review deadline: `2026-08-31 UTC`", "RUSTSEC-2025-0009", "RUSTSEC-2026-0098",
        "RUSTSEC-2026-0099", "RUSTSEC-2026-0104", "RUSTSEC-2026-0119",
        "RUSTSEC-2026-0258", "public-testnet GO",
        "Windows exact-candidate security revalidation remains pending",
    ):
        if required not in document:
            fail(f"decision record is missing {required!r}")

    candidate_sha = os.environ.get("CANDIDATE_SHA") or os.environ.get("GITHUB_SHA", "local")
    summary = {
        "result": "PASS", "source_sha": candidate_sha,
        "validated_utc_date": today.isoformat(), "review_deadline": REVIEW_DEADLINE.isoformat(),
        "ignored_advisories": sorted(EXPECTED_IGNORES), "libp2p_version": version,
        "libp2p_features": sorted(features),
        "vulnerable_locked": [f"{name}@{ver}" for name, ver in sorted(VULNERABLE_LOCKED)],
        "required_patched": [f"{name}@{ver}" for name, ver in sorted(REQUIRED_PATCHED)],
        "compiled_counts": {root: len(compiled) for root, compiled in compiled_by_root.items()},
        "vulnerable_compiled": False,
    }
    (evidence_dir / "validation.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (evidence_dir / "validation.txt").write_text("\n".join([
        "result=PASS", f"source_sha={candidate_sha}", f"validated_utc_date={today.isoformat()}",
        f"review_deadline={REVIEW_DEADLINE.isoformat()}",
        f"ignored_advisories={','.join(sorted(EXPECTED_IGNORES))}",
        "lock_only_vulnerable_versions_compiled=false",
        "authorization=task31-technical-candidate-private-only", "",
    ]), encoding="utf-8")
    print("PASS: Task31 lock-only RustSec exceptions are exact, uncompiled and unexpired")


if __name__ == "__main__":
    main()
