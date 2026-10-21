# ROADMAP v3.0.0 — Pulse Layer Differentiation

Status: **ACTIVE v3.0.0 CANDIDATE ROADMAP**

Date: 2026-09-15 UTC

This document is the active product/protocol roadmap for the `v3.0.0` candidate line.

The repository/version bump and v3 CI target are authorized for development. They do **not** authorize a tag, GitHub Release, public-testnet launch, mainnet launch, default high-cadence activation, PulseVM activation, or `contracts_enabled=true`.

Earlier v2.x release lines are no longer predecessor release gates. Their artifacts remain useful only as compatibility, regression, migration, and provenance inputs. Scale/resilience and bounded-programmability work previously described under v2.5/v2.6 is folded into the v3 backlog and must be proven by v3 workflows on the exact candidate SHA.

Physical NVIDIA/AMD acceptance is not a v3.0.0 GO prerequisite under #781/#794; production GPU claims remain separately gated.

## Required pre-mainnet native-v3 monetary mining rehearsal (decision 2026-10-21)

**Plan:** native-v3 **monetary block production must be activated and tested before the public mainnet launch**, but first in an explicitly configured **isolated non-public rehearsal chain**, distinct from both mainnet and the public parallel testnet. The public parallel testnet still launches together with mainnet under [#781](https://github.com/AuriaLABS/PulseDAG/issues/781). This is a real CPU-mining and economic-state validation milestone, not merely a miner benchmark, dormant PoW-header test or a claim about activated-v2 monetary replay.

- [ ] Freeze native-v3 GHOSTDAG/PoW selected-parent, `K`, ordered blue/red, checked cumulative blue-work and full retarget rules, with reproducible cross-implementation vectors (#1309).
- [ ] Implement native block/transaction/reward/fees/UTXO admission and deterministic DAG replay with safe storage, pruning, orphan processing, restart and recovery.
- [ ] Create an **opt-in isolated rehearsal activation profile**, bound to its own frozen genesis, chain ID, signing domain, storage and P2P peers; prevent configuration or persisted-state bypass on mainnet/public network profiles.
- [ ] **Activate actual native-v3 monetary mining on that non-public chain** and verify valid reward issuance, fees, accepted transaction reconciliation, state roots, double-spend/conflict rejection and steady 500ms behavior with multiple miners/nodes.
- [ ] Preserve adversarial, exact-candidate security/dependency, sustained-cadence, replay and coordinated-launch acceptance gates. Obtain separate formal public GO on #781.
- [ ] Verify smart-contract deployment/execution is still **INACTIVE** throughout the pre-mainnet rehearsal and the v3.0.0 public launch.

**Not activated today:** current explicit `monetary_v3` startup mode is intentionally rejected before `Storage::open`; a stored monetary sidecar is denied after opening storage but before restore/P2P/mining (#1313). A later tested, explicitly isolated activation mechanism may replace these guards only in its own approved rehearsal configuration, while production/public profiles must remain fail-closed until formal #781 GO. Planning does **not** itself change code or release any mining network.

## Pre-mainnet smart-contract readiness with full v3.0 non-activation (decision 2026-10-21)

**Required prelaunch implementation deliverable:** the **defined core smart-contract stack** must be functionally complete, code-reviewed, adversarially tested and reproducibly marked `CONTRACT_IMPLEMENTATION_READY` for an exact candidate before the coordinated public mainnet launch. Implementation readiness does **not** authorize live deployment or execution. The acceptance ledger is [#1333](https://github.com/AuriaLABS/PulseDAG/issues/1333); the sole public GO remains [#781](https://github.com/AuriaLABS/PulseDAG/issues/781).

- [ ] Freeze versioned **core** scope (canonical Contract Transaction v3, bounded PulseScript compiler, deterministic VM, required contract state/resource/fee handling and defined offline integration), with explicit exclusions for optional Based Apps, PulseProgs, generic ZK and unrelated advanced extensions.
- [ ] Finish the core implementation and run deterministic positive/negative, cross-implementation, performance-budget, reorg/restart/pruning, parallel-conflict and fuzz/security suites **in offline or explicitly isolated development harnesses**.
- [ ] Build the exact release candidate reproducibly and record core version, artifact/tree hashes, readiness evidence and any documented limitations; `READY` remains **NOT PROVEN** until these checks pass.
- [ ] Keep `contracts_enabled=false` **mandatory** for all v3.0 mining/consensus modes, including isolated pre-mainnet monetary-v3 mining. No ordinary CLI flag, config, stored sidecar, replay, restart, peer, contract-bearing block or public RPC may activate a contract.
- [ ] Prove **non-interference** across active UTXO/DAG replay, native PoW/work, accepted block ordering, reward/fees, state roots, mining-template/P2P, node startup, RPC, wallet and package behavior by comparing contract-inactive candidate to a verified baseline with the same non-contract inputs.
- [ ] Complete the core readiness and inactive-boundary acceptance before #781 GO. Contract *activation*, public deploy/execute endpoints and general VM consensus remain **out of v3.0.0** and require a later separately versioned upgrade and dedicated GO.

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
- No silent mutation of Transaction Protocol v2 or UTXO semantics.
- No EVM compatibility claim.
- No default high cadence without the measured envelope required by `ROADMAP_V2_5_0.md`.
- Domain-separated PoW preimage (`PulseDAG:pow:v2` + `chain_id` + header v2). Kaspa work is not reusable.
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

This may ship as a v2 extension or as part of versioned tx-v3, but it must not silently reinterpret existing v2 transactions.

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
- versioned snapshot manifests from v2.5 Task 35 remain the bootstrap path.

A third party can lie about history; the receipt cannot.

### Task P6 — Mining identity without an in-node pool

- Keep miner protocol upgrades from v2.5 Task 37 (DAG-aware templates, selected tip, parallel parents).
- Publish work-certificate semantics as an optional UTXO/covenant so PulseDAG work can be proven and transferred without embedding pool software.
- Official Desktop remains first-class solo mining UX.

### Task P7 — Official user surface

3.0 is incomplete without:

- non-custodial wallet (extension and/or Desktop) for tx v2, `submission_id`, vault covenants, and PulseClock;
- explorer that renders the DAG (parents, blue/red, selected parent, pulse), not only a linear block list;
- Desktop as operator plus light wallet, not only a process supervisor.

No official custody wallet is claimed until the v3 user-surface gate is explicitly satisfied.

### Task P8 — Measured finality envelope

Use v2.5 Task 41 cadence evidence (`~1s`, `500ms`, `250ms`) to publish:

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

## Suggested sequencing

| Phase | In scope | Out of scope |
|---|---|---|
| v2.4.0 | GHOSTDAG v1, tx v2, header v2, exact-candidate release gates | this roadmap |
| v2.5.0 | compact relay, pruning/sync, mempool, miner proto v3, NVIDIA+AMD GPU, cadence envelope, 1M-block replay | contracts |
| v2.6-lite / 3.0 spec freeze | PulseClock, access sets, six templates, colored UTXO, based-app v0, `pulsedag verify`, wallet + DAG explorer | PulseVM, generic ZK, NFT standards |
| v3.0.0 genesis (future decision) | fresh identity; active monetary-v3 consensus after internal rehearsal/GO; frozen bounded core contract implementation READY but `contracts_enabled=false` and no contract execution | active general VM, contract deployment and public contract RPC |
| 3.1+ / remaining v2.6.0 | separately versioned activation of already tested PulseScript/VM, advanced assets, ZK PulseProgs and future extensions | — |

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
- It does not start the public-testnet clock or authorize public testnet/mainnet launch.
- It does not activate smart contracts or high cadence merely because the repository now has v3 identity.
- Every launch-affecting claim must be backed by exact-SHA v3 evidence.
