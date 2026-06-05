# Known Limitations v2.2.19

This document records **current known limitations** for `v2.2.19` and must be read together with release evidence and closeout artifacts.

## Scope and intent

- `v2.2.19` is a **private hardening / pre-public-testnet preparation** milestone.
- `v2.2.19` is **not** a `v2.3.0` readiness declaration.
- `v2.2.19` is **not** a public testnet launch.
- `v2.2.19` closeout accepts Docker-reproducible `5N/1M` and `5N/2M` staged convergence evidence while keeping `5N/4M` as observe-only stress evidence.

## Consensus/DAG limitation

- DAG selection is deterministic in current implementation paths.
- Deterministic behavior alone does **not** imply full Kaspa/GHOSTDAG compatibility.
- Compatibility must only be asserted if and when canonical behavior is implemented and validated by explicit tests/evidence.

## Mining limitation

- GPU mining is optional for `v2.2.19`.
- GPU paths are scaffold-only whenever no canonical tested kHeavyHash GPU kernel implementation is present.
- RC acceptance for `v2.2.19` must not depend on GPU availability unless a canonical kernel path and evidence are included in scope.
- Miner remains an external application; pool logic is not part of the miner.

## RPC exposure limitation

- RPC endpoints must remain localhost/private by default.
- Any exposure beyond localhost/private requires explicit hardening, authentication/authorization controls, and operator sign-off.
- Until secured controls are proven, public RPC exposure is out of scope.

## Evidence-first limitation

- Private-testnet RC claims require reproducible evidence artifacts, not narrative claims.
- Restore/rebuild confidence depends on successful, repeatable drill evidence.
- Long-run stability confidence requires sustained rehearsal evidence across time, not single-run outcomes.

## Staged convergence limitation

- A `3N/1M` local PASS is useful smoke evidence, but it is **not enough** for public-testnet readiness.
- `5N/1M baseline` is mandatory staged convergence evidence for v2.2.19 closeout.
- `5N/2M intermediate` is mandatory staged convergence evidence for v2.2.19 closeout.
- Docker closeout evidence records `5N/1M baseline` PASS and `5N/2M intermediate` PASS at commit `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a`.
- `5N/4M stress` is diagnostic/observe-only for v2.2.19. Its observe failure is accepted as non-blocking for v2.2.19 closeout and must be tracked as v2.2.20 hardening work.
- The accepted `5N/4M` stress limitation includes peer drop-to-zero, divergent final tips, and orphan/pending-missing-parent backlog reaching `512` on multiple nodes under 4-miner pressure.
- Every staged run must preserve `public_testnet_ready=false` and attach evidence on both PASS and FAIL.

## Operator interpretation rule

When in doubt, interpret `v2.2.19` outcomes as **readiness signals for additional private rehearsal work**, not as proof of production/public-network readiness.

## Readiness signaling limitation

- `public_testnet_ready` must remain `false` for v2.2.19 unless explicit public-testnet evidence gates are present.
- Missing private/local evidence should be surfaced as warnings or blockers; it must not be converted into optimistic readiness claims.
- `v2.2.19` readiness output must not claim `v2.3.0` or `v3.0` readiness.

## v2.3.0 start-gate limitation

- `v2.3.0` is a future decision target only; `v2.2.19` evidence can support a start decision but cannot itself declare `v2.3.0` ready.
- Formal `v2.3.0` readiness work must not start until the required gate evidence in `docs/V2_3_0_START_CHECKLIST.md` is attached and reviewed.
- A version bump beyond the current approved hardening cycle is out of scope until explicit maintainer approval is recorded after gate evidence passes.

## Current staged-gate limitation

- Docker evidence for `5N/1M` and `5N/2M` may support v2.2.19 private hardening closeout.
- `5N/4M` stress must be PASS or explicitly accepted as a non-blocking limitation with metrics; for v2.2.19 it is accepted as observe-only and tracked forward to v2.2.20.
- Missing archives, missing checksums, all-zero peer counts, all-zero heights, all-zero miner templates, accepted blocks equal to zero without waiver, unknown `chain_id`, required unhealthy nodes, or required unreadiness remain automatic **NO-GO** signals for mandatory gates.

## Public-testnet readiness limitation

- `public_testnet_ready` must remain `false` until a separate public-testnet go/no-go decision proves the public-testnet gates.
- Private rehearsal PASS results are necessary inputs, not sufficient proof, for public-testnet readiness.
- Public-testnet readiness requires security/RPC exposure evidence, chain-isolation evidence, release artifact and rollback evidence, operator runbooks, readiness captures for every required node, and accepted limitation mapping.
- Any accepted limitation must be explicitly non-blocking for public testnet and must include owner, UTC approval date, scope, expiry, and exit criteria.

## Burn-in limitation

- A 30-day burn-in cannot begin before public-testnet launch evidence collection is explicitly authorized.
- Burn-in evidence must cover at least 30 consecutive UTC days and include daily health/readiness snapshots, uptime/restart/incidents, peer metrics, convergence metrics, orphan/missing-parent pressure, miner accept/reject and accepted-block metrics, RPC error/latency summaries, storage/snapshot/restore samples, security observations, waivers, and issue closeout.
- Any unresolved Sev-1 consensus/sync/security incident at the end of burn-in is a **NO-GO** for burn-in completion.
