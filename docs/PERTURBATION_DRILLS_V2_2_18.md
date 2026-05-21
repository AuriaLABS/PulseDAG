# Perturbation Drills — v2.2.18 (Private RC)

This document defines controlled perturbation drills for v2.2.18 private RC readiness.

## Scope and goals

The suite prepares for public readiness expectations (5 nodes + 4 external miners over 24h, at least 4 perturbations, and restore/rebuild on one non-seed node) while staying compatible with private RC constraints.

Required drills:
1. Restart one seed node.
2. Restart one non-seed node.
3. Stop one miner for 15 minutes and restart it.
4. Isolate one node temporarily if practical.
5. Restart all miners.

Target thresholds (aligned with later readiness criteria):
- Sync reconvergence within declared window.
- Miner submit recovery after restart.
- No persistent fork/divergence.
- No unresolved Sev-1 consensus/sync issue.

## Safety and constraints

- Do not require privileged firewall manipulation unless optional.
- Do not fake isolation evidence.
- Do not modify consensus or P2P protocol behavior.

The isolation drill is optional-by-default and only runs when explicit environment hooks are supplied.

## Script

- Runner: `scripts/v2_2_18_run_perturbation_drills.sh`
- Output root: `artifacts/perturbation_drills_v2_2_18/<UTC_TIMESTAMP>/`
- Core outputs:
  - `perturbation_drills.log`
  - `drill_summary.csv`
  - `evidence/*.md` (one evidence record per drill)

## Required evidence fields per drill

Every drill must record:
- UTC start
- UTC end
- affected process
- expected result
- observed result
- recovery time
- pass/fail
- evidence path

The script writes these fields into both:
- `drill_summary.csv`
- individual Markdown evidence files under `evidence/`

## How to run

Example:

```bash
SEED_NODE_SERVICE=pulsedagd-seed-1 \
NON_SEED_NODE_SERVICE=pulsedagd-node-2 \
MINER_SERVICE=pulsedag-miner-1 \
MINER_GROUP_SERVICES="pulsedag-miner-1 pulsedag-miner-2 pulsedag-miner-3 pulsedag-miner-4" \
SYNC_RECONVERGENCE_WINDOW_SECONDS=600 \
MINER_SUBMIT_RECOVERY_WINDOW_SECONDS=300 \
bash scripts/v2_2_18_run_perturbation_drills.sh
```

Optional temporary isolation drill execution:

```bash
ISOLATION_TARGET=pulsedagd-node-3 \
ISOLATION_CMD='sudo /usr/local/bin/isolate_node3_temporarily.sh' \
RESTORE_CMD='sudo /usr/local/bin/restore_node3_connectivity.sh' \
bash scripts/v2_2_18_run_perturbation_drills.sh
```

If `ISOLATION_CMD` and `RESTORE_CMD` are not provided, drill 4 is explicitly marked `SKIP` with a reason (no fake evidence).

## Pass/fail interpretation

- **PASS**: recovery marker observed within the declared timeout window.
- **FAIL**: marker not observed within timeout; investigate node/miner logs and incident status.
- **SKIP**: optional isolation hooks not provided safely.

Suggested follow-ups for FAIL outcomes:
- verify chain-head/hash parity between seed and non-seed nodes,
- inspect miner submit/accepted cadence after restart,
- confirm incident tracker has no unresolved Sev-1 sync/consensus item in the drill window.

## Minimal validation command

```bash
bash -n scripts/v2_2_18_run_perturbation_drills.sh
```
