#!/usr/bin/env python3
"""Exact-candidate reachability audit for PulseDAG v3 monetary authority."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3-monetary-reachability-evidence.v1"
AUDITOR_VERSION = 16

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

REQUIRED_LIVE_CALLS = {
    "crates/pulsedag-core/src/state_replay_v3.rs": [
        "validate_reward_claim_transaction_v3",
        "settlement_outpoint_v3",
    ],
    "crates/pulsedag-core/src/mining_template_v3.rs": [
        "build_reward_claim_transaction_v3",
        "validate_reward_claim_transaction_v3",
    ],
    "crates/pulsedag-core/src/validation_v3.rs": [
        "validate_reward_claim_transaction_v3",
        "max_coinbase_claim_atoms",
    ],
    "crates/pulsedag-core/src/mined_block_v3.rs": [
        "validate_ordered_monetary_reward_v3",
        "audit_monetary_state_v3",
        "accept_activated_v2_mined_block_atomically",
    ],
    "crates/pulsedag-core/src/network_block_v3.rs": [
        "validate_monetary_v3_p2p_staging_envelope",
        "validate_ordered_monetary_reward_v3",
        "audit_monetary_state_v3",
        "validate_live_reward_settlement_v3",
    ],
    "crates/pulsedag-core/src/network_runtime_v3.rs": [
        "validate_monetary_v3_p2p_staging_envelope",
        "audit_authoritative_monetary_state",
        "drive_activated_v2_p2p_block_with_runtime_persistence",
    ],
    "crates/pulsedag-rpc/src/handlers/monetary_activation_guard.rs": [
        "protocol_monetary_activation_record",
    ],
    "crates/pulsedag-rpc/src/handlers/mining_template_protocol.rs": [
        "protocol_monetary_activation_record",
        "build_monetary_mining_template_v3",
        "ensure_legacy_mining_disabled_when_monetary_v3_active",
    ],
    "crates/pulsedag-rpc/src/handlers/mining_submit_protocol.rs": [
        "protocol_monetary_activation_record",
        "accept_monetary_v3_mined_block_atomically",
        "ensure_legacy_mining_disabled_when_monetary_v3_active",
    ],
    "apps/pulsedagd/src/activated_v2_runtime.rs": [
        "protocol_monetary_activation_record",
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

CFG_ATTR_RE = re.compile(r"#\[\s*cfg\s*\(")
CFG_ATTR_ATTR_RE = re.compile(r"#\[\s*cfg_attr\s*\(")
INNER_CFG_ATTR_RE = re.compile(r"#!\[\s*cfg\s*\(")
INNER_CFG_ATTR_ATTR_RE = re.compile(r"#!\[\s*cfg_attr\s*\(")

REQUIRED_LOCAL_DEFINITIONS = {
    "crates/pulsedag-core/src/network_block_v3.rs": {
        "validate_monetary_v3_p2p_staging_envelope",
    },
    "crates/pulsedag-core/src/network_runtime_v3.rs": {
        "audit_authoritative_monetary_state",
    },
}

REQUIRED_LOCAL_CALLS = {
    "crates/pulsedag-core/src/network_runtime_v3.rs": {
        "audit_authoritative_monetary_state": [
            "crate::audit_monetary_state_v3",
            "crate::validate_live_reward_settlement_v3",
        ],
    },
}

REQUIRED_RUNTIME_PERSISTENCE_CALLBACKS = {
    "crates/pulsedag-core/src/network_runtime_v3.rs": {
        "function": "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "constructor": "ActivatedV2P2pRuntimePersistence::new",
        "audit": "audit_authoritative_monetary_state",
        "persist": ["persist_runtime", "persist_one", "persist_bundle"],
        "expected_bodies": [
            """
            validate_runtime_transient_monetary_envelopes(
                prepared_state,
                prepared_runtime,
                identity,
            )?;
            audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
            persist_runtime(prepared_state, prepared_runtime)
            """,
            """
            validate_runtime_transient_monetary_envelopes(
                prepared_state,
                prepared_runtime,
                identity,
            )?;
            validate_runtime_accepted_block_reward(
                prepared_state,
                accepted_block,
                cadence_segments,
            )?;
            audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
            persist_one(accepted_block, prepared_state, prepared_runtime)
            """,
            """
            validate_runtime_transient_monetary_envelopes(
                prepared_state,
                prepared_runtime,
                identity,
            )?;
            validate_runtime_promoted_bundle_rewards(
                prepared_state,
                bundle,
                cadence_segments,
            )?;
            audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
            persist_bundle(bundle, prepared_state, prepared_runtime)
            """,
        ],
    },
}
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
    depth = 1
    j = i + 2
    while j < len(text):
        if text.startswith("/*", j):
            depth += 1
            j += 2
            continue
        if text.startswith("*/", j):
            depth -= 1
            j += 2
            if depth == 0:
                return j
            continue
        j += 1
    return len(text)


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


def _skip_char_literal(text: str, i: int) -> int:
    """Return the end of a Rust char/byte-char literal, or i for a lifetime."""
    start = i
    if i < len(text) and text[i] == "b":
        if i + 1 >= len(text) or text[i + 1] != "'":
            return start
        i += 1
    if i >= len(text) or text[i] != "'":
        return start

    j = i + 1
    if j >= len(text) or text[j] in "\r\n'":
        return start

    if text[j] == "\\":
        j += 1
        if j >= len(text) or text[j] in "\r\n":
            return start
        if text[j] == "u" and j + 1 < len(text) and text[j + 1] == "{":
            close = text.find("}", j + 2)
            if close < 0:
                return start
            j = close + 1
        elif text[j] == "x":
            j += 3
        else:
            j += 1
    else:
        j += 1

    return j + 1 if j < len(text) and text[j] == "'" else start


def _skip_raw_string(text: str, i: int) -> int:
    j = i
    if j < len(text) and text[j] == "b":
        j += 1
    if j >= len(text) or text[j] != "r":
        return i
    j += 1
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
    return _skip_raw_string(text, i) != i


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
    if j < len(text) and text[j] == "!":
        j += 1
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
        if _is_raw_string_start(text, j):
            nxt = _skip_raw_string(text, j)
            if nxt != j:
                j = nxt
                continue
        if ch == '"':
            j = _skip_string(text, j)
            continue
        if ch == "'" or (ch == "b" and j + 1 < len(text) and text[j + 1] == "'"):
            nxt = _skip_char_literal(text, j)
            if nxt != j:
                j = nxt
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
        if ch == '"':
            j = _skip_string(text, j)
            continue
        if ch == "'" or (ch == "b" and j + 1 < len(text) and text[j + 1] == "'"):
            nxt = _skip_char_literal(text, j)
            if nxt != j:
                j = nxt
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


def _looks_like_generic_start(text: str, i: int) -> bool:
    if i <= 0 or text[i] != "<":
        return False

    prev_i = i - 1
    while prev_i >= 0 and text[prev_i].isspace():
        prev_i -= 1
    if prev_i < 0:
        return False

    prev = text[prev_i]
    if prev.isalnum() or prev == "_":
        start = prev_i
        while start > 0 and (text[start - 1].isalnum() or text[start - 1] == "_"):
            start -= 1
        token = text[start : prev_i + 1]
        if token and token[0].isdigit():
            return False
    elif prev not in ":)>]":
        return False

    nxt = i + 1
    while nxt < len(text) and text[nxt].isspace():
        nxt += 1
    if nxt >= len(text) or text[nxt] in "=<":
        return False
    return True


def _item_end_after_keyword(text: str, i: int) -> int:
    j = _skip_ws_and_comments(text, i)
    paren_depth = 0
    bracket_depth = 0
    angle_depth = 0
    in_top_level_initializer = False
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
        if ch == '"':
            j = _skip_string(text, j)
            continue
        if ch == "'" or (ch == "b" and j + 1 < len(text) and text[j + 1] == "'"):
            nxt = _skip_char_literal(text, j)
            if nxt != j:
                j = nxt
                continue
        if ch == "(":
            paren_depth += 1
            j += 1
            continue
        if ch == ")":
            paren_depth = max(0, paren_depth - 1)
            j += 1
            continue
        if ch == "[":
            bracket_depth += 1
            j += 1
            continue
        if ch == "]":
            bracket_depth = max(0, bracket_depth - 1)
            j += 1
            continue
        at_structural_level = paren_depth == 0 and bracket_depth == 0 and angle_depth == 0
        if ch == "=" and at_structural_level:
            in_top_level_initializer = True
            j += 1
            continue
        if ch == "<" and not in_top_level_initializer and _looks_like_generic_start(text, j):
            angle_depth += 1
            j += 1
            continue
        if ch == ">" and angle_depth:
            angle_depth -= 1
            j += 1
            continue
        at_item_level = paren_depth == 0 and bracket_depth == 0 and angle_depth == 0
        if ch == "{" and at_item_level:
            end = _skip_matching_braces(text, j)
            tail = _skip_ws_and_comments(text, end)
            return tail + 1 if tail < len(text) and text[tail] == "," else end
        if ch == ";" and at_item_level:
            return j + 1
        if ch == "," and at_item_level:
            return j + 1
        j += 1
    return len(text)


CFG_FALSE = -1
CFG_UNKNOWN = 0
CFG_TRUE = 1


def _cfg_not(value: int) -> int:
    if value == CFG_TRUE:
        return CFG_FALSE
    if value == CFG_FALSE:
        return CFG_TRUE
    return CFG_UNKNOWN


class _CfgParser:
    """Conservative Rust cfg evaluator for predicates decidable across production builds."""

    def __init__(self, text: str):
        self.text = text
        self.i = 0

    def _ws(self) -> None:
        while self.i < len(self.text):
            if self.text[self.i].isspace():
                self.i += 1
                continue
            if self.text.startswith("//", self.i):
                self.i = _skip_line_comment(self.text, self.i)
                continue
            if self.text.startswith("/*", self.i):
                self.i = _skip_block_comment(self.text, self.i)
                continue
            break

    def _ident(self):
        self._ws()
        match = IDENT_RE.match(self.text, self.i)
        if not match:
            return None
        self.i = match.end()
        return match.group(0)

    def _skip_value(self) -> None:
        self._ws()
        if self.i >= len(self.text):
            return
        if _is_raw_string_start(self.text, self.i):
            self.i = _skip_raw_string(self.text, self.i)
            return
        if self.text[self.i] == '"':
            self.i = _skip_string(self.text, self.i)
            return
        if self.text[self.i] == "'" or (
            self.text[self.i] == "b"
            and self.i + 1 < len(self.text)
            and self.text[self.i + 1] == "'"
        ):
            nxt = _skip_char_literal(self.text, self.i)
            if nxt != self.i:
                self.i = nxt
                return
        while self.i < len(self.text) and self.text[self.i] not in ",)":
            self.i += 1

    def expr(self) -> int:
        name = self._ident()
        if name is None:
            self._skip_value()
            return CFG_UNKNOWN

        self._ws()
        if self.i < len(self.text) and self.text[self.i] == "=":
            self.i += 1
            self._skip_value()
            return CFG_UNKNOWN

        if self.i >= len(self.text) or self.text[self.i] != "(":
            # `test` is never live in the production scan. Other atoms may be
            # enabled by a target/feature/custom --cfg, so keep them potential-live.
            return CFG_FALSE if name == "test" else CFG_UNKNOWN

        self.i += 1
        values = []
        while True:
            self._ws()
            if self.i >= len(self.text):
                return CFG_UNKNOWN
            if self.text[self.i] == ")":
                self.i += 1
                break
            values.append(self.expr())
            self._ws()
            if self.i < len(self.text) and self.text[self.i] == ",":
                self.i += 1
                continue
            if self.i < len(self.text) and self.text[self.i] == ")":
                self.i += 1
                break
            return CFG_UNKNOWN

        if name == "not":
            return _cfg_not(values[0]) if len(values) == 1 else CFG_UNKNOWN
        if name == "all":
            if any(value == CFG_FALSE for value in values):
                return CFG_FALSE
            if all(value == CFG_TRUE for value in values):
                return CFG_TRUE
            return CFG_UNKNOWN
        if name == "any":
            if any(value == CFG_TRUE for value in values):
                return CFG_TRUE
            if all(value == CFG_FALSE for value in values):
                return CFG_FALSE
            return CFG_UNKNOWN
        return CFG_UNKNOWN


def evaluate_cfg_predicate(predicate: str) -> int:
    if _cfg_predicate_contradictory(predicate):
        return CFG_FALSE
    parser = _CfgParser(predicate)
    value = parser.expr()
    parser._ws()
    return value if parser.i == len(predicate) else CFG_UNKNOWN


def _attribute_call_bounds(lexed: str, match) -> tuple:
    open_i = match.end() - 1
    end = _skip_matching_delimiter(lexed, open_i)
    if end <= open_i or end > len(lexed):
        return (open_i, open_i)
    return (open_i + 1, end - 1)


def _split_top_level_args(text: str) -> list:
    lexed = code_source(text)
    args = []
    start = 0
    stack = []
    pairs = {"(": ")", "[": "]", "{": "}"}
    closers = set(pairs.values())
    for i, ch in enumerate(lexed):
        if ch in pairs:
            stack.append(ch)
        elif ch in closers:
            if stack and pairs[stack[-1]] == ch:
                stack.pop()
        elif ch == "," and not stack:
            args.append(text[start:i].strip())
            start = i + 1
    args.append(text[start:].strip())
    return args


def _cfg_call_parts(expr: str):
    stripped = expr.strip()
    match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(", stripped)
    if not match:
        return None
    lexed = code_source(stripped)
    open_i = match.end() - 1
    end = _skip_matching_delimiter(lexed, open_i)
    if end <= open_i or stripped[end:].strip():
        return None
    return (match.group(1), stripped[open_i + 1 : end - 1])


def _cfg_literal_signature(expr: str):
    parts = _cfg_call_parts(expr)
    if parts is not None:
        name, payload = parts
        if name != "not":
            return None
        args = _split_top_level_args(payload)
        if len(args) != 1:
            return None
        inner = _cfg_literal_signature(args[0])
        if inner is None:
            return None
        key, positive = inner
        return (key, not positive)

    key = re.sub(r"\s+", "", expr.strip())
    return (key, True) if key else None


def _cfg_conjunction_literals(expr: str):
    parts = _cfg_call_parts(expr)
    if parts is not None and parts[0] == "all":
        positives = set()
        negatives = set()
        contradiction = False
        for arg in _split_top_level_args(parts[1]):
            pos, neg, child_contradiction = _cfg_conjunction_literals(arg)
            positives.update(pos)
            negatives.update(neg)
            contradiction = contradiction or child_contradiction
        contradiction = contradiction or bool(positives & negatives)
        return (positives, negatives, contradiction)

    literal = _cfg_literal_signature(expr)
    if literal is None:
        return (set(), set(), False)
    key, positive = literal
    return ({key}, set(), False) if positive else (set(), {key}, False)


def _cfg_predicate_contradictory(predicate: str) -> bool:
    positives, negatives, contradiction = _cfg_conjunction_literals(predicate)
    return contradiction or bool(positives & negatives)


def _cfg_combine_conditions(left: str, right: str) -> str:
    if evaluate_cfg_predicate(left) == CFG_TRUE:
        return right
    if evaluate_cfg_predicate(right) == CFG_TRUE:
        return left
    return f"all({left},{right})"


def _cfg_meta_rules(meta: str, parent_condition: str = "all()") -> list:
    """Return (condition, predicate) rules for cfgs emitted by metadata."""
    stripped = meta.strip()
    cfg_match = re.match(r"cfg\s*\(", stripped)
    if cfg_match:
        lexed = code_source(stripped)
        open_i = cfg_match.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return []
        predicate = stripped[open_i + 1 : end - 1]
        return [(parent_condition, predicate)]

    cfg_attr_match = re.match(r"cfg_attr\s*\(", stripped)
    if cfg_attr_match:
        lexed = code_source(stripped)
        open_i = cfg_attr_match.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return []
        args = _split_top_level_args(stripped[open_i + 1 : end - 1])
        if len(args) < 2:
            return []
        condition = _cfg_combine_conditions(parent_condition, args[0])
        rules = []
        for nested in args[1:]:
            rules.extend(_cfg_meta_rules(nested, condition))
        return rules

    return []


def _cfg_attribute_constraints(attr_text: str):
    """Return (direct predicates, conditional emitted-cfg rules) for one attribute."""
    stripped = attr_text.strip()
    direct = re.match(r"#\s*!?\s*\[\s*cfg\s*\(", stripped)
    if direct:
        lexed = code_source(stripped)
        open_i = direct.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return ([], [])
        return ([stripped[open_i + 1 : end - 1]], [])

    conditional = re.match(r"#\s*!?\s*\[\s*cfg_attr\s*\(", stripped)
    if conditional:
        lexed = code_source(stripped)
        open_i = conditional.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return ([], [])
        args = _split_top_level_args(stripped[open_i + 1 : end - 1])
        if len(args) < 2:
            return ([], [])
        rules = []
        for meta in args[1:]:
            rules.extend(_cfg_meta_rules(meta, args[0]))
        return ([], rules)

    return ([], [])


def _cfg_formula_atoms(expr: str) -> set:
    parts = _cfg_call_parts(expr)
    if parts is not None:
        name, payload = parts
        args = [arg for arg in _split_top_level_args(payload) if arg]
        if name in {"all", "any"}:
            atoms = set()
            for arg in args:
                atoms.update(_cfg_formula_atoms(arg))
            return atoms
        if name == "not" and len(args) == 1:
            return _cfg_formula_atoms(args[0])

    key = re.sub(r"\s+", "", expr.strip())
    if not key or key == "test":
        return set()
    return {key}


def _cfg_formula_eval(expr: str, assignment: dict) -> bool:
    parts = _cfg_call_parts(expr)
    if parts is not None:
        name, payload = parts
        args = [arg for arg in _split_top_level_args(payload) if arg]
        if name == "all":
            return all(_cfg_formula_eval(arg, assignment) for arg in args)
        if name == "any":
            return any(_cfg_formula_eval(arg, assignment) for arg in args)
        if name == "not" and len(args) == 1:
            return not _cfg_formula_eval(args[0], assignment)

    key = re.sub(r"\s+", "", expr.strip())
    if key == "test":
        return False
    return assignment.get(key, False)


def _cfg_formula_satisfiable(formulas: list) -> bool:
    """Return whether the conservative production cfg formula has any assignment."""
    atoms = sorted(
        {
            atom
            for formula in formulas
            for atom in _cfg_formula_atoms(formula)
        }
    )
    # Keep the static audit fail-closed against performance abuse: beyond this
    # small exact domain, retain the item instead of claiming it is unreachable.
    if len(atoms) > 16:
        return True

    for mask in range(1 << len(atoms)):
        assignment = {
            atom: bool(mask & (1 << index))
            for index, atom in enumerate(atoms)
        }
        if all(_cfg_formula_eval(formula, assignment) for formula in formulas):
            return True
    return False


def _cfg_constraints_prove_false(predicates: list, rules: list) -> bool:
    """Prove an item unreachable from direct cfgs plus cfg_attr implications."""
    formulas = [predicate for predicate in predicates if predicate.strip()]
    for condition, predicate in rules:
        # cfg_attr(condition, cfg(predicate)) means the emitted predicate is
        # required exactly on builds satisfying condition: condition -> predicate.
        formulas.append(f"any(not({condition}),{predicate})")

    if not formulas:
        return False
    if any(evaluate_cfg_predicate(formula) == CFG_FALSE for formula in formulas):
        return True
    return not _cfg_formula_satisfiable(formulas)


def _cfg_chain_constraints(text: str, attr_start: int):
    predicates = []
    rules = []
    j = attr_start
    while True:
        j = _skip_ws_and_comments(text, j)
        if j >= len(text) or text[j] != "#":
            break
        end = _skip_attribute(text, j)
        if end <= j:
            break
        direct, conditional = _cfg_attribute_constraints(text[j:end])
        predicates.extend(direct)
        rules.extend(conditional)
        j = end
    return (predicates, rules)


def _chained_cfg_attributes_contradict(text: str, attr_start: int) -> bool:
    predicates, rules = _cfg_chain_constraints(text, attr_start)
    return _cfg_constraints_prove_false(predicates, rules)


def _meta_cfg_value(meta: str) -> int:
    stripped = meta.strip()
    if not stripped.startswith("cfg"):
        return CFG_UNKNOWN
    match = re.match(r"cfg\s*\(", stripped)
    if not match:
        return CFG_UNKNOWN
    lexed = code_source(stripped)
    open_i = match.end() - 1
    end = _skip_matching_delimiter(lexed, open_i)
    if end <= open_i:
        return CFG_UNKNOWN
    return evaluate_cfg_predicate(stripped[open_i + 1 : end - 1])


def _meta_disables_item(meta: str) -> bool:
    stripped = meta.strip()
    cfg_match = re.match(r"cfg\s*\(", stripped)
    if cfg_match:
        lexed = code_source(stripped)
        open_i = cfg_match.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return False
        return evaluate_cfg_predicate(stripped[open_i + 1 : end - 1]) == CFG_FALSE

    cfg_attr_match = re.match(r"cfg_attr\s*\(", stripped)
    if cfg_attr_match:
        lexed = code_source(stripped)
        open_i = cfg_attr_match.end() - 1
        end = _skip_matching_delimiter(lexed, open_i)
        if end <= open_i:
            return False
        return _cfg_attr_disables_item(stripped[open_i + 1 : end - 1])

    return False


def _cfg_attr_disables_item(attr_payload: str) -> bool:
    args = _split_top_level_args(attr_payload)
    if len(args) < 2:
        return False
    rules = []
    for meta in args[1:]:
        rules.extend(_cfg_meta_rules(meta, args[0]))
    return _cfg_constraints_prove_false([], rules)


def _cfg_item_span(text: str, attr_start: int):
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


def _inner_cfg_disabled_scopes(text: str, lexed: str) -> list:
    """Combine every inner cfg/cfg_attr applying to the same enclosing scope."""
    groups = {}
    seen = set()
    for regex in (INNER_CFG_ATTR_RE, INNER_CFG_ATTR_ATTR_RE):
        for match in regex.finditer(lexed):
            start = match.start()
            if start in seen:
                continue
            seen.add(start)
            end = _skip_attribute(text, start)
            if end <= start:
                continue
            brace = _enclosing_brace_open(lexed, start)
            key = -1 if brace is None else brace
            predicates, rules = groups.setdefault(key, ([], []))
            direct, conditional = _cfg_attribute_constraints(text[start:end])
            predicates.extend(direct)
            rules.extend(conditional)

    scopes = []
    for brace, (predicates, rules) in groups.items():
        if not _cfg_constraints_prove_false(predicates, rules):
            continue
        if brace == -1:
            scopes.append((0, len(text)))
            continue
        end = _skip_matching_braces(lexed, brace)
        if end > brace:
            scopes.append((brace + 1, max(brace + 1, end - 1)))
    return scopes


def production_source(text: str) -> str:
    """Blank items provably disabled in every production build; preserve potential live code."""
    chars = list(text)
    lexed = macro_opaque_source(text)
    disabled_starts = []

    disabled_scopes = _inner_cfg_disabled_scopes(text, lexed)

    for match in CFG_ATTR_RE.finditer(lexed):
        pred_start, pred_end = _attribute_call_bounds(lexed, match)
        if pred_end <= pred_start:
            continue
        if (
            evaluate_cfg_predicate(text[pred_start:pred_end]) == CFG_FALSE
            or _chained_cfg_attributes_contradict(text, match.start())
        ):
            disabled_starts.append(match.start())

    for match in CFG_ATTR_ATTR_RE.finditer(lexed):
        payload_start, payload_end = _attribute_call_bounds(lexed, match)
        if payload_end <= payload_start:
            continue
        if (
            _cfg_attr_disables_item(text[payload_start:payload_end])
            or _chained_cfg_attributes_contradict(text, match.start())
        ):
            disabled_starts.append(match.start())

    for attr_start in sorted(set(disabled_starts)):
        span = _cfg_item_span(text, attr_start)
        if span is None:
            continue
        start, item_end = span
        for k in range(start, item_end):
            if chars[k] != "\n":
                chars[k] = " "

    for start, end in disabled_scopes:
        for k in range(start, end):
            if chars[k] != "\n":
                chars[k] = " "
    return "".join(chars)


def code_source(text: str) -> str:
    """Blank comments and Rust string/char literals while preserving offsets."""
    chars = list(text)
    i = 0
    while i < len(text):
        if text.startswith("//", i):
            end = _skip_line_comment(text, i)
        elif text.startswith("/*", i):
            end = _skip_block_comment(text, i)
        elif _is_raw_string_start(text, i):
            end = _skip_raw_string(text, i)
            if end == i:
                i += 1
                continue
        elif text[i] == '"':
            end = _skip_string(text, i)
        elif text[i] == "'" or (
            text[i] == "b" and i + 1 < len(text) and text[i + 1] == "'"
        ):
            end = _skip_char_literal(text, i)
            if end == i:
                i += 1
                continue
        else:
            i += 1
            continue
        for k in range(i, end):
            if chars[k] != "\n":
                chars[k] = " "
        i = end
    return "".join(chars)


def _skip_matching_delimiter(text: str, i: int) -> int:
    pairs = {"(": ")", "[": "]", "{": "}"}
    closers = set(pairs.values())
    if i >= len(text) or text[i] not in pairs:
        return i

    stack = [text[i]]
    j = i + 1
    while j < len(text):
        ch = text[j]
        if ch in pairs:
            stack.append(ch)
        elif ch in closers:
            if not stack or pairs[stack[-1]] != ch:
                return len(text)
            stack.pop()
            if not stack:
                return j + 1
        j += 1
    return len(text)


def macro_opaque_source(text: str) -> str:
    """Blank comments/literals and opaque macro token-tree interiors."""
    source = code_source(text)
    chars = list(source)
    macro_re = re.compile(
        r"\b(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*"
        r"[A-Za-z_][A-Za-z0-9_]*!\s*([\(\[\{])"
    )
    for match in macro_re.finditer(source):
        open_i = match.start(1)
        end = _skip_matching_delimiter(source, open_i)
        if end <= open_i:
            continue
        for k in range(open_i + 1, max(open_i + 1, end - 1)):
            if chars[k] != "\n":
                chars[k] = " "
    return "".join(chars)


def executable_source(text: str) -> str:
    """Blank non-executable comments/literals/macros/attributes, preserving offsets."""
    source = macro_opaque_source(text)
    chars = list(source)
    i = 0
    while i < len(source):
        if source[i] == "#":
            end = _skip_attribute(source, i)
            if end > i:
                for k in range(i, end):
                    if chars[k] != "\n":
                        chars[k] = " "
                i = end
                continue
        i += 1
    return "".join(chars)


def _enclosing_brace_open(text: str, offset: int):
    stack = []
    for i, ch in enumerate(text[:offset]):
        if ch == "{":
            stack.append(i)
        elif ch == "}" and stack:
            stack.pop()
    return stack[-1] if stack else None


def _is_enum_variant_declaration(text: str, offset: int) -> bool:
    brace = _enclosing_brace_open(text, offset)
    if brace is None:
        return False
    header = text[max(0, brace - 512) : brace]
    if not re.search(r"\benum\b[^{};]*$", header):
        return False
    i = offset
    while i > brace and text[i - 1].isspace():
        i -= 1
    return i == brace + 1 or (i > brace and text[i - 1] == ",")


def live_call_offsets(text: str, name: str) -> list:
    """Return direct live call-expression offsets outside opaque macro token trees."""
    source = executable_source(text)
    call_re = re.compile(rf"\b{re.escape(name)}\s*\(")
    offsets = []
    for match in call_re.finditer(source):
        if _preceding_keyword(source, match.start()) in {"fn", "struct"}:
            continue
        if _is_enum_variant_declaration(source, match.start()):
            continue
        offsets.append(match.start())
    return offsets


def has_live_call(text: str, name: str) -> bool:
    return bool(live_call_offsets(text, name))


def has_live_qualified_call(text: str, path: str) -> bool:
    """Require an exact qualified Rust call path such as crate::foo(...)."""
    source = executable_source(text)
    parts = path.split("::")
    if not parts or any(not IDENT_RE.fullmatch(part) for part in parts):
        return False
    qualified = r"\s*::\s*".join(re.escape(part) for part in parts)
    return bool(re.search(rf"(?<![A-Za-z0-9_]){qualified}\s*\(", source))


def direct_top_level_call_offsets(text: str, path: str) -> list:
    """Return direct top-level statement calls to an exact Rust path."""
    source = executable_source(text)
    parts = path.split("::")
    if not parts or any(not IDENT_RE.fullmatch(part) for part in parts):
        return []
    qualified = r"\s*::\s*".join(re.escape(part) for part in parts)
    call_re = re.compile(rf"(?<![A-Za-z0-9_]){qualified}\s*\(")
    offsets = []

    for match in call_re.finditer(source):
        if _brace_depth_at(source, match.start()) != 0:
            continue
        statement_start = 0
        depth = 0
        for i, ch in enumerate(source[: match.start()]):
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth = max(0, depth - 1)
                if depth == 0:
                    statement_start = i + 1
            elif ch == ";" and depth == 0:
                statement_start = i + 1
        if source[statement_start : match.start()].strip():
            continue
        offsets.append(match.start())
    return offsets


def has_direct_top_level_qualified_call(text: str, path: str) -> bool:
    """Require an unconditional exact-path call as a direct top-level statement."""
    return bool(direct_top_level_call_offsets(text, path))


def direct_top_level_propagating_call_offsets(text: str, path: str) -> list:
    """Return direct top-level calls whose Result is immediately propagated with ?;."""
    source = executable_source(text)
    direct = set(direct_top_level_call_offsets(source, path))
    if not direct:
        return []
    parts = path.split("::")
    if not parts or any(not IDENT_RE.fullmatch(part) for part in parts):
        return []
    qualified = r"\s*::\s*".join(re.escape(part) for part in parts)
    call_re = re.compile(rf"(?<![A-Za-z0-9_]){qualified}\s*\(")
    offsets = []
    for match in call_re.finditer(source):
        if match.start() not in direct:
            continue
        open_i = source.find("(", match.start(), match.end())
        end = _skip_matching_delimiter(source, open_i)
        if end <= open_i:
            continue
        if re.match(r"\s*\?\s*;", source[end:]):
            offsets.append(match.start())
    return offsets


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
    while i > 0 and text[i - 1].isspace():
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


def _brace_depth_at(text: str, offset: int) -> int:
    depth = 0
    for ch in text[:offset]:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth = max(0, depth - 1)
    return depth


def _pattern_binds_name(pattern: str, name: str) -> bool:
    source = code_source(pattern)
    return any(match.group(0) == name for match in IDENT_RE.finditer(source))


def required_symbol_shadow_hits(text: str, name: str, allow_top_level_definition: bool) -> list:
    """Reject declarations and Rust bindings that can redirect a required callee name."""
    source = executable_source(text)
    hits = []

    declaration_re = re.compile(
        rf"\b(fn|struct|enum|union|type|const|static|mod)\s+{re.escape(name)}\b"
    )
    allowed_top_level_seen = False
    for match in declaration_re.finditer(source):
        kind = match.group(1)
        depth = _brace_depth_at(source, match.start())
        if (
            allow_top_level_definition
            and kind == "fn"
            and depth == 0
            and not allowed_top_level_seen
        ):
            allowed_top_level_seen = True
            continue
        hits.append(
            {"kind": f"{kind}_declaration", "line": line_for_offset(source, match.start())}
        )

    # let / if let / while let patterns, including tuple/struct destructuring.
    let_pattern_re = re.compile(r"\b(?:if\s+let|while\s+let|let)\b([^=;]+)=")
    for match in let_pattern_re.finditer(source):
        if _pattern_binds_name(match.group(1), name):
            hits.append(
                {"kind": "let_pattern_binding", "line": line_for_offset(source, match.start())}
            )

    # for PAT in ITER
    for_pattern_re = re.compile(r"\bfor\s+([^;{}]+?)\s+in\b")
    for match in for_pattern_re.finditer(source):
        if _pattern_binds_name(match.group(1), name):
            hits.append(
                {"kind": "for_pattern_binding", "line": line_for_offset(source, match.start())}
            )

    # Function parameter patterns, including tuple/struct destructuring.
    fn_re = re.compile(r"\bfn\s+[A-Za-z_][A-Za-z0-9_]*\s*\(")
    for match in fn_re.finditer(source):
        open_i = source.find("(", match.start(), match.end())
        end = _skip_matching_delimiter(source, open_i)
        if end <= open_i:
            continue
        for arg in _split_top_level_args(source[open_i + 1 : end - 1]):
            pattern = arg.split(":", 1)[0]
            if _pattern_binds_name(pattern, name):
                hits.append(
                    {
                        "kind": "parameter_pattern_binding",
                        "line": line_for_offset(source, match.start()),
                    }
                )

    # Closure parameters. Treat any occurrence in the parameter token list as
    # a binding because function authority names are not meaningful type paths there.
    closure_re = re.compile(r"\|([^|\n]*)\|")
    for match in closure_re.finditer(source):
        if _pattern_binds_name(match.group(1), name):
            hits.append(
                {"kind": "closure_pattern_binding", "line": line_for_offset(source, match.start())}
            )

    # Match-arm patterns: take the token segment before => and strip a guard.
    for arrow in re.finditer(r"=>", source):
        start = max(
            source.rfind(",", 0, arrow.start()),
            source.rfind("{", 0, arrow.start()),
            source.rfind("\n", 0, arrow.start()),
        )
        segment = source[start + 1 : arrow.start()]
        pattern = re.split(r"\s+if\s+", segment, maxsplit=1)[0]
        if _pattern_binds_name(pattern, name):
            hits.append(
                {"kind": "match_pattern_binding", "line": line_for_offset(source, arrow.start())}
            )

    # Explicit @-patterns and aliases remain useful independent sentinels.
    for kind, pattern in [
        ("pattern_binding", re.compile(rf"\b{re.escape(name)}\s*@")),
        ("alias_binding", re.compile(rf"\bas\s+{re.escape(name)}\b")),
    ]:
        for match in pattern.finditer(source):
            hits.append({"kind": kind, "line": line_for_offset(source, match.start())})

    use_re = re.compile(rf"\buse\b[^;\n]*\b{re.escape(name)}\b[^;\n]*;")
    for match in use_re.finditer(source):
        if _brace_depth_at(source, match.start()) > 0:
            hits.append(
                {"kind": "local_use_shadow", "line": line_for_offset(source, match.start())}
            )

    return hits


def _local_function_body(text: str, function_name: str):
    """Return one production function body, or None if the definition is missing/ambiguous."""
    source = executable_source(text)
    definition_re = re.compile(rf"\bfn\s+{re.escape(function_name)}\b")
    matches = list(definition_re.finditer(source))
    if len(matches) != 1:
        return None

    match = matches[0]
    i = match.end()
    paren_depth = 0
    bracket_depth = 0
    angle_depth = 0
    while i < len(source):
        ch = source[i]
        if ch == "(":
            paren_depth += 1
        elif ch == ")":
            paren_depth = max(0, paren_depth - 1)
        elif ch == "[":
            bracket_depth += 1
        elif ch == "]":
            bracket_depth = max(0, bracket_depth - 1)
        elif ch == "<" and _looks_like_generic_start(source, i):
            angle_depth += 1
        elif ch == ">" and angle_depth:
            angle_depth -= 1
        elif ch == "{" and paren_depth == 0 and bracket_depth == 0 and angle_depth == 0:
            end = _skip_matching_braces(source, i)
            if end <= i:
                return None
            return source[i + 1 : end - 1]
        elif ch == ";" and paren_depth == 0 and bracket_depth == 0 and angle_depth == 0:
            return None
        i += 1
    return None


def required_local_call_hits(text: str, function_name: str, callees: list) -> dict:
    body = _local_function_body(text, function_name)
    if body is None:
        return {
            "function": function_name,
            "missing_definition": True,
            "missing_calls": list(callees),
        }
    return {
        "function": function_name,
        "missing_definition": False,
        "missing_calls": [
            callee
            for callee in callees
            if not has_direct_top_level_qualified_call(body, callee)
        ],
    }


def _braced_closure_bodies(payload: str):
    """Parse a comma-separated list of braced closure expressions, or return None."""
    source = executable_source(payload)
    bodies = []
    i = 0
    while True:
        i = _skip_ws_and_comments(source, i)
        if i >= len(source):
            return bodies
        if source[i] != "|":
            return None
        close_pipe = source.find("|", i + 1)
        if close_pipe < 0:
            return None
        body_open = _skip_ws_and_comments(source, close_pipe + 1)
        if body_open >= len(source) or source[body_open] != "{":
            return None
        body_end = _skip_matching_delimiter(source, body_open)
        if body_end <= body_open:
            return None
        bodies.append(source[body_open + 1 : body_end - 1])
        i = _skip_ws_and_comments(source, body_end)
        if i >= len(source):
            return bodies
        if source[i] != ",":
            return None
        i += 1


def _runtime_callback_macro_invocations(text: str) -> list:
    source = code_source(text)
    macro_re = re.compile(
        r"\b(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*"
        r"([A-Za-z_][A-Za-z0-9_]*)!\s*[\(\[\{]"
    )
    return [match.group(1) for match in macro_re.finditer(source)]


def _normalize_runtime_callback_execution_shape(text: str) -> str:
    """Canonical callback code shape with comments/literals/macros made explicit."""
    source = executable_source(text)
    source = re.sub(r",\s*([)\]])", r"\1", source)
    return re.sub(r"\s+", "", source)


def _runtime_callback_attribute_count(text: str) -> int:
    """Count real Rust attributes in a callback after comments/literals are blanked."""
    source = code_source(text)
    return len(re.findall(r"#\s*!?\s*\[", source))


def required_runtime_persistence_callback_hits(
    text: str,
    function_name: str,
    constructor: str,
    audit_callee: str,
    persist_callees: list,
    expected_bodies=None,
) -> dict:
    body = _local_function_body(text, function_name)
    base = {
        "function": function_name,
        "constructor": constructor,
        "missing_definition": body is None,
        "constructor_count": 0,
        "callback_count": 0,
        "callbacks": [],
    }
    if body is None:
        return base

    source = executable_source(body)
    parts = constructor.split("::")
    if not parts or any(not IDENT_RE.fullmatch(part) for part in parts):
        return base
    qualified_constructor = r"\s*::\s*".join(re.escape(part) for part in parts)
    constructor_re = re.compile(
        rf"(?<![A-Za-z0-9_]){qualified_constructor}\s*\("
    )
    matches = list(constructor_re.finditer(source))
    base["constructor_count"] = len(matches)
    if len(matches) != 1:
        return base

    match = matches[0]
    open_i = source.find("(", match.start(), match.end())
    end = _skip_matching_delimiter(source, open_i)
    if end <= open_i:
        return base
    bodies = _braced_closure_bodies(source[open_i + 1 : end - 1])
    if bodies is None:
        return base
    base["callback_count"] = len(bodies)

    for index, persist_callee in enumerate(persist_callees):
        if index >= len(bodies):
            base["callbacks"].append(
                {
                    "index": index,
                    "persist_callee": persist_callee,
                    "missing_callback": True,
                    "audit_reference_count": 0,
                    "persist_reference_count": 0,
                    "macro_invocation_count": 0,
                    "macro_invocations": [],
                    "attribute_count": 0,
                    "execution_shape_matches": False,
                    "missing_audit": True,
                    "audit_result_propagated": False,
                    "missing_persist": True,
                    "audit_before_persist": False,
                }
            )
            continue
        closure_body = bodies[index]
        closure_code = code_source(closure_body)
        macro_invocations = _runtime_callback_macro_invocations(closure_body)
        attribute_count = _runtime_callback_attribute_count(closure_body)
        observed_shape = _normalize_runtime_callback_execution_shape(closure_body)
        expected_shape = (
            _normalize_runtime_callback_execution_shape(expected_bodies[index])
            if expected_bodies is not None and index < len(expected_bodies)
            else None
        )
        execution_shape_matches = (
            expected_shape is None or observed_shape == expected_shape
        )
        audit_reference_count = len(
            re.findall(rf"\b{re.escape(audit_callee)}\b", closure_code)
        )
        persist_reference_count = len(
            re.findall(rf"\b{re.escape(persist_callee)}\b", closure_code)
        )
        audit_offsets = direct_top_level_call_offsets(closure_body, audit_callee)
        propagating_audit_offsets = direct_top_level_propagating_call_offsets(
            closure_body, audit_callee
        )
        persist_offsets = direct_top_level_call_offsets(closure_body, persist_callee)
        base["callbacks"].append(
            {
                "index": index,
                "persist_callee": persist_callee,
                "missing_callback": False,
                "audit_reference_count": audit_reference_count,
                "persist_reference_count": persist_reference_count,
                "macro_invocation_count": len(macro_invocations),
                "macro_invocations": macro_invocations,
                "attribute_count": attribute_count,
                "execution_shape_matches": execution_shape_matches,
                "missing_audit": not audit_offsets,
                "audit_result_propagated": bool(propagating_audit_offsets),
                "missing_persist": not persist_offsets,
                "audit_before_persist": bool(
                    propagating_audit_offsets
                    and persist_offsets
                    and min(propagating_audit_offsets) < min(persist_offsets)
                ),
            }
        )
    return base


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

    call_checks = []
    shadow_checks = []
    for path, names in REQUIRED_LIVE_CALLS.items():
        text = production_source(read_required(root, path))
        missing = [name for name in names if not has_live_call(text, name)]
        call_checks.append({"path": path, "missing": missing})
        if missing:
            errors.append(
                f"required v3 live call expressions missing in {path}: {missing!r}"
            )

        local_defs = REQUIRED_LOCAL_DEFINITIONS.get(path, set())
        for name in names:
            hits = required_symbol_shadow_hits(text, name, name in local_defs)
            shadow_checks.append({"path": path, "name": name, "hits": hits})
            if hits:
                errors.append(
                    f"required v3 symbol shadowing in {path} for {name}: {hits!r}"
                )

    local_call_checks = []
    for path, functions in REQUIRED_LOCAL_CALLS.items():
        text = production_source(read_required(root, path))
        for function_name, callees in functions.items():
            check = required_local_call_hits(text, function_name, callees)
            check["path"] = path
            local_call_checks.append(check)
            if check["missing_definition"] or check["missing_calls"]:
                errors.append(
                    "required local authority calls missing in "
                    f"{path}::{function_name}: {check!r}"
                )

    runtime_persistence_callback_checks = []
    for path, contract in REQUIRED_RUNTIME_PERSISTENCE_CALLBACKS.items():
        text = production_source(read_required(root, path))
        check = required_runtime_persistence_callback_hits(
            text,
            contract["function"],
            contract["constructor"],
            contract["audit"],
            contract["persist"],
            contract.get("expected_bodies"),
        )
        check["path"] = path
        runtime_persistence_callback_checks.append(check)
        callback_failures = [
            item
            for item in check["callbacks"]
            if item["missing_callback"]
            or item["audit_reference_count"] != 1
            or item["persist_reference_count"] != 1
            or item["macro_invocation_count"] != 0
            or item["attribute_count"] != 0
            or not item["execution_shape_matches"]
            or item["missing_audit"]
            or not item["audit_result_propagated"]
            or item["missing_persist"]
            or not item["audit_before_persist"]
        ]
        if (
            check["missing_definition"]
            or check["constructor_count"] != 1
            or check["callback_count"] != len(contract["persist"])
            or callback_failures
        ):
            errors.append(
                "runtime persistence callbacks are not fully audit-pinned in "
                f"{path}::{contract['function']}: {check!r}"
            )

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
        "required_live_call_checks": call_checks,
        "required_symbol_shadow_checks": shadow_checks,
        "required_local_call_checks": local_call_checks,
        "required_runtime_persistence_callback_checks": runtime_persistence_callback_checks,
        "required_regression_checks": regression_checks,
        "static_scan_scope": ["crates/*/src/**/*.rs", "apps/*/src/**/*.rs"],
        "claim_boundary": {
            "proves_static_live_authority_shape": True,
            "requires_dynamic_guard_tests": True,
            "strips_all_cfg_test_items": True,
            "handles_cfg_test_comma_items": True,
            "discovers_cfg_test_attributes_from_code_only": True,
            "excludes_provably_disabled_cfg_items": True,
            "evaluates_cfg_logic_conservatively": True,
            "detects_contradictory_cfg_predicates": True,
            "detects_contradictory_chained_cfg_attributes": True,
            "detects_cfg_attr_emitted_contradictions": True,
            "evaluates_conditional_cfg_attr_under_active_constraints": True,
            "solves_disjunctive_cfg_attr_constraint_formulas": True,
            "applies_inner_cfg_to_enclosing_scope": True,
            "combines_inner_cfg_constraints_per_scope": True,
            "handles_raw_strings_in_chained_attributes": True,
            "ignores_cfg_inside_opaque_macro_tokens": True,
            "excludes_attribute_tokens_from_live_calls": True,
            "handles_inner_attributes": True,
            "expands_nested_cfg_attr": True,
            "rejects_multiline_fn_definitions_as_calls": True,
            "rejects_tuple_struct_declarations_as_calls": True,
            "rejects_tuple_enum_variant_declarations_as_calls": True,
            "rejects_required_symbol_shadowing": True,
            "detects_destructuring_pattern_shadowing": True,
            "lexes_char_and_byte_char_literals": True,
            "tracks_legacy_aliases": True,
            "verifies_live_call_expressions": True,
            "pins_underlying_calls_to_required_local_authority_helpers": True,
            "pins_required_local_callees_to_crate_paths": True,
            "requires_direct_top_level_local_authority_calls": True,
            "resets_direct_statement_boundary_after_top_level_braces": True,
            "requires_runtime_authority_dynamic_regression": True,
            "pins_every_runtime_persistence_callback_to_authority_audit": True,
            "requires_authority_audit_before_runtime_persist": True,
            "requires_runtime_persistence_audit_result_propagation": True,
            "rejects_alternate_runtime_persistence_references": True,
            "rejects_runtime_persistence_callback_macros": True,
            "rejects_runtime_persistence_callback_attributes": True,
            "freezes_runtime_persistence_callback_execution_shape": True,
            "excludes_opaque_macro_token_trees_from_live_calls": True,
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

    comma_sample = (
        "struct S {\n"
        "  #[cfg(test)] helper: Result<(u8, u8), u16>,\n"
        "  live: (),\n"
        "}\n"
        "enum E {\n"
        "  #[cfg(test)] Test(u8, u8),\n"
        "  Live,\n"
        "}\n"
        "fn later_live() { block_subsidy(9); }\n"
    )
    comma_prod = production_source(comma_sample)
    assert "helper" not in comma_prod
    assert "Test(u8, u8)" not in comma_prod
    assert "live: ()" in comma_prod
    assert "Live," in comma_prod
    assert "block_subsidy(9)" in comma_prod

    hidden_after_field = (
        "struct S { #[cfg(test)] helper: (), }\n"
        "fn live_after_test_field() { block_subsidy(10); }\n"
    )
    hidden_prod = production_source(hidden_after_field)
    assert "block_subsidy(10)" in hidden_prod

    cfg_text_only = (
        "// #[cfg(test)]\n"
        "fn after_cfg_comment() { block_subsidy(11); }\n"
        "const CFG_NOTE: &str = \"#[cfg(test)]\";\n"
        "fn after_cfg_string() { block_subsidy(12); }\n"
        "const CFG_RAW: &str = r#\"#[cfg(test)]\"#;\n"
        "fn after_cfg_raw() { block_subsidy(13); }\n"
    )
    cfg_text_prod = production_source(cfg_text_only)
    assert "block_subsidy(11)" in cfg_text_prod
    assert "block_subsidy(12)" in cfg_text_prod
    assert "block_subsidy(13)" in cfg_text_prod

    comparison_variant = (
        "enum E {\n"
        "  #[cfg(test)] V = (1<2) as isize,\n"
        "  Live,\n"
        "}\n"
        "fn after_comparison_variant() { block_subsidy(14); }\n"
    )
    comparison_prod = production_source(comparison_variant)
    assert "V = (1<2)" not in comparison_prod
    assert "Live," in comparison_prod
    assert "block_subsidy(14)" in comparison_prod

    char_literal_item = (
        "#[cfg(test)] const OPEN: char = '{';\n"
        "#[cfg(test)] const CLOSE: u8 = b'}';\n"
        "fn after_char_literal() { block_subsidy(15); }\n"
    )
    char_literal_prod = production_source(char_literal_item)
    assert "OPEN" not in char_literal_prod
    assert "CLOSE" not in char_literal_prod
    assert "block_subsidy(15)" in char_literal_prod

    raw_chained_attribute = (
        '#[cfg(test)] #[doc = r##"x] y"##] fn helper() { block_subsidy(90); }\n'
        'fn after_raw_attribute() { block_subsidy(16); }\n'
    )
    raw_attr_prod = production_source(raw_chained_attribute)
    assert "helper" not in raw_attr_prod
    assert "block_subsidy(16)" in raw_attr_prod

    cfg_any_dead = (
        "#[cfg(any())]\n"
        "fn dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "fn unrelated_live() {}\n"
    )
    cfg_any_prod = production_source(cfg_any_dead)
    assert not has_live_call(cfg_any_prod, "audit_monetary_state_v3")
    assert "unrelated_live" in cfg_any_prod

    cfg_logic = (
        "#[cfg(all(not(test), any()))]\n"
        "fn also_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "#[cfg(not(test))]\n"
        "fn definitely_production() { audit_monetary_state_v3(prepared, cadence); }\n"
        '#[cfg(any(test, target_os = "linux"))]\n'
        "fn maybe_production() { audit_monetary_state_v3(prepared, cadence); }\n"
    )
    cfg_logic_prod = production_source(cfg_logic)
    assert "also_dead" not in cfg_logic_prod
    assert "definitely_production" in cfg_logic_prod
    assert "maybe_production" in cfg_logic_prod
    assert has_live_call(cfg_logic_prod, "audit_monetary_state_v3")

    contradictory_cfg = (
        "#[cfg(all(unix, not(unix)))]\n"
        "fn contradictory_dead() { validate_live_reward_settlement_v3(state); }\n"
        "fn after_contradiction() { block_subsidy(21); }\n"
    )
    contradictory_prod = production_source(contradictory_cfg)
    assert "contradictory_dead" not in contradictory_prod
    assert "block_subsidy(21)" in contradictory_prod
    assert not has_live_call(
        contradictory_prod, "validate_live_reward_settlement_v3"
    )

    chained_contradictory_cfg = (
        "#[cfg(unix)]\n"
        "#[cfg(not(unix))]\n"
        "fn chained_dead() { validate_live_reward_settlement_v3(state); }\n"
        "fn after_chained_contradiction() { block_subsidy(22); }\n"
    )
    chained_contradictory_prod = production_source(chained_contradictory_cfg)
    assert "chained_dead" not in chained_contradictory_prod
    assert "block_subsidy(22)" in chained_contradictory_prod
    assert not has_live_call(
        chained_contradictory_prod, "validate_live_reward_settlement_v3"
    )

    cfg_attr_contradictory_cfg = (
        "#[cfg(unix)]\n"
        "#[cfg_attr(all(), cfg(not(unix)))]\n"
        "fn cfg_attr_chained_dead() { validate_live_reward_settlement_v3(state); }\n"
        "fn after_cfg_attr_contradiction() { block_subsidy(23); }\n"
    )
    cfg_attr_contradictory_prod = production_source(cfg_attr_contradictory_cfg)
    assert "cfg_attr_chained_dead" not in cfg_attr_contradictory_prod
    assert "block_subsidy(23)" in cfg_attr_contradictory_prod
    assert not has_live_call(
        cfg_attr_contradictory_prod, "validate_live_reward_settlement_v3"
    )

    conditional_cfg_attr_dead = (
        "#[cfg(unix)]\n"
        "#[cfg_attr(unix, cfg(not(unix)))]\n"
        "fn conditional_dead() { validate_live_reward_settlement_v3(state); }\n"
        "fn after_conditional_cfg_attr() { block_subsidy(24); }\n"
    )
    conditional_cfg_attr_prod = production_source(conditional_cfg_attr_dead)
    assert "conditional_dead" not in conditional_cfg_attr_prod
    assert "block_subsidy(24)" in conditional_cfg_attr_prod

    disjunctive_cfg_attr_dead = (
        "#[cfg(any(unix, windows))]\n"
        "#[cfg_attr(unix, cfg(not(unix)))]\n"
        "#[cfg_attr(windows, cfg(not(windows)))]\n"
        "fn disjunctive_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "fn after_disjunctive_cfg_attr() { block_subsidy(26); }\n"
    )
    disjunctive_cfg_attr_prod = production_source(disjunctive_cfg_attr_dead)
    assert "disjunctive_dead" not in disjunctive_cfg_attr_prod
    assert "block_subsidy(26)" in disjunctive_cfg_attr_prod

    cfg_comment_dead = (
        "#[cfg(any(/* still empty */))]\n"
        "fn comment_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
    )
    assert not has_live_call(
        production_source(cfg_comment_dead), "audit_monetary_state_v3"
    )

    cfg_attr_dead = (
        "#[cfg_attr(not(test), cfg(any()))]\n"
        "fn cfg_attr_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "fn after_cfg_attr() { block_subsidy(17); }\n"
    )
    cfg_attr_prod = production_source(cfg_attr_dead)
    assert "cfg_attr_dead" not in cfg_attr_prod
    assert "block_subsidy(17)" in cfg_attr_prod

    nested_cfg_attr_dead = (
        "#[cfg_attr(all(), cfg_attr(all(), cfg(any())))]\n"
        "fn nested_cfg_attr_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "fn after_nested_cfg_attr() { block_subsidy(20); }\n"
    )
    nested_cfg_attr_prod = production_source(nested_cfg_attr_dead)
    assert "nested_cfg_attr_dead" not in nested_cfg_attr_prod
    assert "block_subsidy(20)" in nested_cfg_attr_prod

    inner_attribute_only = (
        "#![cfg_attr(any(), allow(audit_monetary_state_v3(prepared, cadence)))]\n"
        "fn boundary_without_inner_audit() {}\n"
    )
    assert not has_live_call(inner_attribute_only, "audit_monetary_state_v3")

    inner_cfg_dead_module = (
        "mod dead {\n"
        "  #![cfg(any())]\n"
        "  fn fake() { audit_monetary_state_v3(prepared, cadence); }\n"
        "}\n"
        "fn after_inner_cfg() { block_subsidy(25); }\n"
    )
    inner_cfg_prod = production_source(inner_cfg_dead_module)
    assert not has_live_call(inner_cfg_prod, "audit_monetary_state_v3")
    assert "block_subsidy(25)" in inner_cfg_prod

    combined_inner_cfg_dead_module = (
        "mod dead {\n"
        "  #![cfg(unix)]\n"
        "  #![cfg_attr(unix, cfg(not(unix)))]\n"
        "  fn fake() { audit_monetary_state_v3(prepared, cadence); }\n"
        "}\n"
        "fn after_combined_inner_cfg() { block_subsidy(27); }\n"
    )
    combined_inner_cfg_prod = production_source(combined_inner_cfg_dead_module)
    assert not has_live_call(combined_inner_cfg_prod, "audit_monetary_state_v3")
    assert "block_subsidy(27)" in combined_inner_cfg_prod

    multiline_definition = (
        "fn\n"
        "audit_monetary_state_v3(prepared: &State, cadence: &Cadence) {}\n"
    )
    assert not has_live_call(multiline_definition, "audit_monetary_state_v3")

    cfg_inside_macro = (
        "fn live_before() { block_subsidy(18); }\n"
        "quote!(#[cfg(any())] fn generated() { block_subsidy(999); });\n"
        "fn live_after_macro() { block_subsidy(19); }\n"
    )
    cfg_macro_prod = production_source(cfg_inside_macro)
    assert "block_subsidy(18)" in cfg_macro_prod
    assert "block_subsidy(19)" in cfg_macro_prod

    attribute_only_call = (
        "#[my_attr(audit_monetary_state_v3(prepared, cadence))]\n"
        "fn boundary_without_audit() {}\n"
    )
    assert not has_live_call(attribute_only_call, "audit_monetary_state_v3")

    import_only = code_source(
        "use crate::audit_monetary_state_v3;\n"
        "// audit_monetary_state_v3(fake);\n"
        "const NOTE: &str = \"audit_monetary_state_v3(fake)\";\n"
        "const RAW: &str = r\"audit_monetary_state_v3(fake)\";\n"
        "const RAW_HASH: &str = r#\"audit_monetary_state_v3(fake)\"#;\n"
    )
    assert not has_live_call(import_only, "audit_monetary_state_v3")
    real_call = code_source(
        "use crate::audit_monetary_state_v3;\n"
        "fn boundary(prepared: &ChainState) { audit_monetary_state_v3(prepared, cadence); }\n"
    )
    assert has_live_call(real_call, "audit_monetary_state_v3")
    definition_only = code_source("fn audit_monetary_state_v3(state: &State) {}\n")
    assert not has_live_call(definition_only, "audit_monetary_state_v3")
    tuple_struct_only = code_source(
        "fn boundary() { struct validate_live_reward_settlement_v3(); }\n"
    )
    assert not has_live_call(
        tuple_struct_only, "validate_live_reward_settlement_v3"
    )

    tuple_enum_variant_only = code_source(
        "enum Local { validate_live_reward_settlement_v3(), Other }\n"
    )
    assert not has_live_call(
        tuple_enum_variant_only, "validate_live_reward_settlement_v3"
    )

    shadowed_required_call = (
        "fn audit_monetary_state_v3(_: i32) {}\n"
        "fn boundary() { audit_monetary_state_v3(1); }\n"
    )
    assert required_symbol_shadow_hits(
        shadowed_required_call, "audit_monetary_state_v3", False
    )

    destructured_shadow = (
        "fn boundary() {\n"
        "  let (audit_monetary_state_v3,) = (|_: i32| {},);\n"
        "  audit_monetary_state_v3(1);\n"
        "}\n"
    )
    assert required_symbol_shadow_hits(
        destructured_shadow, "audit_monetary_state_v3", False
    )

    if_let_shadow = (
        "fn boundary(v: Option<fn(i32)>) {\n"
        "  if let Some(audit_monetary_state_v3) = v { audit_monetary_state_v3(1); }\n"
        "}\n"
    )
    assert required_symbol_shadow_hits(
        if_let_shadow, "audit_monetary_state_v3", False
    )

    match_shadow = (
        "fn boundary(v: Option<fn(i32)>) { match v {\n"
        "  Some(audit_monetary_state_v3) => audit_monetary_state_v3(1),\n"
        "  None => {},\n"
        "} }\n"
    )
    assert required_symbol_shadow_hits(
        match_shadow, "audit_monetary_state_v3", False
    )

    stringify_only = code_source(
        "fn boundary() { "
        "let _ = stringify!(audit_monetary_state_v3(prepared, cadence)); "
        "}\n"
    )
    assert not has_live_call(stringify_only, "audit_monetary_state_v3")
    opaque_macro_only = code_source(
        "fn boundary() { discard_tokens!{ audit_monetary_state_v3(prepared, cadence) } }\n"
    )
    assert not has_live_call(opaque_macro_only, "audit_monetary_state_v3")
    char_in_macro = code_source(
        "fn boundary() { "
        "let _ = stringify!(')', audit_monetary_state_v3(prepared, cadence)); "
        "}\n"
    )
    assert not has_live_call(char_in_macro, "audit_monetary_state_v3")
    byte_char_in_macro = code_source(
        "fn boundary() { "
        "let _ = stringify!(b']', audit_monetary_state_v3(prepared, cadence)); "
        "}\n"
    )
    assert not has_live_call(byte_char_in_macro, "audit_monetary_state_v3")

    local_authority = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  crate::audit_monetary_state_v3(state)?;\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
        "fn unrelated() { audit_monetary_state_v3(other); }\n"
    )
    local_check = required_local_call_hits(
        local_authority,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert not local_check["missing_definition"]
    assert local_check["missing_calls"] == []

    local_authority_bypass = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
        "fn unrelated() { audit_monetary_state_v3(other); }\n"
    )
    bypass_check = required_local_call_hits(
        local_authority_bypass,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert bypass_check["missing_calls"] == ["crate::audit_monetary_state_v3"]

    local_shadow_bypass = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  let audit_monetary_state_v3 = |_: &State| -> Result<(), Error> { Ok(()) };\n"
        "  audit_monetary_state_v3(state)?;\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
    )
    shadow_check = required_local_call_hits(
        local_shadow_bypass,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert shadow_check["missing_calls"] == ["crate::audit_monetary_state_v3"]

    nested_body_bypass = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  fn unused(state: &State) { crate::audit_monetary_state_v3(state); }\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
    )
    nested_check = required_local_call_hits(
        nested_body_bypass,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert nested_check["missing_calls"] == ["crate::audit_monetary_state_v3"]

    closure_body_bypass = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  let _unused = || crate::audit_monetary_state_v3(state);\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
    )
    closure_check = required_local_call_hits(
        closure_body_bypass,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert closure_check["missing_calls"] == ["crate::audit_monetary_state_v3"]

    conditional_body_bypass = (
        "fn audit_authoritative_monetary_state(state: &State) -> Result<(), Error> {\n"
        "  if false { crate::audit_monetary_state_v3(state); }\n"
        "  crate::validate_live_reward_settlement_v3(state)?;\n"
        "  Ok(())\n"
        "}\n"
    )
    conditional_check = required_local_call_hits(
        conditional_body_bypass,
        "audit_authoritative_monetary_state",
        ["crate::audit_monetary_state_v3", "crate::validate_live_reward_settlement_v3"],
    )
    assert conditional_check["missing_calls"] == ["crate::audit_monetary_state_v3"]

    def runtime_callback_fixture(audit_runtime=True, audit_one=True, audit_bundle=True):
        def callback(audit_enabled, persist_name):
            audit_line = (
                "audit_authoritative_monetary_state(prepared, cadence)?; "
                if audit_enabled
                else ""
            )
            return (
                "|prepared: &State, runtime: &Runtime| { "
                + audit_line
                + f"{persist_name}(prepared, runtime) "
                + "}"
            )

        return (
            "fn drive_monetary_v3_p2p_block_with_runtime_persistence() {\n"
            "  let _p = ActivatedV2P2pRuntimePersistence::new(\n"
            f"    {callback(audit_runtime, 'persist_runtime')},\n"
            f"    {callback(audit_one, 'persist_one')},\n"
            f"    {callback(audit_bundle, 'persist_bundle')}\n"
            "  );\n"
            "}\n"
        )

    callback_contract = required_runtime_persistence_callback_hits(
        runtime_callback_fixture(),
        "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "ActivatedV2P2pRuntimePersistence::new",
        "audit_authoritative_monetary_state",
        ["persist_runtime", "persist_one", "persist_bundle"],
    )
    assert callback_contract["constructor_count"] == 1
    assert callback_contract["callback_count"] == 3
    assert all(
        item["audit_reference_count"] == 1
        and item["persist_reference_count"] == 1
        and not item["missing_audit"]
        and item["audit_result_propagated"]
        and not item["missing_persist"]
        and item["audit_before_persist"]
        for item in callback_contract["callbacks"]
    )

    for omitted_index, flags in enumerate(
        [(False, True, True), (True, False, True), (True, True, False)]
    ):
        omitted = required_runtime_persistence_callback_hits(
            runtime_callback_fixture(*flags),
            "drive_monetary_v3_p2p_block_with_runtime_persistence",
            "ActivatedV2P2pRuntimePersistence::new",
            "audit_authoritative_monetary_state",
            ["persist_runtime", "persist_one", "persist_bundle"],
        )
        assert omitted["callbacks"][omitted_index]["missing_audit"]

    audit_after_persist = (
        "fn drive_monetary_v3_p2p_block_with_runtime_persistence() {\n"
        "  let _p = ActivatedV2P2pRuntimePersistence::new(\n"
        "    |prepared: &State, runtime: &Runtime| { persist_runtime(prepared, runtime); audit_authoritative_monetary_state(prepared, cadence)?; },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_one(prepared, runtime) },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_bundle(prepared, runtime) }\n"
        "  );\n"
        "}\n"
    )
    after_check = required_runtime_persistence_callback_hits(
        audit_after_persist,
        "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "ActivatedV2P2pRuntimePersistence::new",
        "audit_authoritative_monetary_state",
        ["persist_runtime", "persist_one", "persist_bundle"],
    )
    assert after_check["callbacks"][0]["audit_result_propagated"]
    assert not after_check["callbacks"][0]["audit_before_persist"]

    ignored_audit_result = (
        "fn drive_monetary_v3_p2p_block_with_runtime_persistence() {\n"
        "  let _p = ActivatedV2P2pRuntimePersistence::new(\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence); persist_runtime(prepared, runtime) },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_one(prepared, runtime) },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_bundle(prepared, runtime) }\n"
        "  );\n"
        "}\n"
    )
    ignored_result_check = required_runtime_persistence_callback_hits(
        ignored_audit_result,
        "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "ActivatedV2P2pRuntimePersistence::new",
        "audit_authoritative_monetary_state",
        ["persist_runtime", "persist_one", "persist_bundle"],
    )
    assert not ignored_result_check["callbacks"][0]["missing_audit"]
    assert not ignored_result_check["callbacks"][0]["audit_result_propagated"]
    assert not ignored_result_check["callbacks"][0]["audit_before_persist"]

    alternate_persist_path = (
        "fn drive_monetary_v3_p2p_block_with_runtime_persistence() {\n"
        "  let _p = ActivatedV2P2pRuntimePersistence::new(\n"
        "    |prepared: &State, runtime: &Runtime| { if bypass { return persist_runtime(prepared, runtime); } audit_authoritative_monetary_state(prepared, cadence)?; persist_runtime(prepared, runtime) },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_one(prepared, runtime) },\n"
        "    |prepared: &State, runtime: &Runtime| { audit_authoritative_monetary_state(prepared, cadence)?; persist_bundle(prepared, runtime) }\n"
        "  );\n"
        "}\n"
    )
    alternate_persist_check = required_runtime_persistence_callback_hits(
        alternate_persist_path,
        "drive_monetary_v3_p2p_block_with_runtime_persistence",
        "ActivatedV2P2pRuntimePersistence::new",
        "audit_authoritative_monetary_state",
        ["persist_runtime", "persist_one", "persist_bundle"],
    )
    assert alternate_persist_check["callbacks"][0]["persist_reference_count"] == 2

    runtime_contract = REQUIRED_RUNTIME_PERSISTENCE_CALLBACKS[
        "crates/pulsedag-core/src/network_runtime_v3.rs"
    ]
    frozen_shape_fixture = (
        "fn drive_monetary_v3_p2p_block_with_runtime_persistence() {\n"
        "  let _p = ActivatedV2P2pRuntimePersistence::new(\n"
        "    |prepared_state: &State, prepared_runtime: &Runtime| {\n"
        "      validate_runtime_transient_monetary_envelopes(prepared_state, prepared_runtime, identity)?;\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_runtime(prepared_state, prepared_runtime)\n"
        "    },\n"
        "    |accepted_block: &Block, prepared_state: &State, prepared_runtime: &Runtime| {\n"
        "      validate_runtime_transient_monetary_envelopes(prepared_state, prepared_runtime, identity)?;\n"
        "      validate_runtime_accepted_block_reward(prepared_state, accepted_block, cadence_segments)?;\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_one(accepted_block, prepared_state, prepared_runtime)\n"
        "    },\n"
        "    |bundle: &[Block], prepared_state: &State, prepared_runtime: &Runtime| {\n"
        "      validate_runtime_transient_monetary_envelopes(prepared_state, prepared_runtime, identity)?;\n"
        "      validate_runtime_promoted_bundle_rewards(prepared_state, bundle, cadence_segments)?;\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_bundle(bundle, prepared_state, prepared_runtime)\n"
        "    }\n"
        "  );\n"
        "}\n"
    )
    frozen_shape_check = required_runtime_persistence_callback_hits(
        frozen_shape_fixture,
        runtime_contract["function"],
        runtime_contract["constructor"],
        runtime_contract["audit"],
        runtime_contract["persist"],
        runtime_contract["expected_bodies"],
    )
    assert frozen_shape_check["callback_count"] == 3
    assert all(
        item["macro_invocation_count"] == 0
        for item in frozen_shape_check["callbacks"]
    )
    assert all(
        item["execution_shape_matches"]
        and not item["missing_audit"]
        and item["audit_result_propagated"]
        and not item["missing_persist"]
        and item["audit_before_persist"]
        for item in frozen_shape_check["callbacks"]
    )

    conditional_attribute_fixture = frozen_shape_fixture.replace(
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n",
        "      #[cfg(feature = \"never\")]\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n",
        1,
    )
    conditional_attribute_check = required_runtime_persistence_callback_hits(
        conditional_attribute_fixture,
        runtime_contract["function"],
        runtime_contract["constructor"],
        runtime_contract["audit"],
        runtime_contract["persist"],
        runtime_contract["expected_bodies"],
    )
    assert conditional_attribute_check["callbacks"][0]["attribute_count"] == 1
    assert conditional_attribute_check["callbacks"][0]["execution_shape_matches"]

    macro_bypass_fixture = frozen_shape_fixture.replace(
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_runtime(prepared_state, prepared_runtime)\n",
        "      persist_early!(prepared_state, prepared_runtime);\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_runtime(prepared_state, prepared_runtime)\n",
        1,
    )
    macro_bypass_check = required_runtime_persistence_callback_hits(
        macro_bypass_fixture,
        runtime_contract["function"],
        runtime_contract["constructor"],
        runtime_contract["audit"],
        runtime_contract["persist"],
        runtime_contract["expected_bodies"],
    )
    assert macro_bypass_check["callbacks"][0]["macro_invocation_count"] == 1
    assert not macro_bypass_check["callbacks"][0]["execution_shape_matches"]

    helper_bypass_fixture = frozen_shape_fixture.replace(
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_runtime(prepared_state, prepared_runtime)\n",
        "      persist_early(prepared_state, prepared_runtime)?;\n"
        "      audit_authoritative_monetary_state(prepared_state, cadence_segments)?;\n"
        "      persist_runtime(prepared_state, prepared_runtime)\n",
        1,
    )
    helper_bypass_check = required_runtime_persistence_callback_hits(
        helper_bypass_fixture,
        runtime_contract["function"],
        runtime_contract["constructor"],
        runtime_contract["audit"],
        runtime_contract["persist"],
        runtime_contract["expected_bodies"],
    )
    assert helper_bypass_check["callbacks"][0]["macro_invocation_count"] == 0
    assert not helper_bypass_check["callbacks"][0]["execution_shape_matches"]

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
