# PulseDAG v2.2.7 Release Notes

## Summary

v2.2.7 is the final foundation release before the real private-testnet milestone. It closes the current PoW/mining/P2P acceptance foundation and hardens release framing for the next phase.

## What changed

- Workspace release version is bumped to **2.2.7**.
- v2.2.7 roadmap is finalized as a foundation-closing milestone.
- v2.3.0 roadmap alignment is clarified so private-testnet completion remains in v2.3.0.
- Smoke-test guidance is documented for minimal PoW/mining flow validation.

## What is intentionally not included

- No claim of production readiness.
- No smart contract functionality.
- No embedded pool logic inside the miner.
- No claim that full multi-node private-testnet validation is complete in v2.2.7.

## Upgrade notes

- Upgrade to v2.2.7 for documentation/version alignment and milestone closure.
- Keep miner deployment as an external standalone application.
- Keep pool/server-side coordination functionality on node/server side only.
- Plan operational readiness and full private-testnet validation under v2.3.0.

## Validation checklist

- [ ] `cargo fmt`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (if repository is currently clippy-clean)
- [ ] Manual smoke path completed (single-node template/submit + acceptance/rejection verification)
- [ ] Optional partial two-node peer connectivity check (if environment supports it)

## Next milestone: v2.3.0 private testnet

v2.3.0 remains the target for:

- Real private testnet execution.
- Complete P2P behavior and hardening.
- Multi-node active PoW operation through external miner + node interfaces.
- Block propagation, sync/recovery, and basic operator readiness.
