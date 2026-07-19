#!/usr/bin/env python3
"""Run the fail-closed PulseDAG repository hygiene checks."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

ROOT = Path.cwd()
HISTORICAL_DOC_NAME = re.compile(
    r"^(?:"
    r"CLEANUP_|CLOSING_CHECKLIST_V2_2_|RELEASE_NOTES_V2_2_|ROADMAP_V2_2_|"
    r"V2_2_|SMOKE_TEST_V2_2_|P2P_REHEARSAL_V2_2_|SNAPSHOT_RESTORE_DRILL_V2_2_"
    r")",
    re.IGNORECASE,
)
GENERATED_PATH = re.compile(
    r"(?:\.(?:log|tmp|bak|old|orig|swp|swo|zip|tar\.gz|pid|profraw|profdata)$|"
    r"(?:^|/)(?:target|logs|run|ci-evidence|node_modules|__pycache__)/|"
    r"(?:^|/)(?:\.DS_Store|Thumbs\.db|desktop\.ini)$)"
)
REFERENCE_PATH = re.compile(r"\b((?:scripts|configs)/[A-Za-z0-9_.\-/]+)")
MARKDOWN_LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


@dataclass
class Report:
    passes: list[str] = field(default_factory=list)
    failures: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def passed(self, message: str) -> None:
        self.passes.append(message)
        print(f"[PASS] {message}")

    def failed(self, message: str, details: Iterable[str] = ()) -> None:
        detail_list = [detail for detail in details if detail]
        rendered = message
        if detail_list:
            rendered += "\n" + "\n".join(f"  - {detail}" for detail in detail_list)
        self.failures.append(rendered)
        print(f"[FAIL] {message}", file=sys.stderr)
        for detail in detail_list:
            print(f"  - {detail}", file=sys.stderr)

    def warned(self, message: str) -> None:
        self.warnings.append(message)
        print(f"[WARN] {message}", file=sys.stderr)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--strict", action="store_true", help="Fail when a mandatory check fails.")
    mode.add_argument("--report", action="store_true", help="Report failures but return success.")
    return parser.parse_args()


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=False, capture_output=True, text=True)


def tracked_files() -> list[str]:
    result = run(["git", "ls-files", "-z"])
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "git ls-files failed")
    return [entry for entry in result.stdout.split("\0") if entry]


def current_docs() -> list[Path]:
    docs = [Path("README.md"), Path("CONTRIBUTING.md")]
    for path in Path("docs").rglob("*.md"):
        if "archive" in path.parts or "codex_tasks" in path.parts:
            continue
        if path.parent == Path("docs") and HISTORICAL_DOC_NAME.match(path.name):
            continue
        docs.append(path)
    return sorted(set(docs))


def version_check(report: Report) -> None:
    version = Path("VERSION").read_text(encoding="utf-8").strip()
    cargo = Path("Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$', cargo, re.MULTILINE)
    cargo_version = match.group(1) if match else ""
    if cargo_version and version.removeprefix("v") == cargo_version:
        report.passed(f"VERSION and Cargo workspace version are consistent ({version})")
    else:
        report.failed(f"VERSION/Cargo mismatch: VERSION={version} Cargo={cargo_version or 'missing'}")

    primary = {
        "README.md": Path("README.md").read_text(encoding="utf-8", errors="ignore"),
        "docs/VERSION_MATRIX.md": Path("docs/VERSION_MATRIX.md").read_text(
            encoding="utf-8", errors="ignore"
        ),
    }
    missing = [name for name, text in primary.items() if version not in text and cargo_version not in text]
    if missing:
        report.failed("primary version documents do not mention the current version", missing)
    else:
        report.passed("README and VERSION_MATRIX mention the current version")


def required_files_check(report: Report) -> None:
    required = [
        ".editorconfig",
        ".gitignore",
        ".github/pull_request_template.md",
        "CONTRIBUTING.md",
        "README.md",
        "VERSION",
        "Cargo.toml",
        "docs/REPOSITORY_STANDARDS.md",
        "docs/VERSION_MATRIX.md",
        "scripts/check_code_comment_language.py",
        "scripts/list_cleanup_candidates.sh",
        "scripts/repository_hygiene.py",
        "scripts/repository_hygiene.sh",
        "scripts/validate_repo_cleanup.sh",
    ]
    missing = [path for path in required if not Path(path).is_file()]
    if missing:
        report.failed("missing repository governance files", missing)
    else:
        report.passed("required repository governance files exist")


def generated_files_check(report: Report, paths: list[str]) -> None:
    generated = [path for path in paths if GENERATED_PATH.search(path)]
    if generated:
        report.failed("tracked generated, runtime, archive, or editor files detected", generated)
    else:
        report.passed("no tracked generated/runtime/editor files detected")


def secret_filename_check(report: Report, paths: list[str]) -> None:
    unsafe: list[str] = []
    for path in paths:
        parts = path.lower().split("/")
        basename = parts[-1]
        in_fixture = any(part in {"fixtures", "testdata", "tests"} for part in parts)
        if basename == ".env" or (
            basename.startswith(".env.") and basename not in {".env.example", ".env.sample"}
        ):
            unsafe.append(path)
        if not in_fixture and (
            basename in {"id_rsa", "id_ed25519"}
            or basename.endswith((".p12", ".pfx", ".private-key"))
            or (basename.endswith((".pem", ".key")) and "public" not in basename)
        ):
            unsafe.append(path)
    if unsafe:
        report.failed("tracked secret-like files detected", sorted(set(unsafe)))
    else:
        report.passed("no tracked secret-like files detected")


def path_portability_check(report: Report, paths: list[str]) -> None:
    problems: list[str] = []
    folded: dict[str, str] = {}
    for path in paths:
        if path != path.strip() or any(ord(char) < 32 for char in path):
            problems.append(f"invalid whitespace/control character: {path!r}")
        key = path.casefold()
        previous = folded.get(key)
        if previous is not None and previous != path:
            problems.append(f"case collision: {previous} <> {path}")
        folded[key] = path
    if problems:
        report.failed("non-portable or case-colliding tracked paths detected", problems)
    else:
        report.passed("tracked paths are portable and do not collide by case")


def markdown_link_check(report: Report) -> None:
    root = ROOT.resolve()
    problems: list[str] = []
    for document in current_docs():
        if not document.exists():
            continue
        text = document.read_text(encoding="utf-8", errors="ignore")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().split()[0].strip("<>")
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            relative = target.split("#", 1)[0]
            if not relative or any(token in relative for token in ("${", "<", ">", "*")):
                continue
            resolved = (document.parent / relative).resolve()
            try:
                resolved.relative_to(root)
            except ValueError:
                continue
            if not resolved.exists():
                problems.append(f"{document}: {target}")
    if problems:
        report.failed("broken local links detected in current documentation", sorted(set(problems)))
    else:
        report.passed("current Markdown local links resolve")


def referenced_path_check(report: Report) -> None:
    problems: list[str] = []
    for document in current_docs():
        if not document.exists():
            continue
        text = document.read_text(encoding="utf-8", errors="ignore")
        for raw_path in sorted(set(REFERENCE_PATH.findall(text))):
            path = raw_path.rstrip(".,:;)`]")
            if any(token in path for token in ("${", "<", ">", "*")):
                continue
            if not Path(path).exists():
                problems.append(f"{document}: missing referenced path {path}")
    if problems:
        report.failed("current documentation references missing scripts or configuration", problems)
    else:
        report.passed("current documentation references existing scripts and configuration")


def comment_language_check(report: Report) -> None:
    result = run([sys.executable, "scripts/check_code_comment_language.py"])
    if result.returncode == 0:
        report.passed("code comments satisfy the English-language policy")
    else:
        details = [line for line in (result.stderr or result.stdout).splitlines() if line]
        report.failed("non-English code-comment candidates detected", details)


def readiness_claim_check(report: Report) -> None:
    pattern = re.compile(r"public testnet (?:is|now) live", re.IGNORECASE)
    problems: list[str] = []
    for path in [Path("README.md"), Path("docs/VERSION_MATRIX.md"), Path("docs/ROADMAP_V2_3_0.md")]:
        if not path.exists():
            continue
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if pattern.search(line):
                problems.append(f"{path}:{line_number}: {line.strip()}")
    if problems:
        report.failed("unsupported public-testnet live claim detected", problems)
    else:
        report.passed("no unsupported public-testnet live claim detected")


def stale_cleanup_policy_check(report: Report) -> None:
    pattern = re.compile(r"v2_2_17|V2_2_17|v2_2_18|V2_2_18|old doc still in docs root")
    problems: list[str] = []
    for path in [Path("scripts/validate_repo_cleanup.sh"), Path("scripts/list_cleanup_candidates.sh")]:
        if not path.exists():
            continue
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if pattern.search(line):
                problems.append(f"{path}:{line_number}: {line.strip()}")
    if problems:
        report.failed("cleanup entrypoints contain stale release-pinned policy", problems)
    else:
        report.passed("repository hygiene policy is not pinned to obsolete releases")


def write_evidence(report: Report) -> None:
    out_dir_raw = os.environ.get("OUT_DIR")
    if not out_dir_raw:
        return
    out_dir = Path(out_dir_raw)
    out_dir.mkdir(parents=True, exist_ok=True)
    result = "PASS" if not report.failures else "FAIL"
    payload = {
        "gate": "repository-hygiene",
        "result": result,
        "checks": len(report.passes) + len(report.failures),
        "passes": len(report.passes),
        "failure_count": len(report.failures),
        "warning_count": len(report.warnings),
        "failures": report.failures,
        "warnings": report.warnings,
        "public_testnet_ready": False,
        "thirty_day_public_testnet_clock_started": False,
    }
    (out_dir / "repository-hygiene.json").write_text(
        json.dumps(payload, indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / "failures.txt").write_text(
        ("\n\n".join(report.failures) + "\n") if report.failures else "",
        encoding="utf-8",
    )
    (out_dir / "warnings.txt").write_text(
        ("\n".join(report.warnings) + "\n") if report.warnings else "",
        encoding="utf-8",
    )


def main() -> int:
    args = parse_args()
    report = Report()

    for dependency in ("git", "python3"):
        if shutil.which(dependency):
            report.passed(f"dependency available: {dependency}")
        else:
            report.failed(f"missing required dependency: {dependency}")

    try:
        paths = tracked_files() if not report.failures else []
    except RuntimeError as exc:
        report.failed(str(exc))
        paths = []

    required_files_check(report)
    version_check(report)
    generated_files_check(report, paths)
    secret_filename_check(report, paths)
    path_portability_check(report, paths)
    markdown_link_check(report)
    referenced_path_check(report)
    comment_language_check(report)
    readiness_claim_check(report)
    stale_cleanup_policy_check(report)

    write_evidence(report)
    result = "PASS" if not report.failures else "FAIL"
    print(
        f"SUMMARY: {result} ({len(report.passes)}/{len(report.passes) + len(report.failures)} "
        f"checks passed, {len(report.warnings)} warnings)"
    )
    if report.failures and not args.report:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
