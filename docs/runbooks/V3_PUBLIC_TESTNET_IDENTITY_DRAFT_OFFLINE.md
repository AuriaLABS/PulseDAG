# Native-v3 public-testnet identity — offline draft shape check

Status: **PROPOSED / UNAPPROVED DRAFT ONLY**. No testnet or mainnet GO.
Authority: [#781](https://github.com/AuriaLABS/PulseDAG/issues/781), [#1049](https://github.com/AuriaLABS/PulseDAG/issues/1049), [#1418](https://github.com/AuriaLABS/PulseDAG/issues/1418).

This tool checks **JSON field shape only** for a hypothetical future public-testnet identity draft. It neither verifies a genesis against running nodes, validates PoW/GHOSTDAG/GD-08, opens storage, checks real release hashes, proves bootnode ownership, activates the v3 monetary runtime, calls the P2P/miner interfaces, nor sets an authorization. A PASS is **not evidence for GO**. All positive fixtures in the regression tests are synthetic, not frozen source or infrastructure identities.

## Why this remains offline

The current daemon in `apps/pulsedagd/src/startup_protocol.rs` accepts only `legacy` and `ghostdag_v1` startup. It deliberately rejects `monetary_v3` and persisted monetary activation sidecars; `configs/public-testnet/*.template` remain pre-GO legacy-mode examples with placeholder identity, public P2P OFF, readiness OFF and contracts OFF. **Do not modify or simply flip those flags to start a v3 testnet.**

## JSON identity draft contract

`scripts/v3_public_testnet_identity_shape_gate.py` requires an exact set of fields:

- `schema=pulsedag.v3.public-testnet-identity-draft.v1`, `status=UNAPPROVED_DRAFT`, `network_profile=public-testnet-v3`, `consensus_candidate=native-v3-unapproved`.
- Unique proposed `chain_id` within the `pulsedag-public-testnet-v3` namespace, distinct `signing_domain`, nonzero lowercase 32-byte `genesis_hash`, positive `genesis_timestamp_ns` (u64).
- Exact-shape proposed `source_git_sha`, `node_artifact_sha256`, `miner_artifact_sha256`, `config_sha256`; current monetary-policy SHA-256 and cadence SHA-256 as **syntactic candidate inputs**, not independently verified bytes.
- Two to sixteen distinct proposed `bootnode_peer_ids` decoded from **canonical Base58btc multihash** or **CIDv1 lowercase base32** (CID version `1`, multicodec `libp2p-key=0x72`). For this pre-GO draft, the *binary framing* must be either Ed25519 identity multihash with the expected 36-byte protobuf public-key field, or a 32-byte SHA2-256 multihash. A repeated multihash is rejected **even if encoded once as Base58btc and once as CIDv1**. Invalid Base58 digits `0/O/I/l`, CID bits, nonminimal varints, wrong CID versions/codecs, lengths and unsupported multihash algorithms fail closed. This **does not validate Ed25519 curve points, a SHA2-256 digest against a supplied public key, ownership, peer reachability, release artifacts, or a trusted bootnode roster**. Synthetic test IDs remain untrusted even if they decode correctly.
- JSON `false` for **all** `monetary_runtime_enabled`, `contracts_enabled`, `admin_enabled`, `public_testnet_ready`, `thirty_day_clock_started`, `allow_legacy_fallback`.
- `future_mainnet_identity` is **null** when mainnet has no draft identity, or a three-field **unapproved** object `{chain_id, genesis_hash, signing_domain}`. If present, its chain ID must be scoped `pulsedag-mainnet-v3` (optionally with lowercase suffixes), its signing domain `pulsedag:v3:mainnet:<name>`, and neither its chain ID, genesis hash nor signing domain may coincide with public testnet. Shape checks NEVER freeze mainnet or authenticate that the supplied mainnet values correspond to actual artifacts; a final cross-network isolation proof remains mandatory.

Duplicate JSON fields, missing/unexpected fields, malformed hashes, non-Boolean false aliases, placeholders, chain/signing-domain substitutions, malformed or aliased bootnode multihashes and candidate status promoted to GO are rejected. Unknown fields are rejected to prevent unnoticed activation or credential material.

Example invocation **on a local, non-secret draft only**:

```bash
python3 scripts/v3_public_testnet_identity_shape_gate.py /path/to/UNAPPROVED-DRAFT.json
python3 -m unittest discover -v -s scripts/tests -p 'test_v3_public_testnet_identity_shape_gate.py'
```

A successful output explicitly sets `public_testnet_launch_authorized=false`, `mainnet_launch_authorized=false`, `mainnet_cross_identity_frozen=false`, `source_and_artifacts_authenticated=false` and `monetary_runtime_enabled=false`. A supplied future-mainnet draft only sets `future_mainnet_identity_shape_checked=true` (syntactic validation, not proof). It must **never** be interpreted as a runnable config, attestation, readiness clock start or approval to deploy a network.

## Independent remaining release gates

Before an actual `GO_PUBLIC_TESTNET`: implement and freeze native-v3 GHOSTDAG/GD-08 and the monetary/PoW/UTXO/reward runtime, verify exact source-to-artifact/identity consistency **independently of this tool**, make the future native-v3 testnet-only activation explicitly identity-bound and fail-closed for mainnet/legacy, prove security and operator readiness, then obtain the explicit decision in #781.

Once authorized, public testnet becomes the **first live native-v3 monetary-mining network** (no mandatory earlier private live monetary rehearsal). Reorg, prune, multi-node convergence and >=30 days of observed live evidence are testnet tasks preceding a separate `GO_V3_MAINNET`. Smart-contract execution/deployment remains **INACTIVE on both v3.0 networks**, regardless of testnet duration.
