# PulseDAG v2.2.16 release notes

PulseDAG v2.2.16 opens as the miner/node contract hardening milestone after v2.2.15 sustained P2P rehearsal evidence passed.

## Opening status

v2.2.16 starts from the v2.2.15 closeout line where sustained P2P evidence passed on Ubuntu/WSL and the project moved to miner/node contract hardening.

v2.2.16 is not a v2.3.0 readiness claim. It is one of the remaining hardening gates before the private-testnet readiness decision.

## Scope

v2.2.16 focuses on:

- external miner/node contract hardening for the standalone `pulsedag-miner`.
- canonical mining template fields and deterministic PoW preimage documentation/tests.
- mining template freshness, expiry, and stale-template rejection behavior.
- target/difficulty semantics and 256-bit hash-vs-target comparison evidence.
- mining submit validation with a stable rejection taxonomy for external miners.
- miner telemetry, worker metrics, and node-side mining diagnostics.
- external miner restart/reconnect evidence and multi-miner rehearsal evidence.
- CPU miner correctness as the default reference backend.
- optional experimental GPU mining backend work only after the canonical PoW adapter exists, with every GPU-found nonce/result CPU-verified before submit.

## Guardrails

- No smart contracts are added.
- No contract runtime is enabled.
- No pool coordination logic is added inside `pulsedag-miner`.
- The miner remains a standalone external application.
- Miner logic must not be moved into `pulsedagd`.
- Consensus-rule changes remain out of scope; v2.2.16 must not change consensus rules or PoW semantics.
- GPU mining, if present, must live only in the external miner, be optional, be feature-gated, depend on the canonical PoW adapter, CPU-verify every GPU-found nonce/result before submit, and never be required for default builds.
- v2.2.16 is not a v2.3.0 readiness claim and does not claim public-testnet readiness.
- kHeavyHash/PoW alignment must not be presented as full Kaspa or GHOSTDAG compatibility.

## Required validation before closeout

Before closing v2.2.16, collect output for:

```bash
cargo fmt --check
cargo test --workspace
bash scripts/v2-2-16-release-evidence.sh
```

The release evidence bundle should write section logs and `evidence/v2.2.16/summary.md`.

## Optional GPU validation

GPU mining is optional and experimental in v2.2.16, and it is only eligible after the canonical PoW adapter exists. When implemented and available on the host, collect smoke evidence with the GPU feature enabled and CPU-verify every GPU-found nonce before submit. When unavailable, the release evidence should record `SKIP` or `NOT_REQUESTED`, not fail the mandatory CPU miner/node contract gate.

Example future commands:

```bash
cargo build --workspace --features gpu
bash scripts/v2-2-16-gpu-miner-smoke.sh
```

## Known limitations at opening

- v2.2.16 starts as a contract-hardening milestone; release evidence is not yet collected.
- GPU mining is allowed only as experimental external-miner work and should not block closeout when no GPU exists.
- v2.3.0 remains a later private-testnet readiness decision.

## Operator documents

- Roadmap: `docs/ROADMAP_V2_2_16.md`.
- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_16.md`.
- Miner/node contract: `docs/MINER_NODE_CONTRACT_V2_2_16.md`.
- GPU backlog: `docs/MINER_GPU_BACKLOG_V2_2_16.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
