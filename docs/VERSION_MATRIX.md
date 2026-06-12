# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v2.2.20` |
| Cargo workspace version | `2.2.20` |
| Current milestone | `v2.2.20` active hardening and closeout evidence collection |
| Current state | **ACTIVE HARDENING / PRE-PUBLIC-TESTNET PREPARATION** |
| Readiness claims | No `v2.3.0` readiness claim; no `v3.0` readiness claim; no public-testnet launch claim |
| Public testnet signal | `public_testnet_ready=false` |

## v2.2.x progression status

| Version | Scope | Status |
|---|---|---|
| `v2.2.17` | API/operator/security hardening closeout | Historical baseline |
| `v2.2.18` | Private-testnet RC preparation and evidence gates | Historical baseline |
| `v2.2.19` | Private hardening / pre-public-testnet preparation | **CLOSED_WITH_DOCKER_EVIDENCE; `5N/4M` accepted as observe-only limitation for that milestone** |
| `v2.2.20` | Remaining hardening PRs, bounded evidence capture, orphan/peer/RPC/mining-submit stress diagnosis, and closeout checklist | **CURRENT ACTIVE HARDENING; all remaining hardening stays in `v2.2.20`** |
| `v2.3.0` | Future readiness review start only after `v2.2.20` closeout | **NOT STARTED; not ready; no public-testnet readiness claim** |

## v2.2.20 evidence matrix

`v2.2.20` remains the active hardening milestone. The evidence below is private rehearsal and closeout evidence only. It does **not** authorize a public-testnet launch, does **not** start burn-in, does **not** bump `VERSION`, and does **not** declare `v2.3.0` readiness.

| Evidence gate | Current status | Evidence / interpretation |
|---|---|---|
| `5N/1M baseline` | **PASS** | Accepted `v2.2.20` baseline evidence is recorded in `docs/V2_2_20_5N_1M_BASELINE_EVIDENCE.md`. This remains the mandatory regression guard. |
| `5N/2M intermediate` | **FAIL, improved / not closeout-pass** | Latest recorded `v2.2.20` evidence at merge commit `b6950201cd24ed8067c0b5dd228486047a1c27e0` cleared the prior peer/orphan/final-tip failure signature, but failed the accepted-block gate with `MINER_NO_ACCEPTED_BLOCKS`. See `docs/V2_2_20_5N_2M_INTERMEDIATE_EVIDENCE.md`. Later hardening PRs must attach replacement evidence before closeout can pass. |
| `5N/4M stress` | **OBSERVE_FAIL / evidence-only until replaced** | First `v2.2.20` measured stress evidence at commit `6633962c07bb1ccfc8c9e15b8763faf0402f45a6` retained peer collapse, saturated orphan and pending-missing-parent backlogs, and divergent final tips. See `docs/V2_2_20_FIRST_STRESS_EVIDENCE.md`. Later PRs `#600`-`#614` add hardening that must be validated by a new stress bundle before this gate can be closed. |
| Snapshot/restore drill | **REQUIRED FOR CLOSEOUT** | `docs/SNAPSHOT_RESTORE_DRILL_V2_2_20.md` defines the deterministic restore evidence track. Closeout needs the drill artifact or an approved waiver. |
| Workspace validation | **REQUIRED FOR CLOSEOUT** | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings` must be captured for the evaluated commit. |

## v2.2.20 hardening scope retained in this milestone

All remaining hardening PRs stay in `v2.2.20`. The closeout scope includes:

- preserving `5N/1M` baseline behavior;
- recovering `5N/2M` mined-chain advancement while preserving visible peers, one final tip, and drained orphan/missing-parent queues;
- replacing the first `5N/4M` observe failure with bounded, reviewable stress evidence;
- keeping RPC liveness/readiness/status endpoints responsive enough to capture evidence under stress;
- making orphan recovery, parent fetch, peer retention, and mining-submit queues bounded and diagnosable;
- recording snapshot/restore evidence or a formal waiver;
- maintaining `VERSION=v2.2.20`, Cargo workspace version `2.2.20`, and `public_testnet_ready=false`.

## v2.3.0 start prerequisites

Formal `v2.3.0` readiness work can start only after `v2.2.20` closeout is complete and reviewed. These prerequisites do **not** authorize a public-testnet launch and do **not** make a public-testnet readiness claim.

| Gate | Required outcome | Evidence expectation |
|---|---|---|
| v2.2.20 closeout decision | PASS | `docs/CLOSING_CHECKLIST_V2_2_20.md` completed with a `GO_TO_START_V2_3_0_REVIEW` decision, evaluated commit, reviewer, UTC date, evidence paths, and unresolved-waiver ledger. |
| Version guard | PASS | `VERSION` remains `v2.2.20`; Cargo workspace version remains `2.2.20`; no `v2.3.0` version bump diff exists. |
| Build/tests | PASS | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings` logs for the closeout commit. |
| `5N/1M baseline` | PASS | Private convergence evidence with quiescence metrics, final-tip diversity, orphan/missing-parent counts, miner accept/reject data, archive, and checksum. |
| `5N/2M intermediate` | PASS, or explicit release-manager waiver only if marked non-public-testnet and non-readiness | Replacement evidence after the hardening PRs, including accepted-block behavior. A non-PASS result blocks public-testnet readiness and blocks any claim that `v2.3.0` is ready. |
| `5N/4M stress` | PASS or accepted non-blocking limitation | Stress evidence with measured divergence/orphan pressure/missing-parent backlog/RPC liveness; any non-PASS result needs owner, UTC approval, scope, expiry, and exit criteria. |
| Known limitations | PASS | `docs/KNOWN_LIMITATIONS_V2_2_20.md` separates remaining limitations from limitations resolved by PRs `#600`-`#605` and later PRs. |
| Incident/waiver ledger | PASS | No unresolved Sev-1 consensus/sync/security blocker; every waiver has owner, UTC approval, scope, expiry, and exit criteria. |

## Public-testnet readiness rule

`public_testnet_ready` must remain `false`. A public-testnet-ready or public-testnet-live claim is forbidden from `v2.2.20` private rehearsal evidence alone.

Changing the signal requires a separate public-testnet go/no-go decision that proves all launch evidence, including security/RPC exposure review, chain isolation, operator runbooks, release artifacts, rollback evidence, node readiness captures, and accepted limitation posture.

## 30-day burn-in rule

A burn-in pass requires at least 30 consecutive UTC days of public-testnet evidence **after** a separate public-testnet launch authorization. No `v2.2.20` private hardening run starts that burn-in clock.
