# PulseDAG v2.2.9 Release Notes

## Summary

v2.2.9 closes the private-testnet rehearsal release. It is an execution rehearsal milestone before the official v2.3.0 private-testnet readiness decision.

## v2.2.8 hardening baseline

v2.2.8 established the ambitious hardening baseline: version/reporting alignment, operational observability framing, and pre-testnet readiness preparation.

## v2.2.9 private-testnet rehearsal

v2.2.9 validates rehearsal-level multi-node and miner-driven operations with explicit guardrails:

- No public testnet claim.
- No production readiness claim.
- External miner remains mandatory.

## v2.3.0 official private testnet

v2.3.0 remains the official complete private-testnet readiness milestone and sign-off target.

## What changed

- Multi-node rehearsal profiles.
- Start/stop/status scripts for rehearsal operations.
- External miner rehearsal flow against node A.
- Kaspa-based kHeavyHash PoW engine integration path (if merged in this line).
- Block propagation rehearsal validation.
- Sync/restart/catch-up rehearsal validation.
- Network status and runtime visibility rehearsal.
- Rehearsal acceptance tests.
- External server rehearsal runbook.

## What is intentionally not included

- Public testnet rollout.
- Production-readiness declaration.
- Smart contracts.
- Pool logic in miner.
- Long burn-in completion claim.

## Known limitations

- Rehearsal evidence is environment-dependent and may remain partially manual.
- Full private-testnet launch gates and long-duration stability confidence are deferred to v2.3.0.
- Some propagation/catch-up checks may rely on log/observability evidence where deterministic counters are still being hardened.

## Upgrade notes

- Align `VERSION` to `v2.2.9` and workspace version to `2.2.9`.
- Rebuild binaries and confirm `/status` and `/release` report `v2.2.9`.
- Run smoke test and rehearsal checklists before closeout.

## Next milestone: v2.3.0

Proceed to v2.3.0 for official complete private-testnet readiness closure, extended burn-in confidence, and launch-gate sign-off.
