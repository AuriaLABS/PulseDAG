# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v2.2.20` |
| Cargo workspace version | `2.2.20` |
| Current milestone | v2.2.20 active hardening / 5N/4M stress recovery |
| Current state | **ACTIVE HARDENING / PRE-PUBLIC-TESTNET PREPARATION** |
| Readiness claims | No v2.3.0 readiness claim; no v3.0 readiness claim; no public testnet launch claim |
| Public testnet signal | `public_testnet_ready=false` until explicit public-testnet gates pass |

## v2.2.x progression status

| Version | Scope | Status |
|---|---|---|
| v2.2.17 | API/operator/security hardening closeout | Historical baseline |
| v2.2.18 | Private-testnet RC preparation and evidence gates | Historical baseline |
| v2.2.19 | Active hardening / pre-public-testnet preparation | **CLOSED_WITH_DOCKER_EVIDENCE; 5N/4M accepted as observe-only limitation** |
| v2.2.20 | 5N/4M stress hardening: orphan recovery, peer retention, bounded diagnostics | **CURRENT ACTIVE HARDENING (PRE-PUBLIC-TESTNET, NO PUBLIC READINESS CLAIM)** |
| v2.3.0 | Future readiness decision | **FUTURE DECISION ONLY (not readiness claim; do not start until gates below pass)** |

## v2.2.20 target

`v2.2.20` starts from the `v2.2.19` Docker evidence closeout:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS
- `5N/4M stress`: OBSERVE_FAIL, accepted as a tracked non-blocking limitation for v2.2.19 and the first hardening target for v2.2.20

Primary objective: make `5N/4M` stress bounded, diagnosable, and recoverable without changing consensus semantics.

Initial focus areas:

- parent-fetch/orphan-recovery backpressure;
- peer retention under high block/rejection pressure;
- inflight block request caps and retry/fallback behavior;
- RPC responsiveness during stress cleanup and final evidence capture;
- deterministic metrics for peer drop-to-zero and orphan backlog saturation.

## v2.3.0 start prerequisites

`v2.3.0` work can start only after all required evidence is attached and reviewed. These prerequisites do **not** authorize a public-testnet launch and do **not** make a public-testnet readiness claim.

| Gate | Required outcome | Evidence expectation |
|---|---|---|
| Dependency ordering | PASS | PRs `#545`, `#546`, `#547`, and `#548` merged or explicitly deferred with rationale. |
| Build/tests | PASS | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings`. |
| 3N/1M smoke | PASS or approved waiver | Complete local evidence bundle or waiver with owner/expiry. |
| 5N/1M baseline | PASS | Docker/local convergence evidence with quiescence metrics, final-tip diversity, orphan/missing-parent counts, miner accept/reject data, archive, and checksum. |
| 5N/2M intermediate | PASS | Docker/local intermediate evidence with the same convergence and mining metrics as the baseline gate. |
| 5N/4M stress | PASS or accepted non-blocking limitation | Stress evidence with measured divergence/orphan pressure/missing-parent backlog; any non-PASS result must be accepted as non-blocking with owner, expiry, and exit criteria. |
| Release and runtime metadata | PASS | Release workflow artifacts plus `/release`, `/status`, and `/readiness` captures proving the current runtime state. |
| Snapshot/restore | PASS or approved waiver | Snapshot creation, restore/rebuild drill, restored readiness, timing metrics, or waiver with owner and expiry. |
| Known limitations | PASS | Decision-scoped mapping that separates accepted limitations from blockers. |
| Incident/waiver ledger | PASS | No unresolved Sev-1 consensus/sync/security blockers; every waiver has owner, UTC approval, scope, expiry, and exit criteria. |

## Public-testnet readiness rule

`public_testnet_ready` must remain `false` until a separate public-testnet go/no-go decision proves all launch evidence:

- security, RPC exposure, chain isolation, release artifact, rollback, and operator runbook evidence are attached;
- every required node has `/readiness` evidence with no public-testnet blockers;
- known limitations are explicitly accepted as non-blocking with owners, expiry, and exit criteria;
- the final public-testnet decision names the evaluated commit, evidence paths, reviewer, and UTC date.

## 30-day burn-in rule

A burn-in pass requires at least 30 consecutive UTC days of public-testnet evidence after launch authorization. The final report must link daily bundles and aggregate metrics for node health, readiness, uptime, incidents, peer visibility, chain convergence, orphan/missing-parent pressure, miner accept/reject activity, accepted blocks, RPC error/latency observations, storage growth, snapshot/restore samples, security events, waivers, and issue closeout.

## Evidence references

| Item | Path |
|---|---|
| v2.2.19 final closeout evidence checklist | `docs/CLOSING_CHECKLIST_V2_2_19_FINAL.md` |
| v2.2.19 closeout decision | `docs/V2_2_19_CLOSEOUT_DECISION.md` |
| v2.2.20 start plan | `docs/V2_2_20_START.md` |
| v2.2.20 Docker rehearsals | `docs/DOCKER_REHEARSALS_V2_2_20.md` |
| v2.3.0 start checklist | `docs/V2_3_0_START_CHECKLIST.md` |
| public-testnet burn-in evidence root (expected after launch approval) | `artifacts/public_testnet/burn_in_30d/` |
