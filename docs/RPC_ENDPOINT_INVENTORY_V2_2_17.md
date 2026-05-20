# RPC Endpoint Inventory v2.2.17

## Security model alignment
- Admin disabled by default.
- Localhost-first RPC posture.
- Optional operator auth token supported.
- No recommendation to expose admin RPC publicly.
- SSH tunnel recommended for remote operator access.

## Exposure classes
| Class | Description |
|---|---|
| public_safe | Read-only low-risk endpoints (e.g., `/health`). |
| operator | Operational endpoints for trusted operators/private networks. |
| admin | Privileged endpoints; must remain blocked by default/public-internet. |
| dev | Non-production diagnostics/testing surfaces. |

## Closeout-required endpoint checks
- `/health`
- `/status`
- `/release`
- `/readiness`
- Admin probe examples: `/admin/runtime`, `/runtime`, `/admin/diagnostics`, `/diagnostics`

These checks are executed by `scripts/v2_2_17_rpc_security_smoke.sh` and collected by `scripts/v2_2_17_collect_api_security_evidence.sh`.
