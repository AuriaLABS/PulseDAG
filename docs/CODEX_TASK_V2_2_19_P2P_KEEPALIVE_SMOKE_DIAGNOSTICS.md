# Codex task: fix v2.2.19 local 3N/1M P2P keepalive and smoke diagnostics

## Context

The v2.2.19 local 3-node / 1-minute smoke evidence shows that the smoke run still fails before miners start.

Observed result from `artifacts/v2_2_19/local_3n_1m_smoke/evidence.tar.gz`:

```text
result: FAIL
exit_code: 1
healthy_nodes: 1
ready_nodes: 0
peers_total: 0
templates_seen: 0
submissions_seen: 0
accepted_blocks: 0
miner_not_started_reason: pre-mining p2p peer gate failed
final_heights: a=0, b=0, c=0
final_peer_counts: a=0, b=0, c=0
required_failures: 10
```

Important log pattern:

```text
pulsedagd RPC listening on 127.0.0.1:18081
pulsedagd RPC listening on 127.0.0.1:18082
Connection established ... total_peers=1
Connection closed with error KeepAliveTimeout ... total_peers=0
FAIL: pre-mining p2p peer gate failed after 120s (a=0, b=0, c=0)
```

This means B/C do start and expose RPC initially, but libp2p connections to A are established and then dropped quickly with `KeepAliveTimeout`. The smoke script then sees zero sustained peers and never starts miners.

## Goal

Make the local real-libp2p 3N/1M smoke sustain bootnode peer connections long enough for the pre-mining P2P gate to pass, and make smoke diagnostics fail clearly when RPC or P2P gates fail.

This is **not** a public-testnet launch. Keep `public_testnet_ready=false` for v2.2.19.

## Required code changes

### 1. P2P keepalive

Inspect `crates/pulsedag-p2p/src/lib.rs` around `PulseBehaviour` and `run_libp2p_real_runtime`.

Current behaviour contains only gossipsub:

```rust
#[derive(NetworkBehaviour)]
struct PulseBehaviour {
    gossipsub: gossipsub::Behaviour,
}
```

Add a small keepalive behaviour, preferably `libp2p::ping`, so idle bootnode connections do not close immediately before any block/tx/sync traffic exists.

Expected shape:

```rust
use libp2p::ping;

#[derive(NetworkBehaviour)]
struct PulseBehaviour {
    gossipsub: gossipsub::Behaviour,
    ping: ping::Behaviour,
}
```

Construct it in `run_libp2p_real_runtime`:

```rust
let ping = ping::Behaviour::new(
    ping::Config::new()
        .with_interval(Duration::from_secs(10))
        .with_timeout(Duration::from_secs(20)),
);
```

Then build behaviour with both protocols:

```rust
builder.with_behaviour(|_| PulseBehaviour { gossipsub: gossip, ping })
```

Handle ping events in the swarm event loop:

```rust
SwarmEvent::Behaviour(PulseBehaviourEvent::Ping(event)) => {
    note_swarm_event(&inner, format!("ping:{event:?}"));
}
```

Also consider increasing libp2p idle timeout if the builder API in this libp2p version supports it, but do not rely only on a long idle timeout. The preferred fix is to add an actual lightweight behaviour that keeps the connection useful.

Update `crates/pulsedag-p2p/Cargo.toml` if the `ping` feature is not already enabled in the `libp2p` feature list.

### 2. Smoke script diagnostics

Update `scripts/v2_2_19_local_3n_1m_smoke.sh`.

Add an RPC health gate after launching nodes A/B/C and before checking P2P peers:

```bash
wait_rpc_ready() {
  local name="$1"
  local port="$2"

  for _ in $(seq 1 60); do
    if curl -fsS --max-time 3 "http://127.0.0.1:${port}/health" >/dev/null; then
      log "PASS: node-${name} RPC health ready on ${port}"
      return 0
    fi
    sleep 2
  done

  record_fail "node-${name} RPC health did not become ready on ${port}"
  return 1
}
```

Call it for all nodes before the P2P gate:

```bash
wait_rpc_ready a 18080 || exit 1
wait_rpc_ready b 18081 || exit 1
wait_rpc_ready c 18082 || exit 1
```

If the script uses configurable ports, use the script's existing variables instead of hard-coded numbers.

### 3. Curl exit-code bug

Fix any helper that writes endpoint JSON after curl failures. Avoid this bad pattern:

```bash
if ! curl ...; then
  rc=$?
```

Because inside the `then` branch, `$?` can be the status of `!`, not the curl exit status. Use:

```bash
rc=0
curl -fsS --max-time 3 "$url" -o "$out" || rc=$?
if (( rc != 0 )); then
  jq -n \
    --arg url "$url" \
    --argjson exit_code "$rc" \
    '{ok:false,error:"curl failed",url:$url,exit_code:$exit_code}' > "$out"
  return 1
fi
```

### 4. Evidence packaging bug

The evidence package currently emits:

```text
tar: final-convergence-table.txt: Cannot stat: No such file or directory
```

Ensure optional evidence files do not make packaging fail. Either always create the file:

```bash
touch "$RUN_DIR/final-convergence-table.txt"
```

or build a tar file list from existing paths only. Prefer existing-path list over `--ignore-failed-read` if convenient.

### 5. Fix malformed grep/rg diagnostics

Evidence also shows:

```text
rg: node-a:": No such file or directory
```

Find the malformed `rg` invocation in the smoke script and replace it with a safe grep over existing logs, for example:

```bash
grep -Eih "node-a|node-b|node-c|peer|bootnode|dial|Connection|KeepAliveTimeout" "$RUN_DIR"/logs/*.log || true
```

## Tests to run

```bash
cargo fmt --all -- --check
cargo test -p pulsedag-p2p --all-targets --locked
cargo build --workspace --release --locked
bash -n scripts/v2_2_19_local_3n_1m_smoke.sh
```

Then rerun smoke locally:

```bash
pkill -f pulsedagd || true
pkill -f pulsedag-miner || true

rm -rf artifacts/v2_2_19/local_3n_1m_smoke

RUST_LOG=info,libp2p=debug,pulsedag_p2p=debug \
OUT_DIR="$PWD/artifacts/v2_2_19/local_3n_1m_smoke" \
DURATION_SECS=180 \
P2P_CONNECT_WAIT_SECS=180 \
SMOKE_TOTAL_DEADLINE_SECS=900 \
bash scripts/v2_2_19_local_3n_1m_smoke.sh
```

Expected result: no `KeepAliveTimeout` loop that leaves final peers at zero; smoke should either pass the P2P peer gate or fail with a clear RPC/P2P diagnostic and valid evidence archive.

## Acceptance criteria

- `cargo test -p pulsedag-p2p --all-targets --locked` passes.
- `bash -n scripts/v2_2_19_local_3n_1m_smoke.sh` passes.
- Local 3N/1M smoke produces `evidence.tar.gz` and `evidence.tar.gz.sha256` without tar errors.
- Endpoint JSON for failed curls records non-zero curl exit code.
- If nodes B/C expose RPC but P2P drops, evidence clearly shows `KeepAliveTimeout` / peer lifecycle cause.
- Final P2P gate sustains peers long enough for miners to start.
