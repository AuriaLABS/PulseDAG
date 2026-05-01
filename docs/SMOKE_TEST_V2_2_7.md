# PulseDAG v2.2.7 Smoke Test (Manual/Partial)

This smoke test validates the minimum PoW/mining foundation path in v2.2.7. It is intentionally **manual/partial** and does **not** replace the full multi-node private-testnet validation targeted for v2.3.0.

## Scope

This document checks that the release can be built and that the local mining path can be exercised through the external miner or equivalent local test utility.

It does not certify production readiness, a public testnet, or complete private-testnet operation.

## 1) Build and test workspace

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace
```

Optional, only where clippy is already clean in the current environment:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## 2) Start one node

Start `pulsedagd` with a local development configuration.

Expected:

- Node starts successfully.
- Logs identify the node startup path clearly enough for local debugging.
- RPC endpoints required for mining template and submit flows are reachable.

## 3) Request a mining template

Using the existing RPC client, curl workflow, or miner integration, call the mining-template endpoint.

Expected:

- Response includes a candidate/template payload.
- The template can be used by the standalone external miner flow or existing local test utility.

## 4) Mine or simulate a valid nonce

Use one of these options:

- the standalone external miner against the template, or
- an existing local test utility, if available, to produce a valid nonce/hash pair.

Expected:

- Candidate has solvable PoW fields ready for submit.
- Invalid nonce/hash attempts remain rejected.

## 5) Submit block

Submit the solved candidate through the mining submit RPC flow.

Expected:

- Valid solution path: accepted response.
- Invalid/stale solution path: rejected response with a diagnostic reason where currently available.

## 6) Confirm node-side acceptance/rejection behavior

Validate logs or RPC state transitions showing that the block acceptance path invokes PoW verification and applies an accept/reject outcome.

Expected:

- A valid block is accepted once.
- A duplicate or invalid block is not accepted as new work.
- Invalid PoW does not enter the accepted DAG path.

## 7) Optional peer-connectivity check (manual/partial)

If the current local environment/config supports it, start a second node and verify basic peer connection establishment.

Expected:

- Basic connectivity signal only.
- Do not treat this as full multi-node private-testnet validation.

## Deferred to v2.3.0

Full multi-node PoW operation, propagation, sync/recovery, operator runbooks, and burn-in validation are deferred to v2.3.0.
