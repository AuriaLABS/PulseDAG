#!/usr/bin/env python3
"""Operator-facing baseline capture harness for v2.2.4.

Focuses on repeatable measurement capture for:
- runtime/status responsiveness
- read-side RPC latency surfaces
- sync selected-peer stabilization timing
- churn/rejoin convergence timing (optional command hooks)
- restore/rebuild drill command durations
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import shlex
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

DEFAULT_ENDPOINTS = [
    "/runtime/status",
    "/status",
    "/sync/status",
    "/p2p/status",
    "/blocks/latest",
    "/tx/mempool",
    "/address/ping",
]


@dataclass
class EndpointSample:
    endpoint: str
    iteration: int
    latency_ms: float
    http_status: int
    ok: bool
    error: str


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sanitize_slug(raw: str) -> str:
    return "".join(c if c.isalnum() or c in "-_." else "_" for c in raw)


def request_json(url: str, timeout_seconds: float) -> tuple[int, Any, str]:
    req = Request(url, headers={"Accept": "application/json"})
    try:
        with urlopen(req, timeout=timeout_seconds) as resp:
            status = getattr(resp, "status", 200)
            body = resp.read().decode("utf-8", errors="replace")
            return status, json.loads(body) if body else {}, ""
    except HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace") if exc.fp else ""
        return int(exc.code), {}, body or str(exc)
    except URLError as exc:
        return 0, {}, str(exc)


def request_text_timed(url: str, timeout_seconds: float) -> tuple[float, int, bool, str]:
    started = time.perf_counter()
    req = Request(url, headers={"Accept": "application/json"})
    try:
        with urlopen(req, timeout=timeout_seconds) as resp:
            _ = resp.read()
            elapsed_ms = (time.perf_counter() - started) * 1000.0
            status = getattr(resp, "status", 200)
            return elapsed_ms, status, 200 <= status < 300, ""
    except HTTPError as exc:
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        return elapsed_ms, int(exc.code), False, str(exc)
    except URLError as exc:
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        return elapsed_ms, 0, False, str(exc)


def pick_first_key(payload: Any, keys: list[str]) -> Any:
    if not isinstance(payload, dict):
        return None
    for key in keys:
        if key in payload:
            return payload[key]
    data = payload.get("data")
    if isinstance(data, dict):
        for key in keys:
            if key in data:
                return data[key]
    return None


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    if len(values) == 1:
        return values[0]
    rank = (len(values) - 1) * p
    lo = int(rank)
    hi = min(lo + 1, len(values) - 1)
    if lo == hi:
        return values[lo]
    frac = rank - lo
    return values[lo] * (1.0 - frac) + values[hi] * frac


def write_endpoint_csv(path: Path, rows: list[EndpointSample]) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["endpoint", "iteration", "latency_ms", "http_status", "ok", "error"])
        for row in rows:
            writer.writerow([
                row.endpoint,
                row.iteration,
                f"{row.latency_ms:.3f}",
                row.http_status,
                str(row.ok).lower(),
                row.error,
            ])


def summarize_endpoint_rows(rows: list[EndpointSample]) -> list[dict[str, Any]]:
    by_endpoint: dict[str, list[EndpointSample]] = {}
    for row in rows:
        by_endpoint.setdefault(row.endpoint, []).append(row)
    summary: list[dict[str, Any]] = []
    for endpoint in sorted(by_endpoint):
        endpoint_rows = by_endpoint[endpoint]
        latencies = sorted(r.latency_ms for r in endpoint_rows)
        ok_rows = [r for r in endpoint_rows if r.ok]
        summary.append(
            {
                "endpoint": endpoint,
                "samples": len(endpoint_rows),
                "success_rate_pct": round((len(ok_rows) / len(endpoint_rows)) * 100.0, 2) if endpoint_rows else 0.0,
                "mean_ms": round(statistics.fmean(latencies), 3) if latencies else 0.0,
                "p50_ms": round(percentile(latencies, 0.50), 3) if latencies else 0.0,
                "p95_ms": round(percentile(latencies, 0.95), 3) if latencies else 0.0,
                "max_ms": round(max(latencies), 3) if latencies else 0.0,
            }
        )
    return summary


def run_endpoint_latency(base_url: str, endpoints: list[str], iterations: int, timeout_seconds: float, cooldown_seconds: float) -> list[EndpointSample]:
    rows: list[EndpointSample] = []
    for endpoint in endpoints:
        url = f"{base_url.rstrip('/')}{endpoint}"
        for i in range(1, iterations + 1):
            latency_ms, status, ok, error = request_text_timed(url, timeout_seconds)
            rows.append(EndpointSample(endpoint=endpoint, iteration=i, latency_ms=latency_ms, http_status=status, ok=ok, error=error))
            if cooldown_seconds > 0:
                time.sleep(cooldown_seconds)
    return rows


def run_sync_stabilization(base_url: str, timeout_seconds: float, poll_seconds: float, stable_polls: int, max_wait_seconds: float, lag_threshold: int) -> dict[str, Any]:
    url = f"{base_url.rstrip('/')}/sync/status"
    started = time.perf_counter()
    last_peer = None
    stable_count = 0
    polls = 0

    while True:
        polls += 1
        status, payload, err = request_json(url, timeout_seconds)
        if status < 200 or status >= 300:
            raise RuntimeError(f"sync/status request failed with status={status} err={err}")

        selected_peer = pick_first_key(payload, ["selected_peer", "selectedPeer", "selected"])
        lag = pick_first_key(payload, ["lag", "sync_lag", "block_lag", "height_lag"])
        try:
            lag_int = int(lag) if lag is not None else 0
        except (TypeError, ValueError):
            lag_int = 0

        if selected_peer == last_peer and selected_peer not in (None, ""):
            stable_count += 1
        else:
            stable_count = 1 if selected_peer not in (None, "") else 0
            last_peer = selected_peer

        elapsed = time.perf_counter() - started
        if stable_count >= stable_polls and lag_int <= lag_threshold:
            return {
                "result": "stabilized",
                "polls": polls,
                "elapsed_seconds": round(elapsed, 3),
                "selected_peer": selected_peer,
                "stable_polls": stable_count,
                "lag": lag_int,
                "lag_threshold": lag_threshold,
            }

        if elapsed > max_wait_seconds:
            return {
                "result": "timeout",
                "polls": polls,
                "elapsed_seconds": round(elapsed, 3),
                "selected_peer": selected_peer,
                "stable_polls": stable_count,
                "lag": lag_int,
                "lag_threshold": lag_threshold,
            }

        time.sleep(poll_seconds)


def run_command_timed(command: str, cwd: Path) -> dict[str, Any]:
    started = time.perf_counter()
    proc = subprocess.run(command, shell=True, cwd=str(cwd), capture_output=True, text=True)
    elapsed = time.perf_counter() - started
    return {
        "command": command,
        "elapsed_seconds": round(elapsed, 3),
        "return_code": proc.returncode,
        "stdout": proc.stdout[-8000:],
        "stderr": proc.stderr[-8000:],
    }


def write_markdown_report(path: Path, run_meta: dict[str, Any], endpoint_summary: list[dict[str, Any]], sync_summary: dict[str, Any], command_results: list[dict[str, Any]]) -> None:
    lines = [
        f"# v2.2.4 p2p/sync/runtime/rpc baseline run ({run_meta['run_id']})",
        "",
        f"- Captured at (UTC): **{run_meta['captured_at_utc']}**",
        f"- Target node: **{run_meta['base_url']}**",
        f"- Iterations per endpoint: **{run_meta['iterations']}**",
        "",
        "## RPC latency summary",
        "",
        "| Endpoint | Samples | Success % | Mean (ms) | p50 (ms) | p95 (ms) | Max (ms) |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for row in endpoint_summary:
        lines.append(
            f"| `{row['endpoint']}` | {row['samples']} | {row['success_rate_pct']:.2f} | {row['mean_ms']:.3f} | {row['p50_ms']:.3f} | {row['p95_ms']:.3f} | {row['max_ms']:.3f} |"
        )

    lines.extend([
        "",
        "## Sync selected-peer stabilization",
        "",
        f"- Result: **{sync_summary.get('result', 'not-run')}**",
        f"- Elapsed seconds: **{sync_summary.get('elapsed_seconds', 0)}**",
        f"- Polls: **{sync_summary.get('polls', 0)}**",
        f"- Selected peer: **{sync_summary.get('selected_peer', 'n/a')}**",
        f"- Stable polls observed: **{sync_summary.get('stable_polls', 0)}**",
        f"- Final lag / threshold: **{sync_summary.get('lag', 'n/a')} / {sync_summary.get('lag_threshold', 'n/a')}**",
    ])

    if command_results:
        lines.extend(["", "## Drill command timing", "", "| Command | Exit | Elapsed (s) |", "| --- | ---: | ---: |"])
        for result in command_results:
            lines.append(
                f"| `{result['command']}` | {result['return_code']} | {result['elapsed_seconds']:.3f} |"
            )

    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Capture v2.2.4 p2p/sync/runtime/rpc baseline evidence")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080", help="node base URL")
    parser.add_argument("--iterations", type=int, default=25, help="samples per endpoint")
    parser.add_argument("--timeout-seconds", type=float, default=2.0, help="HTTP timeout")
    parser.add_argument("--cooldown-seconds", type=float, default=0.05, help="sleep between samples")
    parser.add_argument("--endpoint", action="append", dest="endpoints", help="endpoint path (repeatable)")
    parser.add_argument("--output-dir", default="docs/benchmarks/artifacts", help="baseline artifact root")
    parser.add_argument("--sync-poll-seconds", type=float, default=1.0)
    parser.add_argument("--sync-stable-polls", type=int, default=5)
    parser.add_argument("--sync-max-wait-seconds", type=float, default=180.0)
    parser.add_argument("--sync-lag-threshold", type=int, default=0)
    parser.add_argument("--drill-command", action="append", default=[], help="optional command to time; repeatable")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    endpoints = args.endpoints if args.endpoints else DEFAULT_ENDPOINTS

    run_id = sanitize_slug(datetime.now(timezone.utc).strftime("v2_2_4_%Y%m%dT%H%M%SZ"))
    out_dir = Path(args.output_dir) / run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    endpoint_rows = run_endpoint_latency(
        base_url=args.base_url,
        endpoints=endpoints,
        iterations=args.iterations,
        timeout_seconds=args.timeout_seconds,
        cooldown_seconds=args.cooldown_seconds,
    )
    endpoint_csv = out_dir / "rpc_latency_samples.csv"
    write_endpoint_csv(endpoint_csv, endpoint_rows)
    endpoint_summary = summarize_endpoint_rows(endpoint_rows)

    sync_summary: dict[str, Any]
    try:
        sync_summary = run_sync_stabilization(
            base_url=args.base_url,
            timeout_seconds=args.timeout_seconds,
            poll_seconds=args.sync_poll_seconds,
            stable_polls=args.sync_stable_polls,
            max_wait_seconds=args.sync_max_wait_seconds,
            lag_threshold=args.sync_lag_threshold,
        )
    except Exception as exc:  # noqa: BLE001
        sync_summary = {"result": "error", "error": str(exc)}

    command_results: list[dict[str, Any]] = []
    for command in args.drill_command:
        command_results.append(run_command_timed(command=command, cwd=Path.cwd()))

    run_meta = {
        "run_id": run_id,
        "captured_at_utc": now_utc_iso(),
        "base_url": args.base_url,
        "iterations": args.iterations,
        "endpoints": endpoints,
    }

    (out_dir / "run_meta.json").write_text(json.dumps(run_meta, indent=2) + "\n", encoding="utf-8")
    (out_dir / "rpc_latency_summary.json").write_text(json.dumps(endpoint_summary, indent=2) + "\n", encoding="utf-8")
    (out_dir / "sync_stabilization.json").write_text(json.dumps(sync_summary, indent=2) + "\n", encoding="utf-8")
    (out_dir / "drill_command_results.json").write_text(json.dumps(command_results, indent=2) + "\n", encoding="utf-8")

    markdown_report = out_dir / "BASELINE_REPORT.md"
    write_markdown_report(markdown_report, run_meta, endpoint_summary, sync_summary, command_results)

    print(f"[baseline] artifacts written to {out_dir}")
    print(f"[baseline] markdown summary: {markdown_report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
