# v2.2.20 incident and waiver ledger

Date: 2026-07-19 UTC

Evaluated candidate: `e65c6c199e07214303b49f7863f5b4988a8ce107`

Final Actions run: `29662737906`

## Incident review

The closeout review checked:

- final gate manifests and blockers from workspace, staged network, relay, lag injection and prune/restart/rejoin;
- PR `#755` review state and unresolved review threads;
- open repository issues matching `Sev-1`, `severity 1`, `blocker`, and the combined terms `consensus sync security`.

Result: no unresolved Sev-1 consensus, sync, storage or security blocker was identified for the `v2.2.20` hardening closeout.

## Waiver ledger

| Gate | Waiver | Reason |
|---|---|---|
| Workspace validation | None | PASS |
| `5N/1M` | None | PASS |
| `5N/2M` | None | Replacement PASS evidence attached |
| `5N/4M` | None | Replacement PASS evidence attached |
| Mempool/relay | None | PASS |
| Selected-segment lag recovery | None | PASS |
| Prune/restart/rejoin | None | PASS |
| Public-testnet readiness | Not applicable | Not claimed or authorized |

No limitation was converted into a PASS by waiver. All mandatory closeout gates passed on the evaluated candidate.

## Guardrails

- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock has not started.
- This ledger does not authorize a version bump, public-testnet launch, release publication or readiness claim.
