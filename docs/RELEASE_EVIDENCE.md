# Release Evidence Policy (v2.2.18 private-testnet RC preparation)

## Status gate
- Current status: **PLANNED / BLOCKED BY v2.2.17 EVIDENCE**.
- No v2.2.18 rehearsal execution evidence is expected until v2.2.17 evidence is complete.

## Expected artifact directory
- Preferred: `artifacts/v2_2_18_private_testnet_rc/<run_id>/`
- Alternate: `evidence/v2.2.18/<run_id>/`

## Required evidence groups (when unblocked)
1. Topology manifest (`topology/manifest.yaml`) for 5-node / 4-miner layout.
2. Deterministic startup/shutdown logs (`orchestration/startup.log`, `orchestration/shutdown.log`).
3. Sync convergence outputs (`sync/convergence_summary.md`, raw samples).
4. Miner acceptance/rejection telemetry (`miners/accept_reject_summary.md`, per-miner logs).
5. Snapshot/restore drill outputs (`snapshot/restore_timing.csv`, notes).
6. Perturbation drill outputs (`perturbation/*.md`, timestamps and recovery metrics).
7. RPC security verification outputs reusing v2.2.17 controls/scripts.
8. `summary.md` + `go_no_go_report.md` + compressed bundle.

## Pass/fail table template
| Item | Evidence path | Status |
|---|---|---|
| v2.2.17 gate complete | `docs/CLOSING_CHECKLIST_V2_2_17.md` + artifact links | PENDING |
| Topology manifest | `topology/manifest.yaml` | PENDING |
| Deterministic startup/shutdown | `orchestration/startup.log`, `orchestration/shutdown.log` | PENDING |
| Sync convergence | `sync/convergence_summary.md` | PENDING |
| Miner acceptance/rejection | `miners/accept_reject_summary.md` | PENDING |
| Snapshot/restore drill | `snapshot/restore_timing.csv` | PENDING |
| Perturbation drills | `perturbation/summary.md` | PENDING |
| RPC security verification reuse | `security/rpc_security_reuse_summary.md` | PENDING |
| Go/no-go report | `go_no_go_report.md` | PENDING |

## Evidence bundle naming
- `v2_2_18_private_testnet_rc_evidence_<run_id>.tar.gz`

## Guardrail note
- v2.2.18 evidence package must not claim consensus/PoW changes, smart contracts, pool logic additions, or v2.3.0 readiness.

## v2.3.0 decision handoff note
- v2.2.18 closeout must publish `docs/V2_3_0_READINESS_DECISION_INPUTS.md` as a decision-review input summary.
- If evidence is missing, mark the corresponding item as **PENDING**.
- Any unresolved Sev-1 consensus/sync issue requires a **NO-GO** recommendation.
