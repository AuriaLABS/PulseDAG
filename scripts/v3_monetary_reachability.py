#!/usr/bin/env python3
"""Exact-candidate reachability audit for PulseDAG v3 monetary authority."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "pulsedag.v3-monetary-reachability-evidence.v1"
AUDITOR_VERSION = 4

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
        while self.i < len(self.text) and self.text[self.i].isspace():
            self.i += 1

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


def _cfg_attr_disables_item(attr_payload: str) -> bool:
    args = _split_top_level_args(attr_payload)
    if len(args) < 2:
        return False
    condition = evaluate_cfg_predicate(args[0])
    if condition != CFG_TRUE:
        # Unknown means there is at least one plausible production build where
        # this attribute is inactive, so it cannot prove the item dead.
        return False
    return any(_meta_cfg_value(meta) == CFG_FALSE for meta in args[1:])


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


def production_source(text: str) -> str:
    """Blank items provably disabled in every production build; preserve potential live code."""
    chars = list(text)
    lexed = code_source(text)
    disabled_starts = []

    for match in CFG_ATTR_RE.finditer(lexed):
        pred_start, pred_end = _attribute_call_bounds(lexed, match)
        if pred_end <= pred_start:
            continue
        if evaluate_cfg_predicate(text[pred_start:pred_end]) == CFG_FALSE:
            disabled_starts.append(match.start())

    for match in CFG_ATTR_ATTR_RE.finditer(lexed):
        payload_start, payload_end = _attribute_call_bounds(lexed, match)
        if payload_end <= payload_start:
            continue
        if _cfg_attr_disables_item(text[payload_start:payload_end]):
            disabled_starts.append(match.start())

    for attr_start in sorted(set(disabled_starts)):
        span = _cfg_item_span(text, attr_start)
        if span is None:
            continue
        start, item_end = span
        for k in range(start, item_end):
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


def executable_source(text: str) -> str:
    """Blank comments, literals and opaque macro token trees, preserving offsets."""
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


def live_call_offsets(text: str, name: str) -> list:
    """Return direct live call-expression offsets outside opaque macro token trees."""
    source = executable_source(text)
    call_re = re.compile(rf"\b{re.escape(name)}\s*\(")
    offsets = []
    for match in call_re.finditer(source):
        if _preceding_keyword(source, match.start()) == "fn":
            continue
        offsets.append(match.start())
    return offsets


def has_live_call(text: str, name: str) -> bool:
    return bool(live_call_offsets(text, name))


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

    call_checks = []
    for path, names in REQUIRED_LIVE_CALLS.items():
        text = code_source(production_source(read_required(root, path)))
        missing = [name for name in names if not has_live_call(text, name)]
        call_checks.append({"path": path, "missing": missing})
        if missing:
            errors.append(
                f"required v3 live call expressions missing in {path}: {missing!r}"
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
            "handles_raw_strings_in_chained_attributes": True,
            "lexes_char_and_byte_char_literals": True,
            "tracks_legacy_aliases": True,
            "verifies_live_call_expressions": True,
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

    cfg_attr_dead = (
        "#[cfg_attr(not(test), cfg(any()))]\n"
        "fn cfg_attr_dead() { audit_monetary_state_v3(prepared, cadence); }\n"
        "fn after_cfg_attr() { block_subsidy(17); }\n"
    )
    cfg_attr_prod = production_source(cfg_attr_dead)
    assert "cfg_attr_dead" not in cfg_attr_prod
    assert "block_subsidy(17)" in cfg_attr_prod

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
