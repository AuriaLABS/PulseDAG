#!/usr/bin/env python3
"""Fail-closed Task30 control-plane harness adaptations.

This file is never part of the candidate under test. It copies a harness from the
frozen candidate into a temporary directory, applies one narrowly-scoped runtime
adaptation, validates syntax, and writes a unified diff plus hashes.
"""
from __future__ import annotations

import difflib
import hashlib
import sys
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one patch target, found {count}")
    return text.replace(old, new, 1)


def write_outputs(source: Path, output: Path, text: str) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    original = source.read_text()
    output.write_text(text)
    diff = "".join(
        difflib.unified_diff(
            original.splitlines(True),
            text.splitlines(True),
            fromfile=str(source),
            tofile=str(output),
        )
    )
    (output.parent / "harness.diff").write_text(diff)
    rows = []
    for label, data in (("candidate_source", original.encode()), ("adapted_copy", text.encode())):
        rows.append(f"{hashlib.sha256(data).hexdigest()}  {label}\n")
    (output.parent / "SHA256SUMS").write_text("".join(rows))


def adapt_multi(root: Path, output: Path) -> None:
    source = root / "scripts/v2_2_20_private_5n_4m_rehearsal.sh"
    text = source.read_text()
    text = replace_once(
        text,
        'ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"',
        'ROOT_DIR="${TASK30_CANDIDATE_ROOT:?TASK30_CANDIDATE_ROOT is required}"',
        "multi candidate root",
    )
    text = replace_once(
        text,
        'OUT_DIR="$OUT_DIR" run_with_global_timeout preflight "$ROOT_DIR/scripts/v2_2_20_preflight_check.sh"',
        'echo "Task30 control-plane adaptation: skip historical v2.2.20 release-surface preflight only; exact v2.4.0 candidate SHA/version is verified separately"\nprintf "%s\\n" "candidate_sha=$REPO_COMMIT_FULL" "historical_v2_2_20_preflight_skipped=true" > "$OUT_DIR/task30-harness-adaptation.txt"',
        "obsolete v2.2.20 preflight",
    )
    write_outputs(source, output, text)


def adapt_lag(root: Path, output: Path) -> None:
    source = root / "scripts/lib/v2_3_0_runtime_harness.sh"
    text = source.read_text()
    text = replace_once(
        text,
        '  local peer_id boot idx',
        '  local peer_id boot boot_list node_boot idx',
        "lag startup locals",
    )
    text = replace_once(
        text,
        '  for idx in 2 3 4 5; do _v230_lag_start_node "$idx" "$boot"; done',
        '''  boot_list="$boot"
  for idx in 2 3 4 5; do
    _v230_lag_start_node "$idx" "$boot_list"
    if ! pulsedag_wait_http_ok "$(_v230_lag_rpc_url "$idx")/p2p/status" "$out_dir/endpoints/n${idx}-p2p-bootstrap.json" "$startup_timeout"; then
      _v230_lag_abort "n$idx p2p bootstrap status unavailable during explicit K5 wiring"; return 1
    fi
    peer_id="$(jq -r '.data.peer_id // .data.local_peer_id // .data.local_node_id // empty' "$out_dir/endpoints/n${idx}-p2p-bootstrap.json")"
    [[ -n "$peer_id" ]] || { _v230_lag_abort "unable to extract n$idx peer id during explicit K5 wiring"; return 1; }
    node_boot="/ip4/127.0.0.1/tcp/$((base_p2p_port + idx))/p2p/$peer_id"
    boot_list="${boot_list},${node_boot}"
  done
  printf '%s\\n' "$boot_list" > "$out_dir/bootnodes-explicit-k5.txt"''',
        "lag star-to-K5 startup",
    )
    write_outputs(source, output, text)


def adapt_relay(root: Path, output: Path) -> None:
    source = root / "scripts/v2_3_0_mempool_tx_relay_evidence.sh"
    text = source.read_text()
    text = replace_once(
        text,
        'for i in 2 3 4 5; do start_node "$i" "$BOOT"; done',
        '''BOOT_LIST="$BOOT"
for i in 2 3 4 5; do
  start_node "$i" "$BOOT_LIST"
  pulsedag_wait_http_ok "$(rpc_url "$i")/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-bootstrap.json" "$STARTUP_TIMEOUT" || { fail "n$i p2p bootstrap status unavailable during explicit K5 wiring"; write_manifest FAIL; exit 1; }
  PEER_I="$(jq -r '.data.peer_id // .data.local_peer_id // empty' "$OUT_DIR/endpoints/n${i}-p2p-bootstrap.json")"
  [[ -n "$PEER_I" ]] || { fail "unable to extract n$i peer id during explicit K5 wiring"; write_manifest FAIL; exit 1; }
  BOOT_I="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+i))/p2p/$PEER_I"
  BOOT_LIST="${BOOT_LIST},${BOOT_I}"
done
printf '%s\\n' "$BOOT_LIST" > "$OUT_DIR/bootnodes-explicit-k5.txt"''',
        "relay star-to-K5 startup",
    )
    write_outputs(source, output, text)


def adapt_prune(root: Path, output: Path) -> None:
    source = root / "scripts/lib/v2_3_0_prune_restart_rejoin_harness.sh"
    text = source.read_text()
    text = replace_once(
        text,
        '  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"',
        '  root_dir="${TASK30_CANDIDATE_ROOT:?TASK30_CANDIDATE_ROOT is required}"',
        "prune candidate root",
    )
    text = replace_once(
        text,
        '  local -a node_pids=()',
        '  local -a node_pids=() bootnodes=()',
        "prune bootnode array",
    )
    text = replace_once(
        text,
        '    (( idx > 1 )) && args+=(--bootnode "$bootnode_1")',
        '''    local bootstrap="" j
    for ((j=1; j<=node_count; j++)); do
      (( j == idx )) && continue
      [[ -n "${bootnodes[$j]:-}" ]] || continue
      bootstrap="${bootstrap:+${bootstrap},}${bootnodes[$j]}"
    done
    [[ -n "$bootstrap" ]] && args+=(--bootnode "$bootstrap")''',
        "prune per-node bootstrap list",
    )
    text = replace_once(
        text,
        '''  bootnode_1="/ip4/127.0.0.1/tcp/$(_v230_p2p_port 1)/p2p/$peer_id"
  echo "$bootnode_1" > "$out_dir/bootnode.txt"
  for ((idx=2; idx<=node_count; idx++)); do
    _v230_start_node "$idx"
    _v230_wait_endpoint "$idx" /health
    _v230_wait_endpoint "$idx" /p2p/status
  done''',
        '''  bootnode_1="/ip4/127.0.0.1/tcp/$(_v230_p2p_port 1)/p2p/$peer_id"
  bootnodes[1]="$bootnode_1"
  echo "$bootnode_1" > "$out_dir/bootnode.txt"
  for ((idx=2; idx<=node_count; idx++)); do
    _v230_start_node "$idx"
    _v230_wait_endpoint "$idx" /health
    _v230_wait_endpoint "$idx" /p2p/status
    peer_id="$(_v230_http_get "$(_v230_rpc_url "$idx")/p2p/status" | jq -er '.data.peer_id // .data.local_peer_id')"
    bootnodes[$idx]="/ip4/127.0.0.1/tcp/$(_v230_p2p_port "$idx")/p2p/$peer_id"
  done
  printf '%s\\n' "${bootnodes[@]}" > "$out_dir/bootnodes-explicit-k5.txt"''',
        "prune star-to-K5 startup",
    )
    write_outputs(source, output, text)


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: adapt_harness.py MODE CANDIDATE_ROOT OUTPUT", file=sys.stderr)
        return 64
    mode, root_raw, output_raw = sys.argv[1:]
    root = Path(root_raw).resolve()
    output = Path(output_raw).resolve()
    if mode == "multi":
        adapt_multi(root, output)
    elif mode == "lag":
        adapt_lag(root, output)
    elif mode == "relay":
        adapt_relay(root, output)
    elif mode == "prune":
        adapt_prune(root, output)
    else:
        raise SystemExit(f"unknown mode: {mode}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
