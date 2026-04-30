# Public testnet readiness criteria (preparation scope)

These criteria define operational readiness for a later public-testnet decision.

Scope guardrails:
- no consensus changes,
- miner stays external and standalone,
- no pool logic,
- operations/release readiness only.

## A. Explicit auditable criteria (stricter thresholds)

For each item, record: owner, UTC start/end, evidence path, pass/fail, and waiver ID (if any).

1. **Topology and continuity**
   - Minimum dry-run shape completed: **5 nodes + 4 external miners for at least 24 contiguous UTC hours**.
   - Allowance: at most 1 planned operator maintenance window <= 15 minutes; all unplanned interruptions are failures.
   - Evidence: `artifacts/release-evidence/<run_id>/dry-run/topology.md` + `timeline.md`.
2. **Sync reconvergence under perturbation**
   - At least **4 perturbation events** (restart/churn/isolation mix); each event must show `sync_lag_blocks` returning to pre-event baseline band within **30 minutes**.
   - A single event may be marked conditional only with linked root cause and successful re-test.
   - Evidence: `dry-run/timeline.md`, `metrics-summary.md`, and raw captures.
3. **Recovery execution confidence**
   - Snapshot restore/rebuild drill completes on at least **1 non-seed node** and rejoins healthy propagation within **45 minutes** of restore completion.
   - Evidence must include lineage reference and repeated-run comparison notes.
   - Evidence: `restore-rebuild/restore-timing.csv` + dry-run recovery notes.
4. **External miner health stability**
   - Submit acceptance/rejection trend shows no unresolved degradation for >= 2 consecutive hours.
   - Threshold: stale/invalid/reject classes may spike during drills, but must recover to pre-drill baseline band within **30 minutes**.
   - Evidence: `mining-telemetry/daily-summary.csv` + `metrics-summary.md`.
5. **Safety incident threshold**
   - **Zero unresolved Sev-1 consensus/sync incidents** at closeout, and zero Sev-1 older than 24h without a published mitigation timeline.
   - Evidence: `runtime-alerts/alerts.csv` + `dry-run/incident-log.md`.
6. **Evidence completeness and traceability**
   - 100% of required evidence paths in `docs/RELEASE_EVIDENCE.md` present, reviewable, and linked from `CHECKLIST.md`.
   - Missing timestamp/owner metadata counts as incomplete evidence.
   - Evidence: `CHECKLIST.md` completeness sign-off.

## B. Staging drill set (must be practical)

1. **Bootstrap + attach drill**
   - Bring up 5 nodes, attach 4 external miners, validate tip progression.
2. **Restart drill**
   - Restart one seed and one non-seed node; measure time-to-healthy.
3. **Miner churn drill**
   - Disconnect one miner for 15 minutes; reattach and validate submit recovery.
4. **Peer isolation drill**
   - Isolate one non-seed node for 10 minutes; verify rejoin and no persistent desync.
5. **Restore/rebuild drill**
   - Run snapshot restore/rebuild on one node and confirm reconvergence.

All staging drills require UTC timestamps, owner attribution, pass/fail result, and follow-up action if failed.

## C. Decision posture

- Passing this checklist means **ready for public-testnet decision review**, not launch.
- Any failed criterion or missing evidence keeps status at **not ready / no-go**.

## D. Drill scoring model (practical + auditable)

Score each required drill 0-2 and record rationale in `dry-run/go-no-go.md`:
- **2 = pass**: threshold met with complete evidence.
- **1 = conditional**: threshold narrowly missed, but root cause + mitigation + successful re-test are all present.
- **0 = fail**: threshold missed without successful re-test or evidence is incomplete.

Required drills: bootstrap+attach, restart, miner churn, peer isolation, restore/rebuild.

Scoring gates:
- Total score must be **>= 8/10**.
- No individual drill may score 0.
- Any conditional (score 1) requires explicit waiver owner + UTC expiry.

## E. No-go criteria (hard stop)

Status remains **NO-GO** if any are true:
- Any criterion in section A is failed or missing evidence.
- Drill score total < 8/10, any drill scored 0, or conditional drill lacks successful re-test evidence.
- Unresolved Sev-1 consensus/sync incident exists at closeout.
- Recovery/rejoin thresholds are not met within declared bounds.
- External miner degradation remains unresolved at closeout.

Passing all criteria indicates **ready for public-testnet decision review only** (not launch authorization).
