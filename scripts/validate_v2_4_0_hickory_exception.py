#!/usr/bin/env python3
"""Fail-closed validator for the temporary v2.4.0 Hickory exception."""

from __future__ import annotations

import datetime as dt
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_IGNORES = {"RUSTSEC-2026-0118", "RUSTSEC-2026-0119"}
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
FORBIDDEN_ACTIVE_PACKAGES = {
    "hickory-proto",
    "hickory-resolver",
    "libp2p-dns",
    "libp2p-mdns",
}
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


def activated_tree(package: str) -> str:
    command = [
        "cargo",
        "tree",
        "--locked",
        "-p",
        package,
        "--edges",
        "normal",
    ]
    result = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        fail(f"cargo tree failed for {package}")
    return result.stdout


def main() -> None:
    today = dt.datetime.now(dt.timezone.utc).date()
    if today > REVIEW_DEADLINE:
        fail(
            f"exception expired on {REVIEW_DEADLINE.isoformat()}; "
            "remove or re-review it before continuing"
        )

    audit_path = ROOT / ".cargo" / "audit.toml"
    audit = tomllib.loads(audit_path.read_text())
    ignored = set(audit.get("advisories", {}).get("ignore", []))
    if ignored != EXPECTED_IGNORES:
        fail(f"unexpected advisory ignore set: {sorted(ignored)}")

    manifest_path = ROOT / "crates" / "pulsedag-p2p" / "Cargo.toml"
    manifest = tomllib.loads(manifest_path.read_text())
    libp2p = manifest.get("dependencies", {}).get("libp2p")
    if not isinstance(libp2p, dict):
        fail("libp2p must remain an explicit dependency table")
    version = str(libp2p.get("version", ""))
    if not (version == "0.56" or version.startswith("0.56.")):
        fail(f"exception is version-specific to libp2p 0.56.x, found {version!r}")
    if libp2p.get("default-features") is not False:
        fail("libp2p default features must remain disabled")
    features = set(libp2p.get("features", []))
    if features != EXPECTED_LIBP2P_FEATURES:
        fail(f"unexpected libp2p feature set: {sorted(features)}")
    selected_forbidden = features & FORBIDDEN_FEATURES
    if selected_forbidden:
        fail(f"forbidden libp2p features selected: {sorted(selected_forbidden)}")

    lock_text = (ROOT / "Cargo.lock").read_text()
    if package_versions(lock_text, "hickory-proto") != ["0.25.2"]:
        fail("expected exactly hickory-proto 0.25.2 in the optional locked graph")
    if package_versions(lock_text, "quinn-proto") != ["0.11.15"]:
        fail("quinn-proto must resolve exactly to patched version 0.11.15")

    p2p_source = (ROOT / "crates" / "pulsedag-p2p" / "src" / "lib.rs").read_text()
    if "state.mdns = false;" not in p2p_source:
        fail("runtime status must report mDNS disabled")
    if "mdns::Behaviour" in p2p_source or "mdns::tokio::Behaviour" in p2p_source:
        fail("an mDNS behaviour is instantiated")

    config_source = (ROOT / "apps" / "pulsedagd" / "src" / "config.rs").read_text()
    if "PULSEDAG_P2P_MDNS=true is unsupported in v2.4.0" not in config_source:
        fail("daemon configuration does not reject mDNS enablement")
    if "p2p_mdns: true" in config_source:
        fail("a daemon profile still defaults mDNS to true")

    evidence_dir = ROOT / "ci-evidence" / "dependency-final-remediation"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    for package in ("pulsedag-p2p", "pulsedagd"):
        tree = activated_tree(package)
        (evidence_dir / f"{package}-normal-tree.txt").write_text(tree)
        active = sorted(
            name
            for name in FORBIDDEN_ACTIVE_PACKAGES
            if re.search(rf"(^|[\s├└│]){re.escape(name)} v", tree, re.MULTILINE)
        )
        if active:
            fail(f"{package} activates forbidden packages: {active}")

    decision = (
        ROOT / "docs" / "security" / "V2_4_0_HICKORY_REACHABILITY_EXCEPTION.md"
    ).read_text()
    for required in (
        "RUSTSEC-2026-0118",
        "RUSTSEC-2026-0119",
        "2026-08-31 UTC",
        "libp2p 0.56.0",
        "public-testnet GO",
    ):
        if required not in decision:
            fail(f"decision record is missing {required!r}")

    summary = evidence_dir / "hickory-exception-validation.txt"
    summary.write_text(
        "\n".join(
            [
                "result=PASS",
                f"validated_utc_date={today.isoformat()}",
                f"review_deadline={REVIEW_DEADLINE.isoformat()}",
                f"ignored_advisories={','.join(sorted(EXPECTED_IGNORES))}",
                f"libp2p_version={version}",
                f"libp2p_features={','.join(sorted(features))}",
                "hickory_proto_locked=0.25.2",
                "hickory_active_in_node=false",
                "hickory_active_in_p2p=false",
                "quinn_proto_locked=0.11.15",
                "",
            ]
        )
    )
    print("PASS: v2.4.0 Hickory exception remains narrow, unreachable and unexpired")


if __name__ == "__main__":
    main()
