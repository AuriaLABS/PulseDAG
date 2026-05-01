# Roadmap v2.2.7 — Clean Foundation Closure

v2.2.7 is the **clean foundation-closing release** before the v2.3.0 private-testnet milestone. Its purpose is to align the repository version, release documentation, PoW/mining boundary, P2P foundation status, and validation expectations without claiming that the full private testnet is complete.

## Release positioning

- **Current release:** v2.2.7.
- **Next hardening line:** v2.2.8 may continue pre-testnet hardening if needed.
- **Next major milestone:** v2.3.0 remains the first complete private-testnet readiness milestone.
- **Production status:** not production-ready.
- **Testnet status:** not a public testnet and not yet the official complete private testnet.

## Version alignment

- [x] Workspace package version is aligned to `2.2.7`.
- [x] Workspace members inherit the workspace package version through `version.workspace = true` where applicable.
- [x] v2.2.7 docs identify this release as a foundation closure, not as v2.3.0.
- [x] v2.3.0 docs remain the target for complete private-testnet readiness.

## What v2.2.7 closes

- [x] PoW validation foundation.
- [x] Mining template RPC foundation.
- [x] Mining submit RPC foundation.
- [x] Block acceptance path calls PoW verification.
- [x] Basic P2P message/network foundation.
- [x] External-miner boundary is preserved.
- [x] Release notes and smoke-test documentation exist for manual closure checks.
- [x] Repository framing is aligned for the next private-testnet phase.

## Acceptance expectations for closing v2.2.7

Before tagging or treating v2.2.7 as closed, maintainers should verify:

- [ ] `cargo fmt --check`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` if the repository is clippy-clean in the current environment.
- [ ] Manual single-node mining smoke path from `docs/SMOKE_TEST_V2_2_7.md`.
- [ ] Optional basic peer-connectivity check where local environment supports it.

## What is intentionally deferred to v2.3.0

- [ ] Full multi-node private testnet.
- [ ] Complete P2P sync/propagation hardening.
- [ ] Multi-node active PoW operation over a real private topology.
- [ ] End-to-end network burn-in.
- [ ] Operational dashboards and runbooks where still incomplete.
- [ ] Release-grade peer discovery/bootstrap flow where still incomplete.
- [ ] Snapshot/prune/restore operational drills if not already covered elsewhere.

## Scope guardrails

- v2.2.7 does **not** claim production readiness.
- v2.2.7 does **not** add smart contracts.
- v2.2.7 does **not** enable a smart-contract runtime.
- The miner remains an **external standalone application**.
- Pool/server-side coordination logic remains on the node/server side, not inside the miner.
- v2.3.0 remains the milestone for complete P2P, multi-node PoW operation, sync/propagation, and operator readiness.
