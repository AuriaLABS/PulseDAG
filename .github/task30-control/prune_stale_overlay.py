#!/usr/bin/env python3
"""Fail-closed Task30 overlay for explicit stale-template miner skips.

Applied only to a temporary adapted prune harness outside the candidate checkout.
It does not turn a stale template into success: it permits another bounded mining
attempt. The surrounding harness must still observe an accepted submit before
its mining stage can pass.
"""
from __future__ import annotations

import difflib
import hashlib
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: prune_stale_overlay.py ADAPTED_HARNESS", file=sys.stderr)
        return 64

    path = Path(sys.argv[1])
    before = path.read_text()
    needle = '''        echo "external miner exited successfully without an accepted submit or explicit no-PoW result for $label" >&2
'''
    replacement = '''        if grep -Eq 'template_skipped_stale|skip_reason[=:][[:space:]]*expired|skip_reason.*expired' "$attempt_log"; then
          _v230_prune_log "miner skipped stale/expired template for $label attempt=$attempt/$max_attempts; retrying after ${retry_wait}s"
          sleep "$retry_wait"
          continue
        fi
        echo "external miner exited successfully without an accepted submit or explicit no-PoW result for $label" >&2
'''
    count = before.count(needle)
    if count != 1:
        raise SystemExit(
            f"prune stale-template retry: expected exactly one patch target, found {count}"
        )

    after = before.replace(needle, replacement, 1)
    path.write_text(after)

    diff = "".join(
        difflib.unified_diff(
            before.splitlines(True),
            after.splitlines(True),
            fromfile=f"{path}.before-stale-overlay",
            tofile=str(path),
        )
    )
    (path.parent / "stale-template-overlay.diff").write_text(diff)
    (path.parent / "STALE_OVERLAY_SHA256SUMS").write_text(
        f"{hashlib.sha256(before.encode()).hexdigest()}  adapted_before_stale_overlay\n"
        f"{hashlib.sha256(after.encode()).hexdigest()}  adapted_after_stale_overlay\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
