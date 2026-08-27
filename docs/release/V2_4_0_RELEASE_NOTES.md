# PulseDAG v2.4.0 release notes

Status: **TASK31 CANDIDATE PREPARATION — NOT RELEASED / NOT ACTIVATED**

These notes describe the current v2.4.0 technical candidate. They do not authorize a tag, publication, protocol activation, public-testnet launch, Day 0, default high cadence, smart contracts, or an official end-user custody wallet.

## Protocol and consensus scope

PulseDAG v2.4.0 introduces an explicit activated protocol path rather than reinterpreting historical v1 data in place:

- transaction/signing version 2 is chain-bound to the canonical `chain_id`;
- block-header/mining-preimage version 2 is chain-bound and versioned;
- `ghostdag_v1` is the release-capable deterministic DAG consensus mode and remains distinct from development-only `ghostdag_dev`;
- persisted storage/snapshot activation identity fails closed on incompatible chain/protocol/schema state;
- P2P compatibility and sync require compatible chain/genesis/activated-consensus identity;
- mining, mempool and RPC admission consume the same activation identity.

The authoritative compatibility, mixed-version, migration and rollback contract is `docs/PROTOCOL_ACTIVATION_V2_4_0.md`.

## Node and miner scope

The technical candidate contains:

- the `pulsedagd` node;
- the standalone `pulsedag-miner` external miner;
- deterministic and adversarial Task30 validation gates;
- packaged-node/miner recovery validation;
- exact-SHA packaging/provenance checks;
- a fail-closed `public_safe` RPC profile and pre-GO public-testnet configuration templates;
- the v2.4.0 release-candidate observability package under `ops/observability/v2.4.0/`.

The CPU miner path remains the canonical operational reference. No pool, payout or accounting service is implied.

## Wallet boundary

There is **no official end-user custody wallet** in this v2.4.0 candidate. Legacy raw-private-key node wallet RPC flows have been removed/tombstoned from the supported architecture.

Issue #819 remains the professional wallet/custody program. It blocks advertising or distributing an official end-user wallet, but does not by itself authorize or prohibit a separately reviewed node-only public testnet.

## Security boundary

Issue #803 remains the authoritative dependency-security/public-GO record. Reachable `atty 0.2.14`, `linkme 0.2.10` and `lru 0.12.5` remain visible public-testnet blockers under the current fail-closed disposition until a supported parent-stack upgrade removes them or a separate reviewed disposition explicitly changes the launch decision.

A stable expected RustSec warning set is **not** security approval. Unsupported transitive leaf patches or unreleased dependency migrations are not accepted merely to make a scanner green.

See root `SECURITY.md` for private vulnerability reporting and severity handling.

## Upgrade, migration and rollback

The v2.4.0 public-testnet strategy is a clean activated protocol chain with one frozen chain ID/genesis/protocol identity.

- Historical v1 transaction/header/replay semantics remain immutable.
- Mixed-version behavior is explicit and capability-gated; incompatible peers cannot claim consensus sync.
- v1 state is not silently reinterpreted as activated v2 state.
- Snapshot restore validates activation identity before publication.
- Before launch, a candidate may be abandoned, but affected exact-SHA evidence must be rerun on its replacement.
- After activated public launch, there is **no supported in-place downgrade from `ghostdag_v1` to `legacy`**. Recovery uses the same activated protocol and known-good state/snapshot, or a separately reviewed forward repair/activation protocol.

## Packaging and provenance

Release evidence is exact-SHA scoped. Final candidate freeze must bind the source SHA to version identity, chain/genesis/config identity, node/miner package digests, provenance and the workflow/evidence artifacts that tested that same candidate.

Linux and Windows packages must be verified from extracted release archives, not substituted with binaries from `target/`.

Evidence from different source SHAs or activation contracts must not be combined to claim final readiness.

## Observability and operations

The v2.4.0 package under `ops/observability/v2.4.0/` polls only public-safe read surfaces (`/metrics`, `/status`, `/mempool`) and does not require admin/runtime RPC or secrets. Its inventory and alerts cover commit/state-root safety, snapshot verification, mining-submit actor health, P2P recovery, sync convergence and RPC liveness in addition to basic node/mempool state.

Repository dashboards/templates do not prove real public infrastructure. Failure-domain separation, persistent P2P identities, DNS/TLS ownership, firewall rules, NTP, storage/backup, on-call ownership and incident/recovery drills remain launch evidence in #794/#781.

## Public-testnet boundary

Issue #781 is the only public-testnet launch-control record. Until it records an explicit `GO_PUBLIC_TESTNET` and the actual launch is recorded:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- default high cadence is not authorized;
- smart contracts remain disabled;
- no Day 0 timestamp may be claimed.

The private 24-hour burn-in in #789 begins only on one intentionally frozen, unchanged candidate after all prerequisites for starting that clock are satisfied.

See `docs/release/V2_4_0_KNOWN_LIMITATIONS.md`, `docs/runbooks/V2_4_0_PUBLIC_TESTNET_PREP.md`, #873, #789, #794, #803 and #781.
