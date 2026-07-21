#!/usr/bin/env python3
"""Select and flatten the exact v2.3.0 candidate asset set."""

from __future__ import annotations

import argparse
import shutil
from collections import Counter
from pathlib import Path

ARCHIVE_SUFFIXES = (".tar.gz", ".zip")
SIDECAR_SUFFIXES = (".tar.gz.sha256", ".zip.sha256", ".tar.gz.json", ".zip.json")
EXPECTED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-apple-darwin",
}
EXPECTED_BINARIES = {"pulsedagd", "pulsedag-miner"}


def is_candidate_file(path: Path) -> bool:
    name = path.name
    return name.endswith(ARCHIVE_SUFFIXES + SIDECAR_SUFFIXES)


def classify(name: str) -> tuple[str, str, str]:
    suffix = next((item for item in SIDECAR_SUFFIXES + ARCHIVE_SUFFIXES if name.endswith(item)), None)
    if suffix is None:
        raise ValueError(f"unsupported candidate filename: {name}")
    stem = name[: -len(suffix)]
    prefix = "-v2.3.0-"
    if prefix not in stem:
        raise ValueError(f"candidate filename does not contain {prefix}: {name}")
    binary, target = stem.split(prefix, 1)
    return binary, target, suffix


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--dest", type=Path, required=True)
    args = parser.parse_args()

    all_files = sorted(path for path in args.source.rglob("*") if path.is_file())
    selected = [path for path in all_files if is_candidate_file(path)]
    ignored = [path for path in all_files if path not in selected]

    if ignored:
        print("Ignoring non-release files added by artifact transport:")
        for path in ignored:
            print(f" - {path}")

    names = [path.name for path in selected]
    duplicates = sorted(name for name, count in Counter(names).items() if count > 1)
    if duplicates:
        raise SystemExit(f"duplicate candidate asset names: {duplicates}")
    if len(selected) != 18:
        raise SystemExit(f"expected 18 candidate asset files, found {len(selected)}: {names}")

    observed: dict[tuple[str, str], set[str]] = {}
    for path in selected:
        binary, target, suffix = classify(path.name)
        if binary not in EXPECTED_BINARIES:
            raise SystemExit(f"unexpected candidate binary: {binary}")
        if target not in EXPECTED_TARGETS:
            raise SystemExit(f"unexpected candidate target: {target}")
        observed.setdefault((binary, target), set()).add(suffix)

    expected_suffixes_by_target = {
        target: ({".zip", ".zip.sha256", ".zip.json"} if "windows" in target else {".tar.gz", ".tar.gz.sha256", ".tar.gz.json"})
        for target in EXPECTED_TARGETS
    }
    for binary in EXPECTED_BINARIES:
        for target in EXPECTED_TARGETS:
            actual = observed.get((binary, target), set())
            expected = expected_suffixes_by_target[target]
            if actual != expected:
                raise SystemExit(
                    f"incomplete candidate set for {binary}/{target}: expected {sorted(expected)}, found {sorted(actual)}"
                )

    if args.dest.exists():
        shutil.rmtree(args.dest)
    args.dest.mkdir(parents=True)
    for path in selected:
        shutil.copy2(path, args.dest / path.name)

    print(f"Flattened {len(selected)} verified candidate files into {args.dest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
