# PulseDAG v3.0.0 network-parameter freeze

Status: **PRE-FREEZE / LAUNCH-BLOCKING**

This document is the canonical checklist for every network-visible parameter that must be explicit before the coordinated v3.0.0 mainnet + parallel-testnet launch.

## Freeze principle

No production parameter may be inferred from a developer default, environment variable, example config or historical v2.x network. Final values must be committed/reviewed, tied to the exact v3.0.0 candidate and recorded in `V3_0_0_LAUNCH_MANIFEST.md`.

## Consensus and protocol

Freeze for each public network:

- chain ID;
- network/signing domain;
- genesis hash and input/config digest;
- block-header protocol version;
- transaction protocol version;
- DAG ordering/GHOSTDAG version and parameters;
- merge-set/K parameters;
- PoW algorithm/version;
- target/difficulty encoding and PoW limit;
- difficulty-adjustment algorithm and all windows/bounds;
- target block cadence / accepted operating point;
- timestamp median/future-drift rules;
- finality rule/version;
- maximum block/transaction/script/contract/proof sizes;
- monetary-policy version/digest;
- canonical monetary-score algorithm/version/digest from `MONETARY_SCORE_V3_0_0.md`;
- reward-index definition: authoritative ordered-DAG ordinal, genesis score 0;
- production monetary cadence segment table `(activation_score, target_interval_ns)`;
- exact cadence-activation boundary shared by consensus cadence and monetary interval;
- reward-settlement/finality implementation version and digest;
- coinbase maturity;
- fee and burn/distribution rules;
- contract/VM/proof versions and activation boundary;
- storage/schema/snapshot compatibility version;
- pruning/checkpoint/bootstrap rules.

Every consensus parameter must be included in a deterministic configuration digest or compiled-constant manifest so two nodes can prove they are on the same network contract.

## Monetary score and cadence freeze

The v3 monetary index is **not** ordinary height and is **not** raw block-header `blue_score`. Genesis is monetary score 0; each non-genesis position in the frozen authoritative deterministic ordered DAG receives one unique ordinal.

Economic time is derived only from the versioned cadence segment table. Each segment is `(activation_score, target_interval_ns)`, and a segment activating at score `S` governs transition `S -> S+1` and later transitions until the next activation. This makes a cadence change continuous: past economic time is never repriced.

The consensus BPS change and monetary interval change activate at the same monetary score. A transition that changes only one side is invalid.

Reference intervals are:

- 1 BPS / ~1 s: `1_000_000_000 ns` per score transition;
- 2 BPS / ~500 ms: `500_000_000 ns`;
- 4 BPS / ~250 ms: `250_000_000 ns`.

These are supported/reference mappings, not authorization of the final launch cadence. The exact starting cadence and activation table remain `TBD` until performance/replay evidence freezes them.

A reward becomes spendable only after both the approved 3,600 economic-second maturity and the frozen v3 finality/settlement rule protect its monetary position. The legacy genesis-only finality baseline is not sufficient for production reward settlement.

## Mainnet identity — final values TBD

- Profile: `TBD`
- Chain ID: `TBD`
- Network/signing domain: `TBD`
- Genesis hash: `TBD`
- Genesis manifest digest: `TBD`
- Consensus/config digest: `TBD`
- Monetary-policy digest: `TBD`
- Monetary-score implementation digest: `TBD`
- Initial monetary cadence segment table: `TBD`
- Reward-settlement/finality digest: `TBD`
- Address/HRP/prefix: `TBD`
- Default P2P port: `TBD`
- Public RPC/API port(s): `TBD`
- Event/WebSocket port(s): `TBD`
- DNS seeds: `TBD`
- Bootnode peer IDs/multiaddrs: `TBD`
- Public status endpoint: `TBD`
- Explorer/indexer endpoint(s), if official: `TBD`
- Checkpoint policy/initial checkpoints: `TBD`

## Parallel-testnet identity — final values TBD

- Profile: `TBD`
- Chain ID: `TBD`
- Network/signing domain: `TBD`
- Genesis hash: `TBD`
- Genesis manifest digest: `TBD`
- Consensus/config digest: `TBD`
- Monetary-policy digest: `TBD`
- Monetary-score implementation digest: `TBD`
- Initial monetary cadence segment table: `TBD`
- Reward-settlement/finality digest: `TBD`
- Address/HRP/prefix: `TBD`
- Default P2P port: `TBD`
- Public RPC/API port(s): `TBD`
- Event/WebSocket port(s): `TBD`
- DNS seeds: `TBD`
- Bootnode peer IDs/multiaddrs: `TBD`
- Public status endpoint: `TBD`
- Explorer/indexer endpoint(s), if official: `TBD`
- Checkpoint policy/initial checkpoints: `TBD`

## Mandatory separation tests

Before GO, automated tests must prove:

- mainnet chain ID != testnet chain ID;
- mainnet network/signing domain != testnet domain;
- mainnet genesis hash != testnet genesis hash;
- mainnet address/prefix configuration cannot be silently interpreted as testnet where network encoding supports separation;
- mainnet bootnode set is independent from testnet persistent bootnode identities;
- node handshake fails closed on network mismatch;
- miner job/submission fails closed on network mismatch;
- wallet signing/broadcast fails closed on network mismatch;
- contract/application/proof domain separation prevents cross-network replay.

## Bootstrap and peer discovery

The final launch record must include at least two independent bootstrap paths per network where operationally possible. For every official bootnode record:

- public peer ID;
- canonical multiaddr(s);
- operator/failure domain;
- region/provider class where disclosure is appropriate;
- expected transport/security profile;
- monitoring ownership.

DNS seeds must be owned and controlled by the project, have documented TTL/change procedures and never expose admin endpoints.

## Public API and operator boundaries

Freeze:

- public RPC methods/profile;
- request/body/query limits;
- rate-limit policy;
- CORS policy;
- TLS termination ownership;
- event-stream limits;
- admin/operator RPC bind policy;
- metrics exposure policy;
- wallet relay boundary.

Admin/operator surfaces must remain loopback/private-management by default and must not be included in public seed/RPC templates.

## Wallet and address/network identity

Before launch, the wallet must derive/display the active network unambiguously and bind signatures/submissions to the correct chain/domain. Freeze address encoding/prefix/HRP, network selection UX, derivation policy and transaction/contract signing domain.

## Checkpoints and fast bootstrap

If checkpoints or snapshot manifests are used, freeze their trust model and verification semantics. A checkpoint may accelerate bootstrap but must not silently redefine consensus. Record exact checkpoint height/score/hash/state-root and signer/attestation policy where applicable.

## Activation and upgrades

v3.0.0 genesis-time protocol activation must be explicit. Any feature not active at genesis requires an exact activation condition/version and mixed-version behavior. Future upgrades must rehearse on parallel testnet and require separately authorized mainnet activation.

A future cadence change is consensus-visible and monetary-visible. Its new target cadence and corresponding `target_interval_ns` MUST activate at the same frozen monetary score; changing one without the other is invalid.

## Required artifacts

The final freeze produces:

- network-specific canonical config files;
- config SHA-256 digests;
- genesis manifests and hashes;
- bootnode manifest;
- DNS/public-endpoint manifest;
- monetary-policy digest;
- monetary-score/cadence-segment digest;
- reward-settlement/finality digest;
- protocol/activation manifest;
- wallet/network-domain manifest;
- checkpoint/bootstrap manifest where applicable.

`GO_V3_DUAL_LAUNCH` is invalid while any launch-required field remains `TBD`.