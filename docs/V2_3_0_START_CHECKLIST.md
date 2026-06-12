# v2.3.0 Start Checklist

This checklist defines the objective gate for **starting** formal `v2.3.0` readiness work. It is not a version bump approval, release approval, or public-testnet launch approval.

## Non-negotiable guardrails

- `v2.3.0` work can start only after `v2.2.20` closeout is complete, reviewed, and recorded as `GO_TO_START_V2_3_0_REVIEW`.
- Do **not** bump `VERSION` from `v2.2.20` unless a later explicit maintainer approval is recorded after the closeout gates pass.
- Cargo workspace version must remain `2.2.20` during this checklist unless that same later explicit maintainer approval authorizes a version bump.
- Keep `public_testnet_ready=false`; this checklist cannot authorize public-testnet launch, public-testnet live status, or public-testnet readiness.
- Do not claim `v2.3.0` readiness, `v3.0` readiness, or public-testnet launch readiness from private rehearsal evidence alone.
- Treat missing evidence as **NO-GO**, not as a warning, unless this checklist explicitly allows an approved waiver.

## Required gates before v2.3.0 work can start

| Gate | Required status | Minimum evidence |
|---|---|---|
| v2.2.20 closeout | PASS | Completed `docs/CLOSING_CHECKLIST_V2_2_20.md` with final decision `GO_TO_START_V2_3_0_REVIEW`, exact commit, reviewer, UTC date, and evidence paths. |
| Version guard | PASS | `VERSION` remains `v2.2.20`; Cargo workspace version remains `2.2.20`; no unauthorized version bump diff. |
| Workspace validation | PASS | `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings` logs from the evaluated closeout commit. |
| Local smoke | PASS | `3N/1M` or equivalent local smoke bundle with node health, readiness, peer visibility, miner activity, accepted-block count, and `public_testnet_ready=false`. |
| Private baseline | PASS | `5N/1M` run bundle with pre/post-quiescence convergence, worst lag, distinct final tips, orphan/missing-parent counts, miner accept/reject summary, archive, and checksum. |
| Private intermediate | PASS or explicit non-readiness waiver | Replacement `5N/2M` run after `v2.2.20` hardening PRs. Any waiver must state that it blocks public-testnet readiness and does not make `v2.3.0` ready. |
| Private stress | PASS or accepted non-blocking limitation | Replacement `5N/4M` stress bundle with metrics. If not PASS, the release manager must accept a non-blocking limitation that includes measured divergence, orphan pressure, missing-parent backlog, RPC liveness, peer visibility, final tips, recovery behavior, owner, expiry, and exit criteria. |
| Release workflow | PASS | Preflight and release workflow logs, `/release`, `/status`, and `/readiness` captures, and reproducible artifact/checksum evidence for the closeout commit. |
| Snapshot/restore | PASS or approved waiver | Snapshot creation and restore/rebuild drill logs, restored node health/readiness, and timing metrics, or an explicit waiver with owner, UTC approval, scope, expiry, and exit criteria. |
| Known limitations mapping | PASS | Decision-scoped mapping from `docs/KNOWN_LIMITATIONS_V2_2_20.md` that identifies remaining limitations, resolved limitations from PRs `#600`-`#605` and later PRs, blockers, owners, and exit criteria. |
| Incident and waiver ledger | PASS | Exported incident list, Sev-1 consensus/sync/security status, waiver owners, UTC approvals, scope, expiry, and exit criteria. |

## Final v2.3.0 start decision

Before work may be described as formal `v2.3.0` readiness work, create `artifacts/v2_2_20/closeout_decision/v2_3_0_start_decision.md` with:

- final decision: `GO_TO_START_V2_3_0_REVIEW | NO_GO | WAIVED_WITH_REASON`;
- exact git commit and branch evaluated;
- evidence bundle paths for every required gate;
- unresolved incidents and waivers, if any;
- explicit statement that `VERSION` remained `v2.2.20` for the evaluated closeout;
- explicit statement that Cargo workspace version remained `2.2.20` for the evaluated closeout;
- explicit statement that `public_testnet_ready` remains `false`;
- explicit statement that the decision does not claim public-testnet readiness or public-testnet live status;
- reviewer name and UTC date.

## Public-testnet readiness evidence requirements

Public-testnet readiness can be considered only after a later public-testnet go/no-go process, separate from this start checklist, proves all launch evidence. Required evidence includes:

- security posture: RPC exposure review, authentication/authorization posture, firewall/listener configuration, secret redaction check, and operator approval;
- network posture: declared bootnodes, chain ID, peer admission policy, restart/rejoin behavior, NAT/firewall assumptions, and chain-isolation evidence;
- operational posture: runbooks for deployment, rollback, incident response, snapshot/restore, monitoring, and evidence collection;
- release posture: deterministic binaries or archives, checksums, install verification, `/release` metadata, and rollback target artifacts;
- readiness posture: `/readiness` captures for every required node showing no blockers relevant to public testnet, plus an aggregate summary proving the gate;
- limitation posture: all accepted known limitations have owners, expiry dates, exit criteria, and are explicitly marked non-blocking for public testnet.

Until these items pass in that separate process, the only allowed state is `public_testnet_ready=false`.

## 30-day burn-in requirements

A public-testnet burn-in clock starts only after a separate public-testnet go/no-go decision authorizes launch evidence collection. The burn-in evidence must cover at least 30 consecutive UTC days and include daily node health/readiness, uptime, restart, incident, peer, convergence, orphan/missing-parent, miner, RPC, storage, snapshot/restore, security, waiver, and incident-ledger evidence.

The 30-day burn-in passes only if no unresolved Sev-1 consensus/sync/security issue remains, all waivers are still valid and explicitly accepted, and the final burn-in report links the daily evidence bundles and aggregate metrics.
