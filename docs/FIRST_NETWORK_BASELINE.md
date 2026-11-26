# PulseDAG first-network baseline

PulseDAG has no previously deployed network. The first public network is native v3 testnet, followed by independently authorized mainnet. Software version v3.0.0 is not a network migration.

## Active authority

- [Launch roadmap](ROADMAP_V3_0_0.md), [monetary policy](MONETARY_POLICY_V3_0_0.md), [testnet-first runbook](runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md).
- [Launch control #781](https://github.com/AuriaLABS/PulseDAG/issues/781), [cleanup #1524](https://github.com/AuriaLABS/PulseDAG/issues/1524).
- Native monetary authority #1466/#1410, recovery #1465, identity #1049, public testnet #1418, bounded contract core #1333.

## Cleanup rules

1. No legacy network compatibility, migration, private live network or old chain identity is required. Historical source remains recoverable through Git history.
2. Consolidate applicable requirements from retired prototype roadmaps and activation documents before removing their active copies and references. Historical installation, activation, private-testnet and release checklists must not be active operator instructions.
3. Replace obsolete runtime paths with native equivalents before deleting them. Preserve independent cryptographic vectors, invalid-input tests and security controls; adapt fixtures to the first-network protocol.
4. Close superseded issues and PRs only with explicit replacement links and retained evidence. Do not treat administrative closure as technical acceptance.
5. Validate links, configs, scripts, full CI and startup boundaries on each cleanup candidate. No activation changes in documentation-only cleanup.

## Retained first-network invariants

The prototype roadmap and activation contract are superseded, not launch evidence. Preserve the following safety requirements in the native implementation:

- Freeze chain ID, genesis, canonical header/signing domains, PoW/retarget rules, consensus/order policy and storage/snapshot schema as one reviewed candidate identity. Testnet and mainnet identities are distinct. Reject mismatched persisted state and peers before admission; never infer compatible consensus from a profile name.
- Derive selected parents, blue/red sets, checked work, total order and monetary state independently of arrival, peer and iteration order. Bound ancestry/classification costs and reject incomplete or forged history. Restore only authenticated state; verify exact replay equivalence, finality and rollback.
- Bind templates, miner submissions, transaction admission, wallet signatures, RPC and P2P to the same network identity. Reject unsupported wire versions without fallback; keep retry identity distinct from txid and wallet-plan identity. Preserve the no-RBF contract and exact accepted-transaction fees/rewards. Keep private keys outside the node.
- Keep ordinary isolated/degraded nodes fail closed; local test profiles are explicit and cannot authorize public launch. Require bounded control-plane liveness, submit-finality reconciliation, missing-parent recovery and resource limits.
- Preserve exact-candidate regression vectors, permutations, cross-chain negatives, invalid work/signature tests, restart/rejoin/prune/restore, public RPC abuse tests and reproducible package verification. Apply the T0/T1/M0 sequence in the launch runbook rather than a prototype release gate.
- A change to canonical bytes, network identity, work/order, money/state, persistence, consensus P2P or miner semantics invalidates affected evidence. Do not combine different candidate identities to manufacture a PASS. Recovery after launch uses verified compatible state or a separately reviewed repair; testnet resets follow the published reset policy. No predecessor network migration is required.
- Keep credentials, private keys, seeds and local runtime state out of Git. No implicit activation through time, height, flags or documentation; only #781 authorizes the corresponding public launch.

## Consolidated prototype regression obligations

The former version-specific Codex tasks, retired PowerShell examples, and prototype release notes are provenance only. Preserve their applicable requirements without retaining old deployment instructions:

- P2P: bounded and deduplicated orphan-parent recovery, rate-limit-aware retries, fair provider failover, authenticated header inventory and selected-segment recovery, bounded queues and expiry, explicit degraded state, observable convergence and deterministic recovery after isolation/restart. Preserve delayed-parent, partition/rejoin and prune/snapshot regressions in #1465 and #1367; no skipped or failed gate is counted as PASS.
- Mempool/RPC: actual signed relay, duplicate/conflict/stale/capacity rejection, no rejected transaction fees, bounded request size/rate, public-safe admin route denial, no raw-key custody or secret-bearing evidence, terminal miner-submit outcomes and no submit-actor starvation. Keep current RPC security integration regressions and verify real native runtime behavior before GO.
- Evidence: fail closed for absent/partial/invalid reports, distinct environment versus runtime failures, exact source and artifact identity, all applicable gates, strict formatting/lint, reproducible extracted Linux/Windows packages and immutable checksum/provenance. No private live-network prerequisite and no private burn-in clock.
- Operations: immutable release directories, atomic activation, exclusive lifecycle lock, PID identity checks, non-shell configuration parsing, least privilege, persistent peer keys, DNS/TLS ownership, time sync, failure-domain separation, scoped fault injection, redacted incident evidence, validated storage restore and rollback. Native operator procedures must implement and verify these controls; prototype helper success is not acceptance.
- Security: use the current exact-candidate RustSec gate and independent PoW vectors, not obsolete advisory exceptions. No hidden advisories or unsupported transitive patches; do not advertise custody, pool accounting, GPU parity or high cadence without the corresponding evidence.

See [historical provenance](archive/README.md), [public RPC policy](PUBLIC_SAFE_RPC.md), [current security gate](security/V3_DEPENDENCY_SECURITY.md), [native launch gates](runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md) and [release evidence](RELEASE_EVIDENCE.md).

## Launch boundaries

PRE-GO preparation authorized; public testnet runtime not yet authorized. Mainnet NO-GO. Monetary mining OFF until testnet-specific GO. Contracts INACTIVE on both networks; bounded core readiness remains a mainnet requirement. No private live network. Prolonged live tests occur on public testnet after minimum safety acceptance and GO.
