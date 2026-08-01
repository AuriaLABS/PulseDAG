#!/usr/bin/env python3
"""One-shot migration for legacy PoW fixtures on release/2.4.0."""

from __future__ import annotations

import subprocess


STABLE_WORKFLOW_COMMIT = "164559b913ee2e9e95df15b91475911d248a5060"
STABLE_WORKFLOW_PATH = ".github/workflows/one_shot_v2_4_fixture_fix_stable.yml"


def main() -> None:
    source = subprocess.check_output(
        ["git", "show", f"{STABLE_WORKFLOW_COMMIT}:{STABLE_WORKFLOW_PATH}"],
        text=True,
    )
    marker = "          python3 - <<'PY'\n"
    start = source.index(marker) + len(marker)
    end = source.index("\n          PY\n", start)
    script = "\n".join(
        line[10:] if line.startswith("          ") else line
        for line in source[start:end].splitlines()
    )

    original_replace_once_tail = '''    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1))
'''
    robust_replace_once_tail = '''    if count == 0 and "refresh_block_consensus_ids" in old:
        updated = text.replace(
            "refresh_block_consensus_ids,\\n",
            "refresh_block_consensus_ids as raw_refresh_block_consensus_ids,\\n",
            1,
        ).replace(
            "refresh_block_consensus_ids_with_state,\\n",
            "refresh_block_consensus_ids_with_state as raw_refresh_block_consensus_ids_with_state,\\n",
            1,
        )
        if updated != text:
            file.write_text(updated)
            return
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1))
'''
    if original_replace_once_tail not in script:
        raise SystemExit("replace_once body not found in stable migration")
    script = script.replace(
        original_replace_once_tail,
        robust_replace_once_tail,
        1,
    )
    exec(compile(script, "stable_fixture_migration.py", "exec"), {})


if __name__ == "__main__":
    main()
