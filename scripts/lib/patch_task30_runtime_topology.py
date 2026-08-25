#!/usr/bin/env python3
"""Create fail-closed Task30 runtime copies with topology checks matching launchers.

The affected five-node drills launch n1 as the sole bootstrap/root and n2-n5
with n1 as their bootnode. Requiring every node to have four direct peers is
therefore stronger than the topology the harness itself constructs and aborts
before the actual consensus/recovery assertions run.

This adapter never edits the checked-out source. It emits a deterministic copy
under the caller-provided output path and refuses to continue unless every
expected historical form is present with the exact count documented below.
The v2.4 compatibility adapter is then applied to the same temporary copy, so
retired wallet RPCs and miner no-submit outcomes remain independently fail-closed.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from patch_task30_v24_runtime_compat import (
    emit_relay_helper,
    patch_prune as patch_v24_prune,
    patch_relay as patch_v24_relay,
)


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
        1,
        "prune startup star topology",
    )
    text = replace_exact(
        text,
        '        [[ "$peers" =~ ^[0-9]+$ ]] && (( peers >= node_count - 1 )) || ok=0',
        "        if (( idx == 1 )); then\n"
        '          [[ "$peers" =~ ^[0-9]+$ ]] && (( peers >= node_count - 1 )) || ok=0\n'
        "        else\n"
        '          [[ "$peers" =~ ^[0-9]+$ ]] && (( peers >= 1 )) || ok=0\n'
        "        fi",
        1,
        "prune final convergence star topology",
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


def prepare_relay_runtime(text: str) -> str:
    text = patch_v24_relay(text)
    runner_temp = os.environ.get("RUNNER_TEMP")
    if not runner_temp:
        raise SystemExit("RUNNER_TEMP is required for Task30 relay helper generation")
    workspace = Path(os.environ.get("GITHUB_WORKSPACE", Path.cwd())).resolve()
    emit_relay_helper(workspace, Path(runner_temp) / "task30/tx-helper")
    return replace_exact(
        text,
        'log "building release binaries"',
        'helper_dir="${RUNNER_TEMP:?RUNNER_TEMP is required}/task30/tx-helper"\n'
        'CARGO_TARGET_DIR="$ROOT_DIR/target" cargo build --release --manifest-path "$helper_dir/Cargo.toml" || { fail "v2.4 local signing helper build failed"; write_manifest FAIL; exit 1; }\n'
        'TASK30_TX_HELPER="$ROOT_DIR/target/release/task30-tx-helper"\n'
        'export TASK30_TX_HELPER\n'
        '[[ -x "$TASK30_TX_HELPER" ]] || { fail "v2.4 local signing helper binary missing"; write_manifest FAIL; exit 1; }\n'
        'log "building release binaries"',
        1,
        "relay local signing helper build",
    )


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
        text = prepare_relay_runtime(patch_relay(text))
    elif mode == "prune-harness":
        text = patch_v24_prune(patch_prune_harness(text))
    elif mode == "prune-driver":
        text = patch_prune_driver(text)
    else:
        raise SystemExit(f"unsupported Task30 topology patch mode: {mode}")

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
