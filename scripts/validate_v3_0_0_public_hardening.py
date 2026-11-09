#!/usr/bin/env python3
"""Fail-closed validator for the active PulseDAG v3.0.0 public-safety preparation."""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"v3.0.0 public hardening validation failed: {message}")


def text(path: str) -> str:
    target = ROOT / path
    if not target.is_file():
        fail(f"required file missing: {path}")
    return target.read_text(encoding="utf-8")


def require(path: str, *needles: str) -> str:
    body = text(path)
    for needle in needles:
        if needle not in body:
            fail(f"{path} missing required guardrail: {needle!r}")
    return body


def function_slice(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        fail(f"function marker missing: {marker}")
    end = source.find("\nfn ", start + len(marker))
    return source[start:] if end < 0 else source[start:end]


# Exact assignment checks for UNRENDERED pre-GO templates. A grep match in a
# comment, duplicated env entry, shell expression or alternate assignment is
# not evidence that a future operator would load a safe configuration.
PUBLIC_TEMPLATE_REQUIRED = {
    "PULSEDAG_CONFIG_PROFILE": "testnet",
    "PULSEDAG_NETWORK_PROFILE": "__TASK31_FREEZE_REQUIRED__",
    "PULSEDAG_CHAIN_ID": "__TASK31_FREEZE_REQUIRED__",
    "PULSEDAG_CONSENSUS_MODE": "legacy",
    "PULSEDAG_PROTOCOL_CONSENSUS_MODE": "ghostdag_v1",
    "PULSEDAG_P2P_ENABLED": "false",
    "PULSEDAG_P2P_BOOTSTRAP": "__TASK31_FREEZE_REQUIRED__",
    "PULSEDAG_PUBLIC_P2P_MULTIADDR": "__TASK31_FREEZE_REQUIRED__",
    "PULSEDAG_RPC_BIND": "127.0.0.1:8080",
    "PULSEDAG_API_PROFILE": "public_safe",
    "PULSEDAG_ADMIN_ENABLED": "false",
    "PULSEDAG_EXPERIMENTAL_FAST_CADENCE": "false",
    "PULSEDAG_CONTRACTS_ENABLED": "false",
    "PULSEDAG_PUBLIC_TESTNET_READY": "false",
    "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED": "false",
}


def validate_public_template_assignments(body: str, path: str) -> dict[str, str]:
    """Parse literal pre-GO env assignments, never evaluate or render shell."""
    if len(body.encode("utf-8")) > 64 * 1024:
        fail(f"{path}: pre-GO env template exceeds 64 KiB")
    assignments: dict[str, str] = {}
    for number, raw_line in enumerate(body.splitlines(), 1):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        # Reject whitespace aliases, shell substitutions, inline comments,
        # export statements, hidden secondary commands and mixed-case keys.
        match = re.fullmatch(r"(PULSEDAG_[A-Z0-9_]+)=([A-Za-z0-9_./:,+-]*)", raw_line)
        if match is None:
            fail(f"{path}:{number}: only unquoted literal PULSEDAG_KEY=value allowed")
        key, value = match.groups()
        if key in assignments:
            fail(f"{path}:{number}: duplicate assignment for {key}")
        assignments[key] = value

    for key, expected in PUBLIC_TEMPLATE_REQUIRED.items():
        if assignments.get(key) != expected:
            fail(f"{path}: active {key} must be exact pre-GO value {expected!r}")
    return assignments


def main() -> None:
    if text("VERSION").strip() != "v3.0.0":
        fail("VERSION is not v3.0.0")
    cargo = text("Cargo.toml")
    if 'version = "3.0.0"' not in cargo:
        fail("Cargo workspace version is not 3.0.0")

    security = require(
        "SECURITY.md",
        "security/advisories/new",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
        "contracts_enabled=false",
    )
    if re.search(r"BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY", security):
        fail("SECURITY.md contains an apparent private key")

    for path in (
        "configs/public-testnet/seed.env.template",
        "configs/public-testnet/node.env.template",
        "configs/public-testnet/observer.env.template",
    ):
        body = require(
            path,
            "__TASK31_FREEZE_REQUIRED__",
            "PULSEDAG_P2P_ENABLED=false",
            "PULSEDAG_API_PROFILE=public_safe",
            "PULSEDAG_ADMIN_ENABLED=false",
            "PULSEDAG_EXPERIMENTAL_FAST_CADENCE=false",
            "PULSEDAG_CONTRACTS_ENABLED=false",
            "PULSEDAG_PUBLIC_TESTNET_READY=false",
            "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false",
        )
        validate_public_template_assignments(body, path)
        if re.search(r"BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY", body):
            fail(f"{path} contains an apparent private key")
        for forbidden in ("ghp_", "github_pat_", "AKIA", "xoxb-", "xoxp-"):
            if forbidden in body:
                fail(f"{path} contains apparent credential marker {forbidden!r}")

    require(
        "configs/public-testnet/miner.args.template",
        "__TASK31_FREEZE_REQUIRED__",
        "--node",
        "--miner-address",
        "--backend",
        "cpu",
    )
    templates_readme = require(
        "configs/public-testnet/README.md",
        "v3.0.0",
        "pre-GO templates",
        "PULSEDAG_P2P_ENABLED=false",
        "GO_PUBLIC_TESTNET",
        "__TASK31_FREEZE_REQUIRED__",
        "validate_v3_0_0_public_hardening.py",
        "no mandatory private live network",
        "legacy/ghostdag_v1",
        "native-v3",
    )
    public_runbook = require(
        "docs/runbooks/V3_0_0_PUBLIC_TESTNET_PREP.md",
        "v3.0.0",
        "PRE-GO / NOT LAUNCHED",
        "no mandatory private live network",
        "first live native-v3 monetary-mining network",
        "GO_PUBLIC_TESTNET",
        "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true",
        "Smart contracts remain INACTIVE throughout public testnet and mainnet v3.0.0",
    )
    for path, body in (
        ("configs/public-testnet/README.md", templates_readme),
        ("docs/runbooks/V3_0_0_PUBLIC_TESTNET_PREP.md", public_runbook),
    ):
        for obsolete_gate in (
            "After the private burn-in",
            "5-node/4-miner private rehearsal PASS",
            "demonstrated in the private rehearsal before public GO",
            "until at least 30 accepted public-testnet days",
        ):
            if obsolete_gate in body:
                fail(f"{path} retains superseded live-network gate: {obsolete_gate!r}")
    require(
        "docs/VERSION_MATRIX.md",
        "PENDING_EXACT_CANDIDATE_EVIDENCE",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
        "contracts_enabled=false",
    )
    require(
        "docs/ROADMAP_V3_0_0.md",
        "ACTIVE v3.0.0 CANDIDATE ROADMAP",
        "contracts_enabled=true",
    )
    require(
        "docs/dashboard/README.md",
        "ops/observability/v3.0.0/",
        "public_safe",
        "GET /metrics",
        "GET /status",
        "GET /mempool",
    )
    require(
        "ops/observability/v3.0.0/metrics-inventory.json",
        '"release_line":"v3.0.0"',
        '"/metrics"',
        '"/status"',
        '"/mempool"',
        "pulsedag_chain_commit_publish_mismatch_total",
        "pulsedag_sync_selected_tip_mismatch",
        "pulsedag_rpc_liveness_current_degraded",
    )
    require(
        "ops/observability/v3.0.0/alert-rules.yml",
        "PulseDAGSelectedTipMismatch",
        "PulseDAGSnapshotVerificationStableFailure",
        "PulseDAGPeerIsolation",
        "PulseDAGMiningSubmitActorTimeout",
    )
    require(
        "ops/observability/v3.0.0/prometheus-scrape.example.yml",
        "pulsedag-v3.0.0",
        "network: private-testnet-v3.0.0",
        "alert-rules-operations.yml",
    )
    require(
        "scripts/private_testnet/runtime_metrics_exporter.py",
        'DEFAULT_INVENTORY = Path("ops/observability/v3.0.0/metrics-inventory.json")',
        '"v3.0.0"',
    )

    require(
        "docs/API_V1.md",
        "Admin is disabled by default for **all** profiles and RPC binds.",
        "request body limit: **128 KiB**",
        "rate limit: **30 requests per 60 seconds**",
        "wildcard CORS origin (`*`): **rejected**",
    )
    config = require(
        "apps/pulsedagd/src/config.rs",
        "PublicSafe",
        '"public_safe" => Ok(Self::PublicSafe)',
        "admin endpoints cannot be enabled",
        "wildcard origin is not allowed",
    )
    if "fn default_admin_enabled(_network_profile: &str, _rpc_bind: &str) -> bool {\n    false\n}" not in config:
        fail("admin default is no longer fail-closed")

    routes = require(
        "crates/pulsedag-rpc/src/routes.rs",
        "ApiExposureProfile::PublicSafe",
        "public_safe_routes",
    )
    public_safe = function_slice(routes, "fn public_safe_routes")
    for endpoint in ('"/metrics"', '"/status"', '"/mempool"'):
        if endpoint not in public_safe:
            fail(f"public_safe route block lost {endpoint}")
    for forbidden in ('"/admin"', '"/runtime"'):
        if forbidden in public_safe:
            fail(f"public_safe route block unexpectedly contains {forbidden}")

    print("PASS: v3.0.0 public hardening remains fail-closed and pre-GO")


if __name__ == "__main__":
    main()
