# Roadmap v3.0 — Long-Lived Functional Core

Status: **SUPPLEMENTAL / SEQUENCING SUPERSEDED BY `ROADMAP_V3_0_0.md`**

This document preserves the long-lived-core engineering philosophy behind v3.0. It is **not** the current launch sequence.

The authoritative launch roadmap is [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md): PulseDAG targets **v3.0.0 in Q4 2026**, with **mainnet and a parallel public testnet launched in one coordinated release window** after the final decision in #781.

The earlier staged sequence in which v2.5.x/v2.6.x led to a standalone public testnet and a mandatory **30-day stable-testnet burn-in before v3.0.0** is superseded. Those version labels remain historical planning references only and do not impose launch order.

## Long-lived-core philosophy retained

- v3.0 is earned by exact, reviewable evidence rather than by a version-number declaration;
- durability, migration safety, reproducibility and operator recovery take priority over feature expansion;
- consensus, storage, P2P, sync, mining, wallet and release boundaries must be documented and tested;
- release decisions must be tied to exact source/artifact identities;
- incompatible evidence from different SHAs, dependency graphs, chain identities or protocol activation contracts must not be combined;
- the external miner remains separate from the node;
- pool coordination/share accounting/payout logic remains separate infrastructure;
- unsupported compatibility claims are prohibited.

## Historical milestone context

The v2.2.x and v2.3.0 milestones established much of the evidence discipline used by the v3 program: multi-node rehearsals, replay/storage/snapshot recovery, external miner contracts, operator RPC hardening and release provenance.

v2.4.x then became a substantial implementation/validation line, including protocol-v2/GHOSTDAG-style work, public-safe RPC foundations, wallet hardening and adversarial recovery evidence. Those releases and branches are retained as historical and regression inputs.

The previously drafted v2.5.x, v2.6.x, v2.7.x and v2.8.x sequence is no longer a mandatory version ladder. Work that remains relevant from those roadmaps is reclassified into the v3.0.0 launch program under #794.

## v3.0.0 long-lived-core gates

The long-lived-core quality bar remains mandatory even though the launch sequencing changed. v3.0.0 must not receive `GO_V3_DUAL_LAUNCH` unless the exact candidate demonstrates:

- no unresolved Sev-1 consensus, state, storage, replay, sync, mining, wallet-security or operator-safety issue;
- deterministic replay/order/state reconstruction on the final protocol contract;
- restart, snapshot, restore, pruning and clean-bootstrap recovery;
- multi-node/multi-miner convergence under normal and adversarial conditions;
- reproducible node/miner/wallet release artifacts with checksums/provenance;
- documented storage migration and rollback boundaries;
- public/operator/development RPC boundaries and fail-closed public-safe exposure;
- exact dependency/security/reachability review under #803;
- production wallet/custody acceptance under #819 for the approved launch scope;
- production infrastructure, monitoring, incident response and rollback readiness under #794;
- independent frozen mainnet and parallel-testnet chain/genesis/network identities;
- final launch review and `GO_V3_DUAL_LAUNCH` only in #781.

## Testnet role after the strategy change

The parallel public testnet is still essential, but its role changes:

- it launches alongside mainnet in the same v3.0.0 release window;
- it remains a permanent public validation network for future upgrades;
- future consensus/network upgrades should rehearse there before separately authorized mainnet activation;
- it is **not** a prerequisite 30-day public launch phase that must complete before initial v3 mainnet.

Private and release-candidate rehearsals before launch still need sufficient duration and perturbation coverage to support the final safety decision. Duration is evidence-driven and defined by #794/#781, not by the superseded 30-day standalone-testnet rule.

## Smart-contract boundary

Smart contracts are not automatically enabled by the v3.0.0 version number and are not mechanically unlocked by a testnet-day counter.

- If smart contracts are included in the frozen v3.0.0 launch scope, their implementation, consensus, security, resource, wallet and recovery gates must be complete on the exact candidate.
- If those gates are incomplete, contracts remain disabled at launch and require a separate later activation decision.

## Miner and pool boundary

The miner remains external for v3.0.0. The node provides mining templates and validates submissions; the miner performs work and returns submissions.

Pool membership, share accounting, authentication and payouts do not belong inside the canonical standalone miner.

## Promotion rule

A candidate may be promoted to v3.0.0 only when the current launch evidence is complete, exact and reviewable under `ROADMAP_V3_0_0.md`, #794, #803, #819 and #781.

Missing, stale or contradicted evidence keeps the release as a candidate and requires delay/rebaseline rather than weakening the launch gates.
