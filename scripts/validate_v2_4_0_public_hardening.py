#!/usr/bin/env python3
"""Fail-closed validator for v2.4.0 public-safety freeze preparation."""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"v2.4.0 public hardening validation failed: {message}")


def text(path: str) -> str:
    p = ROOT / path
    if not p.is_file():
        fail(f"required file missing: {path}")
    return p.read_text(encoding="utf-8")


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


def main() -> None:
    security = require(
        "SECURITY.md",
        "security/advisories/new",
        "Do **not** open a public GitHub issue",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
        "contracts_enabled=false",
    )
    if re.search(r"BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY", security):
        fail("SECURITY.md contains an apparent private key")

    require(
        "configs/private-testnet/miner.env.example",
        "PULSEDAG_MINER_NODE_URL=http://127.0.0.1:8280",
        "REPLACE_WITH_VALUELESS_PRIVATE_TEST_ADDRESS",
        "--miner-address",
        "--backend",
    )

    template_paths = [
        "configs/public-testnet/seed.env.template",
        "configs/public-testnet/node.env.template",
        "configs/public-testnet/observer.env.template",
    ]
    for path in template_paths:
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
        if "PULSEDAG_P2P_ENABLED=true" in body:
            fail(f"{path} enables P2P before GO")
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
    require(
        "configs/public-testnet/README.md",
        "pre-GO templates",
        "PULSEDAG_P2P_ENABLED=false",
        "GO_PUBLIC_TESTNET",
        "__TASK31_FREEZE_REQUIRED__",
    )
    require(
        "docs/runbooks/V2_4_0_PUBLIC_TESTNET_PREP.md",
        "PRE-GO / NOT LAUNCHED",
        "#789 private burn-in",
        "5-node/4-miner",
        "#803",
        "GO_PUBLIC_TESTNET",
        "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=true",
    )
    require(
        "docs/release/V2_4_0_KNOWN_LIMITATIONS.md",
        "no official end-user custody wallet",
        "atty 0.2.14",
        "linkme 0.2.10",
        "lru 0.12.5",
        "Evidence from different SHAs must not be combined",
        "smart contracts remain disabled",
    )
    require(
        "docs/release/V2_4_0_RELEASE_NOTES.md",
        "NOT RELEASED / NOT ACTIVATED",
        "no official end-user custody wallet",
        "#803",
        "GO_PUBLIC_TESTNET",
        "public_testnet_ready=false",
        "thirty_day_public_testnet_clock_started=false",
    )
    require(
        "docs/dashboard/README.md",
        "ops/observability/v2.4.0/",
        "public_safe",
        "GET /metrics",
        "GET /status",
        "GET /mempool",
    )
    require(
        "ops/observability/v2.4.0/README.md",
        "release-candidate",
        "GET /metrics",
        "GET /status",
        "GET /mempool",
        "does not set `public_testnet_ready=true`",
    )
    require(
        "ops/observability/v2.4.0/metrics-inventory.json",
        '"release_line":"v2.4.0"',
        '"/metrics"',
        '"/status"',
        '"/mempool"',
        "pulsedag_chain_commit_publish_mismatch_total",
        "pulsedag_state_invalid_root_total",
        "pulsedag_mining_submit_actor_timeout_total",
        "pulsedag_sync_selected_tip_mismatch",
        "pulsedag_rpc_liveness_current_degraded",
        "pulsedag_node_uptime_seconds",
    )
    require(
        "ops/observability/v2.4.0/alert-rules.yml",
        "PulseDAGSelectedTipMismatch",
        "PulseDAGSnapshotVerificationStableFailure",
        "PulseDAGPeerIsolation",
        "PulseDAGMiningSubmitActorTimeout",
    )
    require(
        "ops/observability/v2.4.0/alert-rules-operations.yml",
        "PulseDAGSubmitFinalityUnknown",
        "pulsedag_mining_submit_actor_timeout_total",
        "PulseDAGSharedStateLockStarvation",
        "pulsedag_rpc_liveness_current_degraded",
        "pulsedag_rpc_oldest_inflight_handler_age_ms",
        "PulseDAGUnexpectedRestart",
        "pulsedag_node_uptime_seconds",
        "PulseDAGDiskPressure",
        "node_filesystem_avail_bytes",
        "node_filesystem_size_bytes",
    )
    require(
        "ops/observability/v2.4.0/prometheus-scrape.example.yml",
        "alert-rules-operations.yml",
        "pulsedag-v2.4.0-host",
        ":9100",
        "network: private-testnet-v2.4.0",
    )
    require(
        "ops/observability/v2.4.0/HOST_METRICS.md",
        "node_exporter",
        "node_filesystem_avail_bytes",
        "node_filesystem_size_bytes",
        "pulsedag_node_uptime_seconds",
        "PulseDAGUnexpectedRestart",
        "PulseDAGSharedStateLockStarvation",
        "PulseDAGSubmitFinalityUnknown",
        "private management/monitoring network",
    )
    require(
        "docs/API_V1.md",
        "Admin is disabled by default for **all** profiles and RPC binds.",
        "request body limit: **128 KiB**",
        "rate limit: **30 requests per 60 seconds**",
        "wildcard CORS origin (`*`): **rejected**",
        "Unsafe overrides are not part of the supported public-testnet baseline",
    )

    config = require(
        "apps/pulsedagd/src/config.rs",
        "PublicSafe",
        '"public_safe" => Ok(Self::PublicSafe)',
        "admin endpoints cannot be enabled",
        "wildcard origin is not allowed",
        "PULSEDAG_RPC_RATE_LIMIT_UNSAFE_ALLOW_DISABLED",
    )
    if "fn default_admin_enabled(_network_profile: &str, _rpc_bind: &str) -> bool {\n    false\n}" not in config:
        fail("admin default is no longer fail-closed")

    routes = require(
        "crates/pulsedag-rpc/src/routes.rs",
        "ApiExposureProfile::PublicSafe",
        "request_body_limit_bytes: 128 * 1024",
        "requests_per_window: 30",
        "public_safe_api_v1_router",
        "public_safe_routes",
    )
    public_safe = function_slice(routes, "fn public_safe_routes")
    for required in ('"/metrics"', '"/status"', '"/mempool"'):
        if required not in public_safe:
            fail(f"public_safe route block lost observability route {required}")
    for forbidden in ("/admin", '"/runtime"', "post_snapshot_create", "post_prune_chain", "post_sync_rebuild"):
        if forbidden in public_safe:
            fail(f"public_safe route block unexpectedly contains {forbidden!r}")

    print("PASS: v2.4.0 public hardening prep remains fail-closed and pre-GO")


if __name__ == "__main__":
    main()
