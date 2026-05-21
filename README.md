# PulseDAG v2.2.18 RC preparation status

## Current milestone
- **v2.2.18 private RC closing checklist** (**PLANNED / BLOCKED BY v2.2.17 EVIDENCE**).

## Execution gate
- v2.2.18 closeout starts only after v2.2.17 required evidence is closed or explicitly waived.

## Mandatory closeout evidence for v2.2.18
- VERSION/Cargo/README/version matrix alignment.
- `cargo fmt --check` PASS.
- `cargo test --workspace` PASS.
- `cargo build --workspace --release` PASS.
- Local 3-node + 1-miner rehearsal PASS.
- RC 5-node + 4-miner rehearsal attempted, or marked pending with owner/date.
- Sync convergence, miner telemetry, perturbation drills, snapshot/restore, and RPC security smoke evidence.
- Release artifact dry run evidence.
- Go/no-go report generated.
- Known limitations documented.
- Risk register updated.

## Guardrails reaffirmed for v2.2.18
- No consensus changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- GPU optional only.
- No v2.3.0 readiness claim.
- No v3.0 readiness claim.

## Closeout references
- `docs/RELEASE_NOTES_V2_2_18.md`
- `docs/CLOSING_CHECKLIST_V2_2_18.md`
- `docs/VERSION_MATRIX.md`
- `docs/RELEASE_EVIDENCE.md`
