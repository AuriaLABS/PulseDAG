# PulseDAG v3.0.0 launch manifest

Launch state: **PRE_FREEZE**

This is the single human-readable authority that binds the exact v3.0.0 release candidate, monetary policy, genesis identities, network configuration, release artifacts and operations evidence used by #781.

`GO_V3_DUAL_LAUNCH` MUST NOT be recorded while any **pre-GO freeze field** is `TBD`, while `Launch state` is not `FROZEN`, or while referenced evidence belongs to incompatible candidates.

The section `Launch boundary — populate only after GO` is intentionally excluded from the pre-GO `TBD` prohibition. Those fields document the decision and actual launch after #781 authorizes it. Optional pre-GO items that are intentionally unsupported must be recorded explicitly as `NOT_APPLICABLE` rather than left `TBD`.

## Release identity

- Release: `v3.0.0`
- Exact source commit SHA: `TBD`
- Exact source tree SHA: `TBD`
- VERSION/Cargo identity: `TBD`
- Protocol activation identity/digest: `TBD`
- Transaction protocol identity/digest: `TBD`
- DAG/GHOSTDAG ordering identity/digest: `TBD`
- PoW identity/digest: `TBD`
- Storage/schema/snapshot identity/digest: `TBD`
- Contract/VM/proof identity/digest: `TBD`
- Build/release workflow run: `TBD`

## Monetary policy freeze

Approved mainnet policy parameters, canonical monetary-score algorithm, deferred-settlement architecture and economic-time finality architecture are selected, but this section remains pre-freeze until exact live state integration, network cadence/finality tables, testnet policy, vectors and digests are attached.

- Policy version: `v3.0.0-mainnet-policy-v1`
- `docs/MONETARY_POLICY_V3_0_0.md` SHA-256: `TBD`
- `docs/MONETARY_SCORE_V3_0_0.md` SHA-256: `TBD`
- `docs/REWARD_SETTLEMENT_V3_0_0.md` SHA-256: `TBD`
- `docs/FINALITY_V3_0_0.md` SHA-256: `TBD`
- Consensus monetary constants/config digest: `TBD`
- Atomic unit/precision: `8 decimals / 100,000,000 atoms per coin`
- Maximum mainnet supply: `1,000,000,000.00000000 coins`
- Mainnet genesis-issued supply: `0 coins`
- Mainnet premine/treasury/foundation allocation: `0 coins`
- Mainnet allocation manifest: `NO_SPENDABLE_GENESIS_ALLOCATIONS` — exact genesis manifest digest `TBD`
- Year-1 mining budget: `500,000,000.00000000 coins`
- Equivalent initial average emission: `~15.854895991882293252 coins/economic-second`; informational only
- Annual subsidy reduction: `50% every 31,536,000 economic seconds (365 days)`
- Reward-index definition: `canonical monetary_score = authoritative ordered-DAG ordinal; genesis = 0; not height and not raw header blue_score`
- Monetary-score source: `crates/pulsedag-core/src/monetary_v3.rs`; exact frozen source digest `TBD`
- Economic-time mapping: versioned `(activation_score, target_interval_ns)` cadence segments
- Initial mainnet cadence segment table: `TBD`
- Initial parallel-testnet cadence segment table: `TBD`
- Cadence activation rule: consensus BPS change and monetary interval change activate at the same monetary score
- Emission schedule implementation/vector digest: `TBD`
- Coinbase maturity: `3,600 economic seconds`
- Reward settlement: state-derived/provisional before finality; exact settlement implementation digest `TBD`
- Reward-settlement source: `crates/pulsedag-core/src/reward_settlement_v3.rs`; exact frozen source digest `TBD`
- Reward finality policy/version/digest: `TBD`
- Finality source: `crates/pulsedag-core/src/finality_v3.rs`; exact frozen source digest `TBD`
- Mainnet finality delay in economic seconds: `TBD`
- Parallel-testnet finality delay in economic seconds: `TBD`
- Finality-conflict detection/resolution policy/version/digest: `TBD`
- Finality persistence/snapshot schema digest: `TBD`
- Pruning/checkpoint policy/version/digest: `TBD`
- Pruning duration/depth and retained-context contract: `TBD`
- Ordinary transaction fees: `100% eligible miner/reward recipient`
- Programmable compute/state fees: `100% eligible miner/reward recipient`
- Proof-verification fees: `100% eligible miner/reward recipient`
- Consensus fee burn: `0% in v3.0.0`
- Tail emission: `none`
- Terminal/max-supply rule: `hard cap at 100,000,000,000,000,000 atomic units; final residual atom settled at start of economic year 57; then subsidy permanently zero`
- Full emission-vector digest: `TBD`
- Reordering/reward-settlement vector digest: `TBD`
- Independent supply-accounting implementation/vector digest: `TBD`

## Mainnet genesis and network identity

- Network profile: `TBD`
- Chain ID: `TBD`
- Network/signing domain: `TBD`
- Address/HRP/prefix: `TBD`
- Genesis input-manifest digest: `TBD`
- Canonical genesis bytes SHA-256: `TBD`
- Genesis transaction ID(s): `TBD`
- Genesis Merkle root: `TBD`
- Genesis initial state root: `TBD`
- Genesis block hash: `TBD`
- Genesis generator source/binary digest: `TBD`
- Consensus/network config digest: `TBD`
- Bootnode manifest digest: `TBD`
- DNS/public endpoint manifest digest: `TBD`
- Checkpoint/bootstrap manifest digest: `TBD`
- Independent genesis verification evidence: `TBD`

## Parallel-testnet genesis and network identity

- Network profile: `TBD`
- Chain ID: `TBD`
- Network/signing domain: `TBD`
- Address/HRP/prefix: `TBD`
- Genesis input-manifest digest: `TBD`
- Canonical genesis bytes SHA-256: `TBD`
- Genesis transaction ID(s): `TBD`
- Genesis Merkle root: `TBD`
- Genesis initial state root: `TBD`
- Genesis block hash: `TBD`
- Genesis generator source/binary digest: `TBD`
- Consensus/network config digest: `TBD`
- Bootnode manifest digest: `TBD`
- DNS/public endpoint manifest digest: `TBD`
- Checkpoint/bootstrap manifest digest: `TBD`
- Independent genesis verification evidence: `TBD`

## Release artifacts

### Node

- Linux x86_64 artifact + SHA-256: `TBD`
- Linux additional supported targets: `TBD`
- Windows artifact + SHA-256: `TBD`
- macOS artifact + SHA-256, if supported: `TBD`
- Container/image digest, if official: `TBD`

### Miner

- CPU/reference miner artifact + SHA-256: `TBD`
- NVIDIA artifact/runtime matrix + SHA-256: `TBD`
- AMD/ATI artifact/runtime matrix + SHA-256: `TBD`
- Multi-GPU validation evidence: `TBD`
- CPU/NVIDIA/AMD equivalence-vector digest: `TBD`

### Wallet and application tooling

- Wallet artifact(s) + SHA-256: `TBD`
- Wallet network-domain manifest digest: `TBD`
- Contract/PulseScript compiler/toolchain artifacts + SHA-256: `TBD`
- Proof/verifiable-program tooling + SHA-256 where included: `TBD`

### Supply chain

- SBOM digest/reference: `TBD`
- Provenance/attestation digest/reference: `TBD`
- Signed release manifest/reference: `TBD`
- Dependency/reachability gate (#803) evidence: `TBD`
- Secret scanning / workflow least-privilege evidence: `TBD`

## Consensus and economic evidence

- >=1,000,000-block deterministic DAG replay: `TBD`
- >=1,000,000 programmable-operation replay: `TBD`
- exact supply-accounting replay: `TBD`
- subsidy-boundary vectors: `TBD`
- annual-halving/cadence-equivalence vectors: `TBD`
- canonical monetary-score arrival-order-equivalence vectors: `TBD`
- cadence-transition continuity vectors: `TBD`
- provisional-reorder/final-reward-ledger vectors: `TBD`
- coinbase economic-maturity + finality vectors: `TBD`
- finality 1/2/4-BPS economic-time equivalence vectors: `TBD`
- finality cadence-transition continuity vectors: `TBD`
- finality monotonic replay/restart vectors: `TBD`
- finality protected-prefix conflict vectors: `TBD`
- partition/rejoin finality-conflict recovery vectors: `TBD`
- finality/pruning separation invariant: `TBD`
- fee/program/proof accounting vectors: `TBD`
- zero-genesis-issuance invariant: `TBD`
- hard-cap / year-57 terminal-residual invariant: `TBD`
- no unexplained consensus/application-state divergence: `TBD`

## Rehearsal and burn-in evidence

- >=25-node / >=16-miner adversarial rehearsal: `TBD`
- >=168 contiguous hours exact-candidate release burn-in: `TBD`
- 30 accepted days programmability-enabled exact-candidate evidence: `TBD`
- snapshot/prune/restore/rejoin evidence: `TBD`
- finality persistence/restart/rejoin evidence: `TBD`
- finality-conflict operational drill: `TBD`
- clean bootstrap evidence: `TBD`
- partition/seed-loss/GPU-loss/chaos evidence: `TBD`

## Production operations

- Primary release owner: `TBD`
- Primary operations owner: `TBD`
- Backup operations owner: `TBD`
- Security review owner/reference: `TBD`
- Mainnet seed/bootnode ownership manifest: `TBD`
- Testnet seed/bootnode ownership manifest: `TBD`
- DNS/TLS ownership: `TBD`
- Firewall/admin-plane review: `TBD`
- Monitoring/alerting dashboards and owners: `TBD`
- Finality-conflict alert/incident owner and runbook: `TBD`
- Backup/restore policy evidence: `TBD`
- NTP/time-source monitoring: `TBD`
- Incident/status communication path: `TBD`
- Rollback/hard-stop procedure review: `TBD`
- Coordinated UTC launch window: `TBD`

## Mandatory separation assertions

Before freeze all must be `PASS`:

- Mainnet chain ID differs from testnet: `TBD`
- Mainnet signing/network domain differs from testnet: `TBD`
- Mainnet genesis hash differs from testnet: `TBD`
- Mainnet/testnet peer bootstrap cannot cross-connect by default: `TBD`
- Wallet cross-network signing/broadcast fails closed: `TBD`
- Miner cross-network job/submission fails closed: `TBD`
- Contract/proof/application replay is domain separated: `TBD`
- Mainnet/testnet finality state/policy identities cannot be silently interchanged: `TBD`

## Freeze approvals

- Monetary-policy parameter approval: `APPROVED` — 1B hard cap / zero premine / 500M year-1 / annual halving / 3,600s maturity / miner fees / zero burn
- Monetary-score algorithm approval: `APPROVED` — authoritative ordered-DAG ordinal + versioned cadence segments + cumulative-difference issuance
- Deferred reward-settlement architecture approval: `APPROVED` — amountless chain-bound reward claim + state-derived subsidy/fees + finality+maturity settlement
- Finality architecture approval: `APPROVED` — policy-digested economic-time delay + selected-chain anchor + ordered-prefix binding + monotonic conflict detection; production duration/conflict resolution/pruning still TBD
- Monetary-policy implementation freeze: `TBD`
- Reward-settlement/finality freeze: `TBD`
- Genesis ceremony approval: `TBD`
- Consensus/release approval: `TBD`
- Security approval: `TBD`
- Wallet/custody approval (#819): `TBD`
- Operations approval: `TBD`
- #794 completion reference: `TBD`
- Launch-control authority: `#781`

## Launch boundary — populate only after GO

These fields may remain `TBD` while the manifest is `FROZEN` and awaiting the #781 decision. They are not inputs to `launch_ready=true`.

- #781 decision: `TBD`
- Decision UTC: `TBD`
- Mainnet first accepted block hash/height-or-score: `TBD`
- Mainnet first accepted block UTC: `TBD`
- Parallel-testnet first accepted block hash/height-or-score: `TBD`
- Parallel-testnet first accepted block UTC: `TBD`
- Published release/checksum reference: `TBD`
- Published network/bootnode/endpoints reference: `TBD`

## Immutability rule

Once `Launch state: FROZEN` is recorded, any change to source SHA, consensus, monetary policy, finality policy, genesis input, chain identity, signing domain, release artifact, wallet signing behavior or security-relevant dependency requires an explicit rebaseline. A changed genesis input always creates a new network identity.

After #781 records GO and the launch occurs, fill the post-GO launch-boundary fields without changing the frozen pre-GO identity. If the launch requires changing a frozen pre-GO field, stop and rebaseline instead of editing around the freeze.