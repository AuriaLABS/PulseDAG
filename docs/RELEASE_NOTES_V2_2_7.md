# PulseDAG v2.2.7 Release Notes

## Summary

v2.2.7 is the clean foundation-closing release before the real private-testnet milestone. It aligns the repository around the current PoW/mining/P2P foundation, preserves the external miner boundary, and documents what is ready to validate manually before v2.3.0.

This release should be treated as a **pre-private-testnet foundation release**, not as production software and not as the official full private testnet.

## Version alignment

- Workspace package version: `2.2.7`.
- Workspace members inherit the workspace package version where they use `version.workspace = true`.
- Release documentation consistently identifies v2.2.7 as the foundation closure before v2.3.0.
- v2.3.0 remains the target for complete private-testnet readiness.

## What changed

- Finalized v2.2.7 roadmap as a clean foundation-closing milestone.
- Clarified the release boundary between v2.2.7 and v2.3.0.
- Documented the manual smoke path for PoW/mining validation.
- Kept the miner as an external standalone application.
- Kept pool/server-side coordination out of the miner.
- Clarified that full multi-node private-testnet validation belongs to v2.3.0.

## What v2.2.7 is expected to cover

- PoW validation foundation.
- Mining template RPC foundation.
- Mining submit RPC foundation.
- Block acceptance path invoking PoW verification.
- Basic P2P message/network foundation.
- Manual smoke documentation for minimal validation.

## What is intentionally not included

- No production-readiness claim.
- No public testnet claim.
- No official full private-testnet completion claim.
- No smart contract functionality.
- No smart-contract runtime enablement.
- No embedded pool logic inside the miner.
- No guarantee of complete P2P propagation/sync/recovery burn-in.

## Upgrade notes

- Upgrade to v2.2.7 for version/documentation alignment and foundation closure.
- Keep miner deployment as an external standalone application.
- Keep pool/server-side coordination functionality on the node/server side only.
- Use the v2.2.7 smoke test to verify the basic local mining acceptance path.
- Plan operational readiness, multi-node propagation, sync/recovery, and burn-in under v2.3.0.

## Validation checklist

Before tagging or closing the release, verify:

- [ ] `cargo fmt --check`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` if the repository is currently clippy-clean.
- [ ] Manual single-node mining smoke path from `docs/SMOKE_TEST_V2_2_7.md`.
- [ ] Optional partial two-node peer-connectivity check if the local environment supports it.

## Known limitations

- Multi-node private-testnet execution is not complete in v2.2.7.
- P2P propagation, sync/recovery, and burn-in still require the v2.3.0 workstream.
- Operator dashboards/runbooks may still need expansion before a real private testnet.
- This release closes foundation scope only.

## Next milestone: v2.3.0 private testnet

v2.3.0 remains the target for:

- Real private testnet execution.
- Complete P2P behavior and hardening.
- Multi-node active PoW operation through external miner + node interfaces.
- Block propagation, transaction propagation, sync/recovery, and basic operator readiness.
