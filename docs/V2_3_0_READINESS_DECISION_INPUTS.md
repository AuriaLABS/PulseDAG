# v2.3.0 Readiness Decision Inputs (from v2.2.19 hardening closeout)

> This document is a **decision-review input**. It is not a standalone readiness declaration, version bump approval, or public-testnet launch approval.

## Decision context

- Source milestone: `v2.2.19` active hardening / pre-public-testnet preparation.
- Decision target: whether to open formal `v2.3.0` readiness review.
- Dependency ordering: `#545`, `#546`, and `#547` must be merged; `#548` should be merged unless explicitly deferred with rationale.
- Guardrail: `VERSION` remains `v2.2.19` unless explicit maintainer approval authorizes a bump after evidence passes.
- Signal guardrail: `public_testnet_ready=false` remains mandatory until a separate public-testnet go/no-go proves launch readiness.

## Evidence summary table

| Input category | Current value / summary | Evidence path(s) | Status |
|---|---|---|---|
| Dependency ordering | Merge/defer confirmation not attached. | `artifacts/v2_2_19/closeout_decision/dependency_ordering.md` | PENDING |
| Version guard | `VERSION` must remain `v2.2.19`; Cargo workspace must remain `2.2.19`. | `artifacts/v2_2_19/closeout_decision/version_guard.md` | PENDING |
| Build/test validation | Required fmt/check/test/clippy logs must be attached. | `artifacts/v2_2_19/validation/cargo_validation_suite.log` | PENDING |
| 3N/1M local smoke | Required PASS before v2.3.0 start. | `artifacts/v2_2_19/local_3n_1m_smoke/evidence-summary.md` | PENDING |
| 5N/1M private baseline | Required PASS before v2.3.0 start. | `artifacts/v2_2_19/private_5n_1m_rehearsal/evidence-summary.md` | PENDING |
| 5N/2M private intermediate | Required PASS before v2.3.0 start. | `artifacts/v2_2_19/private_5n_2m_rehearsal/evidence-summary.md` | PENDING |
| 5N/4M private stress | Required PASS or accepted non-blocking limitation with metrics. | `artifacts/v2_2_19/private_5n_4m_rehearsal/evidence-summary.md`; `artifacts/v2_2_19/closeout_decision/v2_3_0_known_limitations_mapping.md` | PENDING |
| Release/runtime metadata | Preflight, release workflow, `/release`, `/status`, and `/readiness` captures must be attached. | `artifacts/v2_2_19/preflight/`; `artifacts/v2_2_19/release_workflow/` | PENDING |
| Snapshot/restore | Restore confidence must be attached or explicitly waived. | `artifacts/v2_2_19/snapshot_restore/` | PENDING |
| Public-testnet readiness evidence | Not sufficient until security, network, runbook, release, readiness, and limitation gates pass. | `artifacts/public_testnet/readiness/` | PENDING |
| 30-day burn-in | Cannot start before public-testnet launch evidence collection is authorized. | `artifacts/public_testnet/burn_in_30d/` | PENDING |
| Known limitations | Decision-scoped mapping from current limitations is not attached. | `docs/KNOWN_LIMITATIONS_V2_2_19.md`; `artifacts/v2_2_19/closeout_decision/v2_3_0_known_limitations_mapping.md` | PENDING |
| Unresolved incidents | Incident export not attached. Any unresolved Sev-1 consensus/sync/security issue is automatic NO-GO. | `artifacts/v2_2_19/closeout_decision/incident_waiver_ledger.md` | PENDING |
| Waivers | Waiver ledger not attached. | `artifacts/v2_2_19/closeout_decision/incident_waiver_ledger.md` | PENDING |

## Required staged-gate evidence fields

Each staged rehearsal bundle must include enough machine-readable or reviewable evidence to evaluate:

- topology, node count, miner count, git commit, version, command line, start/end timestamps, and duration;
- node health and readiness before and after the run;
- peer counts, peer compatibility/chain ID, and restart/rejoin behavior where applicable;
- pre-quiescence and post-quiescence convergence, worst lag, distinct final tips, and final heights;
- orphan count, missing-parent count, maximum backlog, and recovery timing;
- miner template count, submission count, accept/reject summary, accepted blocks, and rejection classes;
- `public_testnet_ready=false` in readiness captures;
- `evidence.tar.gz` and `evidence.tar.gz.sha256` for both PASS and FAIL outcomes.

## Public-testnet readiness evidence requirements

Public-testnet readiness can be considered only after the v2.3.0 start decision is `GO_TO_START_V2_3_0_REVIEW`. Required launch evidence includes:

- RPC exposure/security posture, authentication/authorization controls, firewall/listener configuration, secret redaction, and operator approval;
- declared bootnodes, chain ID, peer admission policy, chain-isolation evidence, NAT/firewall assumptions, and restart/rejoin behavior;
- deployment, rollback, incident response, snapshot/restore, monitoring, and evidence-collection runbooks;
- release artifacts, checksums, install verification, `/release` metadata, and rollback targets;
- `/readiness` captures for every required node plus an aggregate readiness report;
- accepted known limitations marked non-blocking with owner, UTC approval date, scope, expiry, and exit criteria.

Until those items pass, `public_testnet_ready` must remain `false`.

## 30-day burn-in requirements

A burn-in pass requires at least 30 consecutive UTC days of evidence after public-testnet launch evidence collection is authorized. Required evidence includes:

- daily node health/readiness snapshots for every required node;
- uptime, restart, and incident summaries;
- peer-count and peer-diversity metrics;
- chain progress, height/tip convergence, final-tip diversity, worst lag, and reorg/fork observations;
- orphan and missing-parent counts, including maximum backlog and recovery timing;
- miner template, submission, accept/reject, and accepted-block metrics;
- RPC error-rate and latency summaries;
- storage growth, snapshot, restore/rebuild, and checksum verification samples;
- security/abuse observations and firewall/listener changes;
- waiver/incident ledger updates and closeout status for every burn-in issue.

## GO / NO-GO recommendation

- **Recommendation: NO-GO (provisional)**.
- Rationale:
  1. Required v2.3.0 start gate evidence is still PENDING.
  2. Public-testnet readiness evidence is still PENDING and cannot be inferred from private rehearsal evidence.
  3. The 30-day burn-in cannot begin until public-testnet launch evidence collection is authorized.
  4. Readiness cannot be declared while incident, waiver, and known-limitation mapping are incomplete.

## Exit criteria to change recommendation

Change this recommendation only when all rows above are PASS or explicitly `WAIVED_WITH_REASON`, the final start decision is attached, and the decision explicitly preserves the version and `public_testnet_ready=false` guardrails unless separately authorized.
