#!/usr/bin/env python3
"""Verify a fail-closed Task 38 physical GPU evidence matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Iterable

EXPECTED_SCHEMA = "pulsedag-task38-physical-packaged-gpu-evidence-v1"
EXPECTED_REPOSITORY = "AuriaLABS/PulseDAG"
EXPECTED_TAG = "v3-task38-phase24"
EXPECTED_CUDA_SOURCE = "apps/pulsedag-miner/cuda/kheavyhash_launch.cu"
EXPECTED_CUDA_METADATA_SCHEMA = "pulsedag-task38-cuda-module-v1"
REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
EXPECTED_CUDA_ARCH = "sm_60"
EXPECTED_CUDA_COMPILER = "nvidia/cuda:12.9.2-devel-ubuntu24.04"
EXPECTED_INCLUDED_FILES = sorted(
    ["pulsedag-kheavyhash-launch.ptx", "pulsedag-kheavyhash-launch.ptx.json"]
)
SUPPORTED_SYSTEMS = {"Linux", "Windows"}
PROTOCOL_CASES = ("legacy_v1", "activated_v2")


def parse_marker_line(line: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for token in line.strip().split():
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        if key in fields:
            raise RuntimeError(f"duplicate marker field {key!r}")
        fields[key] = value
    return fields


def exact_sha(value: str) -> bool:
    return len(value) == 40 and all(ch in "0123456789abcdef" for ch in value)


def valid_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(ch in "0123456789abcdefABCDEF" for ch in value)
    )


def single_line(lines: list[str], prefix: str) -> str:
    matches = [line for line in lines if line.startswith(prefix)]
    if len(matches) != 1:
        raise RuntimeError(f"expected exactly one {prefix!r} line, found {len(matches)}")
    return matches[0]


def git_object_bytes(candidate_sha: str, repository_path: str) -> bytes:
    if not exact_sha(candidate_sha):
        raise RuntimeError("candidate SHA must be an exact 40-character hexadecimal commit")
    completed = subprocess.run(
        [
            "git",
            "-C",
            str(REPOSITORY_ROOT),
            "show",
            f"{candidate_sha}:{repository_path}",
        ],
        check=False,
        capture_output=True,
    )
    if completed.returncode != 0:
        stderr = completed.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(
            f"cannot read {repository_path!r} from candidate {candidate_sha}: {stderr}"
        )
    return completed.stdout


def candidate_source_sha256(candidate_sha: str) -> str:
    return hashlib.sha256(
        git_object_bytes(candidate_sha, EXPECTED_CUDA_SOURCE)
    ).hexdigest()


def current_head_sha() -> str:
    completed = subprocess.run(
        ["git", "-C", str(REPOSITORY_ROOT), "rev-parse", "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError("cannot resolve repository HEAD for self-test")
    value = completed.stdout.strip()
    if not exact_sha(value):
        raise RuntimeError("repository HEAD is not an exact commit SHA")
    return value


def validate_evidence(path: Path, candidate_sha: str) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != EXPECTED_SCHEMA:
        raise RuntimeError(f"{path}: unexpected evidence schema")
    if data.get("status") != "PASS":
        raise RuntimeError(f"{path}: evidence status is not PASS")
    if data.get("candidate_sha") != candidate_sha:
        raise RuntimeError(f"{path}: candidate SHA mismatch")

    backend = data.get("backend")
    if backend not in {"cuda", "opencl", "both"}:
        raise RuntimeError(f"{path}: invalid backend {backend!r}")

    host = data.get("host") or {}
    system = host.get("system")
    if system not in SUPPORTED_SYSTEMS:
        raise RuntimeError(f"{path}: unsupported host system {system!r}")

    machine = host.get("machine")
    if not isinstance(machine, str) or machine.lower() not in {"amd64", "x86_64", "x64"}:
        raise RuntimeError(f"{path}: unsupported physical host architecture {machine!r}")

    manifest = data.get("artifact_manifest") or {}
    if manifest.get("tag") != EXPECTED_TAG:
        raise RuntimeError(f"{path}: artifact tag mismatch")
    if manifest.get("build_features") != ["cuda", "gpu"]:
        raise RuntimeError(f"{path}: artifact build-feature provenance mismatch")
    expected_binary = (
        "pulsedag-gpu-equivalence-probe.exe"
        if system == "Windows"
        else "pulsedag-gpu-equivalence-probe"
    )
    if manifest.get("binary") != expected_binary:
        raise RuntimeError(f"{path}: packaged probe binary mismatch")
    expected_target = (
        "x86_64-pc-windows-msvc"
        if system == "Windows"
        else "x86_64-unknown-linux-gnu"
    )
    if manifest.get("target") != expected_target:
        raise RuntimeError(f"{path}: artifact target does not match physical host OS")
    if sorted(manifest.get("included_files") or []) != EXPECTED_INCLUDED_FILES:
        raise RuntimeError(f"{path}: candidate-bound CUDA sidecars are missing")
    provenance = manifest.get("provenance") or {}
    if provenance.get("repository") != EXPECTED_REPOSITORY:
        raise RuntimeError(f"{path}: repository provenance mismatch")
    if provenance.get("commit") != candidate_sha:
        raise RuntimeError(f"{path}: artifact candidate mismatch")
    for key in ("github_run_id", "github_run_attempt"):
        if not provenance.get(key):
            raise RuntimeError(f"{path}: artifact provenance missing {key}")

    archive = data.get("archive") or {}
    archive_sha = archive.get("sha256")
    manifest_archive_sha = manifest.get("archive_sha256")
    if not valid_sha256(archive_sha) or not valid_sha256(manifest_archive_sha):
        raise RuntimeError(f"{path}: archive SHA256 provenance missing or invalid")
    if archive_sha != manifest_archive_sha:
        raise RuntimeError(f"{path}: evidence/archive manifest SHA mismatch")

    final_flags = data.get("final_flags") or {}
    if final_flags.get("GPU_MINING_NVIDIA_PASS") != "NOT_CLAIMED":
        raise RuntimeError(f"{path}: premature NVIDIA final PASS claim")
    if final_flags.get("GPU_MINING_AMD_PASS") != "NOT_CLAIMED":
        raise RuntimeError(f"{path}: premature AMD final PASS claim")

    cuda_module = data.get("cuda_module") or {}
    cuda_metadata = cuda_module.get("metadata") or {}
    if cuda_metadata.get("schema") != EXPECTED_CUDA_METADATA_SCHEMA:
        raise RuntimeError(f"{path}: packaged CUDA module metadata schema mismatch")
    if cuda_metadata.get("candidate_sha") != candidate_sha:
        raise RuntimeError(f"{path}: packaged CUDA module candidate mismatch")
    if cuda_metadata.get("repository") != EXPECTED_REPOSITORY:
        raise RuntimeError(f"{path}: packaged CUDA module repository mismatch")
    if cuda_metadata.get("source") != EXPECTED_CUDA_SOURCE:
        raise RuntimeError(f"{path}: packaged CUDA module source mismatch")
    source_sha = cuda_metadata.get("source_sha256")
    actual_source_sha = candidate_source_sha256(candidate_sha)
    if not valid_sha256(source_sha) or source_sha.lower() != actual_source_sha:
        raise RuntimeError(
            f"{path}: packaged CUDA module source digest does not match candidate Git object"
        )
    if cuda_metadata.get("ptx_arch") != EXPECTED_CUDA_ARCH:
        raise RuntimeError(f"{path}: packaged CUDA module architecture mismatch")
    if cuda_metadata.get("compiler_container") != EXPECTED_CUDA_COMPILER:
        raise RuntimeError(f"{path}: packaged CUDA module compiler provenance mismatch")
    cuda_sha = cuda_module.get("sha256")
    cuda_metadata_sha = cuda_metadata.get("sha256")
    if not valid_sha256(cuda_sha) or not valid_sha256(cuda_metadata_sha):
        raise RuntimeError(f"{path}: packaged CUDA module digest missing or invalid")
    if cuda_sha != cuda_metadata_sha:
        raise RuntimeError(f"{path}: packaged CUDA module digest mismatch")
    if cuda_module.get("used_for_execution") != (backend in {"cuda", "both"}):
        raise RuntimeError(f"{path}: CUDA-module execution marker mismatch")
    probe = data.get("probe") or {}
    if probe.get("binary") != expected_binary or not valid_sha256(probe.get("sha256")):
        raise RuntimeError(f"{path}: packaged probe evidence binding mismatch")

    overrides = data.get("repository_override_environment")
    if not isinstance(overrides, dict):
        raise RuntimeError(f"{path}: repository runtime override evidence is missing")
    for name in ("PULSEDAG_CUDA_DRIVER_LIBRARY", "PULSEDAG_OPENCL_LIBRARY"):
        if name not in overrides:
            raise RuntimeError(f"{path}: repository runtime override evidence missing {name}")
        value = overrides.get(name)
        if value is not None and (not isinstance(value, str) or value.strip()):
            raise RuntimeError(f"{path}: repository runtime override is active: {name}")

    stdout = data.get("stdout")
    stderr = data.get("stderr")
    if not isinstance(stdout, str):
        raise RuntimeError(f"{path}: stdout evidence missing")
    if not isinstance(stderr, str):
        raise RuntimeError(f"{path}: stderr evidence missing")
    combined_output = "\n".join(part for part in (stdout, stderr) if part)
    if (
        "GPU_MINING_NVIDIA_PASS=true" in combined_output
        or "GPU_MINING_AMD_PASS=true" in combined_output
    ):
        raise RuntimeError(f"{path}: probe output contains premature final PASS")

    lines = [
        line.strip()
        for line in combined_output.splitlines()
        if line.strip()
    ]
    summary = parse_marker_line(single_line(lines, "physical_gpu_equivalence_probe=PASS "))
    if summary.get("canonical_vectors_per_protocol") != "7":
        raise RuntimeError(f"{path}: canonical vector count mismatch")
    if summary.get("protocols") != "2" or summary.get("cpu_reference") != "PASS":
        raise RuntimeError(f"{path}: protocol/CPU summary mismatch")
    if summary.get("GPU_MINING_NVIDIA_PASS") != "NOT_CLAIMED":
        raise RuntimeError(f"{path}: NVIDIA non-claim marker missing")
    if summary.get("GPU_MINING_AMD_PASS") != "NOT_CLAIMED":
        raise RuntimeError(f"{path}: AMD non-claim marker missing")

    for case in PROTOCOL_CASES:
        fields = parse_marker_line(single_line(lines, f"physical_vector_case={case} "))
        if fields.get("nonces") != "7" or fields.get("cpu_reference") != "PASS":
            raise RuntimeError(f"{path}: {case} CPU/vector contract mismatch")
        expected_cuda = "PASS" if backend in {"cuda", "both"} else "NOT_RUN"
        expected_opencl = "PASS" if backend in {"opencl", "both"} else "NOT_RUN"
        expected_cross = "PASS" if backend == "both" else "NOT_RUN"
        if fields.get("cuda_exact_match") != expected_cuda:
            raise RuntimeError(f"{path}: {case} CUDA exact-match mismatch")
        if fields.get("opencl_exact_match") != expected_opencl:
            raise RuntimeError(f"{path}: {case} OpenCL exact-match mismatch")
        if fields.get("cuda_opencl_same_input") != expected_cross:
            raise RuntimeError(f"{path}: {case} cross-backend mismatch")

    cuda_lines = [
        line for line in lines if line.startswith("physical_cuda_identity=")
    ]
    if len(cuda_lines) > 1:
        raise RuntimeError(f"{path}: multiple CUDA identity lines")
    if cuda_lines and not cuda_lines[0].startswith("physical_cuda_identity=PASS "):
        raise RuntimeError(f"{path}: invalid CUDA identity marker")
    cuda_identity = bool(cuda_lines)
    opencl_lines = [
        line for line in lines if line.startswith("physical_opencl_identity=")
    ]
    if len(opencl_lines) > 1:
        raise RuntimeError(f"{path}: multiple OpenCL identity lines")
    if opencl_lines and not opencl_lines[0].startswith("physical_opencl_identity=PASS "):
        raise RuntimeError(f"{path}: invalid OpenCL identity marker")
    opencl_identity = bool(opencl_lines)
    opencl_fields = parse_marker_line(opencl_lines[0]) if opencl_lines else {}
    amd_identity = opencl_fields.get("amd_identity") == "PASS"

    if backend in {"cuda", "both"}:
        if not cuda_identity or summary.get("cuda_execution") != "PASS":
            raise RuntimeError(f"{path}: CUDA physical identity/execution missing")
    elif cuda_identity or summary.get("cuda_execution") != "NOT_RUN":
        raise RuntimeError(f"{path}: unexpected CUDA execution")

    if backend in {"opencl", "both"}:
        if not opencl_identity or summary.get("opencl_execution") != "PASS":
            raise RuntimeError(f"{path}: OpenCL physical identity/execution missing")
    elif opencl_identity or summary.get("opencl_execution") != "NOT_RUN":
        raise RuntimeError(f"{path}: unexpected OpenCL execution")

    expected_cross = "PASS" if backend == "both" else "NOT_RUN"
    if summary.get("cross_backend_same_input") != expected_cross:
        raise RuntimeError(f"{path}: cross-backend summary mismatch")

    if bool(data.get("require_amd_opencl")) and not amd_identity:
        raise RuntimeError(f"{path}: require_amd_opencl evidence lacks AMD identity")

    return {
        "path": str(path),
        "system": system,
        "backend": backend,
        "nvidia": backend in {"cuda", "both"} and cuda_identity,
        "amd": backend in {"opencl", "both"} and amd_identity,
        "cross_vendor": backend == "both" and cuda_identity and amd_identity,
        "artifact_run_id": str(provenance.get("github_run_id")),
        "artifact_run_attempt": str(provenance.get("github_run_attempt")),
    }


def parse_cell(value: str) -> tuple[str, str]:
    match = re.fullmatch(r"(nvidia|amd):(Linux|Windows)", value)
    if not match:
        raise argparse.ArgumentTypeError(
            "cell must be nvidia:Linux, nvidia:Windows, amd:Linux, or amd:Windows"
        )
    return match.group(1), match.group(2)


def build_summary(records: list[dict], candidate_sha: str) -> dict:
    cells = {
        "nvidia": {
            system: any(r["system"] == system and r["nvidia"] for r in records)
            for system in sorted(SUPPORTED_SYSTEMS)
        },
        "amd": {
            system: any(r["system"] == system and r["amd"] for r in records)
            for system in sorted(SUPPORTED_SYSTEMS)
        },
    }
    cross_vendor = {
        system: any(r["system"] == system and r["cross_vendor"] for r in records)
        for system in sorted(SUPPORTED_SYSTEMS)
    }
    return {
        "schema": "pulsedag-task38-physical-gpu-matrix-v1",
        "candidate_sha": candidate_sha,
        "evidence_count": len(records),
        "cells": cells,
        "cross_vendor_same_host": cross_vendor,
        "records": records,
        "final_flags": {
            "GPU_MINING_NVIDIA_PASS": "NOT_CLAIMED",
            "GPU_MINING_AMD_PASS": "NOT_CLAIMED",
        },
        "not_proven_by_matrix_verifier": [
            "physical multi-GPU correctness",
            "physical watchdog/device recovery",
            "long-duration soak",
            "final release PASS promotion",
        ],
    }


def enforce_requirements(
    summary: dict,
    required_cells: list[tuple[str, str]],
    required_cross_vendor_os: list[str],
) -> None:
    missing: list[str] = []
    for vendor, system in required_cells:
        if not summary["cells"][vendor][system]:
            missing.append(f"{vendor}:{system}")
    for system in required_cross_vendor_os:
        if not summary["cross_vendor_same_host"][system]:
            missing.append(f"cross-vendor:{system}")
    if missing:
        raise RuntimeError("required physical evidence missing: " + ", ".join(missing))


def synthetic_evidence(
    candidate: str,
    system: str,
    backend: str,
    *,
    amd: bool,
    run_id: str,
) -> dict:
    cuda = backend in {"cuda", "both"}
    opencl = backend in {"opencl", "both"}
    cross = backend == "both"
    lines: list[str] = []
    if cuda:
        lines.append(
            'physical_cuda_identity=PASS device_index=0 device_name="Synthetic NVIDIA" compute_capability=8.6 driver_version=13.4'
        )
    if opencl:
        lines.append(
            'physical_opencl_identity=PASS device_index=0 vendor="Advanced Micro Devices, Inc." '
            f'device_name="Synthetic AMD" amd_identity={"PASS" if amd else "NOT_CLAIMED"}'
        )
    for case in PROTOCOL_CASES:
        lines.append(
            f"physical_vector_case={case} nonces=7 cpu_reference=PASS "
            f"cuda_exact_match={'PASS' if cuda else 'NOT_RUN'} "
            f"opencl_exact_match={'PASS' if opencl else 'NOT_RUN'} "
            f"cuda_opencl_same_input={'PASS' if cross else 'NOT_RUN'}"
        )
    lines.append(
        "physical_gpu_equivalence_probe=PASS canonical_vectors_per_protocol=7 "
        f"protocols=2 cpu_reference=PASS cuda_execution={'PASS' if cuda else 'NOT_RUN'} "
        f"opencl_execution={'PASS' if opencl else 'NOT_RUN'} "
        f"cross_backend_same_input={'PASS' if cross else 'NOT_RUN'} "
        "GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED"
    )
    archive_sha = (run_id * 64)[:64]
    module_sha = ("a" + run_id * 64)[:64]
    return {
        "schema": EXPECTED_SCHEMA,
        "status": "PASS",
        "candidate_sha": candidate,
        "backend": backend,
        "require_amd_opencl": opencl and amd,
        "host": {
            "system": system,
            "release": "selftest",
            "version": "selftest",
            "machine": "x86_64",
            "python": "3",
        },
        "archive": {
            "path": "synthetic",
            "sha256": archive_sha,
            "manifest_path": "synthetic.json",
        },
        "probe": {
            "binary": (
                "pulsedag-gpu-equivalence-probe.exe"
                if system == "Windows"
                else "pulsedag-gpu-equivalence-probe"
            ),
            "sha256": "b" * 64,
        },
        "cuda_module": {
            "path": "pulsedag-kheavyhash-launch.ptx",
            "sha256": module_sha,
            "metadata": {
                "schema": EXPECTED_CUDA_METADATA_SCHEMA,
                "candidate_sha": candidate,
                "repository": EXPECTED_REPOSITORY,
                "source": EXPECTED_CUDA_SOURCE,
                "source_sha256": candidate_source_sha256(candidate),
                "ptx_arch": EXPECTED_CUDA_ARCH,
                "compiler_container": EXPECTED_CUDA_COMPILER,
                "sha256": module_sha,
            },
            "used_for_execution": cuda,
        },
        "artifact_manifest": {
            "tag": EXPECTED_TAG,
            "archive_sha256": archive_sha,
            "binary": (
                "pulsedag-gpu-equivalence-probe.exe"
                if system == "Windows"
                else "pulsedag-gpu-equivalence-probe"
            ),
            "target": (
                "x86_64-pc-windows-msvc"
                if system == "Windows"
                else "x86_64-unknown-linux-gnu"
            ),
            "build_features": ["cuda", "gpu"],
            "included_files": EXPECTED_INCLUDED_FILES,
            "provenance": {
                "repository": EXPECTED_REPOSITORY,
                "commit": candidate,
                "github_run_id": run_id,
                "github_run_attempt": "1",
            },
        },
        "repository_override_environment": {
            "PULSEDAG_CUDA_DRIVER_LIBRARY": None,
            "PULSEDAG_OPENCL_LIBRARY": None,
        },
        "stdout": "\n".join(lines) + "\n",
        "stderr": "",
        "final_flags": {
            "GPU_MINING_NVIDIA_PASS": "NOT_CLAIMED",
            "GPU_MINING_AMD_PASS": "NOT_CLAIMED",
        },
    }


def run_self_test() -> None:
    candidate = current_head_sha()
    if candidate != candidate.lower() or not exact_sha(candidate):
        raise RuntimeError("self-test repository HEAD is not canonical lowercase SHA")
    if exact_sha(candidate.upper()):
        raise RuntimeError("self-test accepted non-canonical uppercase candidate SHA")
    expected_source_sha = candidate_source_sha256(candidate)
    if not valid_sha256(expected_source_sha):
        raise RuntimeError("self-test candidate Git-object CUDA source digest is invalid")
    with tempfile.TemporaryDirectory(prefix="task38-matrix-selftest-") as temp:
        root = Path(temp)
        fixtures = [
            ("nvidia-windows.json", synthetic_evidence(candidate, "Windows", "cuda", amd=False, run_id="1")),
            ("amd-linux.json", synthetic_evidence(candidate, "Linux", "opencl", amd=True, run_id="2")),
            ("mixed-windows.json", synthetic_evidence(candidate, "Windows", "both", amd=True, run_id="3")),
        ]
        paths: list[Path] = []
        for name, payload in fixtures:
            path = root / name
            path.write_text(json.dumps(payload), encoding="utf-8")
            paths.append(path)

        records = [validate_evidence(path, candidate) for path in paths]
        summary = build_summary(records, candidate)
        enforce_requirements(
            summary,
            [("nvidia", "Windows"), ("amd", "Linux"), ("amd", "Windows")],
            ["Windows"],
        )
        if summary["final_flags"]["GPU_MINING_NVIDIA_PASS"] != "NOT_CLAIMED":
            raise RuntimeError("self-test final NVIDIA non-claim changed")


        # Match the collector's native x86 aliases on both packaged platforms.
        for system in sorted(SUPPORTED_SYSTEMS):
            for machine in ("x86_64", "AMD64", "x64"):
                payload = synthetic_evidence(candidate, system, "both", amd=True, run_id="c")
                payload["host"]["machine"] = machine
                path = root / "native-host.json"
                path.write_text(json.dumps(payload), encoding="utf-8")
                validate_evidence(path, candidate)
            for machine in ("aarch64", "arm64", "i686", "", None, 64):
                payload = synthetic_evidence(candidate, system, "both", amd=True, run_id="c")
                payload["host"]["machine"] = machine
                path = root / "invalid-host.json"
                path.write_text(json.dumps(payload), encoding="utf-8")
                try:
                    validate_evidence(path, candidate)
                except RuntimeError as exc:
                    if "host architecture" not in str(exc):
                        raise
                else:
                    raise RuntimeError("self-test accepted incompatible host architecture")

        # Reject duplicate keys within a single marker line; conflicting values
        # must never be resolved by last-write-wins parsing.
        for duplicate_line in (
            "physical_opencl_identity=PASS amd_identity=NOT_CLAIMED amd_identity=PASS",
            "physical_cuda_identity=PASS device_index=0 device_index=1",
            "physical_gpu_equivalence_probe=PASS protocols=2 protocols=1",
        ):
            try:
                parse_marker_line(duplicate_line)
            except RuntimeError as exc:
                if "duplicate marker field" not in str(exc):
                    raise
            else:
                raise RuntimeError("self-test accepted duplicate marker field")

        # Reject duplicates/conflicts across either captured stream, for both vendors.
        for vendor, unused_backend in (("cuda", "opencl"), ("opencl", "cuda")):
            prefix = f"physical_{vendor}_identity="
            for stream in ("stdout", "stderr"):
                for marker in ("PASS device_index=1", "FAIL", "NOT_RUN"):
                    payload = synthetic_evidence(candidate, "Linux", "both", amd=True, run_id="d")
                    payload[stream] += f"\n{prefix}{marker}\n"
                    path = root / "duplicate-identity.json"
                    path.write_text(json.dumps(payload), encoding="utf-8")
                    try:
                        validate_evidence(path, candidate)
                    except RuntimeError as exc:
                        if "identity lines" not in str(exc):
                            raise
                    else:
                        raise RuntimeError("self-test accepted duplicate/conflicting identity")
            for backend in ("both", unused_backend):
                payload = synthetic_evidence(candidate, "Linux", backend, amd=True, run_id="e")
                payload["stdout"] = "\n".join(
                    line for line in payload["stdout"].splitlines()
                    if not line.startswith(prefix)
                )
                payload["stderr"] = f"{prefix}FAIL\n"
                path = root / "failed-identity.json"
                path.write_text(json.dumps(payload), encoding="utf-8")
                try:
                    validate_evidence(path, candidate)
                except RuntimeError as exc:
                    if "identity marker" not in str(exc):
                        raise
                else:
                    raise RuntimeError("self-test accepted failed identity marker")

        broken = synthetic_evidence(candidate, "Windows", "cuda", amd=False, run_id="4")
        broken["final_flags"]["GPU_MINING_NVIDIA_PASS"] = "true"
        broken_path = root / "broken.json"
        broken_path.write_text(json.dumps(broken), encoding="utf-8")
        try:
            validate_evidence(broken_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted premature final PASS")

        stderr_pass = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="5"
        )
        stderr_pass["stderr"] = "GPU_MINING_NVIDIA_PASS=true\n"
        stderr_pass_path = root / "stderr-pass.json"
        stderr_pass_path.write_text(json.dumps(stderr_pass), encoding="utf-8")
        try:
            validate_evidence(stderr_pass_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted premature final PASS in stderr")

        stderr_duplicate = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="6"
        )
        stderr_duplicate["stderr"] = (
            "physical_gpu_equivalence_probe=PASS canonical_vectors_per_protocol=7 "
            "protocols=2 cpu_reference=PASS cuda_execution=PASS "
            "opencl_execution=NOT_RUN cross_backend_same_input=NOT_RUN "
            "GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED\n"
        )
        stderr_duplicate_path = root / "stderr-duplicate.json"
        stderr_duplicate_path.write_text(
            json.dumps(stderr_duplicate), encoding="utf-8"
        )
        try:
            validate_evidence(stderr_duplicate_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted duplicate PASS summary in stderr")

        missing_digest = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="7"
        )
        missing_digest["cuda_module"].pop("sha256")
        missing_digest["cuda_module"]["metadata"].pop("sha256")
        missing_digest_path = root / "missing-digest.json"
        missing_digest_path.write_text(json.dumps(missing_digest), encoding="utf-8")
        try:
            validate_evidence(missing_digest_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted absent CUDA module digests")

        forged_amd = synthetic_evidence(
            candidate, "Linux", "opencl", amd=False, run_id="8"
        )
        forged_amd["stdout"] = forged_amd["stdout"].replace(
            'vendor="Advanced Micro Devices, Inc."',
            'vendor="forged amd_identity=PASS vendor"',
        )
        forged_path = root / "forged-amd.json"
        forged_path.write_text(json.dumps(forged_amd), encoding="utf-8")
        try:
            validate_evidence(forged_path, candidate)
        except RuntimeError as exc:
            if "duplicate marker field" not in str(exc):
                raise
        else:
            raise RuntimeError("self-test accepted AMD marker embedded in identity text")

        active_override = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="9"
        )
        active_override["repository_override_environment"][
            "PULSEDAG_CUDA_DRIVER_LIBRARY"
        ] = "mock-cuda-driver.dll"
        active_override_path = root / "active-override.json"
        active_override_path.write_text(json.dumps(active_override), encoding="utf-8")
        try:
            validate_evidence(active_override_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted active repository runtime override")

        bad_schema = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="a"
        )
        bad_schema["cuda_module"]["metadata"]["schema"] = "unexpected-schema"
        bad_schema_path = root / "bad-schema.json"
        bad_schema_path.write_text(json.dumps(bad_schema), encoding="utf-8")
        try:
            validate_evidence(bad_schema_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted invalid CUDA metadata schema")

        stale_source = synthetic_evidence(
            candidate, "Windows", "cuda", amd=False, run_id="b"
        )
        stale_source["cuda_module"]["metadata"]["source_sha256"] = "0" * 64
        stale_source_path = root / "stale-source.json"
        stale_source_path.write_text(json.dumps(stale_source), encoding="utf-8")
        try:
            validate_evidence(stale_source_path, candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted stale CUDA source digest")

        unknown_candidate = "f" * 40
        try:
            candidate_source_sha256(unknown_candidate)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted unavailable candidate Git object")

        try:
            enforce_requirements(summary, [("nvidia", "Linux")], [])
        except RuntimeError:
            pass
        else:
            raise RuntimeError("self-test accepted missing matrix cell")

    print("task38_physical_gpu_matrix_verifier_self_test=PASS")
    print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
    print("GPU_MINING_AMD_PASS=NOT_CLAIMED")


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-sha")
    parser.add_argument("--evidence", action="append", type=Path, default=[])
    parser.add_argument("--require-cell", action="append", type=parse_cell, default=[])
    parser.add_argument(
        "--require-cross-vendor-os",
        action="append",
        choices=sorted(SUPPORTED_SYSTEMS),
        default=[],
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return args
    if not args.candidate_sha or not exact_sha(args.candidate_sha):
        parser.error(
            "--candidate-sha must be an exact lowercase 40-character hexadecimal commit"
        )
    if not args.evidence:
        parser.error("at least one --evidence JSON is required")
    return args


def main() -> int:
    args = parse_args()
    if args.self_test:
        run_self_test()
        return 0

    candidate = args.candidate_sha
    records = [validate_evidence(path.resolve(), candidate) for path in args.evidence]
    summary = build_summary(records, candidate)
    enforce_requirements(summary, args.require_cell, args.require_cross_vendor_os)

    encoded = json.dumps(summary, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
        print(f"task38_physical_gpu_matrix=PASS output={args.output}")
    else:
        print(encoded, end="")
        print("task38_physical_gpu_matrix=PASS")

    print("GPU_MINING_NVIDIA_PASS=NOT_CLAIMED")
    print("GPU_MINING_AMD_PASS=NOT_CLAIMED")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError, json.JSONDecodeError) as exc:
        print(f"task38_physical_gpu_matrix=FAIL reason={exc}", file=sys.stderr)
        raise SystemExit(1)
