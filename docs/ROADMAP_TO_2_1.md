# Roadmap to v2.1.0

## Objective
Deliver **v2.1.0** as the first post-testnet hardening and operator-quality release for PulseDAG v2, focused on reliability, observability, and safe throughput improvements while preserving consensus behavior.

## Scope guardrails
- **No consensus-rule breaking changes** in the v2.1 line.
- **No smart-contract runtime enablement** in v2.1.
- Keep miner as an **external application**; improve only node↔miner integration and safety controls.

## Streams and deliverables

### 1) Network reliability + sync recovery
- Improve peer scoring and backoff behavior under churn.
- Add deterministic sync checkpoints for faster catch-up after restarts.
- Validate 3–5 node partition/rejoin behavior with measurable recovery SLO.

### 2) Mempool + transaction propagation quality
- Tighten mempool eviction policy under sustained pressure.
- Add anti-spam guardrails (rate caps + validation fast-fail path).
- Improve gossip convergence visibility (drop reasons, rebroadcast counters).

### 3) Storage durability + ops safety
- Harden snapshot restore verification with integrity checks.
- Add safer pruning defaults and explicit operator warnings on risky settings.
- Publish backup/restore playbook and periodic restore drill checklist.

### 4) Miner integration + PoW stability
- Stabilize template freshness and stale-work rejection signaling.
- Add miner-side retry/jitter defaults for smoother load.
- Validate multi-miner fairness and orphan-rate envelope under soak.

### 5) API + observability
- Expand RPC diagnostics for sync, mempool, and mining-template lifecycle.
- Publish production dashboard baseline (latency, orphan rate, peer health, mempool pressure).
- Finalize v2.1 operator runbook with incident triage paths.

## Milestones
1. **M1 — Scope freeze + metrics baseline (Week 1–2)**
   - Lock v2.1 non-goals and acceptance criteria.
   - Capture baseline from current testnet behavior.
2. **M2 — Feature complete (Week 3–6)**
   - Land reliability, mempool, storage, and RPC observability changes.
3. **M3 — Hardening + burn-in (Week 7–10)**
   - 14-day uninterrupted soak with no Sev-1 incidents.
4. **M4 — Release readiness (Week 11–12)**
   - Final docs, upgrade notes, rollback drill, and release tag.

## Exit criteria for v2.1.0
1. 14-day burn-in passes with no unresolved consensus/sync Sev-1 defects.
2. Snapshot restore drill succeeds on current state volume with documented RTO.
3. Peer churn/partition tests meet defined recovery SLO.
4. Operator dashboard and runbook published and reviewed.
5. Upgrade + rollback procedure validated on staging before tag.

## Out of scope for v2.1
- Consensus redesign.
- Contract execution engine activation.
- Pool/accounting logic inside miner.
