# PulseDAG v2.2.16 closing checklist

v2.2.16 closes only when the external miner/node contract is canonical, tested, observable, and evidenced. This checklist is a release gate for v2.2.16 and is not a v2.3.0 readiness claim.

## Version and scope gate

- [ ] `VERSION` is `v2.2.16`.
- [ ] Cargo workspace version is `2.2.16`.
- [ ] Cargo workspace license metadata remains `ISC`.
- [ ] `README.md` and `docs/VERSION_MATRIX.md` describe v2.2.16 as the current miner/node contract hardening milestone.
- [ ] v2.2.15 is described as sustained P2P rehearsal evidence passed, not the current milestone.
- [ ] v2.2.17 remains API/operator/security hardening.
- [ ] v2.2.18 remains private-testnet RC.
- [ ] v2.3.0 remains a readiness decision only, not an automatic launch.
- [ ] No smart contracts are added.
- [ ] No contract runtime is enabled.
- [ ] No pool coordination logic is added inside `pulsedag-miner`.
- [ ] The miner remains a standalone external application.
- [ ] No consensus-rule change is included.
- [ ] No PoW semantic change is included.
- [ ] No full Kaspa/GHOSTDAG compatibility claim is made.
- [ ] No v3.0 or public-testnet readiness claim is made.
- [ ] GPU work, if present, is optional, feature-gated, external-miner only, gated on the canonical PoW adapter, and non-blocking when no GPU is available.

## Required command gate

Run these commands from the repository root and attach output or CI links:

```bash
cargo fmt --check
cargo test --workspace
```

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.

## Miner/node contract gate

- [ ] `docs/MINER_NODE_CONTRACT_V2_2_16.md` documents the canonical template fields.
- [ ] Template field order and serialization are documented.
- [ ] PoW preimage fields are documented.
- [ ] Endianness and byte encoding are documented.
- [ ] Hash function is documented.
- [ ] 256-bit hash vs 256-bit target comparison is documented.
- [ ] Nonce semantics are documented.
- [ ] Timestamp and timestamp bounds are documented.
- [ ] `chain_id` behavior is documented.
- [ ] `template_id` behavior is documented.
- [ ] Template freshness and expiry behavior are documented.
- [ ] Stale template behavior is documented.
- [ ] Target/difficulty representation is documented.
- [ ] Compatibility notes for existing miner clients are documented.

## Mining submit validation gate

Tests or evidence cover:

- [ ] Valid submit accepted.
- [ ] Missing template id rejected.
- [ ] Unknown template id rejected.
- [ ] Stale template id rejected.
- [ ] Wrong chain id rejected.
- [ ] Wrong parent/tip rejected.
- [ ] Invalid nonce rejected.
- [ ] Invalid timestamp rejected.
- [ ] Invalid target rejected.
- [ ] PoW hash above target rejected.
- [ ] Duplicate block rejected.
- [ ] Malformed payload rejected.
- [ ] Oversized payload rejected.
- [ ] Unsupported template version rejected.

Stable `data.reason_code` classes are documented and remain lower-snake-case for miner branching:

- `accepted`
- `stale_template`
- `invalid_pow`
- `malformed_block`
- `invalid_height`
- `invalid_parent`
- `duplicate_block`
- `invalid_coinbase`
- `invalid_transaction`
- `chain_id_mismatch` when submit carries chain identity
- `internal_error`
- legacy template refresh classes: `missing_template_id`, `unknown_template`

Machine-readable response requirements:

- [ ] Validation outcomes return `data.accepted` and `data.reason_code`.
- [ ] Miners do not need to parse human-readable `reason` text to classify known outcomes.
- [ ] Node logs and diagnostics preserve specific validation failures instead of generic submit errors.

## External miner integration gate

- [ ] External miner fetches a template from a node.
- [ ] External miner constructs the canonical preimage.
- [ ] External miner submits at least one valid block or test-profile proof.
- [ ] Stale template rejection is evidenced.
- [ ] Template expiry rejection is evidenced.
- [ ] Miner restart/reconnect is evidenced.
- [ ] Multi-miner rehearsal is evidenced without adding pool coordination logic.
- [ ] Node restart recovery is evidenced when practical.
- [ ] Evidence is written under `evidence/v2.2.16/`.
- [ ] The integration uses `pulsedag-miner` or equivalent external miner behavior, not embedded node mining.

## CPU miner reference gate

- [ ] CPU miner remains available by default.
- [ ] CPU miner parses template fields correctly.
- [ ] CPU miner uses the canonical preimage path.
- [ ] CPU miner uses the same target comparison as the node.
- [ ] Worker nonce ranges do not overlap.
- [ ] CPU miner refreshes stale templates.
- [ ] CPU miner handles submit responses deterministically.
- [ ] CPU miner logs hashrate, worker count, per-worker nonce ranges or progress, accepted submits, rejected submits, stale submits, current template id, and last error.

## Optional GPU gate

GPU mining is optional and experimental in v2.2.16.

- [ ] GPU backend is considered only after the canonical PoW adapter exists.
- [ ] GPU backend is feature-gated if implemented.
- [ ] Optional GPU miner documentation exists at `apps/pulsedag-miner/GPU.md`.
- [ ] GPU build command is documented as `cargo build -p pulsedag-miner --release --features gpu`.
- [ ] GPU runtime example with `--backend gpu` is documented.
- [ ] Driver/OpenCL requirements and troubleshooting are documented.
- [ ] Default build does not require GPU dependencies.
- [ ] Machines without GPU can still build and pass mandatory v2.2.16 evidence.
- [ ] CPU fallback remains available.
- [ ] Every GPU-found nonce/result is CPU-verified before submit.
- [ ] GPU smoke evidence is `PASS`, `SKIP`, or `NOT_REQUESTED` with reason.
- [ ] GPU code does not add pool logic.
- [ ] GPU code does not add shares.
- [ ] GPU code does not add payouts.
- [ ] GPU code keeps the miner as a standalone external app.
- [ ] GPU code does not change consensus rules.

## Diagnostics gate

- [ ] Mining diagnostics are documented in `docs/MINER_NODE_CONTRACT_V2_2_16.md` or `docs/API_V1.md`.
- [ ] Node-side mining diagnostics include current template id, target/difficulty, chain id, current tip, accepted submit count, rejected submit count, stale submit count, invalid PoW count, duplicate submit count, last submit error, and last accepted block where practical.
- [ ] Miner-side diagnostics include backend, aggregate hashrate, worker count, per-worker metrics where practical, template id, accepted/rejected/stale submits, reconnect count, last node error, and last submit time where practical.
- [ ] Diagnostics are read-only unless explicitly documented as admin-only.

## Release evidence script gate

Before closing v2.2.16, run the v2.2.16 release evidence bundle from the repository root and attach the transcript plus `evidence/v2.2.16/summary.md`.

```bash
bash scripts/v2-2-16-release-evidence.sh
```

Record closeout metadata:

- [ ] Commit SHA: ______________________________
- [ ] Date (UTC): ______________________________
- [ ] Operator: ________________________________
- [ ] Scripts executed: _________________________
- [ ] Overall pass/fail status: _________________
- [ ] GPU status: `PASS` / `SKIP` / `NOT_REQUESTED` / `FAIL`
- [ ] Known limitations: ________________________
- [ ] Go/no-go decision for moving to v2.2.17: __

## Defect gate

- [ ] No unresolved Sev-1 consensus defect remains open.
- [ ] No unresolved Sev-1 sync defect remains open.
- [ ] No unresolved Sev-1 mining-contract defect remains open.
- [ ] Any unresolved Sev-2 mining, P2P, sync, storage, or operator defect is documented with impact, owner, and follow-up milestone.
- [ ] Any release evidence failure is either fixed and rerun or recorded as a blocking release issue.

## Closeout decision

- [ ] Release notes are updated in `docs/RELEASE_NOTES_V2_2_16.md`.
- [ ] Roadmap scope is updated in `docs/ROADMAP_V2_2_16.md`.
- [ ] Miner/node contract docs are updated in `docs/MINER_NODE_CONTRACT_V2_2_16.md`.
- [ ] GPU status/backlog is updated in `docs/MINER_GPU_BACKLOG_V2_2_16.md`.
- [ ] Optional GPU miner docs are updated in `apps/pulsedag-miner/GPU.md`.
- [ ] Markdown links in updated GPU/miner/release docs are verified.
- [ ] Evidence links are collected in the release issue, PR, or release artifact index.
- [ ] The closeout summary explicitly states that v2.2.16 provides miner/node contract hardening evidence and does not claim v2.3.0 readiness by itself.
