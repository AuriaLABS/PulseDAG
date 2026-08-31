# PulseDAG v3.0.0 genesis contract

Status: **PRE-FREEZE / NO PRODUCTION GENESIS EXISTS YET**

Authority: #781 records the final accepted mainnet and parallel-testnet genesis identities. This document defines how those identities must be produced and verified.

## Core rule

PulseDAG v3.0.0 launches two independent networks. Mainnet and the parallel public testnet MUST have independently frozen genesis blocks and network identities. A development/private/testnet genesis from v2.x must never be promoted by renaming it.

The current implementation already demonstrates chain-bound genesis construction: the genesis transaction ID and block hash depend on `chain_id`. v3 must preserve and strengthen that separation.

## Required genesis inputs

For each network, freeze an input manifest containing at minimum:

- release/source SHA and tree SHA;
- genesis-generator version and source digest;
- network profile name;
- chain ID;
- network/signing domain identifier;
- block-header protocol version;
- transaction protocol version;
- consensus/ordering/GHOSTDAG identity;
- PoW algorithm/version;
- initial difficulty/target encoding;
- timestamp policy and exact timestamp;
- genesis nonce;
- parent set (normally empty by explicit rule);
- canonical genesis message/commitment, if used;
- monetary-policy version and digest;
- exact genesis issuance/allocation list;
- contract/VM/proof activation state at genesis;
- consensus/config digest;
- serialization version.

No field may be supplied from an undocumented environment default during the production ceremony.

## Required deterministic outputs

The generator must emit, for each network:

- canonical serialized genesis bytes;
- genesis transaction bytes and TXID(s);
- genesis Merkle root;
- initial UTXO/state root;
- canonical block-header bytes;
- genesis block hash;
- network/config digest;
- monetary-policy digest used by the generator;
- human-readable decoded manifest;
- machine-readable manifest;
- generator binary/source digest.

At least two clean independent executions from the same frozen inputs must produce byte-identical outputs before freeze is accepted.

## Production allocation safety

The current development implementation creates a `GENESIS_SUPPLY` output to the placeholder `genesis-treasury`. That is development behavior only.

The v3 production generator MUST reject placeholder destinations such as `genesis-treasury`. It must create only allocations present in the approved v3 monetary-policy manifest. The sum of all genesis outputs must exactly equal the approved genesis-issued supply.

If mainnet policy approves zero spendable genesis issuance, the generator must prove there are no spendable allocation outputs. If non-zero issuance is approved, every allocation must be public in the freeze manifest.

## Network separation

Mainnet and parallel testnet must differ by at least the frozen network identity inputs required by the protocol, and must never accidentally share a valid identity. Required checks include:

- different chain IDs;
- different signing/network domains;
- different genesis hashes;
- different genesis transaction IDs where chain binding applies;
- independent persistent bootnode identities;
- network-specific address/HRP/prefix configuration where applicable;
- no cross-network handshake/bootstrap acceptance;
- wallet/miner/node network mismatch fails closed.

The CI/freeze validator must fail if final mainnet and testnet chain IDs or genesis hashes are equal.

## Genesis timestamp

The final timestamp is a freeze input, not `current_ts()` at runtime. The exact UTC timestamp and derivation/approval record must be published. All ceremony participants use the same value.

## Difficulty and PoW

The genesis difficulty/target must be explicitly frozen. It may use a special genesis rule only if that rule is implemented and documented. Subsequent blocks must transition to the exact production difficulty-retarget contract.

The final evidence must verify:

- genesis header encodes the approved target/difficulty;
- genesis hash is accepted by the exact v3 node under the explicit genesis exception/rule;
- block 1 uses the intended production PoW/difficulty transition;
- CPU, NVIDIA and AMD implementations agree on PoW semantics for mineable blocks.

## Initial state

The genesis state commitment must cover all consensus-visible initial state required by v3, including as applicable:

- UTXO set;
- monetary allocations;
- native-asset registries/reserved namespaces;
- contract runtime activation/config state;
- protocol activation identity;
- any consensus-visible governance/upgrade commitment if approved.

No hidden filesystem/config state may affect the resulting genesis hash or initial consensus state.

## No embedded secrets

Genesis/config artifacts must not contain private keys, seed phrases, TLS private keys, cloud credentials or operator secrets. Bootnode peer IDs are public identities; their private keys remain operator secrets and are provisioned separately.

## Genesis freeze record

The final `docs/V3_0_0_LAUNCH_MANIFEST.md` must record separately for mainnet and testnet:

- chain ID;
- network/signing domain;
- genesis input-manifest SHA-256;
- canonical genesis-bytes SHA-256;
- genesis TXID(s);
- Merkle root;
- state root;
- genesis hash;
- config digest;
- monetary-policy digest;
- generator source/binary digest;
- independent verification evidence;
- approval reference.

Any change to a genesis input after freeze creates a new genesis identity and invalidates all network-specific launch rehearsal evidence.