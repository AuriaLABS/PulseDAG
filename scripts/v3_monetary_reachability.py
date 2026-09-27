#!/usr/bin/env python3
"""Exact-candidate reachability audit for PulseDAG v3 monetary authority."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3-monetary-reachability-evidence.v1"

EXPECTED_LEGACY_DEFINITION = "crates/pulsedag-core/src/validation.rs"
EXPECTED_LEGACY_CALLS = {
    "crates/pulsedag-core/src/validation.rs": 1,
    "crates/pulsedag-core/src/mining_template_v2.rs": 1,
}

V3_SENSITIVE_PATHS = [
    "crates/pulsedag-core/src/monetary_v3.rs",
    "crates/pulsedag-core/src/monetary_audit_v3.rs",
    "crates/pulsedag-core/src/validation_v3.rs",
    "crates/pulsedag-core/src/reward_settlement_v3.rs",
    "crates/pulsedag-core/src/live_reward_settlement_v3.rs",
    "crates/pulsedag-core/src/mining_template_v3.rs",
    "crates/pulsedag-core/src/mined_block_v3.rs",
    "crates/pulsedag-core/src/network_block_v3.rs",
    "crates/pulsedag-core/src/network_runtime_v3.rs",
]

FORBIDDEN_V3_LIVE_MARKERS = [
    "block_subsidy(",
    "INITIAL_BLOCK_SUBSIDY",
    "SUBSIDY_HALVING_INTERVAL",
    "validate_coinbase_reward(",
]

REQUIRED_LIVE_MARKERS = {
    "crates/pulsedag-core/src/mining_template_v3.rs": [
        "build_monetary_mining_template_v3",
        "build_reward_claim_transaction_v3",
        "ordinary transaction {} is inputless",
        "reward claim must remain amountless",
    ],
    "crates/pulsedag-core/src/validation_v3.rs": [
        "HiddenIssuancePath",
        "validate_reward_claim_transaction_v3",
        "max_coinbase_claim_atoms",
    ],
    "crates/pulsedag-core/src/monetary_audit_v3.rs": [
        "audit_monetary_state_v3",
        "hidden_issuance_paths: 0",
        "SupplyMismatch",
    ],
    "crates/pulsedag-core/src/mined_block_v3.rs": [
        "accept_monetary_v3_mined_block_atomically",
        "validate_ordered_monetary_reward_v3",
        "audit_monetary_state_v3",
    ],
    "crates/pulsedag-core/src/network_block_v3.rs": [
        "validate_monetary_v3_p2p_staging_envelope",
        "additional inputless transaction",
        "validate_ordered_monetary_reward_v3",
        "audit_monetary_state_v3",
    ],
    "crates/pulsedag-core/src/network_runtime_v3.rs": [
        "validate_monetary_v3_p2p_runtime_snapshot",
        "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "validate_monetary_v3_p2p_staging_envelope",
        "audit_authoritative_monetary_state",
    ],
    "crates/pulsedag-rpc/src/handlers/monetary_activation_guard.rs": [
        "ensure_legacy_mining_disabled_when_monetary_v3_active",
        "protocol_monetary_activation_record",
        "fails closed",
    ],
    "crates/pulsedag-rpc/src/handlers/mining_template_protocol.rs": [
        "protocol_monetary_activation_record",
        "build_monetary_mining_template_v3",
        "ensure_legacy_mining_disabled_when_monetary_v3_active",
        "/mining/template legacy fallback",
    ],
    "crates/pulsedag-rpc/src/handlers/mining_submit_protocol.rs": [
        "protocol_monetary_activation_record",
        "accept_monetary_v3_mined_block_atomically",
        "ensure_legacy_mining_disabled_when_monetary_v3_active",
        "/mining/submit legacy-v1 fallback",
    ],
    "apps/pulsedagd/src/activated_v2_runtime.rs": [
        "protocol_monetary_activation_record",
        "refusing legacy startup fallback",
        "validate_monetary_v3_p2p_runtime_snapshot",
    ],
}

REQUIRED_REGRESSION_MARKERS = {
    "crates/pulsedag-rpc/src/handlers/monetary_activation_guard.rs":
        "monetary_sidecar_disables_every_legacy_mining_surface",
    "apps/pulsedagd/src/activated_v2_runtime.rs":
        "monetary_sidecar_without_activated_capabilities_refuses_legacy_startup",
    "crates/pulsedag-core/src/network_block_v3.rs":
        "legacy_height_subsidy_block_is_rejected_before_p2p_staging",
    "crates/pulsedag-core/src/mined_block_v3.rs":
        "legacy_height_subsidy_coinbase_is_rejected_under_monetary_activation",
}

TEST_MODULE_RE = re.compile(
    r"(?ms)^\s*#\[cfg\(test\)\]\s*\n\s*mod\s+tests\s*\{"
)
BLOCK_SUBSIDY_RE = re.compile(r"\bblock_subsidy\s*\(")


def production_source(text: str) -> str:
    match = TEST_MODULE_RE.search(text)
    return text[: match.start()] if match else text


def line_for_offset(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def rust_source_paths(root: Path) -> list[Path]:
    paths: list[Path] = []
    for base in ("crates", "apps"):
        top = root / base
        if not top.exists():
            continue
        paths.extend(top.glob("*/src/**/*.rs"))
    return sorted(paths)


def rel(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def read_required(root: Path, path: str) -> str:
    target = root / path
    if not target.is_file():
        raise FileNotFoundError(path)
    return target.read_text(encoding="utf-8")


def audit(root: Path, candidate_sha: str, candidate_tree: str) -> dict:
    errors: list[str] = []
    definitions: list[dict] = []
    calls: list[dict] = []

    for path in rust_source_paths(root):
        text = production_source(path.read_text(encoding="utf-8"))
        rp = rel(root, path)
        for match in BLOCK_SUBSIDY_RE.finditer(text):
            prefix = text[max(0, match.start() - 16):match.start()]
            is_definition = bool(re.search(r"\bfn\s+$", prefix))
            item = {"path": rp, "line": line_for_offset(text, match.start())}
            if is_definition:
                definitions.append(item)
            else:
                calls.append(item)

    if len(definitions) != 1 or definitions[0]["path"] != EXPECTED_LEGACY_DEFINITION:
        errors.append(
            f"legacy block_subsidy definition drift: observed={definitions!r}"
        )

    observed_calls: dict[str, int] = {}
    for item in calls:
        observed_calls[item["path"]] = observed_calls.get(item["path"], 0) + 1
    if observed_calls != EXPECTED_LEGACY_CALLS:
        errors.append(
            "legacy block_subsidy live callsite drift: "
            f"expected={EXPECTED_LEGACY_CALLS!r} observed={observed_calls!r}"
        )

    forbidden_hits: list[dict] = []
    for path in V3_SENSITIVE_PATHS:
        text = production_source(read_required(root, path))
        for marker in FORBIDDEN_V3_LIVE_MARKERS:
            if marker in text:
                forbidden_hits.append({"path": path, "marker": marker})
    if forbidden_hits:
        errors.append(f"v3 live code references legacy reward authority: {forbidden_hits!r}")

    marker_checks: list[dict] = []
    for path, markers in REQUIRED_LIVE_MARKERS.items():
        text = production_source(read_required(root, path))
        missing = [marker for marker in markers if marker not in text]
        marker_checks.append({"path": path, "missing": missing})
        if missing:
            errors.append(f"required v3 reachability markers missing in {path}: {missing!r}")

    regression_checks: list[dict] = []
    for path, marker in REQUIRED_REGRESSION_MARKERS.items():
        text = read_required(root, path)
        present = marker in text
        regression_checks.append({"path": path, "marker": marker, "present": present})
        if not present:
            errors.append(f"required regression marker missing in {path}: {marker}")

    monetary_src = read_required(root, "crates/pulsedag-core/src/monetary_v3.rs")
    fp_match = re.search(
        r'MONETARY_POLICY_FINGERPRINT_V3:\s*&str\s*=\s*"([0-9a-f]{64})"',
        monetary_src,
    )
    if not fp_match:
        errors.append("unable to parse MONETARY_POLICY_FINGERPRINT_V3")
        policy_fingerprint = None
    else:
        policy_fingerprint = fp_match.group(1)

    return {
        "schema": SCHEMA,
        "auditor_version": 1,
        "candidate_sha": candidate_sha,
        "candidate_tree_sha": candidate_tree,
        "monetary_policy_fingerprint": policy_fingerprint,
        "legacy_height_subsidy_authority": {
            "definition": definitions,
            "live_calls": calls,
            "expected_live_call_counts": EXPECTED_LEGACY_CALLS,
            "unexpected_live_calls": [
                item for item in calls
                if item["path"] not in EXPECTED_LEGACY_CALLS
            ],
        },
        "v3_live_forbidden_legacy_authority_hits": forbidden_hits,
        "required_live_guard_checks": marker_checks,
        "required_regression_checks": regression_checks,
        "static_scan_scope": ["crates/*/src/**/*.rs", "apps/*/src/**/*.rs"],
        "claim_boundary": {
            "proves_static_live_authority_shape": True,
            "requires_dynamic_guard_tests": True,
            "selects_production_cadence": False,
            "freezes_network_identity": False,
            "authorizes_launch": False,
        },
        "errors": errors,
        "result": "PASS" if not errors else "FAIL",
    }


def self_test() -> None:
    sample = (
        "fn live() { block_subsidy(1); }\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "  fn test_only() { block_subsidy(2); }\n"
        "}\n"
    )
    prod = production_source(sample)
    assert "block_subsidy(1)" in prod
    assert "block_subsidy(2)" not in prod
    assert line_for_offset("a\nb\nc", 2) == 2
    print("v3 monetary reachability auditor self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--candidate-sha")
    parser.add_argument("--candidate-tree")
    parser.add_argument("--out")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    if not args.candidate_sha or not args.candidate_tree or not args.out:
        parser.error("--candidate-sha, --candidate-tree and --out are required")

    root = Path(args.repo_root).resolve()
    evidence = audit(root, args.candidate_sha, args.candidate_tree)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(evidence, sort_keys=True))
    return 0 if evidence["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
