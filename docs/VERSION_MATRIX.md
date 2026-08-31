# PulseDAG Version Matrix

## Current repository baseline

| Area | Value |
|---|---|
| VERSION file | `v2.4.0` |
| Cargo workspace version | `2.4.0` |
| Current development line | v2.4.x / v2.4.1 |
| Scale/resilience milestone | **v2.5.0 workstream incorporated into v3.0.0** |
| Programmability milestone | **v2.6.0 workstream incorporated into v3.0.0** |
| Definitive public-launch target | **v3.0.0** |
| Target launch window | **Q4 2026 (October-December 2026)** |
| Launch model | **mainnet + parallel public testnet in one coordinated release window** |
| Final launch authority | #781 / `GO_V3_DUAL_LAUNCH` |
| Monetary policy | **PRE-FREEZE / launch-blocking** — `MONETARY_POLICY_V3_0_0.md` |
| Production genesis | **not frozen** — `GENESIS_V3_0_0.md` + genesis ceremony required |
| Launch manifest | **PRE_FREEZE / `launch_ready=false`** until all required fields/separation assertions are frozen |
| v3 mainnet identity | `TBD` — chain ID/domain/genesis/config/bootnodes not frozen |
| v3 parallel-testnet identity | `TBD` — independent chain ID/domain/genesis/config/bootnodes not frozen |
| Production wallet | #819 gate |
| Security | #803 gate |
| Smart contracts | mandatory v3.0.0 scope through the incorporated v2.6 workstream |

## Published / historical release state

- `v2.4.0` is an already published node/miner release and remains immutable historical evidence.
- v2.4.x/v2.4.1 is the active implementation base feeding the later milestone workstreams.
- Existing v2.4.x binaries, tags, chain identities and evidence must not be relabeled as v3.0.0.
- The repository remains on `v2.4.0` / `2.4.0` until an explicit reviewed version transition occurs.
- Current v2.4-derived monetary/genesis constants are development implementation baseline, not implicit v3 mainnet policy.

## Version path to v3.0.0

The intended development path is:

`v2.4.x -> v2.5.0 scale/resilience workstream -> v2.6.0 programmability workstream -> v3.0.0 definitive release`

The v2.5.0 and v2.6.0 roadmaps are therefore part of the route to v3.0.0. Their technical requirements are absorbed into the authoritative v3.0.0 launch roadmap so that the final release cannot omit them.

| Version/workstream | Scope | Relationship to v3.0.0 |
|---|---|---|
| `v2.2.x` | Earlier private-testnet hardening/rehearsal | Historical |
| `v2.3.0` | Private-testnet release/readiness baseline | Historical / compatibility evidence |
| `v2.4.0` | Published protocol/node/miner validation release | Current repository baseline + historical exact release |
| `v2.4.1` / v2.4.x follow-ups | Wallet, relay, security and integration | Active foundation work |
| `v2.5.0` | P2P scale, compact relay, fast sync/pruning, deterministic mempool/fees, Mining Protocol v3, NVIDIA+AMD GPU mining, high cadence, replay, rolling upgrades, chaos, supply chain, large rehearsal and burn-in | **Mandatory technical milestone incorporated into v3.0.0** |
| `v2.6.0` | Covenants, Contract Transaction v3, PulseScript, deterministic VM, parallel contract execution, based apps, PulseProgs/ZK, native assets, contract RPC/events, programmable fees, security, economics, replay and programmability burn-in | **Mandatory technical milestone incorporated into v3.0.0** |
| `v3.0.0` | Integrated definitive public-launch release + frozen monetary policy + independent deterministic mainnet/testnet genesis/network identities | **Q4 2026; mainnet + parallel testnet together** |

## Production freeze path

After the v2.5/v2.6 implementation work is integrated, v3 still cannot launch until the production economic/network identity is frozen:

1. approve/freeze `MONETARY_POLICY_V3_0_0.md`;
2. freeze the canonical DAG reward index, subsidy schedule, coinbase maturity, fee/burn and terminal supply rules;
3. produce independent supply-accounting/emission vectors;
4. freeze all network parameters in `NETWORK_PARAMETERS_V3_0_0.md`;
5. run `V3_0_0_GENESIS_CEREMONY.md` independently for mainnet and testnet;
6. record exact policy/genesis/config/artifact/evidence digests in `V3_0_0_LAUNCH_MANIFEST.md`;
7. set the manifest to `FROZEN` only after every launch-required `TBD` is resolved and all separation assertions are `PASS`;
8. require `scripts/validate_v3_0_0_network_freeze.py` to report `launch_ready=true` on the exact candidate;
9. only then may #781 consider `GO_V3_DUAL_LAUNCH`.

The development `genesis-treasury` placeholder, runtime genesis timestamp behavior and current source subsidy/supply constants must be explicitly accepted/replaced rather than silently becoming production policy.

## Sequencing changes versus the older roadmaps

The technical work from v2.5 and v2.6 remains required, but two old sequencing rules are superseded:

- v2.5 no longer requires a standalone public-testnet canary + 30 accepted public-testnet days before the project can move toward mainnet;
- v2.6 no longer waits for that public-testnet clock before programmability work can proceed.

Instead, v3.0.0 consolidates the evidence:

1. complete the v2.5 scale/resilience/GPU workstream;
2. complete the v2.6 programmability/smart-contract workstream;
3. freeze one exact integrated v3.0.0 candidate;
4. satisfy the exact-candidate replay, security, wallet, large-scale rehearsal and burn-in gates;
5. freeze the production monetary policy and independent mainnet/parallel-testnet genesis/network identities;
6. freeze the single exact launch manifest with `launch_ready=true`;
7. record `GO_V3_DUAL_LAUNCH`, delay or no-go only in #781;
8. launch mainnet and parallel testnet in one coordinated Q4 release window.

## Legacy v2.4 validation markers

Some existing repository workflows and hygiene checks still require historical v2.4 no-GO strings. They are retained temporarily for compatibility while those surfaces are migrated:

- `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

These strings do not define the v3 launch state and must not substitute for #781's exact-candidate launch record.

## v3.0.0 evidence rule

The final v3 launch evidence must be tied to one exact integrated release candidate containing the accepted v2.5 and v2.6 workstreams, the approved production monetary policy and the separately frozen mainnet/testnet identities.

Evidence from incompatible source SHAs, dependency graphs, monetary policies, protocol/contract activation contracts, signing domains, chain identities or genesis configurations must never be combined.

Any release-affecting code, dependency, storage, consensus, monetary, genesis, GPU, wallet-signing, contract, proof-system or activation change after candidate freeze requires explicit evidence rebaseline and rerun of affected gates. Any changed frozen genesis input creates a new network identity.
