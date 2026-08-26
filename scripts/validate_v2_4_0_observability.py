#!/usr/bin/env python3
"""Validate the v2.4.0 release-candidate observability package fail closed."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "ops/observability/v2.4.0"
INVENTORY = PACKAGE / "metrics-inventory.json"
DASHBOARD = PACKAGE / "grafana-dashboard.json"
ALERTS = PACKAGE / "alert-rules.yml"
PROMETHEUS = PACKAGE / "prometheus-scrape.example.yml"
EXPORTER = ROOT / "scripts/private_testnet/runtime_metrics_exporter.py"
ROUTES = ROOT / "crates/pulsedag-rpc/src/routes.rs"

FIELD_LINE = re.compile(r"pub\s+([A-Za-z0-9_]+)\s*:")
METRIC_TOKEN = re.compile(r"\b(?:pulsedag_[a-z0-9_]+|up|clamp_min)\b")
PROM_TARGET = re.compile(r"^\s*-\s+([A-Za-z0-9_.-]+:9108)\s*$")
RUNBOOK = re.compile(r"^\s*runbook:\s*(\S+)\s*$")
EXPR = re.compile(r"^\s*expr:\s*(.+?)\s*$")
ALERT_NAME = re.compile(r"^\s*-\s+alert:\s+([A-Za-z0-9_]+)\s*$")

BUILTIN_METRICS = {
    "up",
    "clamp_min",
    "pulsedag_exporter_scrape_success",
    "pulsedag_exporter_last_scrape_timestamp_seconds",
    "pulsedag_exporter_endpoint_success",
    "pulsedag_exporter_info",
}
REQUIRED_PREFIXES = {
    "node": "pulsedag_node_",
    "chain": "pulsedag_chain_",
    "state": "pulsedag_state_",
    "mining": "pulsedag_mining_",
    "p2p": "pulsedag_p2p_",
    "recovery": "pulsedag_recovery_",
    "sync": "pulsedag_sync_",
    "rpc": "pulsedag_rpc_",
    "mempool": "pulsedag_mempool_",
}
REQUIRED_ALERTS = {
    "PulseDAGCommitPublishMismatch",
    "PulseDAGAcceptedHashLost",
    "PulseDAGInvalidStateRoot",
    "PulseDAGSnapshotVerificationStableFailure",
    "PulseDAGMiningSubmitActorTimeout",
    "PulseDAGMiningSubmitActorQueueFull",
    "PulseDAGPeerIsolation",
    "PulseDAGSelectedTipMismatch",
    "PulseDAGStorageReplayGap",
    "PulseDAGQuiescenceTipDivergence",
    "PulseDAGRPCLivenessDegraded",
}
PUBLIC_SAFE_ENDPOINTS = {"/metrics", "/status", "/mempool"}


class Validation:
    def __init__(self) -> None:
        self.errors: list[str] = []
        self.passes: list[str] = []

    def require(self, condition: bool, message: str) -> None:
        if condition:
            self.passes.append(message)
        else:
            self.errors.append(message)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def rust_struct_fields(path: Path, struct_name: str) -> set[str]:
    source = path.read_text(encoding="utf-8")
    match = re.search(rf"pub struct {re.escape(struct_name)}\s*\{{(.*?)\n\}}", source, re.S)
    if not match:
        raise RuntimeError(f"could not locate Rust struct {struct_name} in {path}")
    return {
        field.group(1)
        for line in match.group(1).splitlines()
        if (field := FIELD_LINE.match(line.strip()))
    }


def expressions_from_dashboard(payload: dict[str, Any]) -> list[str]:
    expressions: list[str] = []
    for panel in payload.get("panels", []):
        for target in panel.get("targets", []):
            expr = target.get("expr")
            if isinstance(expr, str):
                expressions.append(expr)
    for variable in payload.get("templating", {}).get("list", []):
        query = variable.get("query")
        if isinstance(query, str):
            expressions.append(query)
    return expressions


def referenced_metrics(expression: str) -> set[str]:
    return set(METRIC_TOKEN.findall(expression))


def function_slice(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        raise RuntimeError(f"route function marker missing: {marker}")
    end = source.find("\nfn ", start + len(marker))
    return source[start:] if end < 0 else source[start:end]


def validate_inventory(validation: Validation, inventory: dict[str, Any]) -> set[str]:
    validation.require(inventory.get("schema_version") == 2, "inventory schema version is 2")
    validation.require(inventory.get("release_line") == "v2.4.0", "inventory release line is v2.4.0")

    endpoint_specs = inventory.get("endpoints", {})
    metrics = inventory.get("metrics", [])
    validation.require(
        isinstance(endpoint_specs, dict) and set(endpoint_specs) == PUBLIC_SAFE_ENDPOINTS,
        "inventory uses exactly /metrics, /status and /mempool",
    )
    validation.require(isinstance(metrics, list) and bool(metrics), "metric inventory exists")

    endpoint_fields: dict[str, set[str]] = {}
    if isinstance(endpoint_specs, dict):
        for endpoint, spec in endpoint_specs.items():
            try:
                file_path = ROOT / spec["rust_file"]
                endpoint_fields[endpoint] = rust_struct_fields(file_path, spec["rust_struct"])
            except (KeyError, OSError, RuntimeError) as exc:
                validation.errors.append(f"invalid endpoint specification for {endpoint}: {exc}")

    names: set[str] = set()
    valid_types = {"gauge", "counter", "enum"}
    if isinstance(metrics, list):
        for metric in metrics:
            if not isinstance(metric, dict):
                validation.errors.append(f"metric entry is not an object: {metric!r}")
                continue
            name = metric.get("name")
            endpoint = metric.get("endpoint")
            field = metric.get("field")
            metric_type = metric.get("type")
            help_text = metric.get("help")
            if not isinstance(name, str) or not re.fullmatch(r"pulsedag_[a-z0-9_]+", name):
                validation.errors.append(f"invalid metric name: {name!r}")
                continue
            if name in names:
                validation.errors.append(f"duplicate metric name: {name}")
            names.add(name)
            if endpoint not in endpoint_fields:
                validation.errors.append(f"metric {name} references unknown endpoint: {endpoint}")
            elif field not in endpoint_fields[endpoint]:
                validation.errors.append(f"metric {name} references missing field {field} in {endpoint}")
            if metric_type not in valid_types:
                validation.errors.append(f"metric {name} has invalid type: {metric_type}")
            if not isinstance(help_text, str) or not help_text.endswith("."):
                validation.errors.append(f"metric {name} help must be a complete English sentence")
            if metric_type == "enum" and not metric.get("values"):
                validation.errors.append(f"enum metric {name} has no allowed values")
            if "scale" in metric and not isinstance(metric["scale"], (int, float)):
                validation.errors.append(f"metric {name} scale must be numeric")

    for surface, prefix in REQUIRED_PREFIXES.items():
        validation.require(any(name.startswith(prefix) for name in names), f"{surface} metrics are present")
    validation.require(len(names) >= 60, "inventory includes at least 60 v2.4 operator metrics")
    return names


def validate_metric_references(
    validation: Validation,
    expressions: list[str],
    allowed_metrics: set[str],
    surface: str,
) -> None:
    for expression in expressions:
        unknown = referenced_metrics(expression) - allowed_metrics - BUILTIN_METRICS
        if unknown:
            validation.errors.append(
                f"{surface} expression references unknown metrics {sorted(unknown)}: {expression}"
            )
    validation.require(bool(expressions), f"{surface} includes metric expressions")


def validate_dashboard(validation: Validation, allowed_metrics: set[str]) -> None:
    dashboard = load_json(DASHBOARD)
    panels = dashboard.get("panels", [])
    validation.require(dashboard.get("uid") == "pulsedag-v240-ops", "dashboard UID is v2.4 versioned")
    validation.require("v2.4.0" in dashboard.get("tags", []), "dashboard carries v2.4.0 tag")
    validation.require("public-safe" in dashboard.get("tags", []), "dashboard carries public-safe tag")
    validation.require(len(panels) >= 15, "dashboard includes at least 15 panels")
    validate_metric_references(
        validation,
        expressions_from_dashboard(dashboard),
        allowed_metrics,
        "dashboard",
    )


def validate_alerts(validation: Validation, allowed_metrics: set[str]) -> None:
    body = ALERTS.read_text(encoding="utf-8")
    expressions = [match.group(1) for line in body.splitlines() if (match := EXPR.match(line))]
    runbooks = [match.group(1) for line in body.splitlines() if (match := RUNBOOK.match(line))]
    alerts = {match.group(1) for line in body.splitlines() if (match := ALERT_NAME.match(line))}
    validation.require(len(alerts) >= 20, "alert package includes at least 20 rules")
    validation.require(REQUIRED_ALERTS <= alerts, "mandatory v2.4 hard-stop alert classes exist")
    validation.require("severity: critical" in body, "critical alert severity exists")
    validation.require("severity: warning" in body, "warning alert severity exists")
    validate_metric_references(validation, expressions, allowed_metrics, "alert")
    validation.require(len(runbooks) == len(alerts), "every alert has a runbook annotation")
    for runbook in runbooks:
        validation.require((ROOT / runbook).is_file(), f"alert runbook exists: {runbook}")


def validate_prometheus(validation: Validation) -> None:
    body = PROMETHEUS.read_text(encoding="utf-8")
    targets = {match.group(1) for line in body.splitlines() if (match := PROM_TARGET.match(line))}
    validation.require(len(targets) == 5, "Prometheus example contains exactly five unique exporter targets")
    validation.require("metrics_path: /metrics" in body, "Prometheus scrapes the exporter metrics path")
    validation.require("alert-rules.yml" in body, "Prometheus loads the versioned alert rules")
    validation.require("private-testnet-v2.4.0" in body, "Prometheus labels the v2.4.0 private rehearsal network")
    validation.require("example.invalid" in body, "Prometheus example uses deliberately non-routable placeholders")


def validate_public_safe_routes(validation: Validation) -> None:
    routes = ROUTES.read_text(encoding="utf-8")
    public_safe = function_slice(routes, "fn public_safe_routes")
    for endpoint in sorted(PUBLIC_SAFE_ENDPOINTS):
        validation.require(f'"{endpoint}"' in public_safe, f"public_safe exposes required read route {endpoint}")
    validation.require('"/runtime"' not in public_safe, "public_safe does not expose /runtime")
    validation.require('"/admin"' not in public_safe, "public_safe does not expose /admin")


def validate_exporter(validation: Validation) -> None:
    source = EXPORTER.read_text(encoding="utf-8")
    validation.require("SUPPORTED_RELEASE_LINES" in source, "exporter has an explicit release-line allowlist")
    validation.require('"v2.3.0"' in source, "exporter preserves v2.3.0 inventory compatibility")
    validation.require('"v2.4.0"' in source, "exporter accepts v2.4.0 inventories")


def validate_files(validation: Validation) -> None:
    required = [INVENTORY, DASHBOARD, ALERTS, PROMETHEUS, EXPORTER, ROUTES]
    for path in required:
        validation.require(path.is_file(), f"required observability file exists: {path.relative_to(ROOT)}")


def main() -> int:
    validation = Validation()
    validate_files(validation)
    try:
        inventory = load_json(INVENTORY)
        metric_names = validate_inventory(validation, inventory)
        validate_dashboard(validation, metric_names)
        validate_alerts(validation, metric_names)
        validate_prometheus(validation)
        validate_public_safe_routes(validation)
        validate_exporter(validation)
    except (OSError, json.JSONDecodeError, RuntimeError) as exc:
        validation.errors.append(str(exc))

    if validation.errors:
        print("v2.4.0 observability validation failed:", file=sys.stderr)
        for error in validation.errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("PASS: v2.4.0 observability package is public-safe, versioned and fail-closed")
    print(f"validated {len(validation.passes)} package invariants")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
