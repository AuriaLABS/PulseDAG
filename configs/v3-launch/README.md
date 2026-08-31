# v3.0.0 integrated dual-network launch configuration authority

Status: **PLACEHOLDER / NOT A DEPLOYABLE CONFIGURATION**

This directory reserves the configuration authority for the coordinated v3.0.0 **mainnet + parallel public testnet** launch after the mandatory v2.5 scale/GPU and v2.6 programmability workstreams are integrated.

No executable production templates are committed here yet because final v3 network identities do not exist. Committing guessed chain IDs, genesis values, bootnode peer IDs, DNS names, activation values or production endpoints would create unsafe pseudo-authority.

## Required release freeze record

Before executable production templates are added, #781/#794 must record the exact integrated v3.0.0 candidate:

- exact source SHA/tree;
- VERSION/Cargo/release identity;
- protocol/transaction activation identity;
- contract/VM/proof-system identity;
- storage/schema/snapshot compatibility identity;
- production monetary/economic policy digest;
- node artifact digests;
- CPU miner artifact digests;
- NVIDIA miner artifact digests for supported targets;
- AMD/ATI miner artifact digests for supported targets;
- wallet artifact digests;
- SBOM/provenance/attestation references where supported.

## Required network freeze records

Issue #781 must record for **mainnet** and **parallel testnet** separately:

- network profile;
- chain ID;
- genesis block/hash;
- consensus/activation/config digest;
- contract/application/proof domain configuration where applicable;
- bootnode peer IDs and `/p2p/<peer-id>` multiaddrs;
- public status/RPC/event DNS and TLS ownership;
- firewall/management policy;
- storage/snapshot/backup policy;
- monitoring/alerting ownership.

## Mandatory separation

The two networks must use distinct:

- chain IDs;
- network profiles;
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
- wallet/custody and dependency/security gates;
- required rehearsal/replay/burn-in evidence.

## Launch guardrail

Do not copy or promote `configs/public-testnet/` v2.4.x templates into v3 production configuration. Those files are retained only for historical v2.4 validation and contain legacy `GO_PUBLIC_TESTNET`/30-day-clock semantics that are not v3 launch authority.

The permanent parallel public testnet is configured as a separate v3 network and launches in the same coordinated release window as mainnet after `GO_V3_DUAL_LAUNCH`.

Executable v3 mainnet/testnet templates should be introduced only in a reviewed change tied to the exact integrated v3 release candidate and the freeze record in #781/#794.
