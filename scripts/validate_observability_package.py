#!/usr/bin/env python3
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

STRUCT_SPECS = {
    "/runtime/status": (ROOT / "crates/pulsedag-rpc/src/handlers/runtime.rs", "RuntimeStatusData"),
    "/status": (ROOT / "crates/pulsedag-rpc/src/handlers/status.rs", "NodeStatusData"),
    "/sync/status": (ROOT / "crates/pulsedag-rpc/src/handlers/sync.rs", "SyncStatusData"),
    "/tx/mempool": (ROOT / "crates/pulsedag-rpc/src/handlers/tx.rs", "MempoolData"),
    "/pow/health": (ROOT / "crates/pulsedag-rpc/src/handlers/pow_health.rs", "PowHealthData"),
}


def extract_struct_fields(file_path: Path, struct_name: str) -> set[str]:
    text = file_path.read_text()
    block = re.search(rf"pub struct {re.escape(struct_name)}\s*\{{(.*?)\n\}}", text, re.S)
    if not block:
        raise RuntimeError(f"could not locate struct {struct_name} in {file_path}")
    fields = set()
    for line in block.group(1).splitlines():
        line = line.strip()
        m = re.match(r"pub\s+([a-zA-Z0-9_]+)\s*:", line)
        if m:
            fields.add(m.group(1))
    return fields


def load_json(path: Path):
    return json.loads(path.read_text())


def main() -> int:
    dashboards = load_json(ROOT / "ops/dashboard/v2.2/official-dashboards.json")
    alerts = load_json(ROOT / "ops/dashboard/v2.2/official-alert-rules.json")

    endpoint_fields = {
        endpoint: extract_struct_fields(file_path, struct_name)
        for endpoint, (file_path, struct_name) in STRUCT_SPECS.items()
    }

    # nested runtime.sync_counters fields surfaced from SyncProgressCounters in runtime.rs usage/tests
    runtime_nested_fields = {
        "sync_counters.blocks_requested",
        "sync_counters.blocks_applied",
    }

    errors = []

    for dashboard in dashboards["dashboards"]:
        for panel in dashboard["panels"]:
            endpoint = panel["endpoint"]
            field = panel["field"]
            if endpoint not in endpoint_fields:
                errors.append(f"unknown endpoint in dashboard panel: {endpoint}")
                continue
            if "." in field:
                if endpoint == "/runtime/status" and field in runtime_nested_fields:
                    continue
                errors.append(f"unsupported nested field reference {field} ({panel['title']})")
                continue
            if field not in endpoint_fields[endpoint]:
                errors.append(f"missing field {field} in {endpoint} ({panel['title']})")

    for alert in alerts["alerts"]:
        endpoint = alert["endpoint"]
        field = alert["field"]
        if endpoint not in endpoint_fields:
            errors.append(f"unknown endpoint in alert: {endpoint}")
            continue
        if field not in endpoint_fields[endpoint]:
            errors.append(f"missing alert field {field} in {endpoint} ({alert['id']})")
        runbook = ROOT / alert["runbook"]
        if not runbook.exists():
            errors.append(f"missing runbook path for alert {alert['id']}: {alert['runbook']}")

    # link validation from docs README
    docs_readme = (ROOT / "docs/dashboard/README.md").read_text()
    required_links = [
        "ops/dashboard/v2.2/official-dashboards.json",
        "ops/dashboard/v2.2/official-alert-rules.json",
        "scripts/validate_observability_package.py",
        "docs/dashboard/ALERTS.md",
    ]
    for link in required_links:
        if link not in docs_readme:
            errors.append(f"docs/dashboard/README.md missing link: {link}")

    if errors:
        print("observability package validation failed:")
        for err in errors:
            print(f"- {err}")
        return 1

    print("observability package validation passed")
    print(f"validated {len(dashboards['dashboards'])} dashboards and {len(alerts['alerts'])} alerts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
