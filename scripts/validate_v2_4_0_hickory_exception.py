#!/usr/bin/env python3
"""Fail-closed validator for the Task31 v2.4.0 Hickory non-reachability exception."""

from __future__ import annotations

import datetime as dt
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_IGNORES = {"RUSTSEC-2026-0119"}
EXPECTED_LIBP2P_FEATURES = {
    "tokio",
    "gossipsub",
    "identify",
    "kad",
    "macros",
    "tcp",
    "noise",
    "yamux",
    "ping",
}
FORBIDDEN_FEATURES = {"dns", "mdns", "quic"}
FORBIDDEN_COMPILED_PACKAGES = {
    "hickory-proto",
    "hickory-resolver",
    "libp2p-dns",
    "libp2p-mdns",
}
COMPILE_ROOTS = ("pulsedag-p2p", "pulsedagd")
REVIEW_DEADLINE = dt.date(2026, 8, 31)


def fail(message: str) -> None:
    raise SystemExit(f"Hickory exception validation failed: {message}")


def package_versions(lock_text: str, package: str) -> list[str]:
    versions: list[str] = []
    for raw in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"$', raw, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"$', raw, re.MULTILINE)
        if name and version and name.group(1) == package:
            versions.append(version.group(1))
    return versions


def package_name_from_id(package_id: object) -> str:
    value = str(package_id)
    match = re.search(r"#([^#@/ ]+)@[^# ]+$", value)
    if match:
        return match.group(1)
    match = re.search(r"/([^/ ]+)\s+[^ ]+\s+\(", value)
    if match:
        return match.group(1)
    return value


def compile_clean_and_capture(package: str, evidence_dir: Path) -> set[str]:
    with tempfile.TemporaryDirectory(prefix=f"pulsedag-{package}-") as target_dir:
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = target_dir
        command = [
            "cargo",
            "check",
            "--locked",
            "-p",
            package,
            "--message-format=json-render-diagnostics",
        ]
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=env,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    (evidence_dir / f"{package}-compiler-messages.jsonl").write_text(result.stdout)
    (evidence_dir / f"{package}-compiler-stderr.txt").write_text(result.stderr)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        fail(f"clean cargo check failed for {package}")

    compiled: set[str] = set()
    for line_number, line in enumerate(result.stdout.splitlines(), start=1):
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            fail(f"non-JSON cargo message for {package} at line {line_number}")
        if message.get("reason") != "compiler-artifact":
            continue
        compiled.add(package_name_from_id(message.get("package_id", "")))

    (evidence_dir / f"{package}-compiled-packages.txt").write_text(
        "\n".join(sorted(compiled)) + "\n"
    )
    return compiled


def main() -> None:
    today = dt.datetime.now(dt.timezone.utc).date()
    if today > REVIEW_DEADLINE:
        fail(
            f"exception expired on {REVIEW_DEADLINE.isoformat()}; "
            "remove or re-review it before continuing"
        )

    audit = tomllib.loads((ROOT / ".cargo" / "audit.toml").read_text())
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    if ignored != EXPECTED_IGNORES:
        fail(f"unexpected advisory ignore set: {sorted(ignored)}")

    manifest = tomllib.loads((ROOT / "crates" / "pulsedag-p2p" / "Cargo.toml").read_text())
    libp2p = manifest.get("dependencies", {}).get("libp2p")
    if not isinstance(libp2p, dict):
        fail("libp2p must remain an explicit dependency table")
    version = str(libp2p.get("version", ""))
    if not (version == "0.54" or version.startswith("0.54.")):
        fail(f"exception is version-specific to libp2p 0.54.x, found {version!r}")
    if libp2p.get("default-features") is not False:
        fail("libp2p default features must remain disabled")
    features = set(libp2p.get("features", []))
    if features != EXPECTED_LIBP2P_FEATURES:
        fail(f"unexpected libp2p feature set: {sorted(features)}")
    selected_forbidden = features & FORBIDDEN_FEATURES
    if selected_forbidden:
        fail(f"forbidden libp2p features selected: {sorted(selected_forbidden)}")

    lock_text = (ROOT / "Cargo.lock").read_text()
    if package_versions(lock_text, "hickory-proto") != ["0.24.4"]:
        fail("expected exactly hickory-proto 0.24.4 in the optional locked graph")

    p2p_source = (ROOT / "crates" / "pulsedag-p2p" / "src" / "lib.rs").read_text()
    if "mdns::Behaviour" in p2p_source or "mdns::tokio::Behaviour" in p2p_source:
        fail("an mDNS behaviour is instantiated")

    for profile in (
        ROOT / "configs" / "private-testnet" / "seed.env.example",
        ROOT / "configs" / "private-testnet" / "node.env.example",
        ROOT / "configs" / "single-node" / "single-node.env.example",
    ):
        if "PULSEDAG_P2P_MDNS=false" not in profile.read_text():
            fail(f"release profile {profile.relative_to(ROOT)} does not keep mDNS disabled")

    evidence_dir = ROOT / "ci-evidence" / "dependency-final-remediation"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    compiled_by_root: dict[str, set[str]] = {}
    for package in COMPILE_ROOTS:
        compiled = compile_clean_and_capture(package, evidence_dir)
        compiled_by_root[package] = compiled
        forbidden = sorted(compiled & FORBIDDEN_COMPILED_PACKAGES)
        if forbidden:
            fail(f"{package} actually compiles forbidden packages: {forbidden}")

    decision = (
        ROOT / "docs" / "security" / "V2_4_0_HICKORY_REACHABILITY_EXCEPTION.md"
    ).read_text()
    for required in (
        "RUSTSEC-2026-0119",
        "hickory-proto 0.24.4",
        "libp2p 0.54",
        "2026-08-31 UTC",
        "public-testnet GO",
    ):
        if required not in decision:
            fail(f"decision record is missing {required!r}")

    candidate_sha = os.environ.get("CANDIDATE_SHA") or os.environ.get("GITHUB_SHA", "local")
    summary = evidence_dir / "hickory-exception-validation.txt"
    summary.write_text(
        "\n".join(
            [
                "result=PASS",
                f"source_sha={candidate_sha}",
                f"validated_utc_date={today.isoformat()}",
                f"review_deadline={REVIEW_DEADLINE.isoformat()}",
                "ignored_advisories=RUSTSEC-2026-0119",
                f"libp2p_version={version}",
                f"libp2p_features={','.join(sorted(features))}",
                "hickory_proto_locked=0.24.4",
                "hickory_compiled_in_node=false",
                "hickory_compiled_in_p2p=false",
                "libp2p_dns_compiled=false",
                "libp2p_mdns_compiled=false",
                *(f"compiled_package_count_{root}={len(compiled_by_root[root])}" for root in COMPILE_ROOTS),
                "",
            ]
        )
    )
    print("PASS: Task31 Hickory exception is narrow, uncompiled and unexpired")


if __name__ == "__main__":
    main()
