# Roadmap v3.0 — Long-Lived Functional Core

v3.0 is not a marketing milestone. It is the first long-lived functional PulseDAG core that can run for years with stable node, PoW, external miner, P2P, sync, storage, snapshots, pruning policy, operator RPC, release evidence, and upgrade policy.

The v3.0 line must prefer durability, migration safety, reproducibility, and operator evidence over feature expansion. Each milestone below exists to reduce operational, consensus, sync, storage, or release risk before the next wider network commitment.

## Philosophy

- v3.0 is earned by evidence, not declared by version number.
- The core protocol, node lifecycle, storage lifecycle, and operator workflows must be stable enough to survive long-running private and public testnet use before v3.0.0 ships.
- Release decisions must be reversible where possible, reproducible from artifacts, and backed by documented evidence.
- New features are subordinate to core stability until the stable core is proven.
- Compatibility claims must be narrow, implemented, tested, and documented.

## Long-term sequence

| Version | Purpose | Exit framing |
| --- | --- | --- |
| v2.2.12 | Full private-testnet rehearsal and hardening | Rehearsal evidence, runbook hardening, and multi-node/operator validation only. |
| v2.2.13 | Consensus/DAG safety audit | DAG invariants, deterministic selection behavior, validation safety, and compatibility-claim review. |
| v2.2.14 | Storage/replay/snapshot/restore/pruning hardening | Durable replay evidence, documented storage migrations, snapshot/restore proofs, and explicit pruning policy. |
| v2.2.15 | Sustained P2P multi-node rehearsal | Longer-running network churn, restart/rejoin, lag recovery, and multi-node convergence evidence. |
| v2.2.16 | Miner/node contract hardening | Stable external miner/node RPC contract; optional GPU backlog only if canonical and non-consensus-disruptive. |
| v2.2.17 | API/operator/security hardening | Public/operator/dev RPC boundary documentation, auth/rate-limit posture, and operator diagnostics. |
| v2.2.18 | Private-testnet RC | Final private-testnet release candidate evidence bundle and go/no-go checklist. |
| v2.3.0 | Private-testnet readiness decision only | Decision milestone; not an automatic public launch. |
| v2.4.x | Private-testnet stable line | Bug-fix and evidence-driven stability line for the private testnet. |
| v2.5.x | Public-testnet preparation | Public operator documentation, bootstrap policy, monitoring, release reproducibility, and support readiness. |
| v2.6.x | Public-testnet candidate and long soak | Candidate public network with long soak, incident tracking, rollback drills, and no unresolved Sev-1 consensus/sync incident. |
| v2.7.x | Protocol freeze | Freeze consensus, network, storage, miner contract, and RPC boundaries except for documented safety fixes. |
| v2.8.x | v3.0 release candidates | Reproducible artifacts, migration rehearsals, snapshot/restore evidence, and final operator sign-off. |
| v3.0.0 | Long-lived functional core | Stable core release intended to run for years under documented upgrade, rollback, storage, and operational policies. |

## Milestone requirements

### v2.2.12 — Full private-testnet rehearsal and hardening

- Rehearse the completed private-testnet path across multiple nodes and operators.
- Capture evidence for block propagation, transaction relay, tip exchange, restart/rejoin, and catch-up behavior.
- Harden runbooks, dashboards, diagnostics, release evidence, and incident notation.
- Keep the milestone as rehearsal and hardening only; do not claim private-testnet readiness.

### v2.2.13 — Consensus/DAG safety audit

- Audit DAG invariants, deterministic tip/selection behavior, parent linkage, height/timestamp validation, missing-parent handling, and orphan adoption safety.
- Add replay/order-independence evidence where practical.
- Review all compatibility language and remove or qualify any unsupported Kaspa/GHOSTDAG claim.
- Treat consensus changes as safety fixes only and require test evidence for each change.

### v2.2.14 — Storage/replay/snapshot/restore/pruning hardening

- Validate deterministic node replay from persisted data.
- Document storage schema and migration policy, including incompatible-change handling.
- Prove snapshot creation, restore, and replay from restored state.
- Define pruning policy, retained data boundaries, restore expectations, and operator warnings.
- Capture evidence for corrupted or partial state handling where practical.

### v2.2.15 — Sustained P2P multi-node rehearsal

- Run sustained multi-node rehearsals with churn, delayed starts, restarts, temporary partitions, and rejoin events.
- Measure sync convergence, peer diagnostics, duplicate suppression, backoff, and chain-id isolation.
- Produce operator-readable evidence for lagging node recovery and multi-node final state agreement.
- Keep failure modes actionable through documented logs, metrics, and RPC responses.

### v2.2.16 — Miner/node contract hardening

- Stabilize the external miner/node contract for work retrieval, submission, error semantics, and operator diagnostics.
- Keep `pulsedag-miner` standalone and free of pool coordination logic.
- Document supported miner API compatibility expectations and deprecation policy.
- Track optional GPU work only as backlog unless the GPU path is canonical, deterministic at the contract boundary, and covered by evidence.

### v2.2.17 — API/operator/security hardening

- Document public, operator, and development RPC boundaries.
- Harden operator RPC behavior, error messages, rate-limit/auth expectations, and safe defaults.
- Review unsafe debug endpoints, sensitive fields, and accidental public exposure risks.
- Provide operator incident workflows that map RPC outputs to remediation steps.

### v2.2.18 — Private-testnet RC

- Assemble the private-testnet release-candidate evidence bundle.
- Verify multi-node, multi-miner, storage, snapshot/restore, pruning, replay, RPC, and release-artifact evidence.
- Close or explicitly waive non-blocking issues; do not waive Sev-1 consensus or sync issues.
- Produce a go/no-go checklist for the v2.3.0 readiness decision.

### v2.3.0 — Private-testnet readiness decision only

- Decide whether PulseDAG is ready for an official private testnet based on v2.2.12 through v2.2.18 evidence.
- Treat v2.3.0 as a decision and release-control milestone, not an automatic public launch.
- Publish the exact known limitations, operator requirements, rollback plan, and evidence index.

### v2.4.x — Private-testnet stable line

- Maintain the private-testnet stable line with conservative bug fixes and operator evidence updates.
- Avoid broad feature expansion that could destabilize consensus, sync, storage, or miner contracts.
- Track incidents, root causes, recovery times, and any required compatibility or migration notes.

### v2.5.x — Public-testnet preparation

- Prepare public-testnet operator documentation, bootstrap policy, monitoring expectations, and support channels.
- Verify release reproducibility and upgrade/rollback drills under private-testnet conditions.
- Define public communication, incident severity, and network reset policies before opening participation.

### v2.6.x — Public-testnet candidate and long soak

- Run a public-testnet candidate with a long soak period and explicit incident tracking.
- Require sustained multi-node and multi-miner evidence before promoting the line.
- Resolve Sev-1 consensus/sync incidents before moving toward protocol freeze.
- Exercise rollback, restore, and migration drills under public-testnet operating assumptions.

### v2.7.x — Protocol freeze

- Freeze consensus rules, P2P protocol boundaries, storage migration expectations, pruning policy, miner contract, and RPC boundaries.
- Permit only documented safety fixes, migration fixes, or release-process fixes after freeze.
- Require compatibility impact notes and rollback guidance for every accepted change.

### v2.8.x — v3.0 release candidates

- Produce one or more v3.0 release candidates with reproducible artifacts and signed evidence indexes.
- Rehearse upgrades from supported previous lines and rollbacks where rollback remains supported.
- Re-run snapshot/restore, replay, pruning, multi-node, multi-miner, and RPC boundary validation.
- Publish final operator documentation and known limitations before v3.0.0.

### v3.0.0 — Long-lived functional core

- Ship only when the core is stable enough to run for years under documented operating assumptions.
- Preserve compatibility and migration expectations unless a documented safety issue requires otherwise.
- Maintain release evidence, upgrade policy, rollback policy, and storage lifecycle policy as first-class release artifacts.

## v3.0.0 minimum gates

v3.0.0 must not ship unless all of the following are true:

- No unresolved Sev-1 consensus or sync incident remains open.
- A completed 30-day stable testnet burn-in exists with incident notes and final sign-off.
- Release artifacts are reproducible and documented.
- Upgrade and rollback policy is documented, including supported paths and explicit non-supported paths.
- Storage migration policy is documented, including schema/version handling and recovery expectations.
- Snapshot/restore evidence exists for supported operating paths.
- Multi-node and multi-miner evidence exists and is linked from release evidence.
- Public, operator, and development RPC boundaries are documented.

## Guardrails

- Do not add smart contracts before the stable core is proven.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep the miner external and standalone.
- Do not claim full Kaspa/GHOSTDAG compatibility unless implemented, tested, and documented.
- v2.3.0 is a readiness decision, not an automatic public launch.
- v3.0 must prefer durability, migration safety, reproducibility, and operator evidence over feature expansion.
