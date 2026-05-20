# Operator Security Runbook v2.2.17

## Posture
- Admin endpoints are disabled by default and must never be publicly exposed.
- RPC should be localhost-first (`127.0.0.1`) unless explicitly hardened behind trusted controls.
- Operator auth token is optional but supported where configured.
- Remote operator access should use SSH tunneling instead of direct internet exposure.

## Endpoint classes
- `public_safe`: minimal read-only endpoints safe for broad usage.
- `operator`: operational endpoints for trusted/private network use.
- `admin`: privileged endpoints (state-changing or sensitive diagnostics); never public.
- `dev`: diagnostic/test helpers, non-production.

## Recommended remote access
`ssh -L 18080:127.0.0.1:18080 <host>` then access local forwarded port.

## Validation workflow
1. Run `bash scripts/v2_2_17_rpc_security_smoke.sh`.
2. Run `bash scripts/v2_2_17_collect_api_security_evidence.sh`.
3. Attach generated summary and tarball to release evidence record.
