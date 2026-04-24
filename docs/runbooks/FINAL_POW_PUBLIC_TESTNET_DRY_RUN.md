# Final PoW public-testnet dry-run (go/no-go)

This runbook defines the **final readiness dry-run** to execute before opening the public testnet.

Scope is intentionally narrow:
- no pool logic,
- miner remains external,
- no speculative feature work,
- operator decision is strictly go/no-go for public testnet readiness.

## 1) Preconditions and freeze

1. Freeze node + miner commits and record SHAs.
2. Confirm active PoW algorithm and parameters match `docs/POW_SPEC_FINAL.md`.
3. Confirm miner contract remains external (`POST /mining/template`, `POST /mining/submit`) per `docs/MINER_FINAL.md`.
4. Confirm no pool endpoints, share accounting, or payout logic are introduced in this dry-run.
5. Create evidence directory (example run id):

```bash
scripts/release/generate_burnin_evidence.sh final-pow-dryrun-YYYYMMDD YYYY-MM-DD
mkdir -p artifacts/release-evidence/final-pow-dryrun-YYYYMMDD/dry-run
```

## 2) Dry-run topology (minimum serious shape)

Run for at least 24h continuously.

### Nodes
- **5 nodes total** across at least 2 availability zones/regions:
  - `seed-a`, `seed-b` (seed-capable)
  - `validator-c`, `validator-d`, `validator-e` (non-seed peers)
- Mixed restart domains (do not co-locate all nodes on one host).

### Miners (external only)
- **4 external miners minimum**:
  - `miner-1`, `miner-2` target `seed-a`
  - `miner-3` targets `validator-c`
  - `miner-4` targets `validator-e`
- Miners may fail over between nodes only by changing endpoint config.
- No coordinator/pool server is allowed.

### Observer
- 1 observer/collector instance scraping:
  - runtime status (`/runtime/status`),
  - alerts,
  - event stream extracts,
  - node logs with UTC timestamps.

## 3) Procedure (actionable sequence)

1. **Bootstrap**
   - Start 5 nodes.
   - Verify peer mesh stabilizes and `sync_lag_blocks` converges.
2. **Miner attach**
   - Start 4 external miners.
   - Verify template->submit loop works from each miner.
3. **Steady-state window (6h minimum inside the 24h run)**
   - Keep full topology stable.
   - Record block cadence and submit acceptance/rejection rates.
4. **Controlled restart checks**
   - Restart one non-seed node.
   - Restart one seed node.
   - Restart one miner process.
   - Confirm recovery and rejoin times stay within acceptance limits.
5. **Churn checks**
   - Disconnect one miner for 15 minutes, then reattach.
   - Isolate one non-seed node for 10 minutes, then rejoin.
   - Confirm no persistent fork/desync after rejoin.
6. **Recovery checks**
   - Force snapshot restore/rebuild drill on one node using runbook paths from `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
   - Confirm node catches up and re-enters normal propagation.
7. **Closeout capture**
   - Export metrics snapshots, incident log, and operator notes.
   - Fill go/no-go table and sign-off section.

## 4) Metrics to watch (and where)

1. **Chain progress & sync**
   - `sync_lag_blocks`, tip height convergence across all 5 nodes.
2. **Mining flow health**
   - template request success,
   - submit accepted vs rejected,
   - rejection taxonomy (stale/invalid/other).
3. **Network health under churn**
   - peer counts, reconnect success, time-to-rejoin.
4. **Recovery posture**
   - startup mode (fast-boot/replay/fallback),
   - restore/rebuild duration,
   - post-recovery tip convergence.
5. **Safety regressions (must stay zero unresolved)**
   - persistent desync,
   - repeated rebuild loop,
   - runaway orphan/mempool growth,
   - Sev-1 consensus/sync incidents.

## 5) Explicit pass criteria

All criteria must pass for **GO**:

1. 24h run completed with 5 nodes + 4 external miners.
2. Block production and propagation remain continuous through steady-state and perturbation checks.
3. No unresolved Sev-1 consensus/sync incident.
4. After each restart/churn event, all nodes reconverge and `sync_lag_blocks` returns to baseline range.
5. Mining submit rejection ratio does not show sustained unexplained increase.
6. Recovery drill completes and recovered node returns to healthy participation.
7. Evidence bundle includes topology map, timelines, metrics, and incident notes sufficient for independent audit.

If any criterion fails: **NO-GO** until corrected and re-run.

## 6) Failure handling and escalation

1. Declare incident with UTC timestamp and owner.
2. Freeze configuration changes unrelated to containment.
3. Follow relevant runbook from `docs/runbooks/INDEX.md`.
4. Record temporary mitigations and whether they alter test validity.
5. If mitigation changes consensus/miner behavior or introduces pool semantics, abort run and restart from clean baseline.
6. Maintain a single incident ledger in `dry-run/incident-log.md`.

## 7) Required evidence artifacts

Store under `artifacts/release-evidence/<run_id>/dry-run/`:

- `topology.md` (node/miner placement + endpoints)
- `timeline.md` (all drills with UTC start/end)
- `metrics-summary.md` (key counters + interpretation)
- `go-no-go.md` (final decision table)
- `incident-log.md` (all failures and handling)
- `raw/` (exported snapshots, charts, logs)

## 8) Go/no-go decision inputs

Decision owners should review at minimum:

1. Pass criteria table (all green required for GO).
2. Incident ledger with residual risk statement.
3. Recovery drill output and rejoin timings.
4. Mining rejection taxonomy review (proof no latent regression).
5. Evidence completeness check against `docs/RELEASE_EVIDENCE.md`.

## 9) Out of scope (do not add during this run)

- pool mining architecture,
- miner-node protocol redesign,
- consensus tuning experiments,
- unrelated operational refactors.

This run is a readiness gate, not an R&D window.
