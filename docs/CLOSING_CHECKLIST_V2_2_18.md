# v2.2.18 Closing Checklist (private-testnet RC preparation)

> Rule: mark PASS only when evidence file/path/output exists. Otherwise keep PENDING and include exact command.

## Milestone gate
- [ ] PASS / [x] PENDING: v2.2.17 evidence is complete and reviewed. Until then v2.2.18 remains **PLANNED / BLOCKED BY v2.2.17 EVIDENCE**.

## Version consistency
- [ ] PASS / [x] PENDING: VERSION/Cargo/README/version matrix/release notes/checklist are mutually consistent for v2.2.18 planning state.

## Documentation deliverables
- [ ] PASS / [x] PENDING: `docs/V2_2_18_PRIVATE_TESTNET_RC_PLAN.md` published.
- [ ] PASS / [x] PENDING: topology manifest template included.
- [ ] PASS / [x] PENDING: deterministic startup/shutdown procedure documented.
- [ ] PASS / [x] PENDING: sync convergence measurement method documented.
- [ ] PASS / [x] PENDING: miner acceptance/rejection telemetry method documented.
- [ ] PASS / [x] PENDING: snapshot/restore drill procedure documented.
- [ ] PASS / [x] PENDING: perturbation drill suite documented.
- [ ] PASS / [x] PENDING: RPC security verification references v2.2.17 runbook/scripts.
- [ ] PASS / [x] PENDING: evidence bundle and go/no-go report template documented.

## Rehearsal execution evidence (when unblocked)
- [ ] PASS / [x] PENDING: Windows/WSL rehearsal evidence captured.
- [ ] PASS / [x] PENDING: Ubuntu/VPS rehearsal evidence captured.
- [ ] PASS / [x] PENDING: 5-node / 4-miner topology instantiated and logged.
- [ ] PASS / [x] PENDING: deterministic startup/shutdown logs captured.
- [ ] PASS / [x] PENDING: sync convergence timings recorded.
- [ ] PASS / [x] PENDING: miner accepted/rejected share metrics captured.
- [ ] PASS / [x] PENDING: snapshot/restore drill success evidence captured.
- [ ] PASS / [x] PENDING: perturbation drill outcomes captured.
- [ ] PASS / [x] PENDING: go/no-go report published with explicit decision.

## Guardrail assertions
- [ ] PASS / [x] PENDING: no consensus rule changes.
- [ ] PASS / [x] PENDING: no PoW semantic changes.
- [ ] PASS / [x] PENDING: no smart contracts added.
- [ ] PASS / [x] PENDING: no pool logic added.
- [ ] PASS / [x] PENDING: miner remains external/standalone.
- [ ] PASS / [x] PENDING: GPU optionality preserved.
- [ ] PASS / [x] PENDING: no v2.3.0 readiness claim.
