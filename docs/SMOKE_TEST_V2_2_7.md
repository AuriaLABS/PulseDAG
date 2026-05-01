# PulseDAG v2.2.7 Smoke Test (Manual/Partial)

This smoke test validates the minimum PoW/mining foundation path in v2.2.7. It is intentionally **manual/partial** and does **not** replace the full multi-node private-testnet validation targeted for v2.3.0.

## 1) Build workspace

```bash
cargo fmt --check
cargo test --workspace
```

Optional (only where clippy is already clean in your environment):

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## 2) Start one node

Start `pulsedagd` with your local development configuration.

Expected: node starts successfully and exposes RPC endpoints required for mining template and submit flows.

## 3) Request a mining template

Using your existing RPC client or curl workflow, call the mining-template endpoint.

Expected: response includes a candidate/template payload that can be solved by the external miner flow.

## 4) Mine or simulate a valid nonce

- Use the standalone external miner against the template, **or**
- Use existing local test utilities (if available) to produce a valid nonce/hash pair.

Expected: candidate now has solvable PoW fields ready for submit.

## 5) Submit block

Submit the solved candidate through the mining submit RPC flow.

Expected:
- Valid solution path: accepted response.
- Invalid/stale solution path: rejected response with diagnostic reason.

## 6) Confirm node-side acceptance/rejection behavior

Validate logs or RPC state transitions showing that block acceptance path invokes PoW verification and applies accept/reject outcome.

## 7) Optional peer connectivity check (manual/partial)

If current environment/config supports it, start a second node and verify basic peer connection establishment.

Expected: basic connectivity signal only.

> Full multi-node PoW operation, propagation, sync/recovery, and burn-in validation are deferred to v2.3.0.
