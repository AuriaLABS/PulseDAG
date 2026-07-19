#!/usr/bin/env python3
"""Validate the v2.3.0 private-testnet operator and incident runbook package."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNBOOKS = ROOT / "docs/runbooks"
INDEX = RUNBOOKS / "INDEX.md"
OPERATIONS = RUNBOOKS / "V2_3_0_PRIVATE_TESTNET_OPERATIONS.md"
INCIDENT = RUNBOOKS / "V2_3_0_INCIDENT_RESPONSE.md"
SECURITY = RUNBOOKS / "V2_3_0_SECURITY_AND_CAPACITY.md"
COLLECTOR = ROOT / "scripts/private_testnet/collect_incident_evidence.py"
ALERTS = ROOT / "ops/observability/v2.3.0/alert-rules.yml"
PATH_REFERENCE = re.compile(r"`((?:docs|scripts|ops|configs)/[A-Za-z0-9_./-]+)`")
RUNBOOK_ANNOTATION = re.compile(r"^\s*runbook:\s*(\S+)\s*$", re.MULTILINE)
FORBIDDEN_CLAIM = re.compile(
    r"(?:public testnet (?:is|now) live|public_testnet_ready\s*[:=]\s*true|"
    r"thirty_day_public_testnet_clock_started\s*[:=]\s*true)",
    re.IGNORECASE,
)


class Validation:
    """Collect deterministic validation failures and passes."""

    def __init__(self) -> None:
        self.errors: list[str] = []
        self.passes: list[str] = []

    def require(self, condition: bool, message: str) -> None:
        if condition:
            self.passes.append(message)
        else:
            self.errors.append(message)


def read(path: Path) -> str:
    """Read one UTF-8 repository document."""

    return path.read_text(encoding="utf-8")


def validate_required_files(validation: Validation) -> None:
    """Require the canonical v2.3.0 runbooks and collector."""

    for path in (INDEX, OPERATIONS, INCIDENT, SECURITY, COLLECTOR, ALERTS):
        validation.require(path.is_file(), f"required file exists: {path.relative_to(ROOT)}")


def validate_index(validation: Validation) -> None:
    """Validate active and compatibility runbook index references."""

    text = read(INDEX)
    validation.require(text.startswith("# PulseDAG v2.3.0"), "runbook index is labeled v2.3.0")
    required_references = [
        "V2_3_0_PRIVATE_TESTNET_OPERATIONS.md",
        "V2_3_0_INCIDENT_RESPONSE.md",
        "V2_3_0_SECURITY_AND_CAPACITY.md",
        "SNAPSHOT_RESTORE.md",
        "REBUILD_FROM_SNAPSHOT_AND_DELTA.md",
        "RELEASE_EVIDENCE.md",
        "P2P_RECOVERY.md",
        "STAGING_UPGRADE.md",
        "STAGING_ROLLBACK.md",
        "docs/dashboard/README.md",
        "collect_incident_evidence.py",
    ]
    for reference in required_references:
        validation.require(reference in text, f"runbook index references {reference}")


def validate_sections(validation: Validation) -> None:
    """Require decision-critical sections in each canonical runbook."""

    required_sections = {
        OPERATIONS: [
            "## Scope and guardrails",
            "## 1. Bootstrap a node host",
            "## 2. Attach the external miner",
            "## 4. Upgrade and rollback",
            "## 5. Snapshot, prune, restore, and backup",
            "## 7. Incident evidence",
            "## Exit criteria for an operator action",
        ],
        INCIDENT: [
            "## Roles",
            "## Severity model",
            "### SEV-1",
            "### SEV-2",
            "### SEV-3",
            "### SEV-4",
            "## Incident phases",
            "## Automatic no-go conditions",
        ],
        SECURITY: [
            "## RPC abuse or request pressure",
            "## Disk pressure",
            "## Node identity rotation",
            "## Operator token rotation",
            "## Monitoring-network access",
            "## Exit criteria",
        ],
    }
    for path, sections in required_sections.items():
        text = read(path)
        for section in sections:
            validation.require(section in text, f"{path.name} contains {section}")


def validate_repository_paths(validation: Validation) -> None:
    """Ensure repository paths referenced by canonical runbooks exist."""

    for document in (INDEX, OPERATIONS, INCIDENT, SECURITY):
        text = read(document)
        for raw_path in sorted(set(PATH_REFERENCE.findall(text))):
            path = raw_path.rstrip(".,:;)")
            validation.require((ROOT / path).exists(), f"referenced path exists: {path}")


def validate_alert_runbooks(validation: Validation) -> None:
    """Ensure every versioned alert points to an existing runbook."""

    text = read(ALERTS)
    runbooks = RUNBOOK_ANNOTATION.findall(text)
    validation.require(bool(runbooks), "alert rules contain runbook annotations")
    for raw_path in runbooks:
        validation.require((ROOT / raw_path).is_file(), f"alert runbook exists: {raw_path}")


def validate_collector_contract(validation: Validation) -> None:
    """Verify evidence redaction, immutability, checksums, and guardrail fields are implemented."""

    text = read(COLLECTOR)
    required_markers = [
        "SENSITIVE_KEY",
        '"<redacted>"',
        '"SHA256SUMS"',
        "incident evidence bundle already exists",
        '"public_testnet_ready": False',
        '"thirty_day_public_testnet_clock_started": False',
        'result = "PASS" if',
        'else "PARTIAL"',
    ]
    for marker in required_markers:
        validation.require(marker in text, f"incident collector contains contract marker: {marker}")
    for endpoint in (
        "/health",
        "/status",
        "/sync/status",
        "/sync/verify",
        "/p2p/status",
        "/tx/mempool",
        "/pow/health",
        "/snapshot",
    ):
        validation.require(endpoint in text, f"incident collector includes endpoint {endpoint}")


def validate_guardrails(validation: Validation) -> None:
    """Reject unsupported launch/readiness claims from active runbook material."""

    for path in (INDEX, OPERATIONS, INCIDENT, SECURITY):
        text = read(path)
        validation.require(
            FORBIDDEN_CLAIM.search(text) is None,
            f"{path.name} contains no unsupported public-testnet claim",
        )
        validation.require(
            "30-day public-testnet clock" in text or "30-day clock" in text,
            f"{path.name} preserves the public-testnet clock guardrail",
        )


def main() -> int:
    """Run the complete Task 11 documentation and evidence contract."""

    validation = Validation()
    validate_required_files(validation)
    if not validation.errors:
        validate_index(validation)
        validate_sections(validation)
        validate_repository_paths(validation)
        validate_alert_runbooks(validation)
        validate_collector_contract(validation)
        validate_guardrails(validation)

    if validation.errors:
        print("v2.3.0 runbook validation failed:", file=sys.stderr)
        for error in validation.errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("v2.3.0 runbook validation passed")
    print(f"validated {len(validation.passes)} package invariants")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
