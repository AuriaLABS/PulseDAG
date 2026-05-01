# Local Multi-Node Lab (v2.2.8)

This document defines a **local/manual** two-node lab to validate the minimum private-testnet-adjacent path in v2.2.8:

- node startup,
- P2P connection,
- mining template request,
- block submit,
- and basic propagation/sync signal.

> Status: **pre-private-testnet hardening**. This is not a full burn-in plan. Official private-testnet burn-in remains deferred to **v2.3.0**.

## Scope and boundaries

This lab intentionally avoids over-automation and assumes some steps may be manual depending on your local setup.

- No smart contracts.
- No pool miner.
- Miner stays external.
- No claim of full private-testnet readiness.

## Prerequisites

- Rust toolchain installed (`cargo`, `rustc`).
- Workspace builds locally.
- Two terminal sessions for two nodes (plus optional third for miner/RPC actions).
- Local RPC and P2P ports available.

## 1) Build workspace

From repo root:

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace
```

Optional (if currently clean in your environment):

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## 2) Start node A

Start `pulsedagd` for node A with local/dev config (manual, because node flags/config can vary by environment).

Expected outcomes:

- Process starts and keeps running.
- Logs confirm node startup and bound RPC/P2P listeners.

Record:

- node A RPC address (example: `127.0.0.1:<rpc_port_a>`)
- node A P2P listen address (example: `/ip4/127.0.0.1/tcp/<p2p_port_a>`)

## 3) Start node B

Start a second `pulsedagd` instance with a separate data dir and non-conflicting RPC/P2P ports.

Expected outcomes:

- Process starts cleanly.
- No lock/port collision with node A.

Record:

- node B RPC address
- node B P2P listen address

## 4) Connect node B to node A

Use the current repo-supported peer configuration or peer-add flow (manual).

Typical options (depending on existing support):

- start node B with a bootstrap/seed/peer argument that points to node A, or
- call an admin/debug peer-add RPC if available.

Expected outcomes:

- Node B attempts dial to node A.
- Logs show successful handshake or established session.

## 5) Verify P2P connection

Use whichever signals currently exist (logs, metrics, peer-list RPC).

Examples of acceptable evidence:

- node A sees node B as connected,
- node B sees node A as connected,
- connection counters/peer list increment as expected.

If only logs are available in v2.2.8, mark validation as log-based/manual.

## 6) Request mining template from node A

Using existing RPC tooling (`curl`, client, or miner-facing wrapper), request a mining template from node A.

Expected outcomes:

- RPC call succeeds.
- Response contains a candidate/template payload usable by the external miner or local PoW utility.

## 7) Mine (or solve via test utility if available)

Use one of:

- external miner path, or
- existing local PoW test utility (if present in this checkout).

Expected outcomes:

- A candidate receives a valid nonce/hash pair.
- Invalid nonce/hash pairs are rejected.

## 8) Submit block to node A

Submit the solved block/candidate using the supported submit RPC.

Expected outcomes:

- Valid solved block accepted.
- Invalid/stale/duplicate paths rejected with diagnostic reason where available.

## 9) Verify acceptance on node A

Confirm with current local evidence sources:

- acceptance log entries,
- chain/DAG height or tip movement,
- submit RPC success state.

At minimum, verify a single valid submission transitions to accepted state.

## 10) Verify node B propagation/sync signal

If v2.2.8 supports the necessary sync observability in your setup, verify node B receives the accepted block (or syncs to equivalent height/tip).

Expected outcomes:

- node B tip/height converges to node A within normal local delay, or
- logs indicate receipt/import/sync of the new block.

If this is not observable in your local config, mark as **known limitation** and keep as manual follow-up.

## Known limitations (v2.2.8)

- Local topology/bootstrap behavior may require hand-tuned flags/config per environment.
- Peer-state visibility may be log-based rather than fully RPC-exposed.
- Mining solve flow depends on external miner or local utility availability.
- This lab is **not** a substitute for full private-testnet burn-in.

## Exit criteria for this lab

For a pass in v2.2.8 local validation:

- Node A and node B start successfully.
- B establishes a P2P connection to A.
- Mining template retrieval from A succeeds.
- One solved block submission to A is accepted.
- Basic propagation/sync to B is observed **or explicitly documented as not yet observable**.

## Deferred work

- Official private-testnet burn-in, reliability gates, and operator-grade multi-node hardening are deferred to **v2.3.0**.
