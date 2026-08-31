# PulseDAG v3.0.0 roadmap and gates

Status: **AUTHORITATIVE Q4 2026 LAUNCH TARGET**

This document supersedes the old launch sequence that required a standalone public testnet and a 30-day public-testnet clock before the production network.

## Launch decision

PulseDAG targets **v3.0.0** as the definitive public-launch release in **Q4 2026 (October-December 2026)**.

The launch model is:

- launch **mainnet and a parallel public testnet in the same coordinated release window**;
- do **not** launch a standalone public testnet first;
- do **not** require a 30-day public-testnet acceptance clock before mainnet;
- keep private/dev/rehearsal networks as engineering and regression evidence only;
- freeze independent mainnet and testnet network identities/genesis/configuration while keeping release provenance tied to one exact v3.0.0 candidate;
- authorize launch only through issue #781 after all v3.0.0 gates pass.

No exact date inside Q4 is authorized here. The final UTC launch window is recorded only after readiness review.

## Version policy

- v2.4.0 and v2.4.1 are development/validation milestones and historical evidence inputs.
- The definitive public launch identity is **v3.0.0**.
- Existing v2.4.x tags, binaries, artifacts and evidence must never be relabeled as v3.0.0.
- `VERSION`, Cargo versions and the v3.0.0 tag are frozen only on the exact final candidate.
- Evidence from incompatible SHAs, dependency graphs, protocol activation contracts, chain identities or genesis configurations must not be combined.

## Program authority

- #781 — sole final launch-control record for coordinated mainnet + parallel-testnet launch.
- #794 — v3.0.0 implementation, release, infrastructure and rehearsal completion program.
- #803 — dependency/security launch gate.
- #819 — production wallet/custody readiness gate.
- #789 — v2.4 operational evidence and regression input; not a v3.0.0 launch authorization.

## Mandatory sequence

1. **Rebaseline the v3.0.0 scope**
   - freeze intended consensus, transaction, wallet, storage, P2P, mining and activation scope;
   - reconcile v2.4.x follow-up work into the v3 launch backlog;
   - classify remaining work as launch-blocking, post-launch or historical.

2. **Close protocol and security blockers**
   - no unresolved Sev-1 consensus/state/storage/replay/sync/mining/operator-safety defects;
   - complete #803 remediation or an explicitly reviewed mainnet/public-exposure disposition;
   - rerun exact-SHA dependency/reachability, workflow least-privilege and secret-scanning gates.

3. **Complete the production wallet boundary**
   - finish the #819 custody requirements selected for launch;
   - complete packaged create/restore/sign/send/recovery tests on supported platforms;
   - freeze network-domain signing, replay/replacement/submission identity and fee/UTXO policy;
   - no raw private-key custody on public node RPC.

4. **Freeze one exact v3.0.0 release candidate**
   - exact source SHA/tree;
   - VERSION/Cargo/release metadata;
   - reproducible node/miner/wallet artifacts with checksums and provenance;
   - immutable protocol activation and storage compatibility boundary.

5. **Freeze two independent public network identities**
   - mainnet chain ID, network profile, genesis, consensus constants and bootnodes;
   - parallel-testnet chain ID, network profile, genesis, consensus constants and bootnodes;
   - separate DNS/RPC/status endpoints and persistent peer identities;
   - no shared genesis, chain ID, signing-domain identity or accidental peer compatibility.

6. **Run final release rehearsals**
   - multi-node/multi-miner convergence and adversarial recovery;
   - restart/snapshot/prune/restore/rejoin;
   - clean bootstrap and retained-history/checkpoint behavior;
   - wallet transaction flow and relay isolation;
   - resource, latency, storage, RPC, P2P and incident-response evidence;
   - repeat affected evidence from zero after any release-candidate change.

7. **Production launch readiness review**
   - infrastructure, backups, NTP, firewall, TLS/DNS and monitoring verified;
   - primary and backup operators named;
   - launch/on-call and rollback windows recorded;
   - status/incident communication path tested;
   - final evidence bundle and rollback plan reviewed.

8. **Single coordinated decision in #781**
   - exactly one of `GO_V3_DUAL_LAUNCH`, `DELAY_V3_DUAL_LAUNCH`, or `NO_GO_V3_DUAL_LAUNCH`;
   - GO applies only to the exact frozen v3.0.0 artifacts and network identities.

9. **Launch mainnet and parallel testnet in the same release window**
   - start/finalize seed and public node meshes for both networks;
   - verify identity separation, peer mesh, canonical convergence, mining/submit flow, wallet/relay behavior, telemetry and public status endpoints;
   - record independent first accepted block/height and UTC launch timestamps;
   - publish operator/user endpoints, checksums, known limitations and security/incident routes.

10. **Post-launch stabilization**
    - enhanced first-24h and first-week monitoring;
    - incident/rollback/recovery recording;
    - testnet remains the permanent parallel validation network for upgrades;
    - separately gated features remain disabled until separately authorized if not included in v3.0.0 scope.

## Removed legacy gates

The following are **not** prerequisites for v3.0.0 mainnet:

- a standalone v2.4.x public-testnet launch;
- the 1 September 2026 target;
- `GO_PUBLIC_TESTNET` as the final project launch decision;
- a 30-day public-testnet clock before mainnet.

Legacy runtime/config fields such as `public_testnet_ready` and `thirty_day_public_testnet_clock_started` may remain temporarily for v2.4.x compatibility and historical validation, but they are not v3.0.0 launch authority.

## Smart contracts

Smart-contract activation is no longer tied mechanically to a pre-mainnet 30-day public-testnet clock. If smart contracts are included in v3.0.0, they require their own completed implementation/security/consensus gates inside the v3 release evidence. If those gates are not complete, contracts remain disabled at launch and require a separate post-launch activation decision.

## Miner and pool boundary

The miner remains external. The node provides mining templates and submit validation; the miner performs work and returns submissions. Pool share accounting, payouts, membership and authentication remain separate infrastructure and are not embedded into the canonical standalone miner.

## Completion rule

This roadmap is complete only when #781 records the exact v3.0.0 candidate, independent mainnet/testnet identities, artifact/config/genesis digests, operator review, final decision and actual launch timestamps—or records a delay/no-go with blockers and next review date.
