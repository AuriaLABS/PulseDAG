#!/usr/bin/env python3
"""Expose versioned PulseDAG RPC fields in Prometheus text format."""

from __future__ import annotations

import argparse
import json
import math
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

DEFAULT_INVENTORY = Path("ops/observability/v2.3.0/metrics-inventory.json")


class ExporterError(RuntimeError):
    """Raised when inventory or RPC data violates the exporter contract."""


def load_inventory(path: Path) -> dict[str, Any]:
    """Load and minimally validate the versioned metric inventory."""

    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("release_line") != "v2.3.0":
        raise ExporterError("metrics inventory must target release line v2.3.0")
    metrics = payload.get("metrics")
    if not isinstance(metrics, list) or not metrics:
        raise ExporterError("metrics inventory must contain a non-empty metrics list")
    names: set[str] = set()
    for metric in metrics:
        name = metric.get("name")
        if not isinstance(name, str) or not name.startswith("pulsedag_"):
            raise ExporterError(f"invalid metric name: {name!r}")
        if name in names:
            raise ExporterError(f"duplicate metric name: {name}")
        names.add(name)
    return payload


def fetch_endpoint(node_url: str, endpoint: str, timeout: float) -> dict[str, Any]:
    """Fetch one RPC endpoint and unwrap the stable ApiResponse data field."""

    url = f"{node_url.rstrip('/')}{endpoint}"
    request = urllib.request.Request(url, headers={"accept": "application/json"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        payload = json.load(response)
    if payload.get("ok") is not True or not isinstance(payload.get("data"), dict):
        error = payload.get("error") or {}
        raise ExporterError(f"RPC endpoint {endpoint} returned an error: {error}")
    return payload["data"]


def prometheus_escape(value: str) -> str:
    """Escape a Prometheus label value."""

    return value.replace("\\", "\\\\").replace("\n", "\\n").replace('"', '\\"')


def numeric_value(value: object, scale: float) -> float | None:
    """Convert booleans and finite numeric values to a scaled sample."""

    if isinstance(value, bool):
        return 1.0 if value else 0.0
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        converted = float(value) * scale
        return converted if math.isfinite(converted) else None
    return None


def format_sample(name: str, value: float, labels: dict[str, str] | None = None) -> str:
    """Format one Prometheus sample."""

    label_text = ""
    if labels:
        rendered = ",".join(
            f'{key}="{prometheus_escape(label_value)}"'
            for key, label_value in sorted(labels.items())
        )
        label_text = "{" + rendered + "}"
    return f"{name}{label_text} {value:g}"


def collect_metrics(
    node_url: str,
    inventory: dict[str, Any],
    timeout: float,
    instance: str,
) -> tuple[str, bool]:
    """Collect all configured endpoints and render a complete scrape."""

    endpoint_data: dict[str, dict[str, Any]] = {}
    endpoint_errors: dict[str, str] = {}
    for endpoint in inventory["endpoints"]:
        try:
            endpoint_data[endpoint] = fetch_endpoint(node_url, endpoint, timeout)
        except (ExporterError, urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            endpoint_errors[endpoint] = str(exc)

    lines = [
        "# HELP pulsedag_exporter_scrape_success Whether all configured node RPC endpoints were collected.",
        "# TYPE pulsedag_exporter_scrape_success gauge",
        format_sample("pulsedag_exporter_scrape_success", 0.0 if endpoint_errors else 1.0),
        "# HELP pulsedag_exporter_last_scrape_timestamp_seconds Unix timestamp of this exporter scrape.",
        "# TYPE pulsedag_exporter_last_scrape_timestamp_seconds gauge",
        format_sample("pulsedag_exporter_last_scrape_timestamp_seconds", time.time()),
        "# HELP pulsedag_exporter_endpoint_success Whether one configured RPC endpoint was collected.",
        "# TYPE pulsedag_exporter_endpoint_success gauge",
    ]
    for endpoint in inventory["endpoints"]:
        lines.append(
            format_sample(
                "pulsedag_exporter_endpoint_success",
                0.0 if endpoint in endpoint_errors else 1.0,
                {"endpoint": endpoint},
            )
        )

    for metric in inventory["metrics"]:
        name = metric["name"]
        metric_type = metric["type"]
        endpoint = metric["endpoint"]
        field = metric["field"]
        lines.append(f"# HELP {name} {metric['help']}")
        lines.append(f"# TYPE {name} {'gauge' if metric_type == 'enum' else metric_type}")

        data = endpoint_data.get(endpoint)
        if data is None or field not in data or data[field] is None:
            continue
        raw_value = data[field]
        if metric_type == "enum":
            if not isinstance(raw_value, str):
                continue
            for allowed in metric["values"]:
                lines.append(
                    format_sample(
                        name,
                        1.0 if raw_value == allowed else 0.0,
                        {"status": allowed},
                    )
                )
            continue

        converted = numeric_value(raw_value, float(metric.get("scale", 1.0)))
        if converted is not None:
            lines.append(format_sample(name, converted))

    lines.extend(
        [
            "# HELP pulsedag_exporter_info Static exporter identity.",
            "# TYPE pulsedag_exporter_info gauge",
            format_sample(
                "pulsedag_exporter_info",
                1.0,
                {"instance": instance, "release_line": inventory["release_line"]},
            ),
        ]
    )
    return "\n".join(lines) + "\n", not endpoint_errors


def build_handler(
    node_url: str,
    inventory: dict[str, Any],
    timeout: float,
    instance: str,
) -> type[BaseHTTPRequestHandler]:
    """Create an HTTP handler bound to one exporter configuration."""

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path not in {"/metrics", "/health"}:
                self.send_error(404)
                return
            body, success = collect_metrics(node_url, inventory, timeout, instance)
            if self.path == "/health":
                body = json.dumps({"status": "ok" if success else "degraded"}) + "\n"
                content_type = "application/json"
            else:
                content_type = "text/plain; version=0.0.4; charset=utf-8"
            encoded = body.encode("utf-8")
            self.send_response(200 if success or self.path == "/metrics" else 503)
            self.send_header("content-type", content_type)
            self.send_header("content-length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def log_message(self, *_args: object) -> None:
            return

    return Handler


def main() -> int:
    """Run one collection or serve a Prometheus endpoint."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node-url", default="http://127.0.0.1:8280")
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    parser.add_argument("--listen", default="127.0.0.1:9108")
    parser.add_argument("--instance", default="pulsedag-node")
    parser.add_argument("--timeout", type=float, default=3.0)
    parser.add_argument("--once", action="store_true")
    args = parser.parse_args()

    try:
        inventory = load_inventory(args.inventory)
        if args.once:
            body, success = collect_metrics(args.node_url, inventory, args.timeout, args.instance)
            print(body, end="")
            return 0 if success else 1
        host, separator, port_raw = args.listen.rpartition(":")
        if not separator or not port_raw.isdigit():
            raise ExporterError(f"invalid --listen value: {args.listen!r}")
        server = ThreadingHTTPServer(
            (host, int(port_raw)),
            build_handler(args.node_url, inventory, args.timeout, args.instance),
        )
        server.serve_forever()
    except (ExporterError, OSError, json.JSONDecodeError) as exc:
        print(f"runtime metrics exporter failed: {exc}", file=__import__("sys").stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
