#!/usr/bin/env python3
"""Validate the complete v2.3.0 private-testnet observability package."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "ops/observability/v2.3.0"
INVENTORY = PACKAGE / "metrics-inventory.json"
DASHBOARD = PACKAGE / "grafana-dashboard.json"
ALERTS = PACKAGE / "alert-rules.yml"
PROMETHEUS = PACKAGE / "prometheus-scrape.example.yml"
EXPORTER = ROOT / "scripts/private_testnet/runtime_metrics_exporter.py"
METRIC_TOKEN = re.compile(r"\b(?:pulsedag_[a-z0-9_]+|up|clamp_min)\b")
FIELD_LINE = re.compile(r"pub\s+([A-Za-z0-9_]+)\s*:")
PROM_TARGET = re.compile(r"^\s*-\s+([A-Za-z0-9_.-]+:9108)\s*$")
RUNBOOK = re.compile(r"^\s*runbook:\s*(\S+)\s*$")
EXPR = re.compile(r"^\s*expr:\s*(.+?)\s*$")

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
    "sync": "pulsedag_sync_",
    "mempool": "pulsedag_mempool_",
    "pow": "pulsedag_pow_",
}


class Validation:
    """Collect validation failures without hiding later diagnostics."""

    def __init__(self) -> None:
        self.errors: list[str] = []
        self.passes: list[str] = []

    def require(self, condition: bool, message: str) -> None:
        if condition:
            self.passes.append(message)
        else:
            self.errors.append(message)


def load_json(path: Path) -> Any:
    """Load one UTF-8 JSON document."""

    return json.loads(path.read_text(encoding="utf-8"))


def rust_struct_fields(path: Path, struct_name: str) -> set[str]:
    """Extract public fields from one Rust response structure."""

    text = path.read_text(encoding="utf-8")
    match = re.search(rf"pub struct {re.escape(struct_name)}\s*\{{(.*?)\n\}}", text, re.S)
    if not match:
        raise RuntimeError(f"could not locate Rust struct {struct_name} in {path}")
    return {
        field.group(1)
        for line in match.group(1).splitlines()
        if (field := FIELD_LINE.match(line.strip()))
    }


def expressions_from_dashboard(payload: dict[str, Any]) -> list[str]:
    """Collect every Grafana PromQL expression, including template queries."""

    expressions: list[str] = []
    for panel in payload.get("panels", []):
        for target in panel.get("targets", []):
            expression = target.get("expr")
            if isinstance(expression, str):
                expressions.append(expression)
    for variable in payload.get("templating", {}).get("list", []):
        query = variable.get("query")
        if isinstance(query, str):
            expressions.append(query)
    return expressions


def referenced_metrics(expression: str) -> set[str]:
    """Return PulseDAG and allowed PromQL identifiers from one expression."""

    return set(METRIC_TOKEN.findall(expression))


def validate_inventory(validation: Validation, inventory: dict[str, Any]) -> set[str]:
    """Validate inventory uniqueness, types, endpoint fields, and required surfaces."""

    validation.require(inventory.get("release_line") == "v2.3.0", "inventory release line is v2.3.0")
    endpoint_specs = inventory.get("endpoints", {})
    metrics = inventory.get("metrics", [])
    validation.require(isinstance(endpoint_specs, dict) and bool(endpoint_specs), "endpoint inventory exists")
    validation.require(isinstance(metrics, list) and bool(metrics), "metric inventory exists")

    endpoint_fields: dict[str, set[str]] = {}
    for endpoint, spec in endpoint_specs.items():
        try:
            file_path = ROOT / spec["rust_file"]
            endpoint_fields[endpoint] = rust_struct_fields(file_path, spec["rust_struct"])
        except (KeyError, OSError, RuntimeError) as exc:
            validation.errors.append(f"invalid endpoint specification for {endpoint}: {exc}")

    names: set[str] = set()
    valid_types = {"gauge", "counter", "enum"}
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
    validation.require(len(names) >= 30, "inventory includes at least 30 operator metrics")
    return names


def validate_metric_references(
    validation: Validation,
    expressions: list[str],
    allowed_metrics: set[str],
    surface: str,
) -> None:
    """Ensure every PulseDAG metric used by a dashboard or alert is defined."""

    for expression in expressions:
        unknown = referenced_metrics(expression) - allowed_metrics - BUILTIN_METRICS
        if unknown:
            validation.errors.append(
                f"{surface} expression references unknown metrics {sorted(unknown)}: {expression}"
            )
    validation.require(bool(expressions), f"{surface} includes metric expressions")


def validate_dashboard(validation: Validation, allowed_metrics: set[str]) -> None:
    """Validate dashboard identity, panel coverage, and PromQL references."""

    dashboard = load_json(DASHBOARD)
    panels = dashboard.get("panels", [])
    validation.require(dashboard.get("uid") == "pulsedag-private-v230", "dashboard UID is versioned")
    validation.require("v2.3.0" in dashboard.get("tags", []), "dashboard carries the v2.3.0 tag")
    validation.require(len(panels) >= 10, "dashboard includes at least 10 panels")
    validate_metric_references(
        validation,
        expressions_from_dashboard(dashboard),
        allowed_metrics,
        "dashboard",
    )


def validate_alerts(validation: Validation, allowed_metrics: set[str]) -> None:
    """Validate alert expressions, severity metadata, and runbook links."""

    text = ALERTS.read_text(encoding="utf-8")
    expressions = [match.group(1) for line in text.splitlines() if (match := EXPR.match(line))]
    runbooks = [match.group(1) for line in text.splitlines() if (match := RUNBOOK.match(line))]
    alert_count = len(re.findall(r"^\s*-\s+alert:\s+", text, re.MULTILINE))
    validation.require(alert_count >= 12, "alert package includes at least 12 rules")
    validation.require("severity: critical" in text, "critical alert severity exists")
    validation.require("severity: warning" in text, "warning alert severity exists")
    validate_metric_references(validation, expressions, allowed_metrics, "alert")
    for runbook in runbooks:
        validation.require((ROOT / runbook).is_file(), f"alert runbook exists: {runbook}")


def validate_prometheus(validation: Validation) -> None:
    """Validate the five-node scrape baseline and rule wiring."""

    text = PROMETHEUS.read_text(encoding="utf-8")
    targets = {match.group(1) for line in text.splitlines() if (match := PROM_TARGET.match(line))}
    validation.require(len(targets) == 5, "Prometheus example contains exactly five unique exporter targets")
    validation.require("metrics_path: /metrics" in text, "Prometheus scrapes the exporter metrics path")
    validation.require("alert-rules.yml" in text, "Prometheus loads the versioned alert rules")
    validation.require("private-testnet-v2.3.0" in text, "Prometheus labels the v2.3.0 private network")


def validate_files(validation: Validation) -> None:
    """Require all canonical package files and exporter entrypoints."""

    required = [INVENTORY, DASHBOARD, ALERTS, PROMETHEUS, EXPORTER]
    for path in required:
        validation.require(path.is_file(), f"required observability file exists: {path.relative_to(ROOT)}")


def main() -> int:
    """Run all package checks and print deterministic diagnostics."""

    validation = Validation()
    validate_files(validation)
    try:
        inventory = load_json(INVENTORY)
        metric_names = validate_inventory(validation, inventory)
        validate_dashboard(validation, metric_names)
        validate_alerts(validation, metric_names)
        validate_prometheus(validation)
    except (OSError, json.JSONDecodeError, RuntimeError) as exc:
        validation.errors.append(str(exc))

    if validation.errors:
        print("v2.3.0 observability validation failed:", file=sys.stderr)
        for error in validation.errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("v2.3.0 observability validation passed")
    print(f"validated {len(validation.passes)} package invariants")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
