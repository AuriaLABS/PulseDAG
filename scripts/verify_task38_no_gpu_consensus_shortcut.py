#!/usr/bin/env python3
"""Fail-closed software audit for #1038 GPU/consensus separation.

This script proves structural properties only. It does not execute a physical GPU
and must never be used as NVIDIA/AMD hardware-equivalence evidence.
"""

from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]

NODE_MANIFEST = ROOT / "apps" / "pulsedagd" / "Cargo.toml"
CORE_MANIFEST = ROOT / "crates" / "pulsedag-core" / "Cargo.toml"
VALIDATION = ROOT / "crates" / "pulsedag-core" / "src" / "validation.rs"
TYPES = ROOT / "crates" / "pulsedag-core" / "src" / "types.rs"

ACCELERATOR_FRAGMENTS = ("gpu", "cuda", "opencl")
DISCRIMINATOR_FRAGMENTS = (
    "backend",
    "accelerator",
    "vendor",
    "device",
    "gpu",
    "cuda",
    "opencl",
)


def fail(message: str) -> None:
    raise RuntimeError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"required path missing: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def masked(segment: str) -> str:
    """Blank a lexical segment without changing its offsets or line structure."""

    return "".join("\n" if char == "\n" else " " for char in segment)


def raw_literal_end(source: str, start: int) -> int | None:
    """Return the exclusive end of a Rust raw string/byte/C-string literal."""

    if source.startswith("br", start) or source.startswith("cr", start):
        cursor = start + 2
    elif source.startswith("r", start):
        cursor = start + 1
    else:
        return None

    hash_start = cursor
    while cursor < len(source) and source[cursor] == "#":
        cursor += 1
    hashes = cursor - hash_start
    if cursor >= len(source) or source[cursor] != '"':
        return None

    terminator = '"' + ("#" * hashes)
    closing = source.find(terminator, cursor + 1)
    if closing < 0:
        fail("unterminated Rust raw string while auditing consensus sources")
    return closing + len(terminator)


def quoted_literal_end(source: str, start: int) -> int:
    """Return the exclusive end of a normal Rust string/byte/C-string literal."""

    escaped = False
    for pos in range(start + 1, len(source)):
        char = source[pos]
        if escaped:
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == '"':
            return pos + 1
    fail("unterminated Rust string while auditing consensus sources")
    raise AssertionError("unreachable")


def char_literal_end(source: str, start: int) -> int | None:
    """Return the exclusive end of a Rust char literal, without eating lifetimes."""

    if start + 2 < len(source) and source[start + 2] == "'":
        return start + 3
    if start + 1 >= len(source) or source[start + 1] != "\\":
        return None

    line_end = source.find("\n", start + 1)
    limit = len(source) if line_end < 0 else line_end
    limit = min(limit, start + 32)
    for pos in range(start + 2, limit):
        if source[pos] != "'":
            continue
        backslashes = 0
        cursor = pos - 1
        while cursor > start and source[cursor] == "\\":
            backslashes += 1
            cursor -= 1
        if backslashes % 2 == 0:
            return pos + 1
    return None


def rust_lexical_view(source: str, *, preserve_literals: bool) -> str:
    """Mask Rust comments and optionally literals while preserving byte offsets.

    Raw strings, nested block comments and char literals are handled explicitly.
    The comment-only view retains string contents so executable configuration such
    as `#[cfg(feature = "gpu")]` remains visible. The structural view masks string
    and char contents so text decoys cannot satisfy code-shape assertions.
    """

    out: list[str] = []
    i = 0
    while i < len(source):
        if source.startswith("//", i):
            newline = source.find("\n", i + 2)
            end = len(source) if newline < 0 else newline + 1
            out.append(masked(source[i:end]))
            i = end
            continue

        if source.startswith("/*", i):
            depth = 1
            cursor = i + 2
            while cursor < len(source) and depth:
                if source.startswith("/*", cursor):
                    depth += 1
                    cursor += 2
                elif source.startswith("*/", cursor):
                    depth -= 1
                    cursor += 2
                else:
                    cursor += 1
            if depth:
                fail("unterminated Rust block comment while auditing consensus sources")
            out.append(masked(source[i:cursor]))
            i = cursor
            continue

        raw_end = raw_literal_end(source, i)
        if raw_end is not None:
            segment = source[i:raw_end]
            out.append(segment if preserve_literals else masked(segment))
            i = raw_end
            continue

        if source[i] == '"':
            end = quoted_literal_end(source, i)
            segment = source[i:end]
            out.append(segment if preserve_literals else masked(segment))
            i = end
            continue

        if source[i] == "'":
            end = char_literal_end(source, i)
            if end is not None:
                segment = source[i:end]
                out.append(segment if preserve_literals else masked(segment))
                i = end
                continue

        out.append(source[i])
        i += 1

    return "".join(out)


def rust_code_without_comments(source: str) -> str:
    """Mask Rust comments while retaining executable/configuration string text."""

    return rust_lexical_view(source, preserve_literals=True)


def rust_structural_code(source: str) -> str:
    """Mask comments and literal contents for code-shape assertions."""

    return rust_lexical_view(source, preserve_literals=False)


def contains_fragment(text: str, fragments: tuple[str, ...]) -> str | None:
    """Return the first forbidden semantic fragment, case-insensitively.

    Substring matching is deliberate and fail-closed. Rust identifiers commonly
    join semantic words with `_` or CamelCase, so word-boundary regexes can miss
    names such as `gpu_backend`, `cudaEnabled`, `DeviceKind`, or `backend_id`.
    """

    lowered = text.casefold()
    for fragment in fragments:
        if fragment.casefold() in lowered:
            return fragment
    return None


def assert_detector_self_test() -> None:
    executable_fixtures = (
        'let gpu_backend = true;',
        'let cuda_enabled = true;',
        'let opencl_device = 0;',
        'struct GpuBackend;',
        '#[cfg(feature = "gpu")] fn guarded() {}',
        'let config = r#"literal quote: \" // gpu /* cuda */ opencl"#;',
    )
    for fixture in executable_fixtures:
        code = rust_code_without_comments(fixture)
        if contains_fragment(code, ACCELERATOR_FRAGMENTS) is None:
            fail(f"accelerator detector self-test missed executable fixture: {fixture}")

    discriminator_fixtures = (
        "backend_id: u8,",
        "DeviceKind: u8,",
        "vendor_code: u16,",
        "acceleratorMode: bool,",
        "gpu_backend: u8,",
    )
    for fixture in discriminator_fixtures:
        if contains_fragment(fixture, DISCRIMINATOR_FRAGMENTS) is None:
            fail(f"discriminator detector self-test missed fixture: {fixture}")

    comment_only = """
// gpu_backend cuda_enabled opencl_device
/* backend_id DeviceKind vendor_code GPU CUDA OpenCL */
let ordinary_consensus_value = 1u64;
"""
    stripped = rust_code_without_comments(comment_only)
    if contains_fragment(stripped, ACCELERATOR_FRAGMENTS) is not None:
        fail("accelerator detector self-test treated comment-only text as executable")
    if contains_fragment(stripped, DISCRIMINATOR_FRAGMENTS) is not None:
        fail("discriminator detector self-test treated comment-only text as executable")

    structural_fixture = r'''
pub fn validate_block(block: &Block, state: &ChainState) {
    let decoy = r#"{ block.header.difficulty != expected_difficulty validate_pow_header(&block.header) }"#;
    let closing_brace = '}';
    let still_inside_function = true;
}
'''
    structural = rust_structural_code(structural_fixture)
    validate_block = braced_item(structural, "pub fn validate_block")
    if "validate_pow_header(&block.header)" in validate_block:
        fail("structural detector accepted a string-only canonical PoW decoy")
    if "block.header.difficulty != expected_difficulty" in validate_block:
        fail("structural detector accepted a string-only difficulty-guard decoy")
    if "still_inside_function" not in validate_block:
        fail("structural detector let a char/string brace truncate the function body")


def braced_item(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        fail(f"required marker missing: {marker}")
    brace = source.find("{", start)
    if brace < 0:
        fail(f"opening brace missing after marker: {marker}")
    depth = 0
    for pos in range(brace, len(source)):
        char = source[pos]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start : pos + 1]
    fail(f"unterminated braced item: {marker}")
    raise AssertionError("unreachable")


def aligned_braced_item(visible: str, structural: str, marker: str) -> tuple[str, str]:
    """Extract aligned comment-only and literal-free views of one Rust item."""

    start = structural.find(marker)
    structural_item = braced_item(structural, marker)
    return visible[start : start + len(structural_item)], structural_item


def assert_manifest_separation() -> None:
    forbidden = (
        "pulsedag-miner",
        "pulsedag_miner",
        "gpu",
        "cuda",
        "opencl",
        "libloading",
    )
    for path in (NODE_MANIFEST, CORE_MANIFEST):
        text = read(path).casefold()
        for token in forbidden:
            if token in text:
                fail(
                    f"accelerator/miner dependency token {token!r} found in "
                    f"{path.relative_to(ROOT)}"
                )


def assert_consensus_sources_have_no_accelerator_path() -> None:
    forbidden = ACCELERATOR_FRAGMENTS + ("pulsedag-miner", "pulsedag_miner")
    roots = (
        ROOT / "apps" / "pulsedagd" / "src",
        ROOT / "crates" / "pulsedag-core" / "src",
    )
    for source_root in roots:
        for path in sorted(source_root.rglob("*.rs")):
            code = rust_code_without_comments(read(path))
            token = contains_fragment(code, forbidden)
            if token is not None:
                fail(
                    f"accelerator/miner executable token {token!r} referenced by "
                    f"consensus/node code: {path.relative_to(ROOT)}"
                )


def assert_header_has_no_backend_discriminator() -> None:
    raw_types = read(TYPES)
    types_visible = rust_code_without_comments(raw_types)
    types_structural = rust_structural_code(raw_types)

    header, _ = aligned_braced_item(
        types_visible, types_structural, "pub struct BlockHeader"
    )
    token = contains_fragment(header, DISCRIMINATOR_FRAGMENTS)
    if token is not None:
        fail(f"BlockHeader contains backend-specific discriminator token: {token}")

    serialization, serialization_structural = aligned_braced_item(
        types_visible, types_structural, "pub fn canonical_block_header_bytes"
    )
    if (
        "header.nonce" not in serialization_structural
        or "header.difficulty" not in serialization_structural
    ):
        fail("canonical block-header serialization is missing nonce/difficulty")
    token = contains_fragment(serialization, DISCRIMINATOR_FRAGMENTS)
    if token is not None:
        fail(f"canonical block-header serialization references backend token: {token}")


def assert_validate_block_uses_canonical_pow() -> None:
    validation = rust_structural_code(read(VALIDATION))
    validate_block = braced_item(validation, "pub fn validate_block")

    signature = validate_block.split("{", 1)[0]
    if "block: &Block" not in signature or "state: &ChainState" not in signature:
        fail("validate_block signature no longer matches block/state-only consensus authority")
    token = contains_fragment(signature, DISCRIMINATOR_FRAGMENTS)
    if token is not None:
        fail(f"validate_block signature gained backend-specific input: {token}")

    pow_call = "validate_pow_header(&block.header)"
    if validate_block.count(pow_call) != 1:
        fail("validate_block must contain exactly one canonical validate_pow_header call")

    expected_pos = validate_block.find("expected_difficulty")
    difficulty_guard_pos = validate_block.find(
        "block.header.difficulty != expected_difficulty"
    )
    pow_pos = validate_block.find(pow_call)
    if not (0 <= expected_pos < difficulty_guard_pos < pow_pos):
        fail("expected-difficulty guard must precede canonical PoW validation")


def main() -> int:
    try:
        assert_detector_self_test()
        assert_manifest_separation()
        assert_consensus_sources_have_no_accelerator_path()
        assert_header_has_no_backend_discriminator()
        assert_validate_block_uses_canonical_pow()
    except RuntimeError as exc:
        print(f"task38_no_gpu_consensus_shortcut=FAIL reason={exc}", file=sys.stderr)
        return 1

    print("detector_negative_contract=PASS")
    print("node_consensus_accelerator_independent=PASS")
    print("canonical_pow_validation_authority=PASS")
    print("node_miner_dependency_absent=PASS")
    print("consensus_header_backend_discriminator_absent=PASS")
    print("physical_gpu_execution=NOT_CLAIMED")
    print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
    print("GPU_MINING_AMD_PASS=NOT_CLAIMED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
