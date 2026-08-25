#!/usr/bin/env python3
"""Fail-closed v2.4 runtime compatibility adapters for Task30.

This file only transforms temporary runtime copies. It does not edit the
historical harnesses or any Rust source in the checked-out candidate.

Relay replaces retired v2.3 wallet RPC construction with deterministic local
Ed25519 signing against the exact candidate's canonical v1 primitives while
keeping /tx/build and /tx/submit as the node contract.

Prune counts a mining stage only after the standalone miner explicitly reports
an accepted submit. Explicit no-PoW, rate-limit, and stale/near-expiry outcomes
are retried with the existing bounded attempt budget; every other outcome fails
closed.
"""

from __future__ import annotations

import sys
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"Task30 v2.4 compatibility precondition failed for {label}: "
            f"expected=1 actual={count}"
        )
    return text.replace(old, new, 1)


def patch_relay(text: str) -> str:
    old_post = "post_json(){ local url=\"$1\" body=\"$2\" out=\"$3\"; curl -fsS --connect-timeout 2 --max-time 20 -H 'content-type: application/json' -d \"$body\" \"$url\" | tee \"$out\" >/dev/null; }"
    new_post = r'''post_json(){ local url="$1" body="$2" out="$3"; curl -fsS --connect-timeout 2 --max-time 20 -H 'content-type: application/json' -d "$body" "$url" | tee "$out" >/dev/null; }
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
}'''
    text = replace_once(text, old_post, new_post, "relay helper insertion")
    old_initial = r'''post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding-wallet.json"
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
    new_initial = r'''FROM="$(task30_address 11)"
FROM2="$(task30_address 12)"
FROM3="$(task30_address 13)"
TO="$(task30_address 21)"
printf '%s\n' "$FROM" > "$OUT_DIR/tx/funding-address.txt"
printf '%s\n' "$FROM2" > "$OUT_DIR/tx/funding2-address.txt"
printf '%s\n' "$FROM3" > "$OUT_DIR/tx/funding3-address.txt"
printf '%s\n' "$TO" > "$OUT_DIR/tx/recipient-address.txt"
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
    old_dup = r'''DUP_BODY="$(jq -ce '{transaction:.data.transaction} | select(.transaction != null)' "$OUT_DIR/tx/submit-n1.json")" || { fail "wallet transfer did not return a duplicate-submittable transaction"; write_manifest FAIL; exit 1; }'''
    text = replace_once(text, old_dup, 'DUP_BODY="$(cat "$OUT_DIR/tx/submit-n1-body.json")"', "relay duplicate body source")
    old_conflict = r'''post_json "$(rpc_url 3)/wallet/new" '{}' "$OUT_DIR/tx/conflict-recipient-wallet.json"
CONFLICT_TO="$(jq -r '.data.address' "$OUT_DIR/tx/conflict-recipient-wallet.json")"
CONFLICT_BODY="{\"from\":\"$FROM\",\"to\":\"$CONFLICT_TO\",\"amount\":2,\"fee\":1,\"private_key\":\"$PRIV\"}"
capture_node before_conflict
post_json "$(rpc_url 3)/wallet/transfer" "$CONFLICT_BODY" "$OUT_DIR/tx/conflict-submit-n3.json" || true'''
    new_conflict = r'''CONFLICT_TO="$(task30_address 22)"
printf '%s\n' "$CONFLICT_TO" > "$OUT_DIR/tx/conflict-recipient-address.txt"
task30_build_signed 3 11 "$FROM" "$CONFLICT_TO" 2 1 conflict-submit-n3
capture_node before_conflict
post_json "$(rpc_url 3)/tx/submit" "$(cat "$OUT_DIR/tx/conflict-submit-n3-body.json")" "$OUT_DIR/tx/conflict-submit-n3.json" || true'''
    text = replace_once(text, old_conflict, new_conflict, "relay retired wallet RPC conflict flow")
    old_capacity = r'''post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient2-wallet.json"; TO2="$(jq -r '.data.address' "$OUT_DIR/tx/recipient2-wallet.json")"
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM2\",\"to\":\"$TO2\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV2\"}" "$OUT_DIR/tx/capacity-fill.json" || true
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient3-wallet.json"; TO3="$(jq -r '.data.address' "$OUT_DIR/tx/recipient3-wallet.json")"
# A zero-fee candidate is strictly lower priority than the two resident fee-1
# transactions, so the bounded mempool must reject it rather than evicting one.
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM3\",\"to\":\"$TO3\",\"amount\":1,\"fee\":0,\"private_key\":\"$PRIV3\"}" "$OUT_DIR/tx/capacity-reject.json" || true'''
    new_capacity = r'''TO2="$(task30_address 23)"
task30_build_signed 1 12 "$FROM2" "$TO2" 1 1 capacity-fill
post_json "$(rpc_url 1)/tx/submit" "$(cat "$OUT_DIR/tx/capacity-fill-body.json")" "$OUT_DIR/tx/capacity-fill.json" || true
TO3="$(task30_address 24)"
# A zero-fee candidate is strictly lower priority than the two resident fee-1
# transactions, so the bounded mempool must reject it rather than evicting one.
task30_build_signed 1 13 "$FROM3" "$TO3" 1 0 capacity-reject
post_json "$(rpc_url 1)/tx/submit" "$(cat "$OUT_DIR/tx/capacity-reject-body.json")" "$OUT_DIR/tx/capacity-reject.json" || true'''
    text = replace_once(text, old_capacity, new_capacity, "relay retired wallet RPC capacity flow")
    old_confirm = r'''post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/confirm-mine.json"'''
    text = replace_once(text, old_confirm, 'task30_mine_until_block 1 "$FROM" confirm-mine || { write_manifest FAIL; exit 1; }', "relay bounded confirmation mining")
    return text


def patch_prune(text: str) -> str:
    old_mining = r'''    for ((attempt=1; attempt<=max_attempts; attempt++)); do
      printf '\n[%s] miner attempt %d/%d\n' "$(date -u +%FT%TZ)" "$attempt" "$max_attempts" >> "$miner_log"
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
    new_mining = r'''    for ((attempt=1; attempt<=max_attempts; attempt++)); do
      local attempt_log="$miner_dir/$label-attempt-$attempt.log"
      printf '\n[%s] miner attempt %d/%d\n' "$(date -u +%FT%TZ)" "$attempt" "$max_attempts" >> "$miner_log"
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
        if grep -Eq 'template_skipped_stale|skip_reason[=:][[:space:]]*(expired|near_expiry)|skip_reason.*(expired|near_expiry)' "$attempt_log"; then
          _v230_prune_log "miner skipped stale/near-expiry template for $label attempt=$attempt/$max_attempts; retrying after ${retry_wait}s"
          sleep "$retry_wait"
          continue
        fi
        echo "external miner exited successfully without an accepted submit or explicit retryable outcome for $label" >&2
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
    return replace_once(text, old_mining, new_mining, "prune accepted-submit bounded retry")


def emit_relay_helper(root: Path, target: Path) -> None:
    core = (root / "crates/pulsedag-core").resolve()
    if not (core / "Cargo.toml").is_file():
        raise SystemExit(f"Task30 relay helper missing candidate core manifest: {core}/Cargo.toml")
    target.mkdir(parents=True, exist_ok=True)
    (target / "src").mkdir(parents=True, exist_ok=True)
    cargo = f'''[package]
name = "task30-tx-helper"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
pulsedag-core = {{ path = "{core.as_posix()}" }}
ed25519-dalek = "=2.2.0"
hex = "=0.4.3"
serde_json = "=1.0.149"
wasm-bindgen = "=0.2.118"
'''
    main_rs = r'''use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{address_from_public_key, compute_txid, signing_message, Transaction};
use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("usage: task30-tx-helper address SEED_U8 | sign SEED_U8 BUILD_RESPONSE_JSON");
    process::exit(64);
}
fn key(seed: &str) -> SigningKey {
    let seed: u8 = seed.parse().unwrap_or_else(|_| {
        eprintln!("SEED_U8 must be an integer in 0..=255");
        process::exit(64);
    });
    SigningKey::from_bytes(&[seed; 32])
}
fn public_key_hex(key: &SigningKey) -> String { hex::encode(key.verifying_key().to_bytes()) }
fn main() {
    let args = env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, cmd, seed] if cmd == "address" => {
            let key = key(seed);
            println!("{}", address_from_public_key(&public_key_hex(&key)));
        }
        [_, cmd, seed, build_path] if cmd == "sign" => {
            let key = key(seed);
            let public_key = public_key_hex(&key);
            let raw = fs::read_to_string(build_path).unwrap_or_else(|error| {
                eprintln!("failed reading build response {build_path}: {error}");
                process::exit(65);
            });
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|error| {
                eprintln!("invalid build response JSON: {error}");
                process::exit(65);
            });
            if value.get("ok").and_then(|v| v.as_bool()) != Some(true) {
                eprintln!("tx/build did not return ok=true: {value}");
                process::exit(66);
            }
            let tx_value = value.get("data").and_then(|v| v.get("transaction")).cloned().unwrap_or_else(|| {
                eprintln!("tx/build response omitted data.transaction: {value}");
                process::exit(66);
            });
            let mut tx: Transaction = serde_json::from_value(tx_value).unwrap_or_else(|error| {
                eprintln!("invalid transaction in tx/build response: {error}");
                process::exit(66);
            });
            if tx.version != 1 {
                eprintln!("Task30 legacy runtime signer expected transaction version 1, got {}", tx.version);
                process::exit(67);
            }
            if tx.inputs.is_empty() {
                eprintln!("tx/build returned transaction without inputs");
                process::exit(67);
            }
            for input in &mut tx.inputs {
                input.public_key = public_key.clone();
                input.signature.clear();
            }
            let message = signing_message(&tx);
            let signature = hex::encode(key.sign(&message).to_bytes());
            for input in &mut tx.inputs { input.signature = signature.clone(); }
            tx.txid = compute_txid(&tx);
            println!("{}", serde_json::json!({"transaction": tx}));
        }
        _ => usage(),
    }
}
'''
    (target / "Cargo.toml").write_text(cargo, encoding="utf-8")
    (target / "src/main.rs").write_text(main_rs, encoding="utf-8")


def main() -> int:
    if len(sys.argv) != 4:
        print(f"usage: {sys.argv[0]} relay|prune|relay-helper INPUT OUTPUT", file=sys.stderr)
        return 64
    mode, input_name, output_name = sys.argv[1:]
    source = Path(input_name)
    target = Path(output_name)
    if mode == "relay-helper":
        emit_relay_helper(source.resolve(), target.resolve())
        return 0
    text = source.read_text(encoding="utf-8")
    if mode == "relay":
        text = patch_relay(text)
    elif mode == "prune":
        text = patch_prune(text)
    else:
        raise SystemExit(f"unsupported Task30 v2.4 compatibility mode: {mode}")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
