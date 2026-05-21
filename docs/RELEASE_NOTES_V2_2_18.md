# PulseDAG v2.2.18 release notes (private RC closeout)

## Scope statement
v2.2.18 is a **private RC closeout and rehearsal verification** milestone.

## Status gate
- **Current status: PLANNED / BLOCKED BY v2.2.17 EVIDENCE**.
- v2.2.18 closeout cannot move to PASS until v2.2.17 evidence is closed or explicitly waived.

## Required closeout checklist coverage
Closeout requires evidence-backed PASS/PENDING/WAIVED tracking for:
- v2.2.17 evidence closed or waived.
- VERSION/Cargo/README/matrix alignment.
- `cargo fmt --check` PASS.
- `cargo test --workspace` PASS.
- `cargo build --workspace --release` PASS.
- local 3-node + 1-miner rehearsal PASS.
- RC 5-node + 4-miner rehearsal attempted or pending with owner/date.
- sync convergence evidence.
- miner telemetry evidence.
- perturbation drills evidence.
- snapshot/restore drill evidence.
- RPC security smoke evidence.
- release artifact dry run evidence.
- go/no-go report generation.
- known limitations documentation.
- risk register update.

## Guardrails and non-goals
- No consensus changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- GPU is optional only.
- No v2.3.0 readiness claim.
- No v3.0 readiness claim.

## Evidence rule
Do not mark PASS without explicit evidence path.

## Closeout references
- `docs/CLOSING_CHECKLIST_V2_2_18.md`
- `docs/RELEASE_EVIDENCE.md`
- `docs/VERSION_MATRIX.md`
- `README.md`
