#!/usr/bin/env python3
"""Supplemental fail-closed enforcement audit for Task38 phase 8.

This audit closes structural gaps that a token-presence check cannot prove:
- expected-difficulty mismatch must enter a rejecting `return Err(...)` guard;
- canonical PoW validation must propagate failure with `?`;
- critical difficulty/PoW guards must execute directly in the `validate_block` body;
- recognized accelerator/vendor/runtime aliases must not appear in node/core
  manifests or executable/configuration Rust.

This is software-only evidence and does not execute physical GPU hardware.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
PRIMARY_AUDIT = ROOT / "scripts" / "verify_task38_no_gpu_consensus_shortcut.py"
VALIDATION = ROOT / "crates" / "pulsedag-core" / "src" / "validation.rs"
NODE_MANIFEST = ROOT / "apps" / "pulsedagd" / "Cargo.toml"
CORE_MANIFEST = ROOT / "crates" / "pulsedag-core" / "Cargo.toml"


def fail(message: str) -> None:
    raise RuntimeError(message)


def load_primary_audit():
    spec = importlib.util.spec_from_file_location("task38_primary_audit", PRIMARY_AUDIT)
    if spec is None or spec.loader is None:
        fail("unable to load primary Task38 audit module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


AUDIT = load_primary_audit()

# Keep aliases contextual where short words such as `hip` or `amd` could create
# ordinary-language false positives. Joined snake_case/kebab/CamelCase forms are
# matched because separators are optional and matching is case-insensitive.
ALIAS_PATTERN = re.compile(
    r"(?:"
    r"accelerator|nvidia|rocm|nvml|nvrtc|wgpu|vulkan|metal|radeon|amdgpu|amdhip|hiprtc|"
    r"amd[_-]?(?:gpu|backend|device|compute)|"
    r"ati[_-]?(?:gpu|backend|device|compute)|"
    r"hip[_-]?(?:gpu|backend|device|compute|runtime|sys)|"
    r"(?:device|vendor)[_-]?backend|backend[_-]?(?:device|vendor)"
    r")",
    re.IGNORECASE,
)


def compact(text: str) -> str:
    return "".join(text.split())


def brace_depth_before(text: str, pos: int) -> int:
    """Return structural brace depth immediately before `pos`."""

    if pos < 0 or pos > len(text):
        fail("internal audit parser received an invalid brace-depth position")
    depth = 0
    for char in text[:pos]:
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth < 0:
                fail("unbalanced closing brace while auditing consensus control flow")
    return depth


def assert_direct_statement_start(text: str, pos: int, label: str) -> None:
    """Reject marker substrings embedded in another identifier/path/expression."""

    prefix = text[:pos].rstrip()
    if not prefix or prefix[-1] not in "{;}":
        fail(f"{label} must begin a direct validate_block statement")


def assert_consensus_markers_top_level(validate_block: str) -> tuple[int, int, int]:
    """Require critical consensus checks to be direct function-body statements."""

    difficulty_marker = "if block.header.difficulty != expected_difficulty"
    pow_marker = "validate_pow_header(&block.header)"
    timestamp_marker = "if block.header.timestamp < newest_parent_timestamp"

    positions: list[int] = []
    for marker, label in (
        (difficulty_marker, "expected-difficulty guard"),
        (pow_marker, "canonical PoW validation"),
        (timestamp_marker, "post-PoW timestamp guard"),
    ):
        if validate_block.count(marker) != 1:
            fail(f"{label} must appear exactly once in validate_block")
        pos = validate_block.find(marker)
        depth = brace_depth_before(validate_block, pos)
        if depth != 1:
            fail(f"{label} must execute directly in validate_block body; brace depth={depth}")
        assert_direct_statement_start(validate_block, pos, label)
        positions.append(pos)

    difficulty_pos, pow_pos, timestamp_pos = positions
    if not (difficulty_pos < pow_pos < timestamp_pos):
        fail("difficulty rejection, canonical PoW validation, and timestamp guard are out of order")
    return difficulty_pos, pow_pos, timestamp_pos


def matching_paren_end(text: str, open_pos: int) -> int:
    if open_pos < 0 or open_pos >= len(text) or text[open_pos] != "(":
        fail("internal audit parser expected an opening parenthesis")
    depth = 0
    for pos in range(open_pos, len(text)):
        if text[pos] == "(":
            depth += 1
        elif text[pos] == ")":
            depth -= 1
            if depth == 0:
                return pos
    fail("unterminated parenthesized expression in consensus audit")
    raise AssertionError("unreachable")


def exact_call_argument(text: str, prefix: str, label: str) -> str:
    """Return one call argument only when the call consumes the full expression."""

    if not prefix.endswith("(") or not text.startswith(prefix):
        fail(f"{label} is no longer a direct {prefix[:-1]} call")
    open_pos = len(prefix) - 1
    close_pos = matching_paren_end(text, open_pos)
    if close_pos != len(text) - 1:
        fail(f"{label} contains trailing control flow or a second expression")
    return text[open_pos + 1 : close_pos]


def assert_alias_detector_self_test() -> None:
    must_reject = (
        "use pulsedag_accelerator::verify;",
        "let backend = NvidiaDevice::new();",
        "let runtime = rocm_runtime();",
        "use nvml_wrapper::Nvml;",
        "let compiler = NvrtcProgram::new();",
        "use wgpu::Device;",
        "let backend = VulkanDevice::new();",
        "let backend = MetalDevice::new();",
        "let backend = RadeonDevice::new();",
        "let backend = AmdBackend::new();",
        "let backend = AtiDevice::new();",
        "let backend = HipDevice::new();",
        "let route = device_backend;",
        "let route = BackendDevice;",
    )
    for fixture in must_reject:
        if ALIAS_PATTERN.search(fixture) is None:
            fail(f"vendor/runtime alias detector missed fixture: {fixture}")

    must_allow = (
        "let ownership_chain = true;",
        "let command_queue = 0;",
        "let validation_state = 1;",
        "let membership_epoch = 2;",
    )
    for fixture in must_allow:
        if ALIAS_PATTERN.search(fixture) is not None:
            fail(f"vendor/runtime alias detector false-positive: {fixture}")


def assert_no_vendor_runtime_aliases() -> None:
    for path in (NODE_MANIFEST, CORE_MANIFEST):
        text = path.read_text(encoding="utf-8")
        match = ALIAS_PATTERN.search(text)
        if match is not None:
            fail(f"GPU vendor/runtime alias {match.group(0)!r} found in {path.relative_to(ROOT)}")

    roots = (
        ROOT / "apps" / "pulsedagd" / "src",
        ROOT / "crates" / "pulsedag-core" / "src",
    )
    for root in roots:
        for path in sorted(root.rglob("*.rs")):
            source = path.read_text(encoding="utf-8")
            code = AUDIT.rust_code_without_comments(source)
            match = ALIAS_PATTERN.search(code)
            if match is not None:
                fail(
                    f"GPU vendor/runtime alias {match.group(0)!r} found in "
                    f"executable/configuration code: {path.relative_to(ROOT)}"
                )


def assert_difficulty_guard_rejects(validate_block: str) -> None:
    marker = "if block.header.difficulty != expected_difficulty"
    guard = AUDIT.braced_item(validate_block, marker)
    normalized = compact(guard)
    expected_prefix = "ifblock.header.difficulty!=expected_difficulty{"
    if not normalized.startswith(expected_prefix) or not normalized.endswith("}"):
        fail("expected-difficulty comparison is no longer a direct rejecting if-guard")

    body = normalized[len(expected_prefix) : -1]
    if not body.startswith("return") or not body.endswith(";"):
        fail("expected-difficulty mismatch must immediately return an error")
    returned = body[len("return") : -1]
    invalid_block = exact_call_argument(returned, "Err(", "difficulty rejection")
    exact_call_argument(
        invalid_block,
        "PulseError::InvalidBlock(",
        "difficulty rejection error",
    )


def assert_pow_failure_is_propagated(validate_block: str) -> None:
    pow_call = "validate_pow_header(&block.header)"
    if validate_block.count(pow_call) != 1:
        fail("validate_block must contain exactly one canonical validate_pow_header call")

    pow_pos = validate_block.find(pow_call)
    assert_direct_statement_start(validate_block, pow_pos, "canonical PoW validation")
    next_guard = validate_block.find("if block.header.timestamp", pow_pos)
    if next_guard < 0:
        fail("timestamp guard missing after canonical PoW validation")
    statement = compact(validate_block[pow_pos:next_guard])
    if not statement.startswith(pow_call):
        fail("canonical PoW validation is no longer the direct validation statement")
    if not statement.endswith("?;"):
        fail("canonical PoW validation result is not propagated with ?")

    expression = statement[: -len("?;")]
    map_call = expression[len(pow_call) :]
    closure = exact_call_argument(map_call, ".map_err(", "canonical PoW error mapping")
    match = re.fullmatch(r"\|[A-Za-z_][A-Za-z0-9_]*\|\{(.*)\}", closure)
    if match is None:
        fail("canonical PoW map_err must use one direct closure body")
    mapped_error = match.group(1)
    exact_call_argument(
        mapped_error,
        "PulseError::InvalidBlock(",
        "canonical PoW mapped error",
    )


def assert_consensus_rejection_control_flow() -> None:
    source = VALIDATION.read_text(encoding="utf-8")
    structural = AUDIT.rust_structural_code(source)
    validate_block = AUDIT.braced_item(structural, "pub fn validate_block")

    assert_consensus_markers_top_level(validate_block)
    assert_difficulty_guard_rejects(validate_block)
    assert_pow_failure_is_propagated(validate_block)


def assert_control_flow_self_test() -> None:
    bad_difficulty = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty { let _ = true; }
    validate_pow_header(&block.header).map_err(|_| { PulseError::InvalidBlock(String::new()) })?;
    if block.header.timestamp == 0 { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    bad_difficulty_nested = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty {
        if false { return Err(PulseError::InvalidBlock(String::new())); }
    }
    validate_pow_header(&block.header).map_err(|_| { PulseError::InvalidBlock(String::new()) })?;
    if block.header.timestamp == 0 { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    bad_pow = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty { return Err(PulseError::InvalidBlock(String::new())); }
    let _ = validate_pow_header(&block.header);
    if block.header.timestamp == 0 { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    bad_pow_wrong_mapping = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty { return Err(PulseError::InvalidBlock(String::new())); }
    validate_pow_header(&block.header).map_err(|_| {
        if false { let _ = PulseError::InvalidBlock(String::new()); }
        PulseError::BlockAlreadyExists
    })?;
    if block.header.timestamp == 0 { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    for fixture, checker, label in (
        (bad_difficulty, assert_difficulty_guard_rejects, "non-rejecting difficulty guard"),
        (
            bad_difficulty_nested,
            assert_difficulty_guard_rejects,
            "nested unreachable difficulty rejection",
        ),
        (bad_pow, assert_pow_failure_is_propagated, "ignored PoW result"),
        (
            bad_pow_wrong_mapping,
            assert_pow_failure_is_propagated,
            "PoW mapping that only mentions InvalidBlock",
        ),
    ):
        structural = AUDIT.rust_structural_code(fixture)
        body = AUDIT.braced_item(structural, "pub fn validate_block")
        try:
            checker(body)
        except RuntimeError:
            continue
        fail(f"control-flow self-test accepted {label}")

    bad_outer_difficulty = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if false {
        if block.header.difficulty != expected_difficulty { return Err(PulseError::InvalidBlock(String::new())); }
    }
    validate_pow_header(&block.header).map_err(|_| { PulseError::InvalidBlock(String::new()) })?;
    if block.header.timestamp < newest_parent_timestamp { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    bad_outer_pow = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty { return Err(PulseError::InvalidBlock(String::new())); }
    if false {
        validate_pow_header(&block.header).map_err(|_| { PulseError::InvalidBlock(String::new()) })?;
        if block.header.timestamp < newest_parent_timestamp { return Err(PulseError::InvalidBlock(String::new())); }
    }
    Ok(())
}
"""
    bad_prefixed_pow = """
pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    let expected_difficulty = 1;
    if block.header.difficulty != expected_difficulty { return Err(PulseError::InvalidBlock(String::new())); }
    bypass_validate_pow_header(&block.header).map_err(|_| { PulseError::InvalidBlock(String::new()) })?;
    if block.header.timestamp < newest_parent_timestamp { return Err(PulseError::InvalidBlock(String::new())); }
    Ok(())
}
"""
    for fixture, label in (
        (bad_outer_difficulty, "outer conditional around difficulty rejection"),
        (bad_outer_pow, "outer conditional around canonical PoW validation"),
        (bad_prefixed_pow, "prefixed helper masquerading as canonical PoW validation"),
    ):
        structural = AUDIT.rust_structural_code(fixture)
        body = AUDIT.braced_item(structural, "pub fn validate_block")
        try:
            assert_consensus_markers_top_level(body)
        except RuntimeError:
            continue
        fail(f"top-level control-flow self-test accepted {label}")


def main() -> int:
    try:
        assert_alias_detector_self_test()
        assert_control_flow_self_test()
        assert_no_vendor_runtime_aliases()
        assert_consensus_rejection_control_flow()
    except RuntimeError as exc:
        print(f"task38_consensus_enforcement=FAIL reason={exc}", file=sys.stderr)
        return 1

    print("gpu_vendor_runtime_alias_contract=PASS")
    print("consensus_difficulty_rejection_propagated=PASS")
    print("canonical_pow_rejection_propagated=PASS")
    print("consensus_critical_guards_top_level=PASS")
    print("physical_gpu_execution=NOT_CLAIMED")
    print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
    print("GPU_MINING_AMD_PASS=NOT_CLAIMED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
