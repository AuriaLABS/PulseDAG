#!/usr/bin/env python3
"""Fail-closed Task30 adaptations for the remaining exact-SHA runtime legs.

The candidate checkout is never edited.  This utility copies a frozen candidate
harness into runner temp, applies only the documented compatibility fixes, and
writes a unified diff plus SHA-256 hashes for audit evidence.
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
    original = source.read_text()
    output.parent.mkdir(parents=True, exist_ok=True)
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
    (output.parent / "SHA256SUMS").write_text(
        f"{hashlib.sha256(original.encode()).hexdigest()}  candidate_source\n"
        f"{hashlib.sha256(text.encode()).hexdigest()}  adapted_copy\n"
    )


def adapt_relay(root: Path, output: Path) -> None:
    source = root / "scripts/v2_3_0_mempool_tx_relay_evidence.sh"
    text = source.read_text()

    text = replace_once(
        text,
        'post_json(){ local url="$1" body="$2" out="$3"; curl -fsS --connect-timeout 2 --max-time 20 -H \'content-type: application/json\' -d "$body" "$url" | tee "$out" >/dev/null; }',
        '''post_json(){ local url="$1" body="$2" out="$3"; curl -fsS --connect-timeout 2 --max-time 20 -H 'content-type: application/json' -d "$body" "$url" | tee "$out" >/dev/null; }
task30_address(){ "$TASK30_TX_HELPER" address "$1"; }
task30_build_signed(){
  local node="$1" seed="$2" from="$3" to="$4" amount="$5" fee="$6" stem="$7"
  post_json "$(rpc_url "$node")/tx/build" "{\"from\":\"$from\",\"to\":\"$to\",\"amount\":$amount,\"fee\":$fee}" "$OUT_DIR/tx/${stem}-build.json"
  "$TASK30_TX_HELPER" sign "$seed" "$OUT_DIR/tx/${stem}-build.json" > "$OUT_DIR/tx/${stem}-body.json"
  jq -e '.transaction.txid and (.transaction.version == 1) and ((.transaction.inputs // []) | length > 0) and all(.transaction.inputs[]; ((.public_key // "") | length) == 64 and ((.signature // "") | length) == 128)' "$OUT_DIR/tx/${stem}-body.json" >/dev/null
}
task30_mine_until_block(){
  local node="$1" address="$2" stem="$3" attempt response
  for attempt in $(seq 1 20); do
    response="$OUT_DIR/tx/${stem}-attempt-${attempt}.json"
    post_json "$(rpc_url "$node")/mine" "{\"miner_address\":\"$address\",\"pow_max_tries\":1000000}" "$response" || true
    if jq -e '.ok == true and (((.data.block_hash // .data.hash // "") | length) > 0)' "$response" >/dev/null 2>&1; then
      cp "$response" "$OUT_DIR/tx/${stem}.json"
      return 0
    fi
    sleep 1
  done
  fail "mining did not find an accepted block for $stem after 20 bounded attempts"
  return 1
}''',
        "relay local signer helper insertion",
    )

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

    old_initial = '''post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding2-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding3-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient-wallet.json"
FROM="$(jq -r '.data.address' "$OUT_DIR/tx/funding-wallet.json")"; PRIV="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding-wallet.json")"; TO="$(jq -r '.data.address' "$OUT_DIR/tx/recipient-wallet.json")"
FROM2="$(jq -r '.data.address' "$OUT_DIR/tx/funding2-wallet.json")"; PRIV2="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding2-wallet.json")"
FROM3="$(jq -r '.data.address' "$OUT_DIR/tx/funding3-wallet.json")"; PRIV3="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding3-wallet.json")"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding-mine.json"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM2\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding2-mine.json"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM3\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding3-mine.json"
sleep 3
TRANSFER_BODY="{\"from\":\"$FROM\",\"to\":\"$TO\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV\"}"
post_json "$(rpc_url 1)/wallet/transfer" "$TRANSFER_BODY" "$OUT_DIR/tx/submit-n1.json"
TXID="$(jq -r '.data.txid // empty' "$OUT_DIR/tx/submit-n1.json")"; [[ -n "$TXID" ]] || { fail "no submitted txid"; write_manifest FAIL; exit 1; }
jq -n --arg txid "$TXID" '[$txid]' > "$OUT_DIR/submitted_txids.json"'''
    new_initial = '''FROM="$(task30_address 11)"
FROM2="$(task30_address 12)"
FROM3="$(task30_address 13)"
TO="$(task30_address 21)"
printf '%s\\n' "$FROM" > "$OUT_DIR/tx/funding-address.txt"
printf '%s\\n' "$FROM2" > "$OUT_DIR/tx/funding2-address.txt"
printf '%s\\n' "$FROM3" > "$OUT_DIR/tx/funding3-address.txt"
printf '%s\\n' "$TO" > "$OUT_DIR/tx/recipient-address.txt"
task30_mine_until_block 1 "$FROM" funding-mine || { write_manifest FAIL; exit 1; }
task30_mine_until_block 1 "$FROM2" funding2-mine || { write_manifest FAIL; exit 1; }
task30_mine_until_block 1 "$FROM3" funding3-mine || { write_manifest FAIL; exit 1; }
sleep 3
task30_build_signed 1 11 "$FROM" "$TO" 1 1 submit-n1
post_json "$(rpc_url 1)/tx/submit" "$(cat "$OUT_DIR/tx/submit-n1-body.json")" "$OUT_DIR/tx/submit-n1.json"
TXID="$(jq -r '.transaction.txid // empty' "$OUT_DIR/tx/submit-n1-body.json")"; [[ -n "$TXID" ]] || { fail "local signer produced no submitted txid"; write_manifest FAIL; exit 1; }
jq -e '.ok == true and .data.accepted == true' "$OUT_DIR/tx/submit-n1.json" >/dev/null || { fail "signed transaction was not accepted on n1"; write_manifest FAIL; exit 1; }
jq -n --arg txid "$TXID" '[$txid]' > "$OUT_DIR/submitted_txids.json"'''
    text = replace_once(text, old_initial, new_initial, "relay retired wallet RPC initial flow")

    text = replace_once(
        text,
        '''DUP_BODY="$(jq -ce '{transaction:.data.transaction} | select(.transaction != null)' "$OUT_DIR/tx/submit-n1.json")" || { fail "wallet transfer did not return a duplicate-submittable transaction"; write_manifest FAIL; exit 1; }''',
        '''DUP_BODY="$(cat "$OUT_DIR/tx/submit-n1-body.json")"''',
        "relay duplicate body source",
    )

    old_conflict = '''post_json "$(rpc_url 3)/wallet/new" '{}' "$OUT_DIR/tx/conflict-recipient-wallet.json"
CONFLICT_TO="$(jq -r '.data.address' "$OUT_DIR/tx/conflict-recipient-wallet.json")"
CONFLICT_BODY="{\"from\":\"$FROM\",\"to\":\"$CONFLICT_TO\",\"amount\":2,\"fee\":1,\"private_key\":\"$PRIV\"}"
capture_node before_conflict
post_json "$(rpc_url 3)/wallet/transfer" "$CONFLICT_BODY" "$OUT_DIR/tx/conflict-submit-n3.json" || true'''
    new_conflict = '''CONFLICT_TO="$(task30_address 22)"
printf '%s\\n' "$CONFLICT_TO" > "$OUT_DIR/tx/conflict-recipient-address.txt"
task30_build_signed 3 11 "$FROM" "$CONFLICT_TO" 2 1 conflict-submit-n3
capture_node before_conflict
post_json "$(rpc_url 3)/tx/submit" "$(cat "$OUT_DIR/tx/conflict-submit-n3-body.json")" "$OUT_DIR/tx/conflict-submit-n3.json" || true'''
    text = replace_once(text, old_conflict, new_conflict, "relay retired wallet RPC conflict flow")

    old_capacity = '''post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient2-wallet.json"; TO2="$(jq -r '.data.address' "$OUT_DIR/tx/recipient2-wallet.json")"
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM2\",\"to\":\"$TO2\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV2\"}" "$OUT_DIR/tx/capacity-fill.json" || true
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient3-wallet.json"; TO3="$(jq -r '.data.address' "$OUT_DIR/tx/recipient3-wallet.json")"
# A zero-fee candidate is strictly lower priority than the two resident fee-1
# transactions, so the bounded mempool must reject it rather than evicting one.
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM3\",\"to\":\"$TO3\",\"amount\":1,\"fee\":0,\"private_key\":\"$PRIV3\"}" "$OUT_DIR/tx/capacity-reject.json" || true'''
    new_capacity = '''TO2="$(task30_address 23)"
task30_build_signed 1 12 "$FROM2" "$TO2" 1 1 capacity-fill
post_json "$(rpc_url 1)/tx/submit" "$(cat "$OUT_DIR/tx/capacity-fill-body.json")" "$OUT_DIR/tx/capacity-fill.json" || true
TO3="$(task30_address 24)"
# A zero-fee candidate is strictly lower priority than the two resident fee-1
# transactions, so the bounded mempool must reject it rather than evicting one.
task30_build_signed 1 13 "$FROM3" "$TO3" 1 0 capacity-reject
post_json "$(rpc_url 1)/tx/submit" "$(cat "$OUT_DIR/tx/capacity-reject-body.json")" "$OUT_DIR/tx/capacity-reject.json" || true'''
    text = replace_once(text, old_capacity, new_capacity, "relay retired wallet RPC capacity flow")

    text = replace_once(
        text,
        'post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/confirm-mine.json"',
        'task30_mine_until_block 1 "$FROM" confirm-mine || { write_manifest FAIL; exit 1; }',
        "relay probabilistic confirmation mining",
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

    old_mining = '''    for ((attempt=1; attempt<=max_attempts; attempt++)); do
      printf '\\n[%s] miner attempt %d/%d\\n' "$(date -u +%FT%TZ)" "$attempt" "$max_attempts" >> "$miner_log"
      if "$miner_bin" --node "$(_v230_rpc_url 1)" --miner-address "v230-task04-$label" --max-tries 1000000 >> "$miner_log" 2>&1; then
        mined=1
        break
      fi
      if v2_3_0_prune_miner_retryable_log "$miner_log"; then
        _v230_prune_log "mining submit rate limited for $label attempt=$attempt/$max_attempts; retrying after ${retry_wait}s"
        sleep "$retry_wait"
        continue
      fi
      tail -80 "$miner_log" >&2 || true
      return 1
    done
    (( mined == 1 )) || {
      echo "external miner remained rate limited for $label after $max_attempts attempts" >&2
      tail -80 "$miner_log" >&2 || true
      return 1
    }'''
    new_mining = '''    for ((attempt=1; attempt<=max_attempts; attempt++)); do
      local attempt_log="$miner_dir/$label-attempt-$attempt.log"
      printf '\\n[%s] miner attempt %d/%d\\n' "$(date -u +%FT%TZ)" "$attempt" "$max_attempts" >> "$miner_log"
      if "$miner_bin" --node "$(_v230_rpc_url 1)" --miner-address "v230-task04-$label" --max-tries 1000000 > "$attempt_log" 2>&1; then
        cat "$attempt_log" >> "$miner_log"
        if grep -q 'submit_result: accepted=true' "$attempt_log"; then
          mined=1
          break
        fi
        if grep -Eq 'backend_verification_failed:.*hash_above_target|backend_verification_failed.*hash_above_target' "$attempt_log"; then
          _v230_prune_log "no PoW solution within 1000000 tries for $label attempt=$attempt/$max_attempts; retrying after ${retry_wait}s"
          sleep "$retry_wait"
          continue
        fi
        echo "external miner exited successfully without an accepted submit or explicit no-PoW result for $label" >&2
        tail -80 "$attempt_log" >&2 || true
        return 1
      fi
      cat "$attempt_log" >> "$miner_log"
      if v2_3_0_prune_miner_retryable_log "$attempt_log"; then
        _v230_prune_log "mining submit rate limited for $label attempt=$attempt/$max_attempts; retrying after ${retry_wait}s"
        sleep "$retry_wait"
        continue
      fi
      tail -80 "$attempt_log" >&2 || true
      return 1
    done
    (( mined == 1 )) || {
      echo "external miner failed to produce an accepted block for $label after $max_attempts bounded attempts" >&2
      tail -80 "$miner_log" >&2 || true
      return 1
    }'''
    text = replace_once(text, old_mining, new_mining, "prune no-PoW bounded retry")
    write_outputs(source, output, text)


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: adapt_harness_v2.py MODE CANDIDATE_ROOT OUTPUT", file=sys.stderr)
        return 64
    mode, root_raw, output_raw = sys.argv[1:]
    root = Path(root_raw).resolve()
    output = Path(output_raw).resolve()
    if mode == "relay":
        adapt_relay(root, output)
    elif mode == "prune":
        adapt_prune(root, output)
    else:
        raise SystemExit(f"unknown mode: {mode}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
