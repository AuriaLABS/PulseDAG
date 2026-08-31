#!/usr/bin/env python3
"""Fail-closed dependency remediation gate for the active v3 development line."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]

EXPECTED_LIBP2P_FEATURES = {
    "tokio", "gossipsub", "identify", "kad", "macros", "tcp", "noise", "yamux", "ping",
}
FORBIDDEN_FEATURES = {"dns", "mdns", "quic", "upnp"}

# These historical v2.4 lock-only exceptions and the reachable lru blocker must
# not survive in the active v3 dependency graph.
FORBIDDEN_LOCKED = {
    ("ring", "0.16.20"),
    ("rustls-webpki", "0.101.7"),
    ("hickory-proto", "0.24.4"),
    ("h2", "0.3.27"),
    ("lru", "0.12.5"),
}

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

# These optional transports can remain represented in Cargo.lock by libp2p,
# but must not be compiler-reachable from PulseDAG's selected feature set.
FORBIDDEN_COMPILED_NAMES = {"libp2p-dns", "libp2p-mdns", "libp2p-quic", "libp2p-upnp"}
COMPILE_ROOTS = ("pulsedag-p2p", "pulsedagd", "pulsedag-miner")

# #803 still treats these as unresolved launch blockers. This gate records that
# they remain visible; it does not claim final v3 security readiness.
REQUIRED_VISIBLE_BLOCKERS = {
    ("atty", "0.2.14"),
    ("linkme", "0.2.10"),
}


def fail(message: str) -> None:
    raise SystemExit(f"v3 dependency security validation failed: {message}")


def run(command: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        check=False,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
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
    with tempfile.TemporaryDirectory(prefix=f"pulsedag-v3-security-{root}-") as target_dir:
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
    audit = tomllib.loads((ROOT / ".cargo" / "audit.toml").read_text(encoding="utf-8"))
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    if ignored:
        fail(f"active v3 policy must not ignore vulnerability advisories: {sorted(ignored)}")

    p2p_manifest = tomllib.loads((ROOT / "crates" / "pulsedag-p2p" / "Cargo.toml").read_text(encoding="utf-8"))
    libp2p = p2p_manifest.get("dependencies", {}).get("libp2p")
    if not isinstance(libp2p, dict):
        fail("libp2p must remain an explicit dependency table")
    version = str(libp2p.get("version", ""))
    if not (version == "0.56" or version.startswith("0.56.")):
        fail(f"v3 remediation is validated against supported libp2p 0.56.x, found {version!r}")
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

    forbidden_present = sorted(FORBIDDEN_LOCKED & locked)
    if forbidden_present:
        fail(f"forbidden vulnerable versions remain locked: {forbidden_present}")

    missing_patched = sorted(REQUIRED_PATCHED - locked)
    if missing_patched:
        fail(f"required patched package versions missing: {missing_patched}")

    stale = sorted(FORBIDDEN_OLD_REACHABLE & locked)
    if stale:
        fail(f"previous vulnerable reachable versions returned: {stale}")

    missing_blockers = sorted(REQUIRED_VISIBLE_BLOCKERS - locked)
    if missing_blockers:
        fail(
            "#803 blocker inventory drifted; review the removal before changing the gate: "
            f"{missing_blockers}"
        )

    evidence_dir = ROOT / "ci-evidence" / "dependency-v3-security"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    compiled_by_root: dict[str, set[tuple[str, str]]] = {}
    for root in COMPILE_ROOTS:
        compiled = compile_clean(root, evidence_dir, by_id)
        compiled_by_root[root] = compiled
        compiled_forbidden_versions = sorted(compiled & FORBIDDEN_LOCKED)
        if compiled_forbidden_versions:
            fail(f"{root} compiles forbidden vulnerable versions: {compiled_forbidden_versions}")
        compiled_names = {name for name, _version in compiled}
        forbidden_names = sorted(compiled_names & FORBIDDEN_COMPILED_NAMES)
        if forbidden_names:
            fail(f"{root} compiles unselected optional libp2p packages: {forbidden_names}")

    candidate_sha = os.environ.get("CANDIDATE_SHA") or os.environ.get("GITHUB_SHA", "local")
    summary = {
        "result": "PASS",
        "source_sha": candidate_sha,
        "libp2p_version": version,
        "libp2p_features": sorted(features),
        "forbidden_vulnerable_locked": False,
        "lru_0_12_5_present": ("lru", "0.12.5") in locked,
        "remaining_launch_blockers": [f"{name}@{ver}" for name, ver in sorted(REQUIRED_VISIBLE_BLOCKERS)],
        "compiled_counts": {root: len(compiled) for root, compiled in compiled_by_root.items()},
        "final_v3_launch_security_ready": False,
    }
    (evidence_dir / "validation.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (evidence_dir / "validation.txt").write_text("\n".join([
        "result=PASS",
        f"source_sha={candidate_sha}",
        f"libp2p_version={version}",
        "lru_0_12_5_present=false",
        "historical_v2_4_lock_only_vulnerable_versions_present=false",
        "remaining_launch_blockers=atty@0.2.14,linkme@0.2.10",
        "final_v3_launch_security_ready=false",
        "",
    ]), encoding="utf-8")
    print("PASS: v3 dependency remediation gate; lru 0.12.5 and historical lock-only vulnerabilities are absent")


if __name__ == "__main__":
    main()
