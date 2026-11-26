# ROADMAP v3.0.0 — Pulse Layer Differentiation

Status: **ACTIVE v3.0.0 CANDIDATE ROADMAP**

Date: 2026-09-15 UTC

This document is the active product/protocol roadmap for the `v3.0.0` candidate line.

The repository/version bump and v3 CI target are authorized for development. They do **not** authorize a tag, GitHub Release, public-testnet launch, mainnet launch, default high-cadence activation, PulseVM activation, or `contracts_enabled=true`.

PulseDAG has never deployed a public network. There are no predecessor network releases, historical balances or compatibility/migration obligations. The first network is native-v3 public testnet. Applicable prototype regressions remain useful but do not establish native acceptance. All launch evidence must be bound to the exact candidate SHA.

Physical NVIDIA/AMD acceptance is not a v3.0.0 GO prerequisite under #781/#794; production GPU claims remain separately gated.

## Public testnet before mainnet — no mandatory private rehearsal (owner decision 2026-11-08)

**Sequence:** (1) finish minimum safe public-testnet bootstrap; (2) obtain an explicit `GO_PUBLIC_TESTNET` decision on #781; (3) **launch public testnet as the FIRST live native-v3 monetary-mining environment**; (4) use the live testnet to verify actual CPU mining, rewards, fees, UTXO, signed transactions, consensus convergence, forks/reorg, prune/restart, 500ms cadence, adversarial load, monitoring and recovery; (5) collect >=30 consecutive days of meaningful public evidence; (6) obtain separate `GO_V3_MAINNET` after all production gates. A separate private rehearsal network and a pre-testnet production-style mining proof are **not** required. Ordinary unit/integration/CI tests may still run locally; they are not a separate network launch phase.

- **Minimal testnet entry gate:** freeze enough native-v3 GHOSTDAG/GD-08 monetary rules, admission, KHeavyHash PoW, consensus identity and chain isolation to safely admit or reject actual blocks and transactions; pass deterministic offline/CI smoke/regression checks for invalid work, inflation, duplicate spends, incompatible peers/config and contract activation. The full scale, long-duration, prune/rejoin, adversarial-multi-node and resilience evidence is **produced on testnet after launch**, not claimed in advance.
- **Testnet-only runtime:** implement a reviewed explicit network-identity-bound monetary-v3 testnet profile, with its own genesis, chain ID, signing/address domain, datastores, bootnodes, wallet keys and endpoints. Retain mainnet denial until a later distinct #781 decision; reject cross-network peers, signatures, sidecars and accidental activation. Do not bypass existing fail-closed startup guards simply by changing config.
- **Testing policy:** test coins are worthless and never migrate to mainnet; published resets, chain upgrades and incident procedures are permissible on testnet, and material resets interrupt any uninterrupted acceptance clock. Testnet users must be told it is experimental. Publish node/miner/wallet interfaces, monitoring and a security contact appropriate to public exposure.
- **Launch state:** `GO_PUBLIC_TESTNET` must be tied to exact source/artifact/config/network identity and operations responsibility. Initial genuine monetary block production, signed transfer/reward reconciliation and multi-node convergence are **post-GO public-testnet tests**. No private monetary rehearsal is required or authorized by this planning change.
- **Contracts:** all v3.0 public network deployment/execution/state transitions and contract RPC remain **INACTIVE** and fail closed. The bounded core can remain under development while testnet runs; `CONTRACT_IMPLEMENTATION_READY` (#1333) is mandatory for final mainnet GO, not for opening testnet.
- **Current state:** public testnet **NO-GO**, mainnet **NO-GO**. `monetary_v3` startup currently rejects; changing the roadmap does **not** deploy a testnet, turn on mining or waive CI/security. Require a tested testnet-only activation path before real testnet startup.

Track public-testnet delivery on #1418 and approval on #781; use `docs/runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md` (PR #1417).

## Pre-mainnet smart-contract readiness with full v3.0 non-activation (decision 2026-10-21)

**Required prelaunch implementation deliverable:** the **defined core smart-contract stack** must be functionally complete, code-reviewed, adversarially tested and reproducibly marked `CONTRACT_IMPLEMENTATION_READY` for an exact candidate before the public **mainnet** launch. Implementation readiness does **not** authorize live deployment or execution. The acceptance ledger is [#1333](https://github.com/AuriaLABS/PulseDAG/issues/1333); the sole record for separate public-testnet and mainnet GO decisions remains [#781](https://github.com/AuriaLABS/PulseDAG/issues/781).

- [ ] Freeze versioned **core** scope (canonical Contract Transaction v3, bounded PulseScript compiler, deterministic VM, required contract state/resource/fee handling and defined offline integration), with explicit exclusions for optional Based Apps, PulseProgs, generic ZK and unrelated advanced extensions.
- [ ] Finish the core implementation and run deterministic positive/negative, cross-implementation, performance-budget, reorg/restart/pruning, parallel-conflict and fuzz/security suites **in offline or explicitly isolated development harnesses**.
- [ ] Build the exact release candidate reproducibly and record core version, artifact/tree hashes, readiness evidence and any documented limitations; `READY` remains **NOT PROVEN** until these checks pass.
- [ ] Keep `contracts_enabled=false` **mandatory** for all v3.0 mining/consensus modes, including isolated pre-mainnet monetary-v3 mining. No ordinary CLI flag, config, stored sidecar, replay, restart, peer, contract-bearing block or public RPC may activate a contract.
- [ ] Prove **non-interference** across active UTXO/DAG replay, native PoW/work, accepted block ordering, reward/fees, state roots, mining-template/P2P, node startup, RPC, wallet and package behavior by comparing contract-inactive candidate to a verified baseline with the same non-contract inputs.
- [ ] Complete core readiness and inactive-boundary acceptance before #781 **mainnet** GO; public testnet requires the independently proven INACTIVE boundary but may start before the bounded core is implementation-ready. Contract *activation*, public deploy/execute endpoints and general VM consensus remain **out of v3.0.0** and require a later separately versioned upgrade and dedicated GO.

The previously accepted option to ship **partially implemented core contracts** is superseded. However, unfinished **optional** proof/application extensions are not misrepresented as part of a complete core. If mandatory core readiness cannot be proven by release freeze, the status is DELAY/NO-GO or an explicit separately recorded scope decision, not a fabricated readiness pass.

## Thesis

PulseDAG 3.0 is the Pulse Layer: high-cadence UTXO money, a DAG-derived clock, bounded covenant templates, and verifiable settlement. L1 does not execute unbounded applications.

Activation intent for the v3.0 candidate genesis:

- `contracts_enabled=false` remains mandatory;
- `covenants_enabled` may be true only for the frozen template set defined here;
- **Live** PulseVM/PulseScript execution and generic ZK verification stay out of 3.0; the **dormant core implementation** of bounded PulseScript/VM is required READY before mainnet (#1333), with separately versioned activation later.

## Non-negotiable principles

- Consensus determinism and fail-closed validation remain primary.
- Miner remains external. No pool, vardiff, share accounting, or payout logic in `pulsedagd` or the official miner.
- CPU mining is the launch-supported production mining path for v3.0.0. NVIDIA/AMD GPU code may remain present, but physical GPU production support stays unadvertised and `GPU_MINING_NVIDIA_PASS` / `GPU_MINING_AMD_PASS` remain `NOT_CLAIMED` until separate physical validation is completed.
- Freeze native transaction/signing and UTXO semantics before testnet; reject unsupported formats.
- No EVM compatibility claim.
- No default high cadence without the measured envelope in Task P8 and the network acceptance program below.
- Freeze native chain-bound KHeavyHash preimage, target and retarget semantics under #1466; work from another network is not reusable.
- Planning text is not protocol identity. Inactive `contract_v3` / `covenant_v1` / `tx_v3` modules on `main` stay disabled until an explicit activation contract exists.

## Work items

These are active v3 work items. Each item remains independently gated and must not be inferred active from code presence alone.

Fail-closed matchers now exist on `main` for P1–P8 (PulseClock through measured envelope, plus explorer/wallet pulse views). Default admission stays inactive. That is not a 3.0 genesis, not `covenants_enabled=true`, and not a public-testnet clock start.

### Task P1 — PulseClock

Define a canonical pulse derived from the selected DAG:

- `pulse_height` bound to selected-tip `blue_score`;
- `pulse_time` as a robust aggregate of recent blue-set timestamps with anti-lie bounds;
- signing / API domain `PulseDAG:pulse:v1`;
- public `GET /pulse` (or equivalent) exposing height, time, uncertainty, and finality lag.

Covenant timelocks, based-app challenge windows, and application finality must cite pulse values, not wall clock.

Required evidence: adversarial clock-skew, partition, and parallel-tip cases produce identical pulse values after the same selected DAG.

### Task P2 — Declared access sets

Extend the future programmable transaction format with an explicit access set (read/write UTXOs and later covenant/asset keys).

The state scheduler applies non-conflicting transactions in parallel under canonical GHOSTDAG order. Conflicts reject deterministically. Arrival order must not change the accepted set.

Product metric: conflict-free transactions applied per pulse.

Use the frozen native transaction format and explicit activation; unsupported prototype formats are rejected.

### Task P3 — Covenant template freeze (no general VM)

3.0 may activate only versioned, statically bounded templates:

| Template | Purpose |
|---|---|
| `vault_v1` | cold storage with pulse delay and emergency path |
| `pay_stream_v1` | vesting / payroll by pulse |
| `htlc_v1` | atomic swap / bridge primitive |
| `multisig_m_n_v1` | treasury |
| `channel_v1` | off-chain state with on-DAG settle |
| `colored_utxo_v1` | native colored asset, one color per UTXO |

Out of 3.0 genesis: PulseVM, PulseScript, multidimensional gas as a general VM economy, PDT-721 / PDT-1155, generic proof-system activation.

Templates must be deterministic, resource-bounded, and unable to touch host time, network, filesystem, or randomness.

### Task P4 — Based Apps v0

L1 provides ordering, data/commitment availability, and settlement.

Minimum 3.0 profile:

- versioned application namespace;
- bounded data/state commitment in outputs or a data envelope;
- canonical order = GHOSTDAG order;
- challenge window measured in pulses;
- no hidden sequencer (any operator must be declared by the app profile).

Validity proofs and a general PulseProg runtime stay post-3.0.

### Task P5 — Reproducible chain product

Productize existing replay/evidence culture:

- `pulsedag verify --from-genesis` reconstructs selected parent, blue/red classification, UTXO, and state digest;
- emits a receipt bound to binary identity and chain identity;
- versioned authenticated snapshot manifests remain the bootstrap path.

A third party can lie about history; the receipt cannot.

### Task P6 — Mining identity without an in-node pool

- Use the native mining protocol (DAG-aware templates, selected tip, parallel parents).
- Publish work-certificate semantics as an optional UTXO/covenant so PulseDAG work can be proven and transferred without embedding pool software.
- Official Desktop remains first-class solo mining UX.

### Task P7 — Official user surface

3.0 is incomplete without:

- non-custodial wallet (extension and/or Desktop) for tx v2, `submission_id`, vault covenants, and PulseClock;
- explorer that renders the DAG (parents, blue/red, selected parent, pulse), not only a linear block list;
- Desktop as operator plus light wallet, not only a process supervisor.

No official custody wallet is claimed until the v3 user-surface gate is explicitly satisfied.

### Task P8 — Measured finality envelope

Collect cadence evidence (`~1s`, `500ms`, `250ms`) to publish:

- chosen public cadence;
- DAG width, stale rate, apply lag, miner fairness;
- `finality_depth(pulse)` under stated honest-hashrate assumptions;
- versioned `k` with an explicit activation rule. Silent `k` changes are forbidden.

## Explicitly out of 3.0.0

- Enabling **execution or deployment** of the general contract VM in the same genesis as first public mainnet identity; having a completed, verified but INACTIVE implementation in release artifacts is allowed only with proven non-interference.
- EVM compatibility.
- Pool/stratum inside the node or official miner.
- Changing PoW away from domain-separated kHeavyHash before a live network exists.
- Default high cadence without measured evidence.
- Representing NVIDIA or AMD GPU mining as production-validated before separate physical hardware evidence exists.
- Opaque treasury or reflection-style tokenomics in consensus.

## First-network delivery sequence

| Phase | In scope |
|---|---|
| Native baseline | GHOSTDAG/GD-08, chain-bound PoW/retarget, signed UTXO, atomic economic admission, rewards, maturity, fees and supply; daemon, CPU miner, wallet, P2P and isolated testnet identity |
| Public testnet entry | Exact-candidate minimum offline safety, public attack-surface controls, operator ownership, independent GO_PUBLIC_TESTNET |
| Public testnet | Genuine blocks, signed transfers, distributed recovery, stress, pruning, cadence, at least 30 meaningful consecutive days |
| Mainnet acceptance | Independent production identity, all applicable exact-candidate production gates, bounded contract core READY but INACTIVE, independent GO_V3_MAINNET |
| Later upgrades | Separately approved activation of programmable functionality and optional extensions |

## Native network and production acceptance program

The following requirements consolidate the former scale/resilience work. Scope and timing follow #781, #794 and the testnet-first runbook; do not infer that a requirement is implemented or passed.

- **P2P (#1034/#1465):** authenticated parent-aware body-on-demand retrieval, peer scoring/diversity, inbound/outbound quotas, eclipse/flood resistance, per-peer CPU/RAM/disk/bandwidth budgets, bounded churn and penalties, compact DAG reconstruction with identical full validation and measured propagation/bandwidth.
- **State (#1367):** authenticated versioned snapshot/state commitments, resumable bounded chunk transfer, pruning-aware frontier, clean catch-up, poisoning/incompatibility rejection, restart and full-replay equivalence, bounded storage.
- **Mempool and fees:** deterministic fee-rate estimation, hard limits, eviction/expiry, replacement, ancestor/descendant/package bounds, conflict handling, anti-spam, restart and reorder reconciliation.
- **Mining (#1037):** external miner; versioned jobs, sequence IDs, notification, invalidation/target updates, parallel-parent templates, stable submission IDs, accepted/rejected/stale/unknown-finality classification, reconciliation, bounded backpressure and telemetry.
- **GPU (deferred physical acceptance):** CPU oracle; NVIDIA and AMD golden parity, nonce/target/endianness/overflow boundaries, device isolation, nonce partitioning, recovery/reconnect and telemetry; profile kernels and long soaks before any production claim. No GPU consensus shortcut; CPU is the required launch path.
- **Cadence and replay (#1039):** approximately 1s/500ms/250ms measurement points, propagation/orphans/DAG width/state-apply/database amplification/template freshness/submit/stale/finality/fairness; choose measured envelope. Replay at least 1,000,000 blocks with identical parents, blue/red, order, transaction outcomes and UTXO roots across arrival permutations, forks, snapshots, pruning and restarts.
- **API/operations (#1039/#1050):** bounded versioned public RPC/API, schema, errors, IDs, pagination, rate limits and event streams; private admin interfaces. Monitor node/miner/storage/network health. Automated chaos covers node/miner loss, disk pressure, packet loss, seed loss, partitions, clock skew, orphan storms, RPC load and recovery; record timeline, metrics, digests and outcomes. Future upgrades of the deployed network require capability negotiation, canary, explicit activation and safe rollback/downgrade refusal; no compatibility with never-deployed prototypes.
- **Release/security (#1047/#1048):** locked reproducible or independently verifiable builds, SBOM, provenance, signed manifests, dependency audit, least privilege, clean-machine native smoke, exact artifact/config hashes; wallet custody and network separation (#1046/#1049).
- **Mainnet distributed validation (#1040):** at least 25 nodes/16 CPU miners, diverse failure domains, adversarial partitions/churn/seed loss/stale work/target changes/pruning/rejoin/upgrade/load, convergence without unexplained state divergence; at least 168 contiguous hours on one unchanged production candidate, plus at least 30 consecutive days meaningful monitored public testnet. Record and resolve launch-blocking incidents. Physical GPU evidence remains separately gated.

## Programmability engineering boundary

Bounded contract core readiness before mainnet is governed by #1333. Preserve canonical contract transaction serialization/signatures/domain binding and resource limits; reproducible typed PulseScript compilation, exact integer/overflow semantics and golden vectors; deterministic VM with instruction/memory/stack/call/state limits, no host I/O/time/randomness; canonical read/write conflicts and rollback, deterministic fee/state accounting, replay, fuzzing and differential security tests. Contract state, events, fees and RPC must not interfere with the active monetary network.

Covenant/application features require explicit independent scope and admission rules; nothing here authorizes contract execution, deployment, state transitions or public contract RPC in v3.0.0. Optional Based Apps, PulseProgs, ZK verification, advanced assets/token standards and public programmable-fee/contract runtime are later-version research or activation work. Future activation needs version/domain and proof limits, resource metering, supply/state replay, bounded APIs, adversarial tests, separately approved network activation, rollback and dedicated burn-in. No previous-version prerequisite release exists.

## Acceptance demo (planning bar, not a release gate)

The v3.0 candidate is not differentiated until a short demo can show:

1. two non-conflicting transactions land in parallel blocks and both apply;
2. a vault covenant moves only after N pulses and is visible in the explorer;
3. a based-app commit settles when the pulse finalizes it;
4. a clean machine runs `pulsedag verify` and matches the published state digest;
5. a non-core user sends value with the official non-custodial wallet.

## Authorization

This document governs the active v3.0.0 candidate line.

- `VERSION` and Cargo workspace identity are `v3.0.0` / `3.0.0`.
- It does not start public-testnet operation or authorize either public-network launch. The 30-day testnet evidence clock starts only after an authorized and evidenced public testnet is running.
- It does not activate smart contracts or high cadence merely because the repository now has v3 identity.
- Every launch-affecting claim must be backed by exact-SHA v3 evidence.
