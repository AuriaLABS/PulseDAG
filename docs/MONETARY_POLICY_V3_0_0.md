# PulseDAG v3.0.0 monetary policy

Status: **POLICY PARAMETERS FROZEN / ACTIVATION INTEGRATION IN PROGRESS**

Authority: #781, #794, #1045. The superseded #1016 policy must not be used for v3.0.0.

## Frozen mainnet policy

- hard cap: **1,000,000,000.00000000 PDG**
- atomic precision: **8 decimals**
- spendable mainnet genesis issuance: **0**
- premine / treasury / foundation allocation: **0**
- first economic-year mining budget: **500,000,000.00000000 PDG**
- reduction: **50% every 31,536,000 economic seconds**
- tail emission: **none**
- coinbase maturity: **3,600 economic seconds**, plus the separately frozen settlement/finality rule
- ordinary transaction fees: **100% to the eligible reward recipient**
- consensus burn: **0%**
- programmable resource fees: **consensus-unreachable on v3.0.0 mainnet because smart-contract deployment/execution is INACTIVE**
- canonical monetary index: deterministic ordered-DAG ordinal, genesis = score 0
- raw block height / raw BPS / header blue score are not monetary authority

The exact integer implementation is `crates/pulsedag-core/src/monetary_v3.rs`.

## Frozen policy fingerprint

Canonical bytes are embedded in `MONETARY_POLICY_CANONICAL_V3`.

SHA-256:

`14605483aa65a17d654ffc4db1571b1416eb45b3f9b56af452d88c9023311366`

This digest binds the economic rules only. The exact cadence table has a separate canonical SHA-256 produced by `monetary_cadence_fingerprint_v3`; persistence binds **protocol fingerprint + policy fingerprint + cadence fingerprint + explicit reward-finality policy version**. Final launch evidence must additionally bind the exact source/tree SHA, network identities, deterministic genesis identities, settlement/finality identity and artifact digests.

## Exact issuance rule

The implementation uses integer-only cumulative issuance. Within each economic year the exact annual budget is distributed linearly over economic time. The annual budget halves at each 31,536,000-second boundary.

For canonical score `s`:

`subsidy(s) = total_supply(s) - total_supply(s - 1)`

This cumulative-difference rule deterministically carries rounding remainders and prevents hidden issuance. At the year-57 terminal boundary the final residual atom is settled, total scheduled supply reaches the hard cap exactly, and subsidy remains zero forever.

Fees are transfers and never increase total supply.

## Cadence and monetary time

Consensus economic time is derived from a versioned list of:

- `activation_score`
- `target_interval_ns`

A cadence change must activate at an exact canonical monetary score. Changing from 1 BPS to 2 BPS or 4 BPS changes reward granularity only; equal economic time must map to equal cumulative issuance.

The production mainnet/testnet cadence tables are separate network-freeze inputs and remain invalid to invent before #781 freezes them. Until then, RPC policy metadata reports `production_cadence_frozen=false` and no production cadence fingerprint.

## Smart-contract boundary

v3.0.0 mainnet keeps smart-contract deployment/execution inactive. Monetary snapshot persistence now fails closed if the chain state has contracts enabled. Therefore programmable compute/state/proof fee paths must not become consensus-active and cannot create, redirect or burn supply. Any later activation requires a separately versioned protocol decision and monetary-policy compatibility review.

## Live integration status

The current #1045 integration line now enforces the monetary contract at the live boundaries rather than only exposing a policy library:

- `/mining/template` uses the amountless v3 reward claim, rechecks protocol/policy/cadence bindings, finalizes the authoritative state root/hash, and only then exposes nonce-search work when a valid monetary sidecar is present;
- `/mining/submit` validates the state-derived canonical monetary reward and the complete accepted-state supply before persistence/commit;
- live inbound P2P routes through the monetary runtime when activated-v2 capabilities and the persisted monetary identity agree;
- incoming, staged and pending P2P blocks must use the amountless reward-claim envelope; legacy amount-bearing coinbases and additional inputless issuance are rejected before they can become authoritative;
- daemon restore revalidates protocol identity, monetary binding, transient P2P queues, complete accepted-state supply and reward-finality compatibility;
- sidecar presence never activates v3 by itself, but it **does** make legacy fallback invalid: startup, inbound P2P, legacy-v1 Mining Protocol fallback, `/mine`, `/mine/preview`, mining jobs, PoW auto-run and PoW mine-capture all fail closed rather than authorizing height-based issuance;
- `/block/validate` uses the monetary validation path when the monetary sidecar is active, so diagnostic validation cannot report a legacy-subsidy block as production-valid;
- the persisted reward-finality policy must equal a finality engine actually implemented by the daemon. The currently implemented `ghostdag-v1-no-prune-before-task30-v1` engine protects only genesis, therefore no non-genesis reward becomes spendable yet;
- maturity/finality settlement is evaluated on every authoritative monetary state. If a future finality engine would make reward UTXOs spendable before explicit UTXO/state-root materialization is integrated, the node fails closed instead of materializing supply implicitly.

Legacy v2.x subsidy constants remain available for historical compatibility and tests, but the monetary activation guards make those paths unreachable as v3 issuance authority.

## Remaining #1045 integration gates

This integration still does **not** close #1045 or claim #781 launch readiness. The exact v3 candidate must still bind and prove:

- the exact production mainnet/testnet cadence tables and their fingerprints;
- the exact production chain IDs, deterministic genesis timestamps/hashes and zero-allocation genesis identities;
- the final production reward-finality policy. The current conservative engine finalizes only genesis, so spendable reward UTXO materialization remains intentionally unavailable rather than guessed;
- downstream explorer consumption of the frozen denomination contract (core/wallet/RPC already share integer atoms, `PDG`, 8 decimals and exact no-float formatting/parsing);
- an exact-candidate reachability/evidence pass demonstrating that no legacy or alternate issuance path is reachable with the frozen production sidecar;
- golden monetary vectors, policy/cadence/finality bindings and artifact digests tied to the exact #781 source/tree and network identities.

The deterministic zero-allocation v3 genesis constructor, accepted-state total-supply audit, live mining/P2P monetary gates, protocol/persistence binding and denomination contract are implemented foundations; their final production identities remain #781 freeze inputs.
