# PulseDAG v2.2.17 release notes

PulseDAG v2.2.17 opens as the API/operator/security hardening milestone after v2.2.16 miner/node contract hardening.

## Opening status

v2.2.17 starts from the v2.2.16 closeout line where external miner/node contract behavior, submit taxonomy, and miner diagnostics were stabilized.

v2.2.17 is not a v2.3.0 readiness claim. It is a focused hardening gate before the private-testnet RC and readiness decision milestones.

## Scope

v2.2.17 focuses on:

- RPC/API surface audit across node HTTP endpoints and operator-visible diagnostics.
- explicit endpoint classification as `public`, `operator`, or `admin`.
- admin endpoint lockdown defaults so privileged routes are not exposed unintentionally.
- local-only defaults for privileged/admin control surfaces unless explicitly overridden.
- optional operator authentication framing for deployments that need authenticated operator APIs.
- rate-limiting expectations for public and operator routes.
- request-size limits and safe payload bounding.
- explicit CORS policy defaults and operator override guidance.
- safe config validation for API security-sensitive toggles.
- diagnostics/log redaction guidance for sensitive fields.
- readiness/release endpoint hardening and minimum disclosure principles.
- an operator runbook for API/security hardening rollout.
- release evidence collection requirements for API/security hardening checks.

## Guardrails

- No smart contracts are added.
- No pool logic is added.
- No consensus-rule change is included.
- No PoW semantic change is included.
- Miner protocol changes are out of scope unless strictly required for security documentation clarity.
- v2.2.17 is not a v2.3.0 readiness claim and does not claim v3.0 readiness.

## Endpoint classification baseline

v2.2.17 requires a documented route inventory and category mapping:

- **Public**: externally callable read/query or submission endpoints intended for user/miner/app integration.
- **Operator**: privileged operational visibility endpoints for trusted infrastructure operators.
- **Admin**: sensitive mutating or control-plane endpoints that must default to local-only exposure and explicit enablement.

Minimum documentation expectations:

- each endpoint has method/path, category, default bind/access posture, and auth requirement.
- admin endpoints are clearly identified as disabled or local-only by default unless explicitly configured.
- readiness/release metadata endpoints avoid leaking internal topology, secrets, or nonessential build/runtime internals.

## Security hardening expectations

Before closing v2.2.17, hardening documentation and evidence should demonstrate:

- API route inventory completeness and endpoint ownership.
- bounded request body size policy.
- bounded request rate policy.
- safe CORS defaults (`deny by default` or explicit-allow policy) with documented override strategy.
- configuration validation errors for insecure/contradictory API settings.
- diagnostics and logs redact secrets, credentials, and sensitive identifiers where practical.
- operator runbook coverage for lock-down, auth rollout, rate-limit tuning, and incident response.

## Required validation before closeout

Before closing v2.2.17, collect output for:

```bash
cargo fmt --check
cargo test --workspace
```

The release evidence bundle should include command output plus API/operator/security documentation updates and checklist completion records under `evidence/v2.2.17/` when evidence capture is performed.

## Known limitations at opening

- v2.2.17 starts as a hardening/documentation milestone; full evidence collection may still be in progress.
- optional operator authentication remains deployment-sensitive and may be phased by environment.
- v2.3.0 remains a later private-testnet readiness decision.

## Operator documents

- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_17.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
- API reference baseline: `docs/API_V1.md`.
- Primary operator runbook: `docs/RUNBOOK.md`.
- Runbook index: `docs/runbooks/INDEX.md`.
