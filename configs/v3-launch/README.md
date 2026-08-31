# v3.0.0 dual-network launch configuration authority

Status: **PLACEHOLDER / NOT A DEPLOYABLE CONFIGURATION**

This directory reserves the configuration authority for the coordinated v3.0.0 **mainnet + parallel public testnet** launch.

No executable production templates are committed here yet because final v3 network identities do not exist. Committing guessed chain IDs, genesis values, bootnode peer IDs, DNS names or production endpoints would create unsafe pseudo-authority.

## Required freeze records

Before executable templates are added, issue #781 must record for **mainnet** and **parallel testnet** separately:

- exact v3.0.0 source SHA/tree and packaged artifact digests;
- network profile;
- chain ID;
- genesis block/hash;
- consensus/activation constants and config digest;
- bootnode peer IDs and `/p2p/<peer-id>` multiaddrs;
- public status/RPC DNS and TLS ownership;
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
- wallet/signing network identities where applicable.

A node, miner or wallet configured for one network must fail closed when pointed at the other.

## Launch guardrail

Do not copy or promote `configs/public-testnet/` v2.4.x templates into v3 production configuration. Those files are retained only for historical v2.4 validation and contain legacy `GO_PUBLIC_TESTNET`/30-day-clock semantics that are not v3 launch authority.

Executable v3 mainnet/testnet templates should be introduced only in a reviewed change that is tied to the exact v3 release candidate and the freeze record in #781/#794.
