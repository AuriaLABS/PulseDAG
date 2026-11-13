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

EXPECTED_KASPA_GIT = "https://github.com/kaspanet/rusty-kaspa"
EXPECTED_KASPA_REV = "cfafeb4c093fa37a303f1b9f19c58f986b870ce3"
EXPECTED_KASPA_DIRECT = {"kaspa-hashes", "kaspa-pow"}
EXPECTED_KASPA_VERSION = "2.0.1"

# Historical v2.4 lock-only exceptions plus reachable #803 blockers already
# remediated on the active v3 line must never return.
FORBIDDEN_LOCKED = {
    ("ring", "0.16.20"),
    ("rustls-webpki", "0.101.7"),
    ("hickory-proto", "0.24.4"),
    ("hickory-proto", "0.25.2"),
    ("rustix", "1.1.4"),
    ("rustix", "1.1.5"),
    ("h2", "0.3.27"),
    ("lru", "0.12.5"),
    ("linkme", "0.2.10"),
    ("intertrait", "0.2.2"),
}

# The libp2p parent migration retires Hickory 0.25.2 from the public lock.
# Quinn stays visible as exact lock-only debt, not an advisory waiver.
REQUIRED_LOCK_ONLY = {
    # #1298: development-only disposition requires exact clean-compiler proof
    # across ALL four launch roots. reqwest also retains the optional quinn
    # parent path; disabled libp2p QUIC alone is not sufficient.
    ("quinn-proto", "0.11.16"),
}

REQUIRED_KASPA = {
    ("kaspa-hashes", EXPECTED_KASPA_VERSION),
    ("kaspa-pow", EXPECTED_KASPA_VERSION),
}

REQUIRED_PATCHED = {
    ("crossbeam-epoch", "0.9.20"),
    ("anyhow", "1.0.103"),
    ("event-listener", "5.4.2"),
    ("rustix", "1.1.6"),
}
FORBIDDEN_OLD_REACHABLE = {
    ("crossbeam-epoch", "0.9.18"),
    ("anyhow", "1.0.102"),
    ("event-listener", "5.4.1"),
    ("rustix", "1.1.4"),
    ("rustix", "1.1.5"),
}

# These optional transports can remain represented in Cargo.lock by libp2p,
# but must not be compiler-reachable from PulseDAG's selected feature set.
FORBIDDEN_COMPILED_NAMES = {
    "libp2p-dns", "libp2p-mdns", "libp2p-quic", "libp2p-upnp",
    # Updated Hickory packages may remain locked by optional DNS/mDNS,
    # but no release root can compile them with these features disabled.
    "hickory-proto", "hickory-resolver",
    # None of the QUIC stack may compile through libp2p OR reqwest while
    # quinn-proto 0.11.16 has unpatched RUSTSEC-2026-0338..0342.
    "quinn", "quinn-proto", "quinn-udp",
}
COMPILE_ROOTS = ("pulsedag-p2p", "pulsedagd", "pulsedag-miner", "pulsedag-wallet")

# #803 still treats atty as an unresolved launch blocker. This gate records
# that it remains visible; it does not claim final v3 security readiness.
REQUIRED_VISIBLE_BLOCKERS = {
    ("atty", "0.2.14"),
}

# RUSTSEC-2026-0306 is an informational unsoundness advisory affecting
# faster_hex::hex_decode_unchecked on x86/x86_64. The package is reachable
# through the reviewed Kaspa parent stack, so the gate must keep it visible
# and prove the affected API is not referenced outside faster-hex itself.
REQUIRED_VISIBLE_INFORMATIONAL = {
    ("faster-hex", "0.9.0"),
}
# #1387: raw informational RustSec advisories, but these packages ARE
# compiler-reachable in every v3.0 launch root (not lock-only waivers).
# Their upstream migration remains a separate pre-GO security obligation.
REQUIRED_COMPILED_UNMAINTAINED_DEBT = {
    ("async-std", "1.13.2"): "RUSTSEC-2025-0052",
    ("pin-utils", "0.1.0"): "RUSTSEC-2026-0441",
}

EXPECTED_PATCHED_RUSTIX_ROOTS = {"pulsedagd", "pulsedag-wallet"}
RETIRED_PUBLIC_ROOT_NAMES = {
    "kaspa-hashes", "kaspa-pow", "async-std", "pin-utils",
    "faster-hex", "atty",
}
REQUIRED_HICKORY = {
    ("hickory-proto", "0.26.3"),
    ("hickory-resolver", "0.26.3"),
    ("libp2p-dns", "0.45.0"),
    ("libp2p-mdns", "0.49.0"),
}

AFFECTED_FASTER_HEX_SYMBOL = "hex_decode_unchecked"


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


def complete_public_lock_inventory() -> set[tuple[str, str]]:
    """Read optional/disabled package versions from Cargo.lock itself.

    Cargo metadata without --all-features omits optional DNS/mDNS packages
    even though a prior Cargo resolution retains them in Cargo.lock.
    Compiler-artifact identification still uses the narrower metadata IDs.
    """
    data = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    packages = data.get("package")
    if not isinstance(packages, list) or not packages:
        fail("public Cargo.lock missing resolved package inventory")
    inventory: set[tuple[str, str]] = set()
    for entry in packages:
        if not isinstance(entry, dict):
            fail("public Cargo.lock contains a malformed package")
        name, version = entry.get("name"), entry.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            fail("public Cargo.lock package missing exact name/version")
        if (name, version) in inventory:
            fail(f"duplicate Cargo.lock package entry: {name}@{version}")
        inventory.add((name, version))
    return inventory


def validate_kaspa_parent_stack(metadata: dict[str, Any]) -> str:
    manifest = tomllib.loads((ROOT / "crates" / "pulsedag-core" / "Cargo.toml").read_text(encoding="utf-8"))
    dependencies = manifest.get("dependencies", {})
    dev_dependencies = manifest.get("dev-dependencies", {})
    # Both dependencies move together or fail closed. In migration mode the
    # pinned crates remain only an independent test oracle, NOT a release root.
    normal = {name for name in EXPECTED_KASPA_DIRECT if name in dependencies}
    dev = {name for name in EXPECTED_KASPA_DIRECT if name in dev_dependencies}
    if not normal and not dev:
        # The upstream Kaspa oracle moved into an independent Cargo workspace.
        # Prove its complete pinned provenance while requiring absence from
        # production root metadata/lock; never infer security from missing data.
        oracle = ROOT / "tools" / "pow-upstream-reference" / "Cargo.toml"
        oracle_lock = oracle.with_name("Cargo.lock")
        manifest_ref = tomllib.loads(oracle.read_text(encoding="utf-8"))
        if manifest_ref.get("workspace", {}).get("resolver") != "2":
            fail("upstream PoW oracle must have separate Cargo workspace")
        selected = manifest_ref.get("dependencies", {})
        oracle_packages = tomllib.loads(oracle_lock.read_text(encoding="utf-8"))["package"]
        for name in EXPECTED_KASPA_DIRECT:
            value = selected.get(name)
            if not isinstance(value, dict) or value.get("git") != EXPECTED_KASPA_GIT:
                fail(f"separate PoW oracle {name} official upstream Git missing")
            if value.get("rev") != EXPECTED_KASPA_REV:
                fail(f"separate PoW oracle {name} pinned revision drift")
            if any(k in value for k in ("branch", "tag", "path")):
                fail(f"separate PoW oracle {name} has unapproved source override")
            found = [p for p in oracle_packages if p.get("name") == name]
            if len(found) != 1 or found[0].get("version") != EXPECTED_KASPA_VERSION:
                fail(f"separate PoW oracle {name} resolved version drift")
            if not str(found[0].get("source", "")).startswith(
                f"git+{EXPECTED_KASPA_GIT}?rev={EXPECTED_KASPA_REV}#"
            ):
                fail(f"separate PoW oracle {name} origin or revision drift")
        run(["cargo", "metadata", "--locked", "--manifest-path", str(oracle),
             "--format-version", "1", "--no-deps"])
        pow_text = (ROOT / "crates/pulsedag-core/src/pow.rs").read_text(encoding="utf-8")
        lib_text = (ROOT / "crates/pulsedag-core/src/lib.rs").read_text(encoding="utf-8")
        backend = ROOT / "crates/pulsedag-core/src/pow_native_kheavyhash.rs"
        if not backend.is_file() or "mod pow_native_kheavyhash;" not in lib_text:
            fail("native production PoW backend missing")
        if any(marker in pow_text for marker in (
            "use kaspa_hashes", "use kaspa_pow", "kaspa_hashes::", "kaspa_pow::"
        )):
            fail("production PoW still imports test-only Kaspa")
        if "native_kheavyhash_digest" not in pow_text or "NativeKHeavyHasher" not in pow_text:
            fail("production native PoW backend not fully wired")
        if any(p.get("name") in EXPECTED_KASPA_DIRECT for p in metadata["packages"]):
            fail("upstream Kaspa oracle leaked into public-root metadata")
        return "native_runtime_external_reference"
    if normal == EXPECTED_KASPA_DIRECT and not dev:
        mode = "pinned_runtime"
        selected = dependencies
    elif not normal and dev == EXPECTED_KASPA_DIRECT:
        mode = "native_runtime_dev_reference"
        selected = dev_dependencies
        native_path = ROOT / "crates/pulsedag-core/src/pow_native_kheavyhash.rs"
        native = native_path.read_text(encoding="utf-8") if native_path.is_file() else ""
        pow_source = (ROOT / "crates/pulsedag-core/src/pow.rs").read_text(encoding="utf-8")
        library = (ROOT / "crates/pulsedag-core/src/lib.rs").read_text(encoding="utf-8")
        if not native or "mod pow_native_kheavyhash;" not in library:
            fail("native runtime claims missing canonical native backend module")
        if any(marker in pow_source for marker in (
            "use kaspa_hashes", "use kaspa_pow", "kaspa_hashes::", "kaspa_pow::"
        )):
            fail("native runtime still directly imports Kaspa")
        if "native_kheavyhash_digest" not in pow_source or "NativeKHeavyHasher" not in pow_source:
            fail("native runtime digest/prepared backend linkage is incomplete")
    else:
        fail("Kaspa runtime/test dependency scope split or missing: review #1387")
    for dependency in EXPECTED_KASPA_DIRECT:
        value = selected.get(dependency)
        if not isinstance(value, dict):
            fail(f"{dependency} must remain explicitly pinned for the current evidence mode")
        if value.get("git") != EXPECTED_KASPA_GIT:
            fail(f"{dependency} must use official rusty-kaspa upstream, found {value.get('git')!r}")
        if value.get("rev") != EXPECTED_KASPA_REV:
            fail(f"{dependency} must remain pinned to reviewed Kaspa v2.0.1 SHA {EXPECTED_KASPA_REV}")
        if any(key in value for key in ("branch", "tag", "path")):
            fail(f"{dependency} must use the exact reviewed rev without branch/tag/path overrides")

    expected_source_prefix = f"git+{EXPECTED_KASPA_GIT}?rev={EXPECTED_KASPA_REV}#"
    observed: dict[str, tuple[str, str]] = {}
    for package in metadata["packages"]:
        name = str(package["name"])
        if name not in EXPECTED_KASPA_DIRECT:
            continue
        observed[name] = (str(package["version"]), str(package.get("source") or ""))

    for dependency in EXPECTED_KASPA_DIRECT:
        version, source = observed.get(dependency, ("", ""))
        if version != EXPECTED_KASPA_VERSION:
            fail(f"{dependency} must resolve to {EXPECTED_KASPA_VERSION}, found {version or 'nothing'}")
        if not source.startswith(expected_source_prefix):
            fail(f"{dependency} resolved from unexpected source {source!r}")
    return mode


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


def require_patched_rustix_compiler_evidence(
    compiled_by_root: dict[str, set[tuple[str, str]]],
) -> dict[str, list[str]]:
    """Require positive compiled-patch evidence for the actual launch roots."""
    if set(compiled_by_root) != set(COMPILE_ROOTS):
        fail("rustix compiler evidence missing or unexpected launch root")
    evidence: dict[str, list[str]] = {}
    for root in COMPILE_ROOTS:
        versions = sorted(v for n, v in compiled_by_root[root] if n == "rustix")
        if any(v != "1.1.6" for v in versions):
            fail(f"{root} compiled affected or unreviewed rustix versions: {versions}")
        if root in EXPECTED_PATCHED_RUSTIX_ROOTS and versions != ["1.1.6"]:
            fail(f"{root} lacks exact patched rustix compiler evidence: {versions}")
        evidence[root] = versions
    return evidence


def validate_faster_hex_advisory_scope(
    metadata: dict[str, Any], evidence_dir: Path
) -> list[str]:
    call_sites: list[str] = []
    reviewed_prefixes = ("kaspa-", "workflow-", "pulsedag-")
    reviewed_exact = {"pulsedagd"}

    for package in metadata["packages"]:
        name = str(package["name"])
        if name == "faster-hex":
            continue
        if not (name.startswith(reviewed_prefixes) or name in reviewed_exact):
            continue

        package_root = Path(str(package["manifest_path"])).parent
        if not package_root.exists():
            fail(f"package source missing while checking {AFFECTED_FASTER_HEX_SYMBOL}: {name}")
        for source in package_root.rglob("*.rs"):
            try:
                text = source.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                text = source.read_text(encoding="utf-8", errors="replace")
            if AFFECTED_FASTER_HEX_SYMBOL in text:
                call_sites.append(f"{name}:{source.relative_to(package_root)}")

    call_sites.sort()
    (evidence_dir / "faster-hex-0306-call-sites.txt").write_text(
        "\n".join(call_sites) + ("\n" if call_sites else ""),
        encoding="utf-8",
    )
    if call_sites:
        fail(
            "RUSTSEC-2026-0306 affected API is referenced by reviewed launch-source packages: "
            f"{call_sites}"
        )
    return call_sites


def require_compiler_reachable_debt(
    compiled_by_root: dict[str, set[tuple[str, str]]],
    *,
    native_runtime: bool = False,
) -> dict[str, list[str]]:
    """Pin exact compiler-reachable inherited advisory inventory, NOT a waiver.

    An upstream dependency migration is a separate security change: it must
    update this evidence policy with a reviewed RustSec report rather than
    quietly being interpreted as a launch GO or a compiled/lock-only waiver.
    """
    if set(compiled_by_root) != set(COMPILE_ROOTS):
        fail("launch-root compiler inventory incomplete or unexpected")
    required = set(REQUIRED_COMPILED_UNMAINTAINED_DEBT)
    evidence: dict[str, list[str]] = {}
    for root in COMPILE_ROOTS:
        observed = compiled_by_root[root]
        if native_runtime:
            # Require positive release-root compiler evidence, not merely the
            # absence of vulnerable names in an empty/suppressed report.
            observed_names = {name for name, _ in observed}
            if root not in observed_names or "pulsedag-core" not in observed_names:
                fail(f"{root} native backend compiler evidence missing root or pulsedag-core")
            # An independent runtime must prove compiled absence, not relabel
            # reachable advisories as lock-only without compiler evidence.
            forbidden_names = {
                name for name, _ in observed
                if name.startswith(("kaspa-", "workflow-"))
                or name in {"async-std", "pin-utils", "faster-hex"}
            }
            if forbidden_names:
                fail(
                    f"{root} still compiles obsolete Kaspa/transitive packages: "
                    f"{sorted(forbidden_names)}"
                )
            evidence[root] = []
            continue
        missing = sorted(required - observed)
        if missing:
            fail(
                f"{root} inherited unmaintained compiled-dependency inventory drifted: "
                f"{missing}; review #1387 before changing its release disposition"
            )
        evidence[root] = [
            f"{name}@{version}:{REQUIRED_COMPILED_UNMAINTAINED_DEBT[(name, version)]}"
            for name, version in sorted(required)
        ]
    return evidence


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
    if version != "=0.57.1":
        fail(f"v3 migration requires exact reviewed libp2p =0.57.1, found {version!r}")
    if libp2p.get("default-features") is not False:
        fail("libp2p default features must remain disabled")
    features = set(libp2p.get("features", []))
    if features != EXPECTED_LIBP2P_FEATURES:
        fail(f"unexpected libp2p feature set: {sorted(features)}")
    if features & FORBIDDEN_FEATURES:
        fail(f"forbidden libp2p features selected: {sorted(features & FORBIDDEN_FEATURES)}")

    metadata = cargo_metadata()
    kaspa_mode = validate_kaspa_parent_stack(metadata)
    native_runtime = kaspa_mode in {"native_runtime_dev_reference", "native_runtime_external_reference"}
    by_id = package_index(metadata)
    # LOCK inventory must include unselected DNS/mDNS optional dependencies.
    # Actual compiled reachability continues to come from compiler-artifact
    # messages mapped against Cargo metadata's exact package IDs.
    locked = complete_public_lock_inventory()

    forbidden_present = sorted(FORBIDDEN_LOCKED & locked)
    if forbidden_present:
        fail(f"forbidden legacy/vulnerable versions remain locked: {forbidden_present}")

    missing_lock_only = sorted(REQUIRED_LOCK_ONLY - locked)
    if missing_lock_only:
        fail(
            "reviewed lock-only vulnerability inventory drifted; review before changing disposition: "
            f"{missing_lock_only}"
        )

    if kaspa_mode != "native_runtime_external_reference":
        fail(f"public-root migration requires independent Kaspa oracle: {kaspa_mode}")
    retired = sorted((name, ver) for name, ver in locked
                     if name in RETIRED_PUBLIC_ROOT_NAMES)
    if retired:
        fail(f"retired advisory-bearing packages remain in public root: {retired}")
    missing_hickory = sorted(REQUIRED_HICKORY - locked)
    if missing_hickory:
        fail(f"patched libp2p Hickory parent packages missing: {missing_hickory}")
    if any(name == "hickory-proto" and ver != "0.26.3" for name, ver in locked):
        fail("Hickory 0.25.x or unreviewed version remains in public lock")
    if ("libp2p", "0.57.1") not in locked:
        fail("reviewed libp2p 0.57.1 parent missing from public lock")

    missing_patched = sorted(REQUIRED_PATCHED - locked)
    if missing_patched:
        fail(f"required patched package versions missing: {missing_patched}")

    stale = sorted(FORBIDDEN_OLD_REACHABLE & locked)
    if stale:
        fail(f"previous vulnerable reachable versions returned: {stale}")

    if any(name == "rustix" and ver != "1.1.6" for name, ver in locked):
        fail("vulnerable or unreviewed rustix in public Cargo.lock")

    evidence_dir = ROOT / "ci-evidence" / "dependency-v3-security"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    compiled_by_root: dict[str, set[tuple[str, str]]] = {}
    for root in COMPILE_ROOTS:
        compiled = compile_clean(root, evidence_dir, by_id)
        compiled_by_root[root] = compiled
        compiled_forbidden_versions = sorted(compiled & FORBIDDEN_LOCKED)
        if compiled_forbidden_versions:
            fail(f"{root} compiles forbidden legacy/vulnerable versions: {compiled_forbidden_versions}")
        compiled_lock_only = sorted(compiled & REQUIRED_LOCK_ONLY)
        if compiled_lock_only:
            fail(f"{root} compiles lock-only vulnerability disposition: {compiled_lock_only}")
        compiled_names = {name for name, _version in compiled}
        forbidden_names = sorted(compiled_names & FORBIDDEN_COMPILED_NAMES)
        if forbidden_names:
            fail(f"{root} compiles unselected optional libp2p packages: {forbidden_names}")

    patched_rustix_by_root = require_patched_rustix_compiler_evidence(compiled_by_root)
    reachable_unmaintained = require_compiler_reachable_debt(compiled_by_root, native_runtime=native_runtime)
    (evidence_dir / "reachable-unmaintained-launch-debt.json").write_text(
        json.dumps(reachable_unmaintained, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    faster_hex_call_sites = validate_faster_hex_advisory_scope(metadata, evidence_dir)

    candidate_sha = os.environ.get("CANDIDATE_SHA") or os.environ.get("GITHUB_SHA", "local")
    summary = {
        "result": "PASS",
        "source_sha": candidate_sha,
        "libp2p_version": version,
        "libp2p_features": sorted(features),
        "kaspa_version": EXPECTED_KASPA_VERSION,
        "kaspa_dependency_mode": kaspa_mode,
        "native_runtime_compiled_kaspa_tree_absent": native_runtime,
        "kaspa_upstream_rev": EXPECTED_KASPA_REV,
        "forbidden_legacy_or_vulnerable_locked": False,
        "lock_only_vulnerability_disposition": [
            f"{name}@{ver}" for name, ver in sorted(REQUIRED_LOCK_ONLY)
        ],
        "lock_only_vulnerabilities_compiled": False,
        "lru_0_12_5_present": ("lru", "0.12.5") in locked,
        "linkme_0_2_10_present": ("linkme", "0.2.10") in locked,
        "intertrait_0_2_2_present": ("intertrait", "0.2.2") in locked,
        "remaining_launch_blockers": [
            f"{name}@{ver}" for name, ver in sorted(REQUIRED_LOCK_ONLY)
        ],
        "retired_compiler_reachable_unmaintained_packages": sorted(
            f"{name}@{ver}" for name, ver in REQUIRED_COMPILED_UNMAINTAINED_DEBT
        ),
        "reachable_unmaintained_compiler_debt_by_launch_root": reachable_unmaintained,
        "reachable_unmaintained_inventory_does_not_authorize_release": True,
        "visible_informational_advisories": [],
        "retired_public_root_informational_advisories": [
            f"{name}@{ver}" for name, ver in sorted(REQUIRED_VISIBLE_INFORMATIONAL)
        ],
        "faster_hex_0306_affected_api_call_sites": faster_hex_call_sites,
        "compiled_counts": {root: len(compiled) for root, compiled in compiled_by_root.items()},
        "patched_rustix_compiled_by_root": patched_rustix_by_root,
        "public_root_retired_kaspa_advisory_packages_absent": True,
        "upstream_oracle_separate_locked_workspace": True,
        "final_v3_launch_security_ready": False,
    }
    (evidence_dir / "validation.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (evidence_dir / "validation.txt").write_text("\n".join([
        "result=PASS",
        f"source_sha={candidate_sha}",
        f"libp2p_version={version}",
        f"kaspa_version={EXPECTED_KASPA_VERSION}",
        f"kaspa_dependency_mode={kaspa_mode}",
        f"kaspa_upstream_rev={EXPECTED_KASPA_REV}",
        "lru_0_12_5_present=false",
        "linkme_0_2_10_present=false",
        "intertrait_0_2_2_present=false",
        "hickory_proto_0_25_2_locked=false",
        "hickory_proto_0_26_3_locked=true",
        "hickory_proto_0_25_2_compiled=false",
        "quinn_proto_0_11_16_lock_only=true",
        "quinn_proto_0_11_16_compiled=false",
        "quinn_stack_compiled=false",
        "historical_v2_4_lock_only_vulnerable_versions_present=false",
        "retired_public_root_advisory_chain=atty,async-std,pin-utils,faster-hex",
        f"pin_utils_0_1_0_compiled_all_four_launch_roots={str(not native_runtime).lower()}",
        "retired_oracle_pin_utils_advisory=RUSTSEC-2026-0441:unmaintained_external_only",
        "retired_oracle_async_std_advisory=RUSTSEC-2025-0052:unmaintained_external_only",
        "visible_informational_advisories_public_root=none_from_archived_kaspa_chain",
        "rustix_1_1_6_patched_lock_only=false",
        "faster_hex_0306_affected_api_call_sites=0",
        "final_v3_launch_security_ready=false",
        "",
    ]), encoding="utf-8")
    print(
        "PASS: source/compiled dependency evidence, NOT release security approval; "
        f"Kaspa mode={kaspa_mode}, compiled-unmaintained "
        f"{'absent' if native_runtime else 'present'} in launch roots"
    )


if __name__ == "__main__":
    main()
