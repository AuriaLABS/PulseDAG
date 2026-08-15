# PulseDAG v2.4.0 release notes

PulseDAG v2.4.0 is approved for software tag and GitHub Release publication once the final exact-SHA release gates pass. Public-testnet launch remains a separate decision.

## Overview

v2.4.0 advances PulseDAG from the v2.3.0 private-testnet baseline toward a public-testnet-eligible node and wallet architecture. The release focuses on canonical PoW retarget behavior, deterministic mining-submit observability, sync/recovery hardening, public-safe RPC exposure, dependency-security gates, and professional local wallet custody/signing boundaries.

## Highlights

### Canonical PoW retarget contract

- Replaces the absorbing integer-difficulty floor behavior with canonical target/compact-bits retarget semantics.
- Uses selected-chain history and fixed consensus parameters.
- Keeps node validation, mining templates and PoW diagnostics aligned on the same current/next target state.
- Preserves the external standalone miner model.

### Mining-submit finality and miner telemetry

- Distinguishes definitive rejection from `submit_finality_unknown` after enqueue.
- Reconciles unknown-finality submissions by block identity rather than blindly resubmitting stale work.
- Separates ordinary nonce-search exhaustion from backend verification failure/invalid PoW telemetry.
- Adds counters/evidence suitable for long-running burn-in and rehearsal review.

### Sync, pruning and recovery hardening

- Adds deterministic recovery coverage for rejoin/common-ancestor and missing-parent/orphan progress failures discovered during private validation.
- Adds pruning-aware retained-history capability semantics so directed historical catch-up can avoid explicitly incompatible peers while retaining mixed-version fallback behavior.
- Keeps archival/seed history requirements explicit until a separate checkpoint/state-sync design exists.

### Public-safe RPC boundary

- Bounded/evicting per-IP rate-limit state.
- Explicit deny-by-default/allowlisted CORS and trusted-proxy boundary.
- Negative route coverage for admin, wallet-custody, mining, snapshot, prune and rebuild operations on the public listener.
- Canonical signed-transaction-only relay at `POST /api/v1/tx/submit` without node-side key custody.

### Professional wallet foundation

- Versioned encrypted deterministic-seed keystore using reviewed password KDF/AEAD primitives.
- Secret redaction/zeroization and bounded lock/unlock session lifecycle.
- Deterministic BIP-39 restore and hardened Ed25519 account/receive/change derivation.
- Secret-free watch-only backup manifests and verification.
- Reviewed transaction plans, deterministic v1 wallet nonce/retry semantics and local signing.
- Dedicated local wallet application flows for restore, address derivation, watch-only operations, transaction preview/signing and relay identity verification/broadcast.
- Normal `pulsedagd`/`pulsedag-rpc` builds are keyless; the legacy raw-private-key wallet RPC implementation is removed and historical route names remain fail-closed.

PulseDAG transaction/signing v1 is intentionally **not** described as cryptographically chain-bound. The v2.4 wallet verifies network/chain identity separately and fails closed; a future versioned chain-bound transaction/signing contract is tracked independently and is not silently introduced into v1.

### Dependency and release security

- Permanent RustSec/dependency audit gates tied to the committed lockfile.
- Public-safe security policy and route isolation checks.
- Exact-SHA validation requirements for repository hygiene, P2P, lint, RPC/release, wallet and release-preparation gates.
- Remaining reachable unsound/unmaintained dependency disposition must be closed before public GO according to the launch programme.

### v2.4.0 private identity and launch gates

- Dedicated private identity: `private-testnet-v2.4.0` / `pulsedag-private-v2.4.0`.
- First-class isolated single-node burn-in profile.
- Private burn-in with restart, snapshot, compact prune, restore and real-P2P second-node rejoin evidence remains part of public-launch qualification.
- Mandatory 5-node/4-miner private launch rehearsal remains required before a public-testnet GO decision.

## Compatibility and operator action

### Versioning

`VERSION`, workspace package versions and the local PulseDAG entries in `Cargo.lock` are `v2.4.0`/`2.4.0`. The version bump does not include unrelated dependency upgrades.

### Storage and chain identity

Do not reuse stale v2.3 private network identity or databases as v2.4 private burn-in state. Preserve evidence and rollback material, but initialize the required v2.4 test identity/state according to the active runbook.

### Wallet and RPC

End-user wallet secrets stay local. Public/operator node RPC must never receive private keys, mnemonic/seed material, wallet passwords or decrypted keystore state. Signed transaction relay accepts only fully formed signed transactions.

### Mining

Mining remains external. Install and operate `pulsedag-miner` separately from `pulsedagd`.

## Release assets

The release workflow builds separate node and standalone-miner archives for Linux x86_64, Windows x86_64 and macOS x86_64. Each archive requires checksum, manifest, provenance attestation and native unpack/smoke verification. Installation and verification instructions are in `docs/INSTALL_BINARIES_V2_4_0.md`.

An official wallet binary, if included in a future publication set, requires its own explicit artifact and custody/restore/sign/broadcast validation record.

## Known limitations / boundaries

- Publishing v2.4.0 does not imply public-testnet GO.
- Smart contracts remain disabled pending the separate post-public-testnet acceptance decision.
- Production/mainnet custody readiness is not claimed.
- Protocol-level cryptographic chain binding/replay/RBF semantics are deferred to a future versioned transaction/signing format rather than mutating v1 in place.
- Public seed/archival history remains required for reliable fresh-node bootstrap until a separately designed checkpoint/state-sync mechanism exists.

## Release state

- Repository version: `v2.4.0`.
- Release decision: `APPROVE_TAG_AND_PUBLICATION` after final exact-SHA software-release gates.
- Authoritative release SHA: the immutable commit referenced by tag `v2.4.0`.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- `contracts_enabled=false`.
