#!/usr/bin/env python3
"""Exact-candidate reachability audit for PulseDAG v3 monetary authority."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3-monetary-reachability-evidence.v1"
AUDITOR_VERSION = 2

EXPECTED_LEGACY_DEFINITION = "crates/pulsedag-core/src/validation.rs"
EXPECTED_LEGACY_CALLS = {
    "crates/pulsedag-core/src/validation.rs": 1,
    "crates/pulsedag-core/src/mining_template_v2.rs": 1,
}
EXPECTED_LEGACY_IMPORTS = {
    "crates/pulsedag-core/src/lib.rs": 1,
    "crates/pulsedag-core/src/mining_template_v2.rs": 1,
}

V3_SENSITIVE_PATHS = [
    "crates/pulsedag-core/src/monetary_v3.rs",
    "crates/pulsedag-core/src/monetary_audit_v3.rs",
    "crates/pulsedag-core/src/validation_v3.rs",
    "crates/pulsedag-core/src/reward_settlement_v3.rs",
    "crates/pulsedag-core/src/state_replay_v3.rs",
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
    "crates/pulsedag-core/src/state_replay_v3.rs": [
        "rebuild_authoritative_state_v3",
        "validate_reward_claim_transaction_v3",
        "eligible_fees_by_score",
        "settlement_outpoint_v3",
    ],
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

CFG_TEST_ATTR_RE = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]")
IDENT_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
BLOCK_SUBSIDY_IDENT_RE = re.compile(r"\bblock_subsidy\b")
ALIAS_RE = re.compile(r"\bblock_subsidy\s+as\s+([A-Za-z_][A-Za-z0-9_]*)")
LET_BINDING_RE = re.compile(
    r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*block_subsidy\s*;"
)
USE_OR_PUB_USE_RE = re.compile(r"\b(?:pub\s+)?use\b")


def line_for_offset(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _skip_line_comment(text: str, i: int) -> int:
    nl = text.find("\n", i)
    return len(text) if nl < 0 else nl


def _skip_block_comment(text: str, i: int) -> int:
    end = text.find("*/", i + 2)
    return len(text) if end < 0 else end + 2


def _skip_string(text: str, i: int) -> int:
    quote = text[i]
    j = i + 1
    while j < len(text):
        ch = text[j]
        if ch == "\\":
            j += 2
            continue
        if ch == quote:
            return j + 1
        j += 1
    return len(text)


def _skip_raw_string(text: str, i: int) -> int:
    j = i
    if j < len(text) and text[j] in "br":
        j += 1
        if j < len(text) and text[j] in "br":
            j += 1
    if j >= len(text) or text[j] != "#":
        return i
    hashes = 0
    while j < len(text) and text[j] == "#":
        hashes += 1
        j += 1
    if j >= len(text) or text[j] != '"':
        return i
    j += 1
    needle = '"' + ("#" * hashes)
    end = text.find(needle, j)
    return len(text) if end < 0 else end + len(needle)


def _is_raw_string_start(text: str, i: int) -> bool:
    j = i
    if j < len(text) and text[j] in "br":
        j += 1
        if j < len(text) and text[j] in "br":
            j += 1
    return j < len(text) and text[j] == "#" and '"' in text[j:j + 16]


def _skip_ws_and_comments(text: str, i: int) -> int:
    n = len(text)
    while i < n:
        if text[i] in " \t\r\n":
            i += 1
            continue
        if text.startswith("//", i):
            i = _skip_line_comment(text, i)
            continue
        if text.startswith("/*", i):
            i = _skip_block_comment(text, i)
            continue
        break
    return i


def _skip_attribute(text: str, i: int) -> int:
    if i >= len(text) or text[i] != "#":
        return i
    j = i + 1
    j = _skip_ws_and_comments(text, j)
    if j >= len(text) or text[j] != "[":
        return i
    depth = 0
    while j < len(text):
        ch = text[j]
        if text.startswith("//", j):
            j = _skip_line_comment(text, j)
            continue
        if text.startswith("/*", j):
            j = _skip_block_comment(text, j)
            continue
        if ch in "\"'":
            j = _skip_string(text, j)
            continue
        if ch == "[":
            depth += 1
            j += 1
            continue
        if ch == "]":
            depth -= 1
            j += 1
            if depth == 0:
                return j
            continue
        j += 1
    return len(text)


def _skip_matching_braces(text: str, i: int) -> int:
    if i >= len(text) or text[i] != "{":
        return i
    depth = 0
    j = i
    while j < len(text):
        ch = text[j]
        if text.startswith("//", j):
            j = _skip_line_comment(text, j)
            continue
        if text.startswith("/*", j):
            j = _skip_block_comment(text, j)
            continue
        if _is_raw_string_start(text, j):
            nxt = _skip_raw_string(text, j)
            if nxt != j:
                j = nxt
                continue
        if ch in "\"'":
            j = _skip_string(text, j)
            continue
        if ch == "{":
            depth += 1
            j += 1
            continue
        if ch == "}":
            depth -= 1
            j += 1
            if depth == 0:
                return j
            continue
        j += 1
    return len(text)


def _item_end_after_keyword(text: str, i: int) -> int:
    j = _skip_ws_and_comments(text, i)
    while j < len(text):
        ch = text[j]
        if text.startswith("//", j):
            j = _skip_line_comment(text, j)
            continue
        if text.startswith("/*", j):
            j = _skip_block_comment(text, j)
            continue
        if ch == "{":
            return _skip_matching_braces(text, j)
        if ch == ";":
            return j + 1
        j += 1
    return len(text)


def _cfg_test_item_span(text: str, attr_start: int):
    j = _skip_attribute(text, attr_start)
    if j == attr_start:
        return None
    while True:
        j = _skip_ws_and_comments(text, j)
        if j < len(text) and text[j] == "#":
            nxt = _skip_attribute(text, j)
            if nxt == j:
                break
            j = nxt
            continue
        break
    return (attr_start, _item_end_after_keyword(text, j))


def production_source(text: str) -> str:
    """Blank every #[cfg(test)] item; keep later production code and line numbers."""
    chars = list(text)
    for match in CFG_TEST_ATTR_RE.finditer(text):
        span = _cfg_test_item_span(text, match.start())
        if span is None:
            continue
        start, end = span
        for k in range(start, end):
            if chars[k] != "\n":
                chars[k] = " "
    return "".join(chars)


def rust_source_paths(root: Path) -> list:
    paths = []
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


def _preceding_keyword(text: str, offset: int):
    i = offset
    while i > 0 and text[i - 1] in " \t":
        i -= 1
    match = None
    for m in IDENT_RE.finditer(text[:i]):
        match = m
    if match and match.end() == i:
        return match.group(0)
    return None


def _enclosing_statement(text: str, offset: int) -> str:
    i = offset
    depth = 0
    while i > 0:
        i -= 1
        ch = text[i]
        if ch == "}":
            depth += 1
        elif ch == "{":
            depth = max(0, depth - 1)
        elif ch == ";" and depth == 0:
            break
    return text[i:offset]


def classify_block_subsidy_hit(text: str, offset: int) -> str:
    keyword = _preceding_keyword(text, offset)
    after = _skip_ws_and_comments(text, offset + len("block_subsidy"))
    if keyword == "fn":
        return "definition"
    stmt = _enclosing_statement(text, offset)
    if USE_OR_PUB_USE_RE.search(stmt):
        tail = text[after:after + 64]
        if re.match(r"as\s+[A-Za-z_]", tail):
            return "alias"
        return "import"
    if after < len(text) and text[after] == "(":
        return "call"
    if after < len(text) and text.startswith("as", after) and (
        after + 2 == len(text) or not (text[after + 2].isalnum() or text[after + 2] == "_")
    ):
        return "alias"
    return "other"


def file_alias_names(text: str):
    names = set(ALIAS_RE.findall(text))
    names.update(LET_BINDING_RE.findall(text))
    return names


def alias_call_hits(text: str, names):
    hits = []
    for name in names:
        call_re = re.compile(rf"\b{re.escape(name)}\s*\(")
        for match in call_re.finditer(text):
            hits.append(match.start())
    return hits


def audit(root: Path, candidate_sha: str, candidate_tree: str) -> dict:
    errors = []
    definitions = []
    calls = []
    imports = []
    aliases = []
    other_refs = []

    for path in rust_source_paths(root):
        raw = path.read_text(encoding="utf-8")
        text = production_source(raw)
        rp = rel(root, path)
        for match in BLOCK_SUBSIDY_IDENT_RE.finditer(text):
            kind = classify_block_subsidy_hit(text, match.start())
            item = {"path": rp, "line": line_for_offset(text, match.start()), "kind": kind}
            if kind == "definition":
                definitions.append(item)
            elif kind == "call":
                calls.append(item)
            elif kind == "import":
                imports.append(item)
            elif kind == "alias":
                aliases.append(item)
            else:
                other_refs.append(item)
        for offset in alias_call_hits(text, file_alias_names(text)):
            item = {"path": rp, "line": line_for_offset(text, offset), "kind": "aliased_call"}
            calls.append(item)
            aliases.append(item)

    if len(definitions) != 1 or definitions[0]["path"] != EXPECTED_LEGACY_DEFINITION:
        errors.append(f"legacy block_subsidy definition drift: observed={definitions!r}")

    observed_calls = {}
    for item in calls:
        observed_calls[item["path"]] = observed_calls.get(item["path"], 0) + 1
    if observed_calls != EXPECTED_LEGACY_CALLS:
        errors.append(
            "legacy block_subsidy live callsite drift: "
            f"expected={EXPECTED_LEGACY_CALLS!r} observed={observed_calls!r}"
        )

    observed_imports = {}
    for item in imports:
        observed_imports[item["path"]] = observed_imports.get(item["path"], 0) + 1
    if observed_imports != EXPECTED_LEGACY_IMPORTS:
        errors.append(
            "legacy block_subsidy live import/reexport drift: "
            f"expected={EXPECTED_LEGACY_IMPORTS!r} observed={observed_imports!r}"
        )

    if aliases:
        errors.append(f"legacy block_subsidy aliases are forbidden in live code: {aliases!r}")
    if other_refs:
        errors.append(
            "legacy block_subsidy live function-value/other references: "
            f"{other_refs!r}"
        )

    forbidden_hits = []
    for path in V3_SENSITIVE_PATHS:
        text = production_source(read_required(root, path))
        names = file_alias_names(text) | {"block_subsidy"}
        for marker in FORBIDDEN_V3_LIVE_MARKERS:
            if marker in text:
                forbidden_hits.append({"path": path, "marker": marker})
        for name in names:
            if name == "block_subsidy":
                continue
            if re.search(rf"\b{re.escape(name)}\s*\(", text):
                forbidden_hits.append({"path": path, "marker": f"{name}("})
    if forbidden_hits:
        errors.append(f"v3 live code references legacy reward authority: {forbidden_hits!r}")

    marker_checks = []
    for path, markers in REQUIRED_LIVE_MARKERS.items():
        text = production_source(read_required(root, path))
        missing = [marker for marker in markers if marker not in text]
        marker_checks.append({"path": path, "missing": missing})
        if missing:
            errors.append(f"required v3 reachability markers missing in {path}: {missing!r}")

    regression_checks = []
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
        "auditor_version": AUDITOR_VERSION,
        "candidate_sha": candidate_sha,
        "candidate_tree_sha": candidate_tree,
        "monetary_policy_fingerprint": policy_fingerprint,
        "legacy_height_subsidy_authority": {
            "definition": definitions,
            "live_calls": calls,
            "live_imports": imports,
            "live_aliases": aliases,
            "live_other_references": other_refs,
            "expected_live_call_counts": EXPECTED_LEGACY_CALLS,
            "expected_live_import_counts": EXPECTED_LEGACY_IMPORTS,
            "unexpected_live_calls": [
                item for item in calls if item["path"] not in EXPECTED_LEGACY_CALLS
            ],
        },
        "v3_live_forbidden_legacy_authority_hits": forbidden_hits,
        "required_live_guard_checks": marker_checks,
        "required_regression_checks": regression_checks,
        "static_scan_scope": ["crates/*/src/**/*.rs", "apps/*/src/**/*.rs"],
        "claim_boundary": {
            "proves_static_live_authority_shape": True,
            "requires_dynamic_guard_tests": True,
            "strips_all_cfg_test_items": True,
            "tracks_legacy_aliases": True,
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
        "fn later_live() { block_subsidy(3); }\n"
        "#[cfg(test)]\n"
        "fn test_helper() { block_subsidy(4); }\n"
        "fn after_helper() { block_subsidy(5); }\n"
    )
    prod = production_source(sample)
    assert "block_subsidy(1)" in prod
    assert "block_subsidy(2)" not in prod
    assert "block_subsidy(3)" in prod
    assert "block_subsidy(4)" not in prod
    assert "block_subsidy(5)" in prod
    assert line_for_offset("a\nb\nc", 2) == 2

    alias_sample = (
        "use crate::block_subsidy as legacy_subsidy;\n"
        "fn live() { legacy_subsidy(1); }\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "  use crate::block_subsidy as ignored;\n"
        "  fn t() { ignored(2); }\n"
        "}\n"
    )
    alias_prod = production_source(alias_sample)
    assert file_alias_names(alias_prod) == {"legacy_subsidy"}
    assert classify_block_subsidy_hit(alias_prod, alias_prod.index("block_subsidy")) == "alias"
    assert alias_call_hits(alias_prod, {"legacy_subsidy"})

    ptr_sample = "fn live() { let minted = block_subsidy; minted(1); }\n"
    assert classify_block_subsidy_hit(ptr_sample, ptr_sample.index("block_subsidy")) == "other"
    assert file_alias_names(ptr_sample) == {"minted"}
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
