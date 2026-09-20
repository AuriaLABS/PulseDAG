#!/usr/bin/env python3
"""Collect fail-closed Task 38 physical evidence from a packaged GPU probe."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shlex
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

FORBIDDEN_OVERRIDES = (
    "PULSEDAG_CUDA_DRIVER_LIBRARY",
    "PULSEDAG_OPENCL_LIBRARY",
)
FINAL_NONCLAIMS = (
    "GPU_MINING_NVIDIA_PASS=NOT_CLAIMED",
    "GPU_MINING_AMD_PASS=NOT_CLAIMED",
)
PROTOCOL_CASES = ("legacy_v1", "activated_v2")
EXPECTED_REPOSITORY = "AuriaLABS/PulseDAG"
EXPECTED_PACKAGE_TAG = "v3-task38-phase24"
CUDA_MODULE_NAME = "pulsedag-kheavyhash-launch.ptx"
CUDA_MODULE_METADATA_NAME = "pulsedag-kheavyhash-launch.ptx.json"
CUDA_MODULE_SOURCE = "apps/pulsedag-miner/cuda/kheavyhash_launch.cu"
CUDA_MODULE_ARCH = "sm_60"
CUDA_MODULE_CONTAINER = "nvidia/cuda:12.9.2-devel-ubuntu24.04"
PROBE_NAMES = (
    "pulsedag-gpu-equivalence-probe",
    "pulsedag-gpu-equivalence-probe.exe",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def archive_base_name(archive: Path) -> str:
    name = archive.name
    if name.endswith(".tar.gz"):
        return name[:-7]
    if name.endswith(".zip"):
        return name[:-4]
    raise RuntimeError(f"unsupported archive format: {archive}")


def _safe_destination(destination: Path, member_name: str) -> Path:
    root = destination.resolve()
    candidate = (destination / member_name).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise RuntimeError(f"archive member escapes extraction root: {member_name!r}") from exc
    return candidate


def unpack_archive(archive: Path, destination: Path) -> None:
    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive, "r") as handle:
            for member in handle.infolist():
                _safe_destination(destination, member.filename)
            handle.extractall(destination)
        return
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as handle:
            for member in handle.getmembers():
                _safe_destination(destination, member.name)
                if member.issym() or member.islnk():
                    raise RuntimeError(
                        f"archive contains unsupported link member: {member.name!r}"
                    )
            handle.extractall(destination)
        return
    raise RuntimeError(f"unsupported archive format: {archive}")


def require_marker(output: str, marker: str) -> None:
    if marker not in output:
        raise RuntimeError(f"required evidence marker missing: {marker}")


def parse_marker_line(line: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for token in line.strip().split():
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        fields[key] = value
    return fields


def validate_probe_output(output: str, backend: str, require_amd_opencl: bool) -> None:
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    summaries = [
        parse_marker_line(line)
        for line in lines
        if line.startswith("physical_gpu_equivalence_probe=PASS")
    ]
    if len(summaries) != 1:
        raise RuntimeError(
            f"expected exactly one physical probe PASS summary, found {len(summaries)}"
        )
    summary = summaries[0]
    if summary.get("canonical_vectors_per_protocol") != "7":
        raise RuntimeError(
            "physical probe summary must record canonical_vectors_per_protocol=7"
        )
    if summary.get("protocols") != "2" or summary.get("cpu_reference") != "PASS":
        raise RuntimeError("physical probe summary protocol/CPU contract is incomplete")

    case_lines: dict[str, dict[str, str]] = {}
    for case in PROTOCOL_CASES:
        matches = [
            parse_marker_line(line)
            for line in lines
            if line.startswith(f"physical_vector_case={case} ")
        ]
        if len(matches) != 1:
            raise RuntimeError(
                f"expected exactly one physical vector line for {case}, found {len(matches)}"
            )
        case_lines[case] = matches[0]

    for case, fields in case_lines.items():
        if fields.get("nonces") != "7" or fields.get("cpu_reference") != "PASS":
            raise RuntimeError(f"{case} vector line is missing 7-nonce CPU-reference PASS")
        expected_cuda = "PASS" if backend in {"cuda", "both"} else "NOT_RUN"
        expected_opencl = "PASS" if backend in {"opencl", "both"} else "NOT_RUN"
        expected_cross = "PASS" if backend == "both" else "NOT_RUN"
        if fields.get("cuda_exact_match") != expected_cuda:
            raise RuntimeError(
                f"{case} cuda_exact_match expected {expected_cuda}, got {fields.get('cuda_exact_match')!r}"
            )
        if fields.get("opencl_exact_match") != expected_opencl:
            raise RuntimeError(
                f"{case} opencl_exact_match expected {expected_opencl}, got {fields.get('opencl_exact_match')!r}"
            )
        if fields.get("cuda_opencl_same_input") != expected_cross:
            raise RuntimeError(
                f"{case} cuda_opencl_same_input expected {expected_cross}, got {fields.get('cuda_opencl_same_input')!r}"
            )

    if backend in {"cuda", "both"}:
        require_marker(output, "physical_cuda_identity=PASS")
        if summary.get("cuda_execution") != "PASS":
            raise RuntimeError("physical probe summary missing cuda_execution=PASS")
    elif summary.get("cuda_execution") != "NOT_RUN":
        raise RuntimeError("CUDA execution must be NOT_RUN when CUDA backend is not selected")

    if backend in {"opencl", "both"}:
        require_marker(output, "physical_opencl_identity=PASS")
        if summary.get("opencl_execution") != "PASS":
            raise RuntimeError("physical probe summary missing opencl_execution=PASS")
    elif summary.get("opencl_execution") != "NOT_RUN":
        raise RuntimeError("OpenCL execution must be NOT_RUN when OpenCL backend is not selected")

    expected_summary_cross = "PASS" if backend == "both" else "NOT_RUN"
    if summary.get("cross_backend_same_input") != expected_summary_cross:
        raise RuntimeError(
            "physical probe summary cross-backend marker does not match selected backend mode"
        )

    if require_amd_opencl:
        require_marker(output, "amd_identity=PASS")

    for marker in FINAL_NONCLAIMS:
        if summary.get(marker.split("=", 1)[0]) != "NOT_CLAIMED":
            raise RuntimeError(f"physical probe summary missing final non-claim: {marker}")
    if "GPU_MINING_NVIDIA_PASS=true" in output or "GPU_MINING_AMD_PASS=true" in output:
        raise RuntimeError("collector refuses premature final GPU PASS promotion")


def validate_environment() -> dict[str, str | None]:
    overrides = {name: os.environ.get(name) for name in FORBIDDEN_OVERRIDES}
    active = {name: value for name, value in overrides.items() if value and value.strip()}
    if active:
        raise RuntimeError(
            "repository runtime override hook(s) set: " + ", ".join(sorted(active))
        )
    return overrides


def expected_native_target() -> str:
    machine = platform.machine().lower()
    arch = (
        "x86_64"
        if machine in {"amd64", "x86_64", "x64"}
        else "aarch64"
        if machine in {"arm64", "aarch64"}
        else machine or "unknown"
    )
    system = platform.system().lower()
    if system == "linux":
        return f"{arch}-unknown-linux-gnu"
    if system == "windows":
        return f"{arch}-pc-windows-msvc"
    raise RuntimeError(
        f"physical packaged evidence collector supports native Linux/Windows hosts, got {system!r}"
    )


def validate_package_binding(
    archive: Path, manifest_path: Path, candidate_sha: str
) -> dict:
    if len(candidate_sha) != 40 or any(
        char not in "0123456789abcdefABCDEF" for char in candidate_sha
    ):
        raise RuntimeError("candidate SHA must be an exact 40-character hexadecimal commit")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("tag") != EXPECTED_PACKAGE_TAG:
        raise RuntimeError(
            f"artifact tag {manifest.get('tag')!r} does not match {EXPECTED_PACKAGE_TAG!r}"
        )
    if manifest.get("archive") != archive.name:
        raise RuntimeError(
            f"manifest archive {manifest.get('archive')!r} does not match {archive.name!r}"
        )
    actual_sha = sha256_file(archive)
    if manifest.get("archive_sha256") != actual_sha:
        raise RuntimeError("artifact manifest SHA256 does not match archive bytes")
    if manifest.get("archive_size_bytes") != archive.stat().st_size:
        raise RuntimeError("artifact manifest size does not match archive bytes")
    if manifest.get("binary") not in PROBE_NAMES:
        raise RuntimeError(
            f"artifact manifest binary is not the GPU equivalence probe: {manifest.get('binary')!r}"
        )
    if manifest.get("build_features") != ["cuda", "gpu"]:
        raise RuntimeError(
            "artifact manifest must record build_features ['cuda', 'gpu']"
        )
    included_files = manifest.get("included_files")
    expected_included = sorted([CUDA_MODULE_NAME, CUDA_MODULE_METADATA_NAME])
    if sorted(included_files or []) != expected_included:
        raise RuntimeError(
            f"artifact manifest must bind packaged CUDA module sidecars {expected_included!r}; got {included_files!r}"
        )
    expected_target = expected_native_target()
    if manifest.get("target") != expected_target:
        raise RuntimeError(
            f"artifact target {manifest.get('target')!r} does not match native host {expected_target!r}"
        )
    if platform.system().lower() == "windows":
        if not archive.name.endswith(".zip"):
            raise RuntimeError("native Windows evidence requires a ZIP package")
    elif not archive.name.endswith(".tar.gz"):
        raise RuntimeError("native Linux evidence requires a tar.gz package")

    provenance = manifest.get("provenance", {})
    if provenance.get("repository") != EXPECTED_REPOSITORY:
        raise RuntimeError(
            f"artifact repository {provenance.get('repository')!r} does not match {EXPECTED_REPOSITORY!r}"
        )
    if provenance.get("commit") != candidate_sha:
        raise RuntimeError(
            f"artifact manifest commit {provenance.get('commit')!r} does not match candidate {candidate_sha!r}"
        )
    for key in ("repository", "github_run_id", "github_run_attempt"):
        if not provenance.get(key):
            raise RuntimeError(f"artifact manifest missing provenance.{key}")
    return manifest


def locate_packaged_payload(
    archive: Path,
    destination: Path,
    manifest: dict,
    candidate_sha: str,
) -> tuple[Path, Path, dict]:
    unpack_archive(archive, destination)
    top = destination / archive_base_name(archive)
    if not top.is_dir():
        raise RuntimeError(f"archive missing top-level directory {top.name}/")

    probe = top / manifest["binary"]
    if not probe.is_file():
        raise RuntimeError(f"archive missing packaged probe {manifest['binary']}")

    cuda_module = top / CUDA_MODULE_NAME
    cuda_metadata_path = top / CUDA_MODULE_METADATA_NAME
    if not cuda_module.is_file():
        raise RuntimeError(f"archive missing packaged CUDA module {CUDA_MODULE_NAME}")
    if not cuda_metadata_path.is_file():
        raise RuntimeError(
            f"archive missing packaged CUDA module metadata {CUDA_MODULE_METADATA_NAME}"
        )

    cuda_metadata = json.loads(cuda_metadata_path.read_text(encoding="utf-8"))
    if cuda_metadata.get("schema") != "pulsedag-task38-cuda-module-v1":
        raise RuntimeError("packaged CUDA module metadata schema is invalid")
    if cuda_metadata.get("candidate_sha") != candidate_sha:
        raise RuntimeError(
            f"packaged CUDA module candidate {cuda_metadata.get('candidate_sha')!r} does not match {candidate_sha!r}"
        )
    if cuda_metadata.get("repository") != EXPECTED_REPOSITORY:
        raise RuntimeError("packaged CUDA module repository provenance is invalid")
    if cuda_metadata.get("source") != CUDA_MODULE_SOURCE:
        raise RuntimeError("packaged CUDA module source provenance is invalid")
    if cuda_metadata.get("ptx_arch") != CUDA_MODULE_ARCH:
        raise RuntimeError("packaged CUDA module PTX architecture is invalid")
    if cuda_metadata.get("compiler_container") != CUDA_MODULE_CONTAINER:
        raise RuntimeError("packaged CUDA module compiler provenance is invalid")
    actual_cuda_sha = sha256_file(cuda_module)
    if cuda_metadata.get("sha256") != actual_cuda_sha:
        raise RuntimeError("packaged CUDA module SHA256 does not match metadata")

    return probe, cuda_module, cuda_metadata


def command_display(command: list[str]) -> str:
    return subprocess.list2cmdline(command) if os.name == "nt" else shlex.join(command)


def build_command(
    args: argparse.Namespace,
    probe: Path,
    packaged_cuda_module: Path,
) -> list[str]:
    command = [str(probe), "--backend", args.backend]
    if args.backend in {"cuda", "both"}:
        command.extend(["--cuda-module", str(packaged_cuda_module)])
        command.extend(["--cuda-device", str(args.cuda_device)])
    if args.backend in {"opencl", "both"}:
        command.extend(["--opencl-device", str(args.opencl_device)])
    if args.require_amd_opencl:
        if args.backend not in {"opencl", "both"}:
            raise RuntimeError("--require-amd-opencl requires opencl/both backend")
        command.append("--require-amd-opencl")
    return command


def run_package_help(probe: Path, timeout_secs: int) -> None:
    completed = subprocess.run(
        [str(probe), "--help"],
        check=False,
        capture_output=True,
        text=True,
        timeout=timeout_secs,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"packaged probe --help failed with exit code {completed.returncode}"
        )
    output = completed.stdout + "\n" + completed.stderr
    require_marker(output, "usage: pulsedag-gpu-equivalence-probe")


def run_self_test() -> None:
    good = "\n".join(
        [
            "physical_cuda_identity=PASS device_index=0",
            "physical_opencl_identity=PASS device_index=0 amd_identity=PASS",
            "physical_vector_case=legacy_v1 nonces=7 cpu_reference=PASS cuda_exact_match=PASS opencl_exact_match=PASS cuda_opencl_same_input=PASS",
            "physical_vector_case=activated_v2 nonces=7 cpu_reference=PASS cuda_exact_match=PASS opencl_exact_match=PASS cuda_opencl_same_input=PASS",
            "physical_gpu_equivalence_probe=PASS canonical_vectors_per_protocol=7 protocols=2 cpu_reference=PASS cuda_execution=PASS opencl_execution=PASS cross_backend_same_input=PASS GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED",
        ]
    )
    validate_probe_output(good, "both", True)

    activated_only_bad = good.replace(
        "physical_vector_case=activated_v2 nonces=7 cpu_reference=PASS cuda_exact_match=PASS",
        "physical_vector_case=activated_v2 nonces=7 cpu_reference=PASS cuda_exact_match=NOT_RUN",
    )
    for broken in (
        good.replace("cuda_opencl_same_input=PASS", "cuda_opencl_same_input=FAIL", 1),
        activated_only_bad,
        good.replace(
            "GPU_MINING_NVIDIA_PASS=NOT_CLAIMED", "GPU_MINING_NVIDIA_PASS=true"
        ),
    ):
        try:
            validate_probe_output(broken, "both", True)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test failed: invalid physical evidence was accepted")

    with tempfile.TemporaryDirectory(prefix="task38-package-selftest-") as temp_dir:
        root = Path(temp_dir)
        if platform.system().lower() == "windows":
            archive = root / "probe-selftest.zip"
            binary_name = "pulsedag-gpu-equivalence-probe.exe"
        else:
            archive = root / "probe-selftest.tar.gz"
            binary_name = "pulsedag-gpu-equivalence-probe"
        archive_root = archive_base_name(archive)
        stage = root / archive_root
        stage.mkdir()
        fake_probe = stage / binary_name
        fake_probe.write_bytes(b"fake-probe-bytes")
        fake_cuda_module = stage / CUDA_MODULE_NAME
        fake_cuda_module.write_bytes(b"fake-candidate-bound-ptx")
        fake_cuda_metadata = {
            "schema": "pulsedag-task38-cuda-module-v1",
            "candidate_sha": "0123456789abcdef0123456789abcdef01234567",
            "repository": EXPECTED_REPOSITORY,
            "source": CUDA_MODULE_SOURCE,
            "source_sha256": "selftest-source-sha",
            "sha256": sha256_file(fake_cuda_module),
            "ptx_arch": CUDA_MODULE_ARCH,
            "compiler_container": CUDA_MODULE_CONTAINER,
        }
        (stage / CUDA_MODULE_METADATA_NAME).write_text(
            json.dumps(fake_cuda_metadata), encoding="utf-8"
        )
        if archive.name.endswith(".zip"):
            with zipfile.ZipFile(archive, "w") as handle:
                for item in sorted(stage.iterdir()):
                    handle.write(item, arcname=f"{archive_root}/{item.name}")
        else:
            with tarfile.open(archive, "w:gz") as handle:
                for item in sorted(stage.iterdir()):
                    handle.add(item, arcname=f"{archive_root}/{item.name}")
        manifest_path = root / f"{archive.name}.json"
        manifest = {
            "tag": EXPECTED_PACKAGE_TAG,
            "archive": archive.name,
            "archive_sha256": sha256_file(archive),
            "archive_size_bytes": archive.stat().st_size,
            "binary": binary_name,
            "build_features": ["cuda", "gpu"],
            "included_files": sorted([CUDA_MODULE_NAME, CUDA_MODULE_METADATA_NAME]),
            "target": expected_native_target(),
            "provenance": {
                "repository": EXPECTED_REPOSITORY,
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "github_run_id": "selftest-run",
                "github_run_attempt": "1",
            },
        }
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        loaded = validate_package_binding(
            archive,
            manifest_path,
            "0123456789abcdef0123456789abcdef01234567",
        )
        extract = root / "extract"
        extract.mkdir()
        probe, cuda_module, cuda_metadata = locate_packaged_payload(
            archive,
            extract,
            loaded,
            "0123456789abcdef0123456789abcdef01234567",
        )
        if probe.read_bytes() != b"fake-probe-bytes":
            raise RuntimeError("self-test failed: packaged probe bytes changed")
        if cuda_module.read_bytes() != b"fake-candidate-bound-ptx":
            raise RuntimeError("self-test failed: packaged CUDA module bytes changed")
        if cuda_metadata.get("sha256") != sha256_file(cuda_module):
            raise RuntimeError("self-test failed: packaged CUDA module digest changed")

    print("task38_physical_packaged_evidence_collector_self_test=PASS")


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--artifact-manifest", type=Path)
    parser.add_argument("--candidate-sha")
    parser.add_argument("--backend", choices=("cuda", "opencl", "both"), default="both")
    parser.add_argument("--cuda-device", type=int, default=0)
    parser.add_argument("--opencl-device", type=int, default=0)
    parser.add_argument("--require-amd-opencl", action="store_true")
    parser.add_argument("--output", type=Path, default=Path("task38-physical-gpu-evidence.json"))
    parser.add_argument("--timeout-secs", type=int, default=180)
    parser.add_argument("--inspect-package", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return args
    if args.archive is None or args.artifact_manifest is None or args.candidate_sha is None:
        parser.error(
            "--archive, --artifact-manifest and --candidate-sha are required unless --self-test is used"
        )
    if args.timeout_secs <= 0:
        parser.error("--timeout-secs must be positive")
    return args


def main() -> int:
    args = parse_args()
    if args.self_test:
        run_self_test()
        return 0

    assert args.archive is not None
    assert args.artifact_manifest is not None
    assert args.candidate_sha is not None

    archive = args.archive.resolve()
    manifest_path = args.artifact_manifest.resolve()
    if not archive.is_file():
        raise RuntimeError(f"probe archive not found: {archive}")
    if not manifest_path.is_file():
        raise RuntimeError(f"probe manifest not found: {manifest_path}")

    manifest = validate_package_binding(archive, manifest_path, args.candidate_sha)
    overrides = validate_environment()

    with tempfile.TemporaryDirectory(prefix="task38-physical-probe-") as temp_dir:
        probe, cuda_module, cuda_metadata = locate_packaged_payload(
            archive,
            Path(temp_dir),
            manifest,
            args.candidate_sha,
        )
        run_package_help(probe, min(args.timeout_secs, 30))

        if args.inspect_package:
            print(
                f"physical_probe_package_binding=PASS archive={archive.name} probe_sha256={sha256_file(probe)}"
            )
            print("physical_gpu_execution=NOT_CLAIMED")
            print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
            print("GPU_MINING_AMD_PASS=NOT_CLAIMED")
            return 0

        command = build_command(args, probe, cuda_module)
        started = datetime.now(timezone.utc)
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.timeout_secs,
            env=os.environ.copy(),
        )
        ended = datetime.now(timezone.utc)
        combined = "\n".join(
            part for part in (completed.stdout, completed.stderr) if part
        )
        if completed.returncode != 0:
            raise RuntimeError(
                f"physical probe failed with exit code {completed.returncode}\n"
                f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
            )
        validate_probe_output(combined, args.backend, args.require_amd_opencl)

        evidence = {
            "schema": "pulsedag-task38-physical-packaged-gpu-evidence-v1",
            "status": "PASS",
            "candidate_sha": args.candidate_sha,
            "backend": args.backend,
            "require_amd_opencl": args.require_amd_opencl,
            "cuda_device": args.cuda_device if args.backend in {"cuda", "both"} else None,
            "opencl_device": args.opencl_device if args.backend in {"opencl", "both"} else None,
            "host": {
                "system": platform.system(),
                "release": platform.release(),
                "version": platform.version(),
                "machine": platform.machine(),
                "python": platform.python_version(),
            },
            "started_at_utc": started.isoformat(),
            "ended_at_utc": ended.isoformat(),
            "duration_seconds": (ended - started).total_seconds(),
            "archive": {
                "path": str(archive),
                "sha256": sha256_file(archive),
                "manifest_path": str(manifest_path),
            },
            "probe": {"binary": manifest["binary"], "sha256": sha256_file(probe)},
            "cuda_module": {
                "path": str(cuda_module),
                "sha256": sha256_file(cuda_module),
                "metadata": cuda_metadata,
                "used_for_execution": args.backend in {"cuda", "both"},
            },
            "artifact_manifest": manifest,
            "repository_override_environment": overrides,
            "command": command,
            "command_shell_display": command_display(command),
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "final_flags": {
                "GPU_MINING_NVIDIA_PASS": "NOT_CLAIMED",
                "GPU_MINING_AMD_PASS": "NOT_CLAIMED",
            },
        }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"physical_packaged_gpu_evidence=PASS output={args.output}")
    print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
    print("GPU_MINING_AMD_PASS=NOT_CLAIMED")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        RuntimeError,
        subprocess.TimeoutExpired,
        OSError,
        json.JSONDecodeError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as exc:
        print(f"physical_packaged_gpu_evidence=FAIL reason={exc}", file=sys.stderr)
        raise SystemExit(1)
