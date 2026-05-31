# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v2.2.19` |
| Cargo workspace version | `2.2.19` |
| Current milestone | v2.2.19 active hardening / pre-public-testnet preparation |
| Current state | **ACTIVE HARDENING / PRE-PUBLIC-TESTNET PREPARATION** |
| Readiness claims | No v2.3.0 readiness claim; no v3.0 readiness claim; no public testnet launch claim |
| Public testnet signal | `public_testnet_ready=false` until explicit public-testnet gates pass |

## v2.2.x progression status

| Version | Scope | Status |
|---|---|---|
| v2.2.17 | API/operator/security hardening closeout | Must be **CLOSED_WITH_EVIDENCE** or **WAIVED_WITH_REASON** before v2.2.18 evidence PASS |
| v2.2.18 | Private-testnet RC preparation and evidence gates | Historical baseline |
| v2.2.19 | Active hardening / pre-public-testnet preparation | **CURRENT ACTIVE HARDENING (PRE-PUBLIC-TESTNET, CLOSEOUT PENDING EVIDENCE)** |
| v2.3.0 | Future readiness decision | **FUTURE DECISION ONLY (not readiness claim; do not start until gates below pass)** |

## v2.3.0 start prerequisites

`v2.3.0` work can start only after all required evidence is attached and reviewed. These prerequisites do **not** authorize a version bump and do **not** make a public-testnet readiness claim.

| Gate | Required outcome | Evidence expectation |
|---|---|---|
| Dependency ordering | PASS | PRs `#545`, `#546`, and `#547` merged; PR `#548` merged or explicitly deferred with rationale. |
| Build/tests | PASS | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings`. |
| 3N/1M smoke | PASS | Complete local evidence bundle with health/readiness, peer visibility, miner activity, accepted blocks, archive, checksum, and `public_testnet_ready=false`. |
| 5N/1M baseline | PASS | Private convergence evidence with quiescence metrics, worst lag, final-tip diversity, orphan/missing-parent counts, miner accept/reject data, archive, and checksum. |
| 5N/2M intermediate | PASS | Private intermediate evidence with the same convergence and mining metrics as the baseline gate. |
| 5N/4M stress | PASS or accepted non-blocking limitation | Stress evidence with measured divergence/orphan pressure/missing-parent backlog; any non-PASS result must be accepted as non-blocking with owner, expiry, and exit criteria. |
| Release and runtime metadata | PASS | Release workflow artifacts plus `/release`, `/status`, and `/readiness` captures proving the current runtime state. |
| Snapshot/restore | PASS or approved waiver | Snapshot creation, restore/rebuild drill, restored readiness, timing metrics, or waiver with owner and expiry. |
| Known limitations | PASS | Decision-scoped mapping from `docs/KNOWN_LIMITATIONS_V2_2_19.md` that separates accepted limitations from blockers. |
| Incident/waiver ledger | PASS | No unresolved Sev-1 consensus/sync/security blockers; every waiver has owner, UTC approval, scope, expiry, and exit criteria. |

## Public-testnet readiness rule

`public_testnet_ready` must remain `false` until a separate public-testnet go/no-go decision proves all launch evidence:

- v2.3.0 start decision is `GO_TO_START_V2_3_0_REVIEW`;
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
| v2.2.19 closeout decision artifact (expected) | `artifacts/v2_2_19/closeout_decision/final_decision.md` |
| v2.3.0 start checklist | `docs/V2_3_0_START_CHECKLIST.md` |
| v2.3.0 start decision artifact (expected) | `artifacts/v2_2_19/closeout_decision/v2_3_0_start_decision.md` |
| public-testnet burn-in evidence root (expected after launch approval) | `artifacts/public_testnet/burn_in_30d/` |
