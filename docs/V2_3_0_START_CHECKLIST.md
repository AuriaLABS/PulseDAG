# v2.3.0 Start Checklist

This checklist defines the objective gate for **starting** formal `v2.3.0` readiness work. It is not a version bump approval, release approval, or public-testnet launch approval.

## Non-negotiable guardrails

- Do **not** bump `VERSION` from `v2.2.19` unless an explicit maintainer approval is recorded after the evidence gates pass.
- Keep `public_testnet_ready=false` until every public-testnet readiness gate below passes and the final go/no-go decision explicitly authorizes changing the signal.
- Do not claim `v2.3.0` readiness, `v3.0` readiness, or public-testnet launch readiness from private rehearsal evidence alone.
- Treat missing evidence as **NO-GO**, not as a warning, unless the checklist explicitly allows an approved waiver.

## Required gates before v2.3.0 can start

| Gate | Required status | Minimum evidence |
|---|---|---|
| Dependency ordering | PASS | Maintainer confirmation that PRs `#545`, `#546`, and `#547` are merged; `#548` is merged or explicitly deferred with rationale. |
| Version guard | PASS | `VERSION` remains `v2.2.19`; Cargo workspace version remains `2.2.19`; no unauthorized version bump diff. |
| Workspace validation | PASS | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings` logs. |
| Local smoke | PASS | `3N/1M` run bundle with node health, readiness, peer visibility, miner activity, accepted-block count, and `public_testnet_ready=false`. |
| Private baseline | PASS | `5N/1M` run bundle with pre/post-quiescence convergence, worst lag, distinct final tips, orphan/missing-parent counts, miner accept/reject summary, archive, and checksum. |
| Private intermediate | PASS | `5N/2M` run bundle with the same convergence and mining metrics as the baseline gate. |
| Private stress | PASS or accepted non-blocking limitation | `5N/4M` stress bundle with metrics. If not PASS, the release manager must accept a non-blocking limitation that includes measured divergence, orphan pressure, missing-parent backlog, peer visibility, final tips, recovery behavior, owner, expiry, and exit criteria. |
| Release workflow | PASS | Preflight and release workflow logs, `/release`, `/status`, and `/readiness` captures, and reproducible artifact/checksum evidence. |
| Snapshot/restore | PASS or approved waiver | Snapshot creation and restore/rebuild drill logs, restored node health/readiness, and timing metrics, or an explicit waiver with owner and expiry. |
| Known limitations mapping | PASS | A decision-scoped mapping from `docs/KNOWN_LIMITATIONS_V2_2_19.md` that identifies accepted limitations, blockers, owners, and exit criteria. |
| Incident and waiver ledger | PASS | Exported incident list, Sev-1 consensus/sync status, waiver owners, UTC approvals, scope, expiry, and exit criteria. |

## Final v2.3.0 start decision

Before work may be described as formal `v2.3.0` readiness work, create `artifacts/v2_2_19/closeout_decision/v2_3_0_start_decision.md` with:

- final decision: `GO_TO_START_V2_3_0_REVIEW | NO_GO | WAIVED_WITH_REASON`;
- exact git commit and branch evaluated;
- evidence bundle paths for every required gate;
- unresolved incidents and waivers, if any;
- explicit statement that `VERSION` was not bumped by the decision;
- explicit statement that `public_testnet_ready` remains `false` unless a separate public-testnet go/no-go authorizes changing it;
- reviewer name and UTC date.

## Public-testnet readiness evidence requirements

Public-testnet readiness can be considered only after the v2.3.0 start decision is `GO_TO_START_V2_3_0_REVIEW` and the following additional evidence exists:

- security posture: RPC exposure review, authentication/authorization posture, firewall/listener configuration, secret redaction check, and operator approval;
- network posture: declared bootnodes, chain ID, peer admission policy, restart/rejoin behavior, NAT/firewall assumptions, and chain-isolation evidence;
- operational posture: runbooks for deployment, rollback, incident response, snapshot/restore, monitoring, and evidence collection;
- release posture: deterministic binaries or archives, checksums, install verification, `/release` metadata, and rollback target artifacts;
- readiness posture: `/readiness` captures for every required node showing no blockers relevant to public testnet, plus an aggregate summary proving the gate;
- limitation posture: all accepted known limitations have owners, expiry dates, exit criteria, and are explicitly marked non-blocking for public testnet.

Until these items pass, the only allowed state is `public_testnet_ready=false`.

## 30-day burn-in requirements

A public-testnet burn-in clock starts only after the public-testnet go/no-go decision authorizes launch evidence collection. The burn-in evidence must cover at least 30 consecutive UTC days and include:

- daily node health/readiness snapshots for every required node;
- uptime, restart, and incident summaries with timestamps and owner notes;
- peer-count and peer-diversity metrics;
- chain progress, height/tip convergence, final-tip diversity, worst lag, and reorg/fork observations;
- orphan and missing-parent counts, including maximum backlog and recovery timing;
- miner template, submission, accept/reject, and accepted-block metrics;
- RPC error-rate and latency summaries for operator-facing endpoints;
- storage growth, snapshot, restore/rebuild, and checksum verification samples;
- security/abuse observations, firewall/listener changes, and credential/secret rotation notes where applicable;
- waiver/incident ledger updates and closeout status for every issue opened during burn-in.

The 30-day burn-in passes only if no unresolved Sev-1 consensus/sync/security issue remains, all waivers are still valid and explicitly accepted, and the final burn-in report links the daily evidence bundles and aggregate metrics.
