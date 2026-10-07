# PulseDAG v3.0.0 monetary policy

Status: **SMOOTH FAIR-EMISSION POLICY IMPLEMENTED / EXACT-CANDIDATE EVIDENCE PENDING**

Authority: #781, #794, #1045. The prior 500M year-one / annual-halving curve, the superseded #1016 tail-emission proposal, and the temporary non-normalized smooth fingerprint are not launch authority.

## Mainnet policy candidate

- hard cap: **1,000,000,000.00000000 PDG**
- atomic precision: **8 decimals**
- spendable mainnet genesis issuance: **0**
- premine / treasury / foundation allocation: **0**
- emission model: **normalized deterministic Q64 exponential decay**
- exact monetary half-life: **3 economic years / 94,608,000 economic seconds**
- fixed-point emission quantum: **21,600 economic seconds (6 hours)**
- quanta per half-life: **4,380**
- Q64 decay factor per quantum: **18,443,825,056,137,834,748**
- frozen Q64 factor at one half-life: **9,223,372,036,854,774,856**
- year-one cumulative target: **206,299,474.01590026 PDG**
- year-three cumulative target: **500,000,000.00000000 PDG exactly**
- tail emission: **none**
- terminal residual settlement: **half-life 57 / economic year 171 / quantum 249,660**
- coinbase maturity: **3,600 economic seconds**, plus the separately frozen settlement/finality rule
- ordinary transaction fees: **100% to the eligible reward recipient**
- consensus burn: **0%**
- programmable resource fees: **consensus-unreachable on v3.0.0 mainnet because smart-contract deployment/execution is INACTIVE**
- canonical monetary index: deterministic ordered-DAG ordinal, genesis = score 0
- production monetary cadence: **500 ms / 2 target score transitions per economic second**, active from monetary score 0 on both mainnet and parallel testnet
- production cadence fingerprint: `6c571a95c57b7987e6a65d744d2abc9ed83c2a064ddb53b42e50cf1d6d03277f`
- raw block height / raw BPS / header blue score are not monetary authority

The exact integer implementation is `crates/pulsedag-core/src/monetary_v3.rs`.

## Policy fingerprint

Canonical bytes are embedded in `MONETARY_POLICY_CANONICAL_V3`.

SHA-256:

`134009249c301682df78d0c950fc1a70604eeddb9396361e9594e6fac121680b`

Policy version:

`pulsedag-monetary-v3.0.0-smooth-v1`

This digest binds the exact economic semantics. The cadence table has a separate canonical SHA-256 produced by `monetary_cadence_fingerprint_v3`; persistence binds **protocol fingerprint + monetary-policy fingerprint + cadence fingerprint + explicit reward-finality policy version**. Final launch evidence must additionally bind the exact source/tree SHA, network identities, deterministic genesis identities, settlement/finality identity and artifact digests.

## Exact issuance rule

The ideal continuous reference is:

`S(t) = Smax * (1 - 2^(-t / 3 years))`

Consensus does not evaluate floating point, logarithms or host-dependent transcendental functions. It freezes:

- `EMISSION_QUANTUM_SECONDS = 21,600`
- `HALF_LIFE_QUANTA = 4,380`
- `Q64_ONE = 2^64`
- `DECAY_FACTOR_Q64 = 18,443,825,056,137,834,748`
- `HALF_LIFE_END_FACTOR_Q64 = 9,223,372,036,854,774,856`

For each three-year interval:

1. the exact integer remaining supply at the interval boundaries is determined first;
2. the six-hour Q64 decay determines relative progress inside the interval;
3. that progress is normalized against `HALF_LIFE_END_FACTOR_Q64`, so the boundary lands exactly on the integer half-life target;
4. cumulative issuance is linearly interpolated with integer arithmetic inside each six-hour quantum;
5. per-score subsidy remains the cumulative difference:

`subsidy(s) = total_supply(s) - total_supply(s - 1)`

Non-terminal half-life boundaries round the sub-atom geometric remainder upward. This prevents issuance from exceeding the authorized geometric envelope before terminal cleanup.

At half-life 57 (economic year 171), the theoretical remainder is below one atomic unit. The terminal rule folds that residual into the final three-year budget. Therefore the state one nanosecond before the terminal boundary is exactly one atom below the hard cap, the terminal boundary reaches the hard cap exactly, and scheduled subsidy remains zero forever afterwards.

Fees are transfers and never increase total supply.

## Golden cumulative checkpoints

| Economic time | Cumulative PDG | Share of cap |
|---|---:|---:|
| 0 | 0.00000000 | 0% |
| 1 year | 206,299,474.01590026 | ~20.63% |
| 2 years | 370,039,475.05256342 | ~37.00% |
| 3 years | 500,000,000.00000000 | 50% |
| 4 years | 603,149,737.00795013 | ~60.31% |
| 6 years | 750,000,000.00000000 | 75% |
| 10 years | 900,787,434.25198753 | ~90.08% |
| 12 years | 937,500,000.00000000 | 93.75% |
| 15 years | 968,750,000.00000000 | 96.875% |
| 20 years | 990,156,866.79769630 | ~99.02% |
| 30 years | 999,023,437.50000000 | 99.90234375% |
| year 171 | 1,000,000,000.00000000 | 100% |

These values are consensus golden vectors. Floating-point arithmetic is not used to calculate them at runtime.

## Cadence and monetary time

Consensus economic time is derived from a versioned list of:

- `activation_score`
- `target_interval_ns`

A cadence change must activate at an exact canonical monetary score. Changing block cadence changes reward granularity only; equal economic time maps to equal cumulative issuance.

The v3.0.0 production cadence is frozen for **both mainnet and the parallel testnet** as the same one-segment monetary-time table:

| activation_score | target_interval_ns | target interval |
|---:|---:|---:|
| 0 | 500,000,000 | 500 ms |

Canonical cadence fingerprint:

`6c571a95c57b7987e6a65d744d2abc9ed83c2a064ddb53b42e50cf1d6d03277f`

The identical cadence table does **not** make the networks compatible. Mainnet and parallel testnet still require independent chain IDs, deterministic genesis identities, protocol/config fingerprints, persistent P2P identities, signing/application domains, bootnodes and public endpoints under #1049.

### Selection evidence

The selection is based on the Task39 exact-candidate three-point rehearsal on `d69500214889a79ee55390a597a46d539f8b296f` (tree `7288238575c4a76836ce2453010ad5d41fb1399b`), workflow run `37645587124`, artifact `11495507503` / `task39-cadence-d69500214889a79ee55390a597a46d539f8b296f`, artifact ZIP SHA-256 `d5e778eac2a6b73c1bcc00dc5c9283ee5beb773fb9e5dd542f289507cda6762e`.

All 1000/500/250 ms points passed the runtime gate on a real three-node libp2p full mesh with complete convergence and zero observed orphan/stale/rejected submissions. The 500 ms point was selected rather than the fastest tested point:

- **500 ms:** 21 accepted blocks in the measured window, zero stale/rejected/unknown-finality outcomes, Jain miner fairness `1.0`, worst observed template-to-submit p95 below 9 ms, worst-node canonical state-apply p95 below 0.4 ms, and storage-amplification proxy about 24.5 KB per accepted block.
- **250 ms:** also passed, but worst observed template-to-submit p95 rose to about 603 ms, the storage proxy rose to about 40.0 KB per accepted block, and fairness was slightly below 1.0. That makes it an unnecessarily aggressive v3.0.0 production default despite passing the bounded rehearsal.
- **1000 ms:** passed cleanly but provides lower throughput without a compensating correctness advantage in this evidence set.

The three per-cadence evidence SHA-256 values are:

- 1000 ms: `8750fb9a37273240eee722e272527384738bd143af3755642962bdf1ed4a9750`
- 500 ms: `6b50c409d3bba9b557c2bc57d045126184e2beb4e70fd99dcdf1967712e209ba`
- 250 ms: `ed9aeef7b6eb29d8ee3753d70c538d43a3345beefcfca674a11416e2c90e7d6a`

This freezes the consensus monetary-time table and its fingerprint. It does not by itself activate a network, authorize launch, or freeze #1049 network identities. The freeze PR must pass fresh exact-head monetary/Task39 CI before this selection is treated as integrated evidence.

The DAG invariant remains:

**economic time determines how much PDG may exist; raw block count and DAG width never determine gross issuance.**

RPC policy metadata reports the frozen cadence fingerprint while the monetary activation state remains explicitly unactivated until the production network identity/activation record is installed.

## Fair-launch and fee rules

The production v3 genesis creates **zero spendable PDG**. There is no premine, protocol treasury, foundation allocation, presale, ICO or bootstrap issuance path.

v3.0.0 fee disposition remains:

`eligible miner reward = scheduled subsidy + 100% eligible ordinary transaction fees`

Consensus burn is zero. There is no reflection mechanism and no permanent tail inflation. Any future treasury, burn, redistribution or programmable-fee split requires a separately versioned monetary-policy decision and a new fingerprint.

## Smart-contract boundary

v3.0.0 mainnet keeps smart-contract deployment/execution inactive. Monetary snapshot persistence fails closed if the chain state has contracts enabled. Programmable compute/state/proof fee paths cannot become consensus-active and cannot create, redirect or burn supply in this release.

## Live integration status

The #1045 integration line enforces the monetary contract at the live boundaries rather than only exposing a policy library:

- `/mining/template` uses the amountless v3 reward claim, rechecks protocol/policy/cadence bindings, finalizes the authoritative state root/hash, and only then exposes nonce-search work when a valid monetary sidecar is present;
- `/mining/submit` validates the state-derived canonical monetary reward and complete accepted-state supply before persistence/commit;
- live inbound P2P routes through the monetary runtime when activated-v2 capabilities and the persisted monetary identity agree;
- incoming, staged and pending P2P blocks must use the amountless reward-claim envelope; legacy amount-bearing coinbases and additional inputless issuance are rejected before they can become authoritative;
- daemon restore revalidates protocol identity, monetary binding, transient P2P queues, complete accepted-state supply and reward-finality compatibility;
- sidecar presence never activates v3 by itself, but it makes legacy fallback invalid: startup, inbound P2P, legacy-v1 Mining Protocol fallback, `/mine`, `/mine/preview`, mining jobs, PoW auto-run and PoW mine-capture all fail closed rather than authorizing height-based issuance;
- `/block/validate` uses the monetary validation path when the monetary sidecar is active;
- the persisted reward-finality policy must equal a finality engine actually implemented by the daemon;
- maturity/finality settlement is evaluated on every authoritative monetary state.

Legacy v2.x subsidy constants remain available for historical compatibility and tests, but monetary activation guards make those paths unreachable as v3 issuance authority.

## Remaining #1045 integration gates

This implementation does **not** by itself close #1045 or claim #781 launch readiness. The exact v3 candidate must still bind and prove:

- clean exact-head CI for fingerprint `134009249c301682df78d0c950fc1a70604eeddb9396361e9594e6fac121680b` and all golden vectors;
- exact production chain IDs, deterministic genesis timestamps/hashes and zero-allocation genesis identities;
- final exact-candidate rebinding of the frozen reward-finality policy `pulsedag-reward-finality-v3.0.0-mature-prefix-v1`;
- downstream explorer consumption of the monetary/denomination contract;
- exact-candidate reachability evidence demonstrating that no legacy or alternate issuance path is reachable;
- artifact/evidence digests tied to the exact #781 source/tree and network identities.

Any evidence bound to the old annual-halving fingerprint, the temporary non-normalized smooth fingerprint, or draft #1254's fingerprint is superseded and must not be mixed with this candidate.
