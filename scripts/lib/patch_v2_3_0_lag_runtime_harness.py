#!/usr/bin/env python3
"""Normalize known Bash defects in the v2.3.0 lag runtime harness.

The candidate driver may point at the repository harness or an externally supplied
copy. This patcher is deliberately fail-closed: every expected unsafe or fixed
form must be present exactly once before the normalized harness is emitted.
"""

from __future__ import annotations

import sys
from pathlib import Path


def replace_once_or_confirm(text: str, unsafe: str, fixed: str, label: str) -> str:
    unsafe_count = text.count(unsafe)
    fixed_count = text.count(fixed)
    if unsafe_count == 1 and fixed_count == 0:
        return text.replace(unsafe, fixed, 1)
    if unsafe_count == 0 and fixed_count == 1:
        return text
    raise SystemExit(
        f"runtime harness patch precondition failed for {label}: "
        f"unsafe={unsafe_count} fixed={fixed_count}"
    )


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} INPUT OUTPUT", file=sys.stderr)
        return 64

    source_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    text = source_path.read_text()

    text = replace_once_or_confirm(
        text,
        '    local idx="$1" boot="$2" data="$out_dir/data/n$idx"',
        '    local idx="$1"\n'
        '    local boot="$2"\n'
        '    local data="$out_dir/data/n$idx"',
        "node local declarations",
    )
    text = replace_once_or_confirm(
        text,
        '    local event="$1" node="${2:-}" details="${3:-{}}"',
        '    local event="$1"\n'
        '    local node="${2:-}"\n'
        '    local details="${3:-}"\n'
        "    [[ -n \"$details\" ]] || details='{}'",
        "event JSON default",
    )
    text = replace_once_or_confirm(
        text,
        "  trap '_v230_lag_unexpected_exit $?' EXIT",
        "  trap '_v230_lag_unexpected_exit $?' ERR",
        "unexpected-exit trap",
    )
    text = replace_once_or_confirm(
        text,
        "\n    trap - EXIT INT TERM\n",
        "\n    trap - ERR INT TERM\n",
        "abort trap cleanup",
    )
    text = replace_once_or_confirm(
        text,
        "\n  trap - EXIT INT TERM\n",
        "\n  trap - ERR INT TERM\n",
        "success trap cleanup",
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
