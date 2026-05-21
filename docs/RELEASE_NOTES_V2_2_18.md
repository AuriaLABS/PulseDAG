# PulseDAG v2.2.18 release notes (private-testnet RC preparation)

## Scope statement
v2.2.18 is a **private-testnet readiness rehearsal preparation** milestone for a 5-node / 4-miner environment.

## Status gate
- **Current status: PLANNED / BLOCKED BY v2.2.17 EVIDENCE**.
- v2.2.18 execution and closeout begin only after v2.2.17 required evidence is complete.

## Planned rehearsal themes
- 5-node / 4-miner rehearsal target.
- Local Windows/WSL rehearsal path.
- Ubuntu/VPS rehearsal path.
- Topology manifest for deterministic role assignment.
- Deterministic startup/shutdown procedure.
- Sync convergence measurement.
- Miner acceptance/rejection telemetry.
- Snapshot/restore drill.
- Perturbation drills.
- RPC security verification reused from v2.2.17 controls.
- Evidence bundle and go/no-go report template.

## Non-goals and explicit exclusions
- v2.2.18 does **not** change consensus rules.
- v2.2.18 does **not** change PoW semantics.
- v2.2.18 does **not** add smart contracts.
- v2.2.18 does **not** add pool logic.
- v2.2.18 keeps miner external and standalone.
- GPU remains optional and must not block v2.2.18.

## Milestone positioning
- v2.2.18 prepares a private-testnet RC rehearsal package.
- v2.2.18 is **not** a v2.3.0 readiness claim.

## Closeout condition
This milestone is considered closed only with explicit evidence coverage per:
- `docs/CLOSING_CHECKLIST_V2_2_18.md`
- `docs/RELEASE_EVIDENCE.md`
- `docs/V2_2_18_PRIVATE_TESTNET_RC_PLAN.md`
