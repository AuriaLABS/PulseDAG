#!/usr/bin/env python3
"""Detect clearly non-English comments in tracked source and maintenance files."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable, Iterator

SLASH_COMMENT_EXTENSIONS = {".rs", ".js", ".jsx", ".ts", ".tsx", ".c", ".h", ".cpp", ".hpp"}
HASH_COMMENT_EXTENSIONS = {".py", ".sh", ".bash", ".zsh", ".yml", ".yaml", ".toml"}
SCAN_ROOTS = ("apps", "crates", "scripts", ".github", "configs")
IGNORED_PARTS = {".git", "target", "node_modules", "vendor", "artifacts", "ci-evidence", "evidence"}

# These markers are intentionally conservative. They target unmistakable Spanish
# prose while avoiding short words that are common identifiers or acronyms.
SPANISH_MARKERS = re.compile(
    r"(?:[áéíóúüñ¿¡]|"
    r"\b(?:aquí|también|porque|cuando|donde|debe|deben|para|evitar|comprueba|"
    r"devuelve|carga|guarda|bloque|nodo|prueba|limpieza|seguridad|comentario|"
    r"función|funciones|archivo|archivos|ruta|rutas|entonces|siguiente|anterior|"
    r"ningún|ninguna|después|siempre)\b)",
    re.IGNORECASE,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="*",
        help="Optional files or directories. Defaults to tracked files in source roots.",
    )
    parser.add_argument(
        "--report",
        action="store_true",
        help="Print findings but return success. The default mode fails on findings.",
    )
    return parser.parse_args()


def tracked_files() -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "ls-files", "-z", "--", *SCAN_ROOTS],
            check=True,
            capture_output=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError) as exc:
        raise RuntimeError("git ls-files is required when paths are not provided") from exc

    return [Path(raw.decode("utf-8")) for raw in result.stdout.split(b"\0") if raw]


def expand_paths(items: Iterable[str]) -> list[Path]:
    files: list[Path] = []
    for item in items:
        path = Path(item)
        if path.is_dir():
            files.extend(candidate for candidate in path.rglob("*") if candidate.is_file())
        elif path.is_file():
            files.append(path)
    return files


def should_scan(path: Path) -> bool:
    if any(part in IGNORED_PARTS for part in path.parts):
        return False
    return path.suffix.lower() in SLASH_COMMENT_EXTENSIONS | HASH_COMMENT_EXTENSIONS


def hash_comment_fragment(line: str) -> str | None:
    stripped = line.lstrip()
    if stripped.startswith("#!"):
        return None

    quote: str | None = None
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
            continue
        if char == "\\" and quote is not None:
            escaped = True
            continue
        if char in {"'", '"'}:
            if quote is None:
                quote = char
            elif quote == char:
                quote = None
            continue
        if char == "#" and quote is None:
            return line[index + 1 :]
    return None


def slash_comment_fragments(lines: list[str]) -> Iterator[tuple[int, str]]:
    in_block = False
    for number, line in enumerate(lines, start=1):
        cursor = 0
        fragments: list[str] = []
        while cursor < len(line):
            if in_block:
                end = line.find("*/", cursor)
                if end == -1:
                    fragments.append(line[cursor:])
                    cursor = len(line)
                else:
                    fragments.append(line[cursor:end])
                    cursor = end + 2
                    in_block = False
                continue

            block = line.find("/*", cursor)
            slash = line.find("//", cursor)
            candidates = [value for value in (block, slash) if value >= 0]
            if not candidates:
                break
            marker = min(candidates)

            # Ignore URL-like markers inside quoted strings. This lightweight
            # check intentionally prefers false negatives over noisy failures.
            prefix = line[:marker]
            if prefix.count('"') % 2 == 1 or prefix.count("'") % 2 == 1:
                cursor = marker + 2
                continue

            if marker == slash:
                fragments.append(line[marker + 2 :])
                break

            end = line.find("*/", marker + 2)
            if end == -1:
                fragments.append(line[marker + 2 :])
                in_block = True
                break
            fragments.append(line[marker + 2 : end])
            cursor = end + 2

        for fragment in fragments:
            yield number, fragment


def comment_fragments(path: Path, text: str) -> Iterator[tuple[int, str]]:
    lines = text.splitlines()
    if path.suffix.lower() in SLASH_COMMENT_EXTENSIONS:
        yield from slash_comment_fragments(lines)
        return

    for number, line in enumerate(lines, start=1):
        fragment = hash_comment_fragment(line)
        if fragment is not None:
            yield number, fragment


def scan_file(path: Path) -> list[tuple[int, str]]:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return []

    findings: list[tuple[int, str]] = []
    for line_number, fragment in comment_fragments(path, text):
        if "language-check: allow" in fragment.lower():
            continue
        if SPANISH_MARKERS.search(fragment):
            findings.append((line_number, fragment.strip()))
    return findings


def main() -> int:
    args = parse_args()
    try:
        files = expand_paths(args.paths) if args.paths else tracked_files()
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    findings: list[tuple[Path, int, str]] = []
    for path in sorted(set(files)):
        if not should_scan(path):
            continue
        findings.extend((path, line, fragment) for line, fragment in scan_file(path))

    if findings:
        print("Non-English code-comment candidates detected:", file=sys.stderr)
        for path, line, fragment in findings:
            print(f"{path}:{line}: {fragment}", file=sys.stderr)
        print(
            "Rewrite technical comments in English or add 'language-check: allow' "
            "only for an intentional localized user-facing example.",
            file=sys.stderr,
        )
        return 0 if args.report else 1

    print("PASS: code comments use the English-language repository standard")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
