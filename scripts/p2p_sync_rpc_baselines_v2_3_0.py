#!/usr/bin/env python3
"""Current v2.3.0 entrypoint for P2P, sync, runtime, and RPC baselines."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import p2p_sync_rpc_baselines as legacy


_original_sanitize_slug = legacy.sanitize_slug
_original_write_markdown_report = legacy.write_markdown_report


def sanitize_slug(raw: str) -> str:
    """Translate the retained engine's historical run prefix to v2.3.0."""
    return _original_sanitize_slug(raw).replace("v2_2_4_", "v2_3_0_", 1)


def write_markdown_report(
    path: Path,
    run_meta: dict[str, Any],
    endpoint_summary: list[dict[str, Any]],
    sync_summary: dict[str, Any],
    command_results: list[dict[str, Any]],
    threshold_summary: dict[str, Any] | None,
) -> None:
    """Write the retained report format with current v2.3.0 identity."""
    _original_write_markdown_report(
        path,
        run_meta,
        endpoint_summary,
        sync_summary,
        command_results,
        threshold_summary,
    )
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        "# v2.2.4 p2p/sync/runtime/rpc baseline run",
        "# v2.3.0 p2p/sync/runtime/rpc baseline run",
        1,
    )
    path.write_text(text, encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture v2.3.0 P2P, sync, runtime, and RPC baseline evidence"
    )
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
    parser.add_argument(
        "--drill-command",
        action="append",
        default=[],
        help="optional command to time; repeatable",
    )
    parser.add_argument(
        "--hot-path",
        action="append",
        choices=sorted(legacy.HOT_PATH_GROUPS.keys()),
        default=[],
        help="hot-path endpoint group to include (repeatable)",
    )
    parser.add_argument(
        "--threshold-profile",
        default=legacy.DEFAULT_THRESHOLD_PROFILE,
        help="JSON threshold profile for pre-burn-in regression classification",
    )
    parser.add_argument(
        "--skip-threshold-check",
        action="store_true",
        help="capture measurements only without threshold classification",
    )
    return parser.parse_args()


def main() -> int:
    legacy.sanitize_slug = sanitize_slug
    legacy.write_markdown_report = write_markdown_report
    legacy.parse_args = parse_args
    return legacy.main()


if __name__ == "__main__":
    raise SystemExit(main())
