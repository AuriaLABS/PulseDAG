# Roadmap v2.2.7 — Foundation Closing Milestone

v2.2.7 is the **final foundation release** that closes the current PoW/mining/P2P groundwork before the full private-testnet milestone in v2.3.0.

## What v2.2.7 closes (completed foundation items)

- [x] PoW validation foundation.
- [x] Mining template RPC foundation.
- [x] Mining submit RPC foundation.
- [x] Block acceptance path calls PoW verification.
- [x] Basic P2P message/network foundation.
- [x] Preparation for private testnet.

## What is intentionally deferred to v2.3.0

- [ ] Full multi-node private testnet.
- [ ] Complete P2P sync/propagation hardening.
- [ ] End-to-end network burn-in.
- [ ] Operational dashboards/runbooks (where still incomplete).
- [ ] Full release-grade peer discovery/bootstrap flow (where still incomplete).

## Scope guardrails

- v2.2.7 does **not** claim production readiness.
- v2.2.7 does **not** add smart contracts.
- Miner remains an **external standalone application**.
- Pool/server-side coordination logic remains on node/server side, not inside the miner.
- v2.3.0 remains the milestone for complete P2P, multi-node PoW operation, sync/propagation, and operator readiness.
