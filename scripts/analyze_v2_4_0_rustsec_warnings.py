#!/usr/bin/env python3
"""Capture exact v2.4.0 RustSec warning reachability evidence.

The script intentionally distinguishes packages recorded in Cargo.lock from
packages for which Cargo emits compiler artifacts when building the native node
and standalone miner from empty target directories.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class WarningPackage:
    name: str
    version: str
    advisory_ids: tuple[str, ...]

    @property
    def cargo_spec(self) -> str:
        return f"{self.name}@{self.version}"


WARNING_PACKAGES = (
    WarningPackage("async-std", "1.13.2", ("RUSTSEC-2025-0052",)),
    WarningPackage("atty", "0.2.14", ("RUSTSEC-2024-0375", "RUSTSEC-2021-0145")),
    WarningPackage("bincode", "1.3.3", ("RUSTSEC-2025-0141",)),
    WarningPackage("instant", "0.1.13", ("RUSTSEC-2024-0384",)),
    WarningPackage("linkme", "0.2.10", ("RUSTSEC-2024-0407",)),
    WarningPackage("paste", "1.0.15", ("RUSTSEC-2024-0436",)),
    WarningPackage("proc-macro-error", "1.0.4", ("RUSTSEC-2024-0370",)),
)

DEFAULT_ROOTS = ("pulsedagd", "pulsedag-miner")


def run(
    command: list[str],
    *,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if check and result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed ({result.returncode}): {' '.join(command)}")
    return result


def cargo_metadata() -> dict[str, Any]:
    result = run(["cargo", "metadata", "--locked", "--format-version", "1"])
    return json.loads(result.stdout)


def package_index(metadata: dict[str, Any]) -> dict[str, dict[str, str]]:
    return {
        str(package["id"]): {
            "name": str(package["name"]),
            "version": str(package["version"]),
            "source": str(package.get("source") or "workspace"),
        }
        for package in metadata["packages"]
    }


def compile_root(
    root: str,
    evidence_dir: Path,
    packages_by_id: dict[str, dict[str, str]],
) -> list[dict[str, Any]]:
    with tempfile.TemporaryDirectory(prefix=f"pulsedag-warning-{root}-") as target_dir:
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = target_dir
        result = run(
            [
                "cargo",
                "check",
                "--locked",
                "-p",
                root,
                "--message-format=json-render-diagnostics",
            ],
            env=env,
        )

    (evidence_dir / f"{root}-compiler-messages.jsonl").write_text(
        result.stdout, encoding="utf-8"
    )
    (evidence_dir / f"{root}-compiler-stderr.txt").write_text(
        result.stderr, encoding="utf-8"
    )

    artifacts: list[dict[str, Any]] = []
    for line_number, line in enumerate(result.stdout.splitlines(), start=1):
        try:
            message = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(
                f"non-JSON cargo message for {root} at line {line_number}: {exc}"
            ) from exc
        if message.get("reason") != "compiler-artifact":
            continue
        package_id = str(message.get("package_id", ""))
        package = packages_by_id.get(
            package_id,
            {"name": package_id, "version": "unknown", "source": "unknown"},
        )
        target = message.get("target") or {}
        artifacts.append(
            {
                **package,
                "target_name": target.get("name"),
                "target_kinds": target.get("kind", []),
                "crate_types": target.get("crate_types", []),
                "features": sorted(message.get("features", [])),
                "fresh": bool(message.get("fresh", False)),
            }
        )

    artifacts.sort(
        key=lambda item: (
            item["name"],
            item["version"],
            str(item["target_name"]),
            json.dumps(item["target_kinds"]),
        )
    )
    (evidence_dir / f"{root}-compiler-artifacts.json").write_text(
        json.dumps(artifacts, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return artifacts


def reverse_trees(evidence_dir: Path) -> dict[str, dict[str, Any]]:
    output: dict[str, dict[str, Any]] = {}
    for package in WARNING_PACKAGES:
        result = run(
            [
                "cargo",
                "tree",
                "--locked",
                "--workspace",
                "--all-targets",
                "--edges",
                "features",
                "--invert",
                package.cargo_spec,
            ],
            check=False,
        )
        filename = f"reverse-{package.name}-{package.version}.txt"
        (evidence_dir / filename).write_text(
            result.stdout + result.stderr, encoding="utf-8"
        )
        output[package.name] = {
            "version": package.version,
            "advisory_ids": list(package.advisory_ids),
            "command_exit_code": result.returncode,
            "tree_file": filename,
        }
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--root", action="append", dest="roots")
    args = parser.parse_args()

    roots = tuple(args.roots or DEFAULT_ROOTS)
    evidence_dir = args.evidence_dir.resolve()
    evidence_dir.mkdir(parents=True, exist_ok=True)

    metadata = cargo_metadata()
    (evidence_dir / "cargo-metadata.json").write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    packages_by_id = package_index(metadata)

    locked = {
        (package["name"], package["version"])
        for package in packages_by_id.values()
    }
    missing = [
        package.cargo_spec
        for package in WARNING_PACKAGES
        if (package.name, package.version) not in locked
    ]
    if missing:
        raise SystemExit(f"expected warning packages missing from locked graph: {missing}")

    trees = reverse_trees(evidence_dir)
    compiled: dict[str, list[dict[str, Any]]] = {}
    warning_reachability: dict[str, dict[str, list[dict[str, Any]]]] = {}
    for root in roots:
        artifacts = compile_root(root, evidence_dir, packages_by_id)
        compiled[root] = artifacts
        warning_reachability[root] = {}
        for warning in WARNING_PACKAGES:
            matches = [
                artifact
                for artifact in artifacts
                if artifact["name"] == warning.name
                and artifact["version"] == warning.version
            ]
            warning_reachability[root][warning.name] = matches

    summary = {
        "source_sha": os.environ.get("GITHUB_SHA", "local"),
        "runner_os": os.environ.get("RUNNER_OS", platform.system()),
        "platform": platform.platform(),
        "python": sys.version,
        "roots": list(roots),
        "warnings": {
            package.name: {
                "version": package.version,
                "advisory_ids": list(package.advisory_ids),
                "reverse_tree": trees[package.name],
                "compiled_by_root": {
                    root: warning_reachability[root][package.name] for root in roots
                },
            }
            for package in WARNING_PACKAGES
        },
        "compiled_artifact_counts": {
            root: len(artifacts) for root, artifacts in compiled.items()
        },
    }
    (evidence_dir / "warning-reachability-summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    human_lines = [
        f"source_sha={summary['source_sha']}",
        f"runner_os={summary['runner_os']}",
        f"platform={summary['platform']}",
    ]
    for package in WARNING_PACKAGES:
        human_lines.append(f"{package.cargo_spec}")
        for root in roots:
            matches = warning_reachability[root][package.name]
            kinds = sorted(
                {
                    kind
                    for match in matches
                    for kind in match.get("target_kinds", [])
                }
            )
            human_lines.append(
                f"  {root}: compiled={str(bool(matches)).lower()} "
                f"target_kinds={','.join(kinds) if kinds else '-'}"
            )
    (evidence_dir / "warning-reachability-summary.txt").write_text(
        "\n".join(human_lines) + "\n", encoding="utf-8"
    )
    print("\n".join(human_lines))


if __name__ == "__main__":
    main()
