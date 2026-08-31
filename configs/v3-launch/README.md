# v3.0.0 integrated dual-network launch configuration authority

Status: **PLACEHOLDER / NOT A DEPLOYABLE CONFIGURATION**

This directory reserves the configuration authority for the coordinated v3.0.0 **mainnet + parallel public testnet** launch after the mandatory v2.5 scale/GPU and v2.6 programmability workstreams are integrated.

No executable production templates are committed here yet because final v3 economic and network identities do not exist. Committing guessed chain IDs, genesis values, monetary parameters, bootnode peer IDs, DNS names, activation values or production endpoints would create unsafe pseudo-authority.

## Authoritative freeze documents

Production configuration is subordinate to:

- `docs/MONETARY_POLICY_V3_0_0.md` — approved issuance/reward/fee/maturity/supply contract;
- `docs/GENESIS_V3_0_0.md` — deterministic genesis input/output contract;
- `docs/NETWORK_PARAMETERS_V3_0_0.md` — consensus and network parameter matrix;
- `docs/runbooks/V3_0_0_GENESIS_CEREMONY.md` — independent genesis reproduction procedure;
- `docs/V3_0_0_LAUNCH_MANIFEST.md` — exact frozen release/economic/genesis/network/artifact authority.

`GO_V3_DUAL_LAUNCH` is invalid while the launch manifest is `PRE_FREEZE`, any required value remains `TBD`, or `scripts/validate_v3_0_0_network_freeze.py` reports `launch_ready=false`.

## Required release freeze record

Before executable production templates are added, #781/#794 must record the exact integrated v3.0.0 candidate:

- exact source SHA/tree;
- VERSION/Cargo/release identity;
- protocol/transaction activation identity;
- contract/VM/proof-system identity;
- storage/schema/snapshot compatibility identity;
- production monetary/economic policy digest and emission-vector digest;
- genesis allocation and supply-accounting digests;
- node artifact digests;
- CPU miner artifact digests;
- NVIDIA miner artifact digests for supported targets;
- AMD/ATI miner artifact digests for supported targets;
- wallet artifact digests;
- SBOM/provenance/attestation references where supported.

## Required network freeze records

Issue #781 and `docs/V3_0_0_LAUNCH_MANIFEST.md` must record for **mainnet** and **parallel testnet** separately:

- network profile;
- chain ID;
- network/signing domain;
- address/HRP/prefix configuration where applicable;
- deterministic genesis input-manifest digest;
- canonical genesis bytes/hash/TXID/root identities;
- consensus/activation/config digest;
- monetary-policy digest;
- contract/application/proof domain configuration where applicable;
- bootnode peer IDs and `/p2p/<peer-id>` multiaddrs;
- public status/RPC/event DNS and TLS ownership;
- checkpoint/bootstrap manifest where applicable;
- firewall/management policy;
- storage/snapshot/backup policy;
- monitoring/alerting ownership.

## Monetary/genesis guardrails

The current development baseline is not a production configuration. In particular:

- do not promote the development `genesis-treasury` destination into mainnet;
- do not infer mainnet genesis issuance from `GENESIS_SUPPLY = 1_000_000_000`;
- do not infer the final v3 emission schedule solely from the current `INITIAL_BLOCK_SUBSIDY = 50` / `SUBSIDY_HALVING_INTERVAL = 210_000` implementation;
- do not construct production genesis with runtime `current_ts()` or any operator-local value;
- do not leave coinbase maturity or reward-index semantics implicit;
- do not deploy any configuration whose policy/genesis/network digest differs from the frozen launch manifest.

All of those fields must be explicitly approved, implemented, tested and content-addressed before deployment.

## Mandatory separation

The two networks must use distinct:

- chain IDs;
- network profiles;
- network/signing domains;
- genesis identities;
- bootnode identities;
- endpoint namespaces;
- wallet/signing network identities;
- application/contract/proof domains where the frozen protocol uses domain separation.

A node, CPU/GPU miner, wallet or application client configured for one network must fail closed when pointed at the other.

## Integrated v2.5/v2.6 launch prerequisites

Executable v3 production configuration must not be promoted until the exact candidate has evidence for:

- P2P v3, compact relay, sync/pruning/bootstrap and deterministic mempool/fee gates;
- CPU/NVIDIA/AMD production PoW equivalence;
- high-cadence operating envelope and deterministic DAG replay;
- covenants, Contract Transaction v3, PulseScript and deterministic VM;
- assets/contracts/application/proof semantics included in the frozen scope;
- programmable resource/fee and production monetary/economic policy;
- deterministic genesis and network-identity reproduction;
- wallet/custody and dependency/security gates;
- required rehearsal/replay/burn-in evidence.

## Executable configuration promotion

When the freeze is ready, add reviewed network-specific files under this directory rather than editing historical v2.4 public-testnet templates. Each executable config must include or reference its exact config digest and must be checked against `V3_0_0_LAUNCH_MANIFEST.md` in release CI/startup validation.

The permanent parallel public testnet is a separate v3 network and launches in the same coordinated release window as mainnet after `GO_V3_DUAL_LAUNCH`.

Do not copy or promote `configs/public-testnet/` v2.4.x templates into v3 production configuration. Those files are retained only for historical v2.4 validation and contain legacy `GO_PUBLIC_TESTNET`/30-day-clock semantics that are not v3 launch authority.
