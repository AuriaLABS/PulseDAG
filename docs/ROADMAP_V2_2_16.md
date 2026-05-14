# PulseDAG v2.2.16 roadmap: miner/node contract hardening

v2.2.16 is the miner/node contract hardening milestone after v2.2.15 sustained P2P rehearsal evidence passed.

This release must make the boundary between `pulsedagd` and the standalone external `pulsedag-miner` canonical, testable, observable, and safe enough for the later v2.3.0 private-testnet readiness decision.

## Non-goals and guardrails

- Do not add smart contracts.
- Do not enable a contract runtime.
- Do not add pool logic.
- Do not move miner logic into `pulsedagd`.
- Keep `pulsedag-miner` external and standalone.
- Do not change consensus rules unless fixing a documented safety bug with tests.
- GPU mining, if attempted, must be optional, experimental, feature-gated, CPU-verified before submit, and non-blocking when no GPU is present.

## Goals

1. Canonical miner/node template contract.
2. Deterministic mining preimage documentation and tests.
3. Stable target/difficulty parse and comparison semantics.
4. Hardened mining submit validation.
5. Stable error taxonomy for miners.
6. External miner integration evidence.
7. Miner restart/reconnect evidence.
8. CPU miner correctness and operator telemetry.
9. Optional experimental GPU miner backend or backlog/scaffolding if practical.
10. v2.2.16 release evidence bundle.

## Workstreams

### 1. Canonical mining template

Document and test:

- `chain_id`.
- network/profile.
- `template_id`.
- parent/tip references.
- height or next-height semantics when available.
- timestamp and timestamp bounds.
- target/difficulty representation.
- canonical header or preimage fields.
- byte order and serialization.
- nonce field.
- hash function and target comparison.
- stale template behavior.

### 2. Submit validation

Harden and test rejection for:

- missing template id.
- unknown template id.
- stale template id.
- wrong chain id.
- wrong parent/tip.
- invalid nonce.
- invalid timestamp.
- invalid target.
- PoW hash above target.
- duplicate block.
- malformed payload.
- oversized payload.
- unsupported template version.

### 3. External miner integration

Add repeatable scripts proving the external miner can:

- fetch templates.
- mine or simulate valid proof in a test profile.
- submit blocks.
- handle stale templates.
- restart and reconnect.
- continue after node restart where practical.
- write evidence under `evidence/v2.2.16/`.

### 4. CPU miner reference hardening

The CPU miner remains the reference implementation. Harden:

- template parsing.
- target parsing.
- preimage construction.
- nonce range partitioning.
- stale template refresh.
- submit response parsing.
- reconnect/backoff.
- graceful shutdown.
- operator telemetry.

### 5. Optional experimental GPU mining

GPU work is allowed only if it respects all guardrails:

- external miner only.
- optional feature flag.
- default build works without GPU dependencies.
- CPU fallback remains available.
- GPU-found results are CPU-verified before submit.
- no pool logic.
- no consensus changes.
- GPU smoke evidence records `PASS`, `SKIP`, or `NOT_REQUESTED`.

## Suggested PR sequence

1. `codex/v2.2.16-open-miner-node-contract`.
2. `codex/v2.2.16-canonical-mining-template`.
3. `codex/v2.2.16-mining-submit-validation`.
4. `codex/v2.2.16-external-miner-integration-rehearsal`.
5. `codex/v2.2.16-cpu-miner-hardening`.
6. `codex/v2.2.16-experimental-gpu-miner`.
7. `codex/v2.2.16-mining-diagnostics`.
8. `codex/v2.2.16-release-evidence-closeout`.

## Exit criteria

v2.2.16 can close only when:

- canonical miner/node contract docs exist.
- template serialization/preimage/target behavior are tested.
- submit validation positive and negative paths are tested.
- stale template behavior is tested.
- external miner integration evidence passes.
- CPU miner reference evidence passes.
- miner restart/reconnect evidence passes.
- mining diagnostics are documented.
- GPU smoke evidence passes, skips cleanly, or is explicitly not requested.
- release evidence bundle passes.
- no unresolved Sev-1 consensus, sync, or mining-contract defect remains.
- v2.3.0 remains a readiness decision only.
