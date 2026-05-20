# PulseDAG v2.2.17 release notes

PulseDAG v2.2.17 is finalized as the API/operator/security hardening closeout following v2.2.16 miner/node contract hardening.

## Finalized status

v2.2.17 is **closed out** as an API/operator/security boundary-hardening milestone with required documentation, validation, and release evidence expectations captured in the v2.2.17 checklist.

This release does **not** claim v2.3.0 readiness and does **not** claim v3.0 readiness.

## Closed scope summary

v2.2.17 closeout scope is:

- RPC endpoint inventory completion.
- API profile classification and validation (`public`, `operator`, `admin`).
- Admin disabled-by-default posture and unsafe admin exposure blocking.
- Optional operator authentication behavior documented and tested.
- Request-rate and request-size controls documented and tested.
- CORS and bind-address policy documented and tested.
- Diagnostics/logging redaction behavior documented and tested.
- `/release` and `/readiness` endpoint hardening and disclosure minimization.
- Secure operator runbook completion.
- RPC security smoke validation and API security evidence bundle generation.

## Required closeout validation

The v2.2.17 closeout gate requires evidence for:

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace --release
```

and accompanying API/operator/security validation artifacts (inventory, auth/admin posture checks, limit/policy tests, redaction tests, endpoint hardening checks, and smoke-script output).

## Guardrails preserved in v2.2.17

- No smart contracts added.
- No pool logic added.
- Miner remains external.
- No consensus rule changes.
- No PoW semantic changes.
- No GPU kernel changes in v2.2.17.
- No readiness claim for v2.3.0 or v3.0.

## Release position in roadmap

- v2.2.16: miner/node contract hardening (completed predecessor).
- v2.2.17: API/operator/security hardening (this closeout).
- v2.2.18: private-testnet RC packaging and go/no-go evidence for the v2.3.0 readiness decision.

## References

- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_17.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
- API baseline: `docs/API_V1.md`.
- Operator runbook: `docs/RUNBOOK.md`.
