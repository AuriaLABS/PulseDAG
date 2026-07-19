# Known Limitations v2.2.20

Date: 2026-07-19 UTC

This document records the final limitation state for the closed `v2.2.20` hardening milestone. It must be read with `docs/CLOSING_CHECKLIST_V2_2_20.md` and `docs/V2_2_20_FINAL_EVIDENCE_INDEX.md`.

## Scope and guardrails

- `v2.2.20` hardening is closed with `GO_TO_START_V2_3_0_REVIEW`.
- `VERSION` remains `v2.2.20`; the Cargo workspace remains `2.2.20`.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock has not started.
- This document does not authorize a version bump, public-testnet launch, smart-contract enablement, embedded pool logic, or a production-readiness claim.

## Closeout blockers resolved by final evidence

| Historical limitation | Final state | Replacement evidence |
|---|---|---|
| `5N/2M` accepted-block recovery | RESOLVED FOR CLOSEOUT | Strict PASS in run `29662737906`; archive sha256 `39fcd1168b65b6e4009b847cffa93b776a5c59012e38706c119612c463a44207`. |
| `5N/4M` stress recovery | RESOLVED FOR CLOSEOUT | Strict PASS in run `29662737906`; archive sha256 `77786930c5f78f4fdbb1703c6a0a68e18385c71009a373fe06066afc483c41bf`. |
| Snapshot/restore and offline rejoin confidence | RESOLVED FOR CLOSEOUT | `v2_3_0_prune_restart_rejoin_29662737906`, digest `sha256:097995cd80e2a371a229c664292c0a81d0e044f6b9d86494ebd566d4591275a0`. |
| Selected-segment continuation and retained-set convergence | RESOLVED FOR CLOSEOUT | `v2_3_0_lag_injection_29662737906`, digest `sha256:85362dcb69643ba657109f7b374c274fd09f199d3331b5cb839484a09c4add21`. |
| Final workspace validation | RESOLVED FOR CLOSEOUT | `v2_3_0_workspace_validation_29662737906`, digest `sha256:b44c550899cc3e3552df5cb3bd6dffb290cbcb8e649903c2d60884a6f5098aa1`. |

No waiver was required for these items.

## Remaining non-readiness limitations

The following remain outside the closed `v2.2.20` hardening scope and must not be interpreted as active closeout blockers or as completed readiness work.

| Area | Current limitation | Required future evidence |
|---|---|---|
| Public RPC exposure | RPC remains private/localhost-oriented; public exposure has not been authorized. | Security review, authentication/authorization posture, listener and firewall configuration, secret-redaction review, abuse controls and operator sign-off. |
| Public-testnet launch | Private five-node rehearsals do not authorize a public launch. | Separate launch GO/NO-GO with bootnodes, network isolation, deployment, rollback, monitoring and incident-response evidence. |
| 30-day burn-in | The clock has not started. | At least 30 consecutive UTC days after an explicitly authorized public-testnet launch. |
| Kaspa/GHOSTDAG compatibility | Deterministic DAG behavior does not prove full compatibility. | Canonical compatibility implementation and dedicated interoperability evidence. |
| GPU mining | GPU support remains optional/scaffold-level unless separately proven. | Canonical tested kernel, deterministic validation and operational miner evidence. |
| `v2.3.0` version bump | Review may start, but the bump is not authorized by the closeout. | Separate explicit maintainer approval after review of scope, release controls and remaining non-readiness work. |

## Incident and waiver status

The decision-scoped ledger is `artifacts/v2_2_20/closeout_decision/incident_waiver_ledger.md`.

- Unresolved Sev-1 closeout blockers: none identified.
- Closeout waivers: none.
- Accepted bounded limitations required for closeout: none; all mandatory gates passed.

## Decision reference

The final decision is `GO_TO_START_V2_3_0_REVIEW`, recorded in `artifacts/v2_2_20/closeout_decision/final_decision.md`.

This is permission to begin review, not a claim that `v2.3.0` or a public testnet is ready.
