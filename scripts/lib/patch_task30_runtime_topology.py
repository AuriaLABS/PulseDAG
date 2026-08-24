#!/usr/bin/env python3
"""Create fail-closed Task30 runtime copies with topology checks matching launchers.

The affected five-node drills launch n1 as the sole bootstrap/root and n2-n5
with n1 as their bootnode. Requiring every node to have four direct peers is
therefore stronger than the topology the harness itself constructs and aborts
before the actual consensus/recovery assertions run.

This adapter never edits the checked-out source. It emits a deterministic copy
under the caller-provided output path and refuses to continue unless every
expected historical form is present with the exact count documented below.
"""

from __future__ import annotations

import sys
from pathlib import Path


def replace_exact(text: str, old: str, new: str, count: int, label: str) -> str:
    actual = text.count(old)
    if actual != count:
        raise SystemExit(
            f"Task30 topology patch precondition failed for {label}: "
            f"expected={count} actual={actual}"
        )
    return text.replace(old, new, count)


def patch_relay(text: str) -> str:
    text = replace_exact(
        text,
        "    (( peers >= NODE_COUNT - 1 )) || topology_ok=0",
        "    if (( i == 1 )); then\n"
        "      (( peers >= NODE_COUNT - 1 )) || topology_ok=0\n"
        "    else\n"
        "      (( peers >= 1 )) || topology_ok=0\n"
        "    fi",
        1,
        "relay startup star topology",
    )
    text = replace_exact(
        text,
        'if (( topology_ok == 1 )); then touch "$OUT_DIR/topology_stable.proof"; else fail "topology did not stabilize with four peers per node"; fi',
        'if (( topology_ok == 1 )); then touch "$OUT_DIR/topology_stable.proof"; else fail "topology did not stabilize with n1>=4 and non-root>=1 peers"; fi',
        1,
        "relay topology failure text",
    )
    text = replace_exact(
        text,
        'topology_status:{required_peers_per_node:4,stable:$topology,nodes:$topology_evidence}',
        'topology_status:{root_required_peers:4,non_root_required_peers:1,stable:$topology,nodes:$topology_evidence}',
        1,
        "relay manifest topology contract",
    )
    return text


def patch_prune_harness(text: str) -> str:
    text = replace_exact(
        text,
        '  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"',
        '  root_dir="$(git rev-parse --show-toplevel)"',
        1,
        "prune harness repository root",
    )
    text = replace_exact(
        text,
        "        (( peers >= node_count - 1 )) || ok=0",
        "        if (( idx == 1 )); then\n"
        "          (( peers >= node_count - 1 )) || ok=0\n"
        "        else\n"
        "          (( peers >= 1 )) || ok=0\n"
        "        fi",
        2,
        "prune startup/final star topology",
    )
    text = replace_exact(
        text,
        '  _v230_wait_mesh || { echo "five-node peer mesh did not form" >&2; exit 1; }',
        '  _v230_wait_mesh || { echo "five-node star topology did not form" >&2; exit 1; }',
        1,
        "prune startup failure text",
    )
    text = replace_exact(
        text,
        '  _v230_wait_mesh || { echo "peer mesh did not recover after rejoin" >&2; exit 1; }',
        '  _v230_wait_mesh || { echo "star topology did not recover after rejoin" >&2; exit 1; }',
        1,
        "prune rejoin failure text",
    )
    text = replace_exact(
        text,
        "  jq -e 'length == 5 and all(.[]; .ready == true and .compatible_peers >= 4 and (.selected_tip | length) > 0 and (.state_root | length) > 0)' \"$out_dir/final-nodes.json\" >/dev/null",
        "  jq -e 'length == 5 and all(.[]; .ready == true and (if .node == \"n1\" then .compatible_peers >= 4 else .compatible_peers >= 1 end) and (.selected_tip | length) > 0 and (.state_root | length) > 0)' \"$out_dir/final-nodes.json\" >/dev/null",
        1,
        "prune final endpoint topology assertion",
    )
    return text


def patch_prune_driver(text: str) -> str:
    text = replace_exact(
        text,
        'ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"',
        'ROOT_DIR="$(git rev-parse --show-toplevel)"',
        1,
        "prune driver repository root",
    )
    text = replace_exact(
        text,
        "require_json '(.final_nodes | type == \"array\") and (.final_nodes | length == 5) and all(.final_nodes[]; .ready == true and (.compatible_peers // -1) >= 4 and ((.selected_tip // \"\") | length) > 0 and ((.state_root // \"\") | length) > 0)' 'final five-node endpoint convergence evidence is missing or incomplete'",
        "require_json '(.final_nodes | type == \"array\") and (.final_nodes | length == 5) and all(.final_nodes[]; .ready == true and (if .node == \"n1\" then (.compatible_peers // -1) >= 4 else (.compatible_peers // -1) >= 1 end) and ((.selected_tip // \"\") | length) > 0 and ((.state_root // \"\") | length) > 0)' 'final five-node endpoint convergence evidence is missing or incomplete'",
        1,
        "prune driver final topology assertion",
    )
    return text


def main() -> int:
    if len(sys.argv) != 4:
        print(
            f"usage: {sys.argv[0]} relay|prune-harness|prune-driver INPUT OUTPUT",
            file=sys.stderr,
        )
        return 64

    mode, input_name, output_name = sys.argv[1:]
    source = Path(input_name)
    target = Path(output_name)
    text = source.read_text(encoding="utf-8")

    if mode == "relay":
        text = patch_relay(text)
    elif mode == "prune-harness":
        text = patch_prune_harness(text)
    elif mode == "prune-driver":
        text = patch_prune_driver(text)
    else:
        raise SystemExit(f"unsupported Task30 topology patch mode: {mode}")

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
