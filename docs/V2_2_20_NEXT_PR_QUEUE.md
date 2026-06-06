# v2.2.20 Next PR Queue

Status: READY_FOR_CODEX

## Current evidence state

- `5N/1M baseline`: PASS.
- `5N/2M intermediate`: FAIL with all effective peer counts at zero, orphan backlog, pending-missing-parent backlog, and divergent final tips.
- `5N/4M stress observe`: OBSERVE_FAIL with the same failure axis amplified.

## Required PR order

### 1. Warning-clean helper PR

Title:

```text
fix: keep block request helpers warning-clean
```

Purpose:

- Clear strict `-D warnings` noise from `apps/pulsedagd/src/block_request.rs`.
- Do not change runtime behavior.
- Do not delete scheduler methods or tests.

Validation:

```bash
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

### 2. 5N/2M recovery PR

Title:

```text
p2p/sync: reconcile peer accounting and orphan backlog recovery
```

Purpose:

- Fix or classify why effective `/p2p/status.peer_count` becomes zero while lifecycle state still indicates connections/recovery.
- Record deterministic reason when peers enter recovering/cooldown.
- Ensure orphan reprocess attempts occur when backlog remains non-zero.
- Recover `5N/2M intermediate` before treating `5N/4M` as closeout-level evidence.

Validation:

```bash
scripts/v2_2_20_private_5n_1m_rehearsal.sh
scripts/v2_2_20_private_5n_2m_rehearsal.sh
```

### 3. Evidence PR after recovery

Title:

```text
docs: record v2.2.20 5N/2M recovery evidence
```

Purpose:

- Record PASS evidence for `5N/2M`, or a deterministic failure with improved diagnostics.
- Include artifact checksum, final heights, final tips, peer counts, orphan counts, pending-missing-parent counts, and miner stats.

## Guardrails

- No consensus-rule breaking changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- No public-testnet readiness claim.
- No v2.3.0 readiness claim.
