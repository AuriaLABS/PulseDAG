# ROADMAP v3.0.0 — Pulse Layer Differentiation

Status: **ACTIVE v3.0.0 CANDIDATE ROADMAP**

Date: 2026-09-15 UTC

This document is the active product/protocol roadmap for the `v3.0.0` candidate line.

The repository/version bump and v3 CI target are authorized for development. They do **not** authorize a tag, GitHub Release, public-testnet launch, mainnet launch, default high-cadence activation, PulseVM activation, or `contracts_enabled=true`.

Earlier v2.x release lines are no longer predecessor release gates. Their artifacts remain useful only as compatibility, regression, migration, and provenance inputs. Scale/resilience and bounded-programmability work previously described under v2.5/v2.6 is folded into the v3 backlog and must be proven by v3 workflows on the exact candidate SHA.

Physical NVIDIA/AMD acceptance is not a v3.0.0 GO prerequisite under #781/#794; production GPU claims remain separately gated.

## Thesis

PulseDAG 3.0 is the Pulse Layer: high-cadence UTXO money, a DAG-derived clock, bounded covenant templates, and verifiable settlement. L1 does not execute unbounded applications.

Activation intent for the v3.0 candidate genesis:

- `contracts_enabled=false` remains mandatory;
- `covenants_enabled` may be true only for the frozen template set defined here;
- PulseVM / PulseScript / generic ZK verification stay out of 3.0 and belong to `3.1+` or a later `v2.6.0` continuation.

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

- Enabling a general contract VM in the same genesis as first public mainnet identity.
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
| v3.0.0 genesis (future decision) | fresh identity, templates on, PulseClock on, based-apps v0 on, `contracts_enabled=false` | general VM |
| 3.1+ / remaining v2.6.0 | PulseScript, PulseVM, PDT-721, ZK PulseProgs | — |

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
