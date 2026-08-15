# PulseDAG v2.4.0 Protocol Scope and Activation Contract

Status: **Task 22 authoritative design contract**

Issue: `#865`

Date: 2026-08-15 UTC

## Purpose

This document freezes the compatibility and activation boundary for the protocol-level work added to PulseDAG v2.4.0 by Tasks 22–31.

It exists to prevent downstream transaction, consensus, storage, P2P, mining, wallet, replay, and release work from inventing incompatible activation rules independently.

The contract is intentionally conservative:

- existing v1 transaction, signing, block-header, block-hash, and historical replay semantics are immutable;
- v2.4.0 protocol behavior is introduced through explicit versioned paths;
- `ghostdag_dev` remains a non-release diagnostic mode and is never silently promoted into release consensus;
- the final v2.4.0 public-testnet protocol is activated on a clean chain boundary rather than by reinterpreting existing private/rehearsal history in place;
- any change to the frozen protocol identity invalidates evidence that depended on the previous identity.

This document does **not** authorize public-testnet launch, Day 0, the 30-day public-testnet clock, high cadence as a default, or smart-contract activation.

## 1. Current compatibility baseline

The repository currently has the following v1 behavior that must remain reproducible:

- `Transaction.version` exists and current canonical transaction/signing domains are v1;
- `BlockHeader.version` exists and current canonical block-header hashing is v1;
- `signing_message()` is the current v1 unsigned-transaction serialization;
- `compute_txid()` is the current v1 transaction serialization hash;
- `ConsensusMode::Legacy` is the default release-capable consensus mode;
- `ConsensusMode::GhostdagDev` enables development metadata only and never permits high cadence;
- persisted DAG state already contains selected-parent, selected-chain, merge-set, blue-work, and ordered-DAG metadata fields, but those fields do not by themselves constitute activated release consensus.

No downstream task may reinterpret those existing v1 byte encodings or historical meanings in place.

## 2. Frozen v2.4.0 protocol identity

The final activated v2.4.0 protocol uses the following version tuple:

| Surface | Legacy / historical | v2.4.0 activated |
| --- | --- | --- |
| Transaction version | `1` | `2` |
| Signing domain | `PulseDAG:unsigned-tx:v1` | `PulseDAG:unsigned-tx:v2` |
| Transaction/txid domain | `PulseDAG:tx:v1` | `PulseDAG:tx:v2` |
| Block header version | `1` | `2` |
| Block-header domain | `PulseDAG:block-header:v1` | `PulseDAG:block-header:v2` |
| Mining-preimage generation | legacy v1 path | explicit versioned v2 path |
| Consensus mode | `legacy` | `ghostdag_v1` |
| Development-only consensus mode | `ghostdag_dev` | remains development-only |
| High cadence | disabled | disabled by default; Task 29 may enable only an explicit experimental profile |
| Smart contracts | disabled | disabled |

`ghostdag_v1` is the release-capable name reserved for the deterministic selected-parent / bounded blue-red / DAG-order rules implemented by Tasks 24–28. It must be added explicitly; `ghostdag_dev` must not be renamed, aliased, or treated as equivalent.

## 3. Canonical chain identity

### 3.1 `chain_id` is consensus-domain identity

For v2.4.0, `chain_id` is the canonical network-domain identifier used by transaction/signing and versioned consensus hashing.

Rules:

1. `chain_id` is immutable for a launched network.
2. Distinct networks or distinct genesis definitions must not reuse the same `chain_id`.
3. `network_profile` is operator configuration and is **not** a substitute for canonical chain identity.
4. The final public-testnet `chain_id`, genesis definition, and resulting genesis hash must be frozen together in Task 31 evidence.
5. A node must fail closed when persisted protocol identity does not match configured `chain_id` and activation identity.

### 3.2 P2P network identity

P2P compatibility must validate at least:

- `chain_id`;
- genesis hash;
- activated consensus protocol/version capability.

Peers with a mismatched chain/genesis/activated consensus identity must not exchange consensus blocks as compatible peers.

A rolling-upgrade compatibility path may retain safe non-consensus capabilities where explicitly implemented, but it must never allow two different consensus identities to appear synchronized.

## 4. Transaction and signing v2 contract

Task 23 implements this section.

### 4.1 v1 is immutable

The following v1 behavior is frozen permanently for historical decoding/replay:

- `PulseDAG:unsigned-tx:v1` signing bytes;
- `PulseDAG:tx:v1` canonical transaction bytes;
- existing v1 txid derivation;
- existing v1 signature verification semantics.

No chain binding may be inserted into v1 bytes.

### 4.2 v2 chain binding

For transaction version `2`, the canonical `chain_id` is encoded into both:

1. the unsigned signing message; and
2. canonical transaction bytes used for txid derivation.

The required high-level byte order is:

```text
v2 signing message:
  len("PulseDAG:unsigned-tx:v2")
  "PulseDAG:unsigned-tx:v2"
  len(chain_id)
  chain_id
  transaction_version = 2
  canonical unsigned transaction fields...

v2 transaction / txid bytes:
  len("PulseDAG:tx:v2")
  "PulseDAG:tx:v2"
  len(chain_id)
  chain_id
  transaction_version = 2
  canonical signed transaction fields...
```

All integer and length encodings must keep the repository's canonical little-endian conventions unless Task 23 documents a separately reviewed versioned primitive.

Consequences:

- the same logical spend signed for two distinct `chain_id` values must produce different signing messages;
- signatures created for one chain must fail verification on another chain;
- canonical v2 txids are chain-bound;
- a missing or wrong v2 chain identity fails closed.

### 4.3 v1/v2 admission

The default release rules are profile-gated rather than ambiguous mixed acceptance:

- `legacy` consensus/profile accepts historical/current v1 behavior and does not silently treat v2 as v1;
- activated `ghostdag_v1` v2.4.0 release networks accept transaction v2 and reject ordinary v1 mempool/RPC submissions by default;
- development/rehearsal tooling may expose an explicit mixed-version compatibility profile only for tests, and that profile is never public-testnet-ready;
- historical v1 blocks and transactions remain decodable/replayable under their original chain/protocol context.

Stable rejection reasons must distinguish unsupported version, inactive version, wrong chain domain, duplicate, conflict, replacement, and malformed canonical encoding.

## 5. Block header and PoW v2 contract

Tasks 24–29 implement the consensus work that depends on this boundary.

### 5.1 v1 header semantics are immutable

Existing `BlockHeader.version = 1`, `PulseDAG:block-header:v1`, block hash behavior, mining preimage behavior, and legacy `blue_score` interpretation remain valid only under their historical protocol rules.

They must not be redefined to mean activated GHOSTDAG consensus.

### 5.2 v2 header path

Activated v2.4.0 consensus uses `BlockHeader.version = 2` and a separate canonical v2 block-header/hash path.

The v2 canonical header and mining preimage must include `chain_id` in the versioned consensus domain so a block candidate is not portable as a valid consensus object across distinct PulseDAG chains.

The versioned path must use:

- header domain `PulseDAG:block-header:v2`;
- explicit chain-id encoding;
- header version `2`;
- deterministic parent serialization required by the activated GHOSTDAG contract;
- activated `blue_score` semantics only after deterministic selected-parent / blue-red rules are available;
- the same canonical target/retarget snapshot for template construction and validation.

Task 24/25 may introduce metadata before activation, but loading a binary containing that metadata must not silently change v1 block validity.

## 6. Consensus activation modes

The accepted mode model is:

### `legacy`

- default for historical/private compatibility until an explicit v2.4.0 activation profile is selected;
- preserves v1 consensus meaning;
- no release GHOSTDAG semantics;
- high cadence disabled.

### `ghostdag_dev`

- development diagnostics only;
- may expose selected-parent, merge-set, blue/red, ordered-DAG, replay, and harness diagnostics as they are implemented;
- never public-testnet-ready;
- high cadence disabled unless a later explicit development-only gate is added by Task 29;
- must not be accepted as equivalent to the release protocol.

### `ghostdag_v1`

- release-capable activated consensus mode introduced by Tasks 24–28;
- requires header v2 and transaction/signing v2 release rules;
- requires deterministic selected-parent, selected-tip, blue/red classification, DAG ordering, transaction conflict handling, state application, finality/pruning, P2P sync, mining, mempool, wallet, replay, and recovery behavior;
- remains blocked from final release until Task 30 passes and Task 31 records GO.

The mode must be visible in status/readiness/evidence surfaces and must participate in persisted activation identity.

## 7. Activation strategy for v2.4.0

### 7.1 Final v2.4.0 candidate: clean protocol chain

Because the public testnet has not yet been authorized/launched, the preferred and frozen v2.4.0 release strategy is a **clean protocol-chain activation**, not an in-place reinterpretation of earlier private/rehearsal history.

The final v2.4.0 public-testnet candidate must therefore freeze a fresh, internally consistent identity comprising:

- final `chain_id`;
- final genesis definition and genesis hash;
- header version `2`;
- transaction/signing version `2`;
- consensus mode `ghostdag_v1`;
- final GHOSTDAG constants and ordering version;
- final difficulty/retarget constants;
- storage/snapshot activation identity;
- P2P protocol capability identity.

Earlier private/rehearsal chains remain evidence/history and are not silently upgraded by reinterpreting their v1 blocks.

### 7.2 No implicit activation height

No implicit height/time/environment-variable activation is allowed for the final v2.4.0 protocol.

If maintainers later choose to preserve an existing chain instead of the clean-chain strategy, that becomes a separate reviewed contract change requiring:

- an explicit activation height/block;
- mixed-version validation rules;
- rollback rules;
- replay fixtures covering both sides of the boundary;
- refreshed Task 30 evidence;
- refreshed Task 31 approval.

Until such a change is explicitly merged, clean-chain activation is authoritative.

## 8. Storage, snapshots, replay, and migration

Tasks 24 and 26 must add a persisted activation identity sufficient to fail closed on mismatched state.

At minimum persisted/snapshot metadata must identify:

- `chain_id`;
- genesis hash;
- transaction protocol version;
- block-header protocol version;
- consensus mode/version;
- DAG ordering version;
- storage/snapshot schema version or equivalent compatibility identifier.

Rules:

1. Opening v1 state under activated `ghostdag_v1` must not silently reinterpret v1 state as v2 state.
2. Opening v2 state under `legacy` must fail closed unless an explicit read-only/replay path exists.
3. Snapshot restore must verify activation identity before publishing restored state.
4. Deterministically recomputable metadata may be rebuilt only when the protocol/schema contract explicitly allows it.
5. Required historical data that cannot be recomputed must block restore/activation rather than being guessed.
6. Replay tools must select the historical protocol from recorded identity, not from the operator's current default.

## 9. P2P and rolling-upgrade contract

Task 27 must provide capability negotiation for the activated consensus path.

Required behavior:

- v2.4.0 peers advertise enough version/capability identity to distinguish `legacy`, `ghostdag_dev`, and `ghostdag_v1` consensus compatibility;
- consensus block exchange/sync requires compatible chain/genesis/consensus identity;
- older peers must not receive undecodable v2 message variants;
- unsupported capabilities produce explicit compatibility status rather than false malicious-peer penalties;
- a node must never report selected-sync complete using consensus-incompatible peers;
- mixed-version tests are mandatory even though the final public network must converge on one frozen release identity.

## 10. Mining, mempool, wallet, and RPC contract

Task 28 must consume the same activation identity rather than independent local rules.

### Mining

- v2 templates use activated selected-tip/selected-parent context;
- header version/domain and `chain_id` are fixed by the active protocol profile;
- miners cannot override consensus versions or chain identity through local environment variables;
- submission finality remains keyed to the exact candidate block identity.

### Mempool / RPC

- admission is version/profile aware;
- wrong-chain v2 signatures fail closed;
- v1 rejection on an activated release network is explicit;
- replacement/RBF and submission identity semantics are frozen by Task 23 and must not vary by node timing.

### Wallet

- the wallet obtains/displays the target `chain_id` before signing;
- v2 offline signing includes the same canonical chain domain as node verification;
- broadcasting a transaction to the wrong chain produces a stable wrong-domain/version rejection;
- wallet-plan identity, submission identity, and final txid remain distinct concepts where Task 23 requires them.

## 11. Rollback and downgrade contract

### Before public launch

A v2.4.0 candidate may be abandoned and replaced, but any consensus/protocol identity change requires fresh affected evidence.

Returning from a v2 candidate to `legacy` means returning to a compatible legacy data directory/snapshot or starting a clean legacy chain. A v2 data directory must not be downgraded in place by ignoring unknown metadata.

### After public launch

There is no supported in-place downgrade from an activated `ghostdag_v1` public chain to `legacy` consensus.

Recovery must use:

- the same activated protocol version and a known-good snapshot/state; or
- a separately specified forward activation/repair protocol.

Any emergency consensus rollback is a new protocol decision and cannot be inferred from this document.

## 12. Candidate and evidence invalidation rules

The following changes invalidate affected burn-in, replay, multi-node, launch-rehearsal, or release evidence and require rerun on the new exact SHA:

- `chain_id` or genesis definition/hash;
- transaction version, signing domain, chain-domain encoding, canonical transaction serialization, or txid derivation;
- block-header version/domain, canonical header serialization, mining preimage, PoW validation, or target/retarget semantics;
- consensus mode/version or activation strategy;
- selected-parent/tip scoring or tie-break rules;
- merge-set discovery, `K`/bounds, blue/red classification, or overflow rules;
- DAG ordering or transaction-conflict semantics;
- state application/rollback/finality/pruning rules;
- storage/snapshot schema or migration rules that affect replay/state identity;
- P2P capability or sync semantics that affect consensus convergence;
- mining-template semantics that affect consensus parent/transaction selection;
- any security fix that changes signed/canonical consensus bytes.

Pure documentation wording, dashboards, non-consensus observability, or packaging changes do not automatically invalidate consensus evidence, but Task 31 must explicitly disposition any change made after evidence capture.

Evidence from different protocol identities or candidate SHAs must never be combined to manufacture a PASS.

## 13. Task dependency contract

The implementation dependency is frozen as:

```text
Task 22 activation contract
   |\
   | +--> Task 23 transaction/signing v2
   |
   +----> Task 24 GHOSTDAG data model
              |
              v
           Task 25 deterministic selection/classification
              |
              v
           Task 26 DAG ordering/state/finality
             / \
            v   v
       Task 27  Task 28
          \       /
           \     /
            v   v
            Task 29
               |
               v
            Task 30
               |
               v
            Task 31
```

Task 23 and Task 24 may proceed independently after Task 22, but no downstream task may contradict this activation contract without first amending Task 22 and invalidating affected evidence.

## 14. Release/readiness requirements

Task 31 may not record `GO_V2_4_0_RELEASE_AND_ACTIVATION` unless:

- transaction/signing v2 is chain-bound and its golden vectors are frozen;
- header/PoW v2 canonical vectors are frozen;
- `ghostdag_v1` deterministic consensus is implemented and separately distinguishable from `ghostdag_dev`;
- storage/replay/snapshot identity is fail-closed;
- P2P capability negotiation prevents consensus-incompatible sync;
- mining/mempool/wallet agree on the same activated protocol identity;
- Task 30 passes on one exact candidate SHA;
- high cadence is either explicitly approved or remains disabled;
- smart contracts remain disabled;
- public-testnet launch is still separately authorized by `#781`.

## 15. Acceptance checklist for Task 22

- [x] Freeze v1 immutability boundary.
- [x] Freeze transaction/signing v2 version and domain names.
- [x] Freeze v2 chain binding through canonical `chain_id` encoding.
- [x] Freeze header v2 requirement for activated GHOSTDAG consensus.
- [x] Separate `ghostdag_dev` from release-capable `ghostdag_v1`.
- [x] Freeze clean-chain activation as the default final v2.4.0 strategy.
- [x] Freeze default v1/v2 admission behavior.
- [x] Freeze storage/snapshot fail-closed identity requirements.
- [x] Freeze P2P consensus-compatibility requirements.
- [x] Freeze rollback/downgrade boundaries.
- [x] Freeze evidence invalidation rules.
- [x] Freeze Tasks 23–31 dependency relationship.

Task 22 is complete when this contract is merged and downstream implementation issues/PRs cite it.