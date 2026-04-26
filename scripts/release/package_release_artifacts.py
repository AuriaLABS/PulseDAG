#!/usr/bin/env python3
"""Package release artifacts and emit checksums with provenance metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import stat
import tarfile
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def detect_target() -> str:
    machine = platform.machine().lower()
    system = platform.system().lower()

    if machine in {"amd64", "x86_64", "x64"}:
        arch = "x86_64"
    elif machine in {"arm64", "aarch64"}:
        arch = "aarch64"
    else:
        arch = machine or "unknown"

    if system == "linux":
        return f"{arch}-unknown-linux-gnu"
    if system == "darwin":
        return f"{arch}-apple-darwin"
    if system == "windows":
        return f"{arch}-pc-windows-msvc"
    return f"{arch}-{system}"


def ensure_executable(path: Path) -> None:
    if platform.system().lower() == "windows":
        return
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--bin-name", default="pulsedagd")
    parser.add_argument("--repository", default="")
    parser.add_argument("--commit", default="")
    parser.add_argument("--run-id", default="")
    parser.add_argument("--run-attempt", default="")
    args = parser.parse_args()

    binary_path = args.binary.resolve()
    if not binary_path.exists():
        raise SystemExit(f"Binary not found: {binary_path}")

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    target = detect_target()
    archive_base = f"{args.bin_name}-{args.tag}-{target}"
    is_windows = platform.system().lower() == "windows"
    staged_binary_name = f"{args.bin_name}.exe" if is_windows else args.bin_name

    stage_dir = output_dir / archive_base
    if stage_dir.exists():
        shutil.rmtree(stage_dir)
    stage_dir.mkdir(parents=True)

    staged_binary = stage_dir / staged_binary_name
    shutil.copy2(binary_path, staged_binary)
    ensure_executable(staged_binary)

    if is_windows:
        archive_path = output_dir / f"{archive_base}.zip"
        with zipfile.ZipFile(archive_path, mode="w", compression=zipfile.ZIP_DEFLATED) as ziph:
            ziph.write(staged_binary, arcname=f"{archive_base}/{staged_binary_name}")
    else:
        archive_path = output_dir / f"{archive_base}.tar.gz"
        with tarfile.open(archive_path, mode="w:gz") as tar:
            tar.add(staged_binary, arcname=f"{archive_base}/{staged_binary_name}")

    checksum = sha256_file(archive_path)
    checksum_file = output_dir / f"{archive_path.name}.sha256"
    checksum_file.write_text(f"{checksum}  {archive_path.name}\n", encoding="utf-8")

    manifest_file = output_dir / f"{archive_path.name}.json"
    file_size_bytes = archive_path.stat().st_size
    manifest_file.write_text(
        json.dumps(
            {
                "tag": args.tag,
                "archive": archive_path.name,
                "archive_sha256": checksum,
                "archive_size_bytes": file_size_bytes,
                "target": target,
                "binary": staged_binary_name,
                "provenance": {
                    "repository": args.repository,
                    "commit": args.commit,
                    "github_run_id": args.run_id,
                    "github_run_attempt": args.run_attempt,
                },
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )

    shutil.rmtree(stage_dir)

    print(f"Packaged: {archive_path}")
    print(f"Checksum: {checksum_file}")
    print(f"Manifest: {manifest_file}")


if __name__ == "__main__":
    main()
