# v2.1 14-day burn-in execution guide

This document defines the **real** v2.1 release burn-in process.

## Non-negotiable guardrails
- Do **not** change consensus during the 14-day run.
- Do **not** change miner behavior during the 14-day run.
- CI workflows (including short soak jobs) are supporting signals only and **do not prove** a full 14-day burn-in.

## What CI covers vs what release burn-in covers
- `Soak Smoke (short CI signal)` workflow: a short health loop to catch obvious regressions quickly.
- 14-day release burn-in: a continuously operated testnet/staging environment with operator monitoring, incident tracking, and formal evidence collection.

## Required 14-day execution model
1. Freeze candidate bits for node + miner and record commit SHAs.
2. Run continuous network operation for **14 consecutive days**.
3. Capture on-call/incident evidence for runtime alerts and recoveries.
4. Run snapshot and pruning operations on the planned production cadence.
5. Execute planned restart and recovery drills.
6. Record p2p recovery timings during churn/rejoin events.

## Required evidence categories
Every day of the 14-day run must be represented in evidence under these categories:
1. Runtime alerts
2. Snapshot cadence
3. Pruning cadence
4. P2P recovery stats
5. Restart/recovery notes

Use `docs/RELEASE_EVIDENCE.md` for the expected folder structure and acceptance checklist.

## Pass/Fail criteria for release managers
A v2.1 burn-in is considered complete only when all of the following are true:
- 14 full days completed with no unresolved Sev-1 incident tied to consensus/sync safety.
- Evidence bundle is complete for all required categories and days.
- Restart + recovery notes include resolution and follow-up actions.
- Snapshot/pruning cadence was run as configured and observed stable.
- Release manager sign-off is attached to the evidence bundle.
