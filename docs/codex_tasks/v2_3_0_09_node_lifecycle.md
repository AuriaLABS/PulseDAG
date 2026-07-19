# v2.3.0 Task 09 — Node lifecycle and safe release switching

## Goal

Provide one supported, idempotent operator interface for installing, starting, stopping, inspecting, upgrading, restarting, and rolling back a private-testnet node.

## Deliverables

- A Python lifecycle controller with a thin shell wrapper.
- Immutable release directories with SHA-256 manifests.
- Atomic `current` and `previous` release links.
- Persistent PID, log, lock, and JSON state files.
- Task 07 preflight integration before process start.
- DNS bootnode resolution before ordinary-node startup.
- Managed-directory ownership and permission enforcement.
- PID reuse protection before sending signals.
- Health-gated upgrades with automatic rollback.
- Idempotent start and stop semantics.
- A regression test and dedicated Actions evidence gate.
- English comments, docstrings, diagnostics, and operator documentation.

## Acceptance criteria

1. Installing the same release identifier with identical bytes is idempotent.
2. Reusing a release identifier with different bytes fails.
3. Starting an already-running managed node succeeds without creating a second process.
4. Stopping an already-stopped node succeeds without error.
5. A healthy upgrade retains the old release as `previous`.
6. An explicit rollback swaps `current` and `previous` and restores health.
7. An unhealthy upgrade restores the previous release automatically.
8. A live PID that does not match recorded Linux process start ticks is never signalled.
9. Bootnode DNS resolution and Task 07 preflight run before startup.
10. Repository hygiene, Lint, RPC/release, and pre-burn-in checks pass.

## Guardrails

- No consensus, PoW, storage-format, miner protocol, or RPC behavior change.
- No embedded pool logic or smart-contract runtime.
- No version bump, tag, artifact publication, or release-readiness claim.
- No public-testnet launch authorization.
- `public_testnet_ready=false` and an unstarted 30-day clock remain mandatory.

## Follow-up

Task 10 adds the versioned metrics inventory, scrape configuration, dashboards, and alert thresholds used by multi-host operators.
