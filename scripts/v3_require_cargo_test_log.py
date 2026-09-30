#!/usr/bin/env python3
"""Fail unless a cargo test log actually executed at least one test."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

RUNNING_RE = re.compile(r"^running (\d+) tests?$", re.M)
RESULT_RE = re.compile(
    r"^test result:\s+\w+\.\s+(\d+)\s+passed;\s+(\d+)\s+failed;",
    re.M,
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("log")
    parser.add_argument("--filter", required=True)
    args = parser.parse_args()

    text = Path(args.log).read_text(encoding="utf-8", errors="replace")
    running = [int(n) for n in RUNNING_RE.findall(text)]
    results = RESULT_RE.findall(text)
    passed = sum(int(p) for p, _failed in results)
    failed = sum(int(f) for _p, f in results)

    if failed:
        print(
            f"required cargo filter failed tests: filter={args.filter!r} failed={failed}",
            file=sys.stderr,
        )
        return 1
    if not running or max(running) < 1 or passed < 1:
        print(
            "required cargo filter matched zero tests: "
            f"filter={args.filter!r} running={running!r} passed={passed}",
            file=sys.stderr,
        )
        return 1
    print(
        f"required cargo filter executed tests: filter={args.filter!r} "
        f"running_max={max(running)} passed={passed}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
