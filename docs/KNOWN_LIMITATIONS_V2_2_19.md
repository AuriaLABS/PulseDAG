# Known Limitations v2.2.19

This document records **current known limitations** for `v2.2.19` and must be read together with release evidence and closeout artifacts.

## Scope and intent

- `v2.2.19` is a **private-testnet RC preparation** milestone.
- `v2.2.19` is **not** a `v2.3.0` readiness declaration.
- `v2.2.19` is **not** a public testnet launch.

## Consensus/DAG limitation

- DAG selection is deterministic in current implementation paths.
- Deterministic behavior alone does **not** imply full Kaspa/GHOSTDAG compatibility.
- Compatibility must only be asserted if and when canonical behavior is implemented and validated by explicit tests/evidence.

## Mining limitation

- GPU mining is optional for `v2.2.19`.
- GPU paths may be scaffold-only where no canonical kernel implementation is present.
- RC acceptance for `v2.2.19` must not depend on GPU availability unless a canonical kernel path and evidence are included in scope.

## RPC exposure limitation

- RPC endpoints must remain localhost/private by default.
- Any exposure beyond localhost/private requires explicit hardening, authentication/authorization controls, and operator sign-off.
- Until secured controls are proven, public RPC exposure is out of scope.

## Evidence-first limitation

- Private-testnet RC claims require reproducible evidence artifacts, not narrative claims.
- Restore/rebuild confidence depends on successful, repeatable drill evidence.
- Long-run stability confidence requires sustained rehearsal evidence across time, not single-run outcomes.

## Operator interpretation rule

When in doubt, interpret `v2.2.19` outcomes as **readiness signals for additional private rehearsal work**, not as proof of production/public-network readiness.
