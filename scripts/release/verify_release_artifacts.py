#!/usr/bin/env python3
"""End-to-end verification for packaged release artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_sha256_file(path: Path) -> tuple[str, str]:
    lines = [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(lines) != 1:
        raise SystemExit(f"Checksum file must contain exactly one entry: {path}")

    parts = lines[0].split()
    if len(parts) < 2:
        raise SystemExit(f"Malformed checksum line in {path}: {lines[0]}")

    return parts[0], parts[-1].lstrip("*./")


def unpack_archive(archive: Path, destination: Path) -> None:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive, "r") as zf:
            zf.extractall(destination)
        return

    if archive.suffixes[-2:] == [".tar", ".gz"]:
        with tarfile.open(archive, "r:gz") as tf:
            tf.extractall(destination)
        return

    raise SystemExit(f"Unsupported archive format: {archive.name}")


def run_smoke(binary: Path, binary_name: str) -> None:
    args = [str(binary), "--version"]
    allow_usage_exit_one = False
    if binary_name == "pulsedag-miner":
        args = [str(binary), "--help"]
        allow_usage_exit_one = True

    result = subprocess.run(args, check=False, capture_output=True, text=True)
    if allow_usage_exit_one and result.returncode == 1:
        output = f"{result.stdout}\n{result.stderr}".lower()
        if "usage:" in output and "pulsedag-miner" in output:
            return

    if result.returncode != 0:
        raise SystemExit(
            f"Smoke command failed for {binary_name} ({' '.join(args)}):\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )


def validate_archive_artifact(archive: Path, expected_tag: str, smoke: bool) -> dict:
    checksum_path = archive.with_name(f"{archive.name}.sha256")
    manifest_path = archive.with_name(f"{archive.name}.json")

    if not checksum_path.exists():
        raise SystemExit(f"Missing checksum sidecar for {archive.name}: {checksum_path.name}")
    if not manifest_path.exists():
        raise SystemExit(f"Missing manifest sidecar for {archive.name}: {manifest_path.name}")

    checksum_from_file, checksum_filename = parse_sha256_file(checksum_path)
    if checksum_filename != archive.name:
        raise SystemExit(
            f"Checksum file {checksum_path.name} points to {checksum_filename}, expected {archive.name}"
        )

    actual_sha = sha256_file(archive)
    if checksum_from_file != actual_sha:
        raise SystemExit(f"SHA256 mismatch for {archive.name}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("tag") != expected_tag:
        raise SystemExit(f"Manifest tag mismatch in {manifest_path.name}: {manifest.get('tag')} != {expected_tag}")
    if manifest.get("archive") != archive.name:
        raise SystemExit(f"Manifest archive mismatch in {manifest_path.name}")
    if manifest.get("archive_sha256") != actual_sha:
        raise SystemExit(f"Manifest digest mismatch in {manifest_path.name}")
    if manifest.get("archive_size_bytes") != archive.stat().st_size:
        raise SystemExit(f"Manifest size mismatch in {manifest_path.name}")

    provenance = manifest.get("provenance", {})
    for key in ["repository", "commit", "github_run_id", "github_run_attempt"]:
        if not provenance.get(key):
            raise SystemExit(f"Manifest missing provenance.{key} in {manifest_path.name}")

    archive_base = archive.name.removesuffix(".tar.gz").removesuffix(".zip")
    binary_name = manifest.get("binary")
    if binary_name not in {"pulsedagd", "pulsedagd.exe", "pulsedag-miner", "pulsedag-miner.exe"}:
        raise SystemExit(f"Unexpected binary name in {manifest_path.name}: {binary_name}")

    with tempfile.TemporaryDirectory(prefix="release-verify-") as tmp_dir:
        temp_root = Path(tmp_dir)
        unpack_archive(archive, temp_root)
        top_dir = temp_root / archive_base
        if not top_dir.is_dir():
            raise SystemExit(f"Archive {archive.name} missing top-level folder {archive_base}/")
        binary_path = top_dir / binary_name
        if not binary_path.exists():
            raise SystemExit(f"Archive {archive.name} missing binary {binary_name}")

        entries = [p for p in top_dir.iterdir()]
        if len(entries) != 1 or entries[0].name != binary_name:
            raise SystemExit(f"Archive {archive.name} must contain exactly one binary inside {archive_base}/")

        if smoke:
            run_smoke(binary_path, binary_name.removesuffix('.exe'))

    return manifest


def validate_consolidated_checksums(archives: list[Path], checksum_list: Path) -> None:
    if not checksum_list.exists():
        raise SystemExit(f"Missing consolidated checksum list: {checksum_list}")

    expected = {archive.name: sha256_file(archive) for archive in archives}
    parsed: dict[str, str] = {}
    for line in checksum_list.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            raise SystemExit(f"Malformed checksum line in {checksum_list}: {line}")
        parsed[parts[-1].lstrip("*./")] = parts[0]

    for name, digest in expected.items():
        if parsed.get(name) != digest:
            raise SystemExit(f"Consolidated checksum mismatch or missing entry for {name}")


def validate_provenance_summary(manifests: list[dict], provenance_path: Path, expected_tag: str) -> None:
    if not provenance_path.exists():
        raise SystemExit(f"Missing release provenance summary: {provenance_path}")

    payload = json.loads(provenance_path.read_text(encoding="utf-8"))
    if payload.get("release_tag") != expected_tag:
        raise SystemExit("release-provenance.json release_tag does not match expected tag")

    actual = sorted(manifests, key=lambda item: item["archive"])
    summarized = sorted(payload.get("artifacts", []), key=lambda item: item.get("archive", ""))
    if actual != summarized:
        raise SystemExit("release-provenance.json artifacts do not match per-archive manifests")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", required=True, type=Path)
    parser.add_argument("--expected-tag", required=True)
    parser.add_argument("--smoke", action="store_true", help="Run extracted binaries with basic smoke commands")
    parser.add_argument("--expect-binaries", nargs="*", default=["pulsedagd", "pulsedag-miner"])
    parser.add_argument("--expect-targets", nargs="*", default=[])
    parser.add_argument("--consolidated-checksums", type=Path)
    parser.add_argument("--provenance-summary", type=Path)
    args = parser.parse_args()

    artifacts_dir = args.artifacts_dir.resolve()
    if not artifacts_dir.is_dir():
        raise SystemExit(f"Artifacts directory not found: {artifacts_dir}")

    archives = sorted([*artifacts_dir.glob("*.tar.gz"), *artifacts_dir.glob("*.zip")])
    if not archives:
        raise SystemExit(f"No release archives found in {artifacts_dir}")

    manifests = [validate_archive_artifact(archive, args.expected_tag, args.smoke) for archive in archives]

    found = {manifest["binary"].removesuffix(".exe") for manifest in manifests}
    missing = sorted(set(args.expect_binaries) - found)
    if missing:
        raise SystemExit(f"Missing expected binary artifacts: {', '.join(missing)}")

    if args.expect_targets:
        found_targets = {manifest["target"] for manifest in manifests}
        missing_targets = sorted(set(args.expect_targets) - found_targets)
        if missing_targets:
            raise SystemExit(f"Missing expected target artifacts: {', '.join(missing_targets)}")

    if args.consolidated_checksums:
        validate_consolidated_checksums(archives, args.consolidated_checksums.resolve())

    if args.provenance_summary:
        validate_provenance_summary(manifests, args.provenance_summary.resolve(), args.expected_tag)

    print(f"Validated {len(archives)} archive(s) in {artifacts_dir}")


if __name__ == "__main__":
    main()
