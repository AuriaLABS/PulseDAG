# PulseDAG documentation

The repository's **current implementation/version surface remains v2.4.x**, while the definitive public-launch target is now **v3.0.0 in Q4 2026**.

The authoritative launch model is **mainnet + a parallel public testnet in one coordinated v3.0.0 release window**. The earlier standalone-public-testnet-first sequence and pre-mainnet 30-day testnet clock are superseded.

## Definitive v3.0.0 launch authority

- [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md) — authoritative Q4 launch roadmap.
- [`runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md`](runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md) — coordinated mainnet/testnet launch runbook.
- [`../configs/v3-launch/README.md`](../configs/v3-launch/README.md) — placeholder network-configuration freeze authority; not deployable until exact identities are frozen.
- Root [`../SECURITY.md`](../SECURITY.md) — v3 public/mainnet security boundary.

Issue authority:

- #781 — sole final `GO_V3_DUAL_LAUNCH` authority;
- #794 — v3 release/security/wallet/infrastructure/rehearsal completion;
- #803 — v3 dependency/reachability mainnet/public security gate;
- #819 — v3 production wallet/custody gate.

## Current v2.4.x implementation authority

The following remain important implementation/history references, but they do not define the final public launch sequence:

- [`ROADMAP_V2_4_0.md`](ROADMAP_V2_4_0.md)
- [`PROTOCOL_ACTIVATION_V2_4_0.md`](PROTOCOL_ACTIVATION_V2_4_0.md)
- [`BLOCK_HEADER_V2_CANONICALIZATION.md`](BLOCK_HEADER_V2_CANONICALIZATION.md)
- [`TRANSACTION_PROTOCOL_V2.md`](TRANSACTION_PROTOCOL_V2.md)
- [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md)
- [`VERSION_MATRIX.md`](VERSION_MATRIX.md)

The published v2.4.0 release and later v2.4.x/v2.4.1 work are development, validation and regression inputs. Existing v2.4 artifacts/evidence must not be relabeled as v3.0.0.

## Operator documentation

- [`RUNBOOK.md`](RUNBOOK.md)
- [`API_V1.md`](API_V1.md)
- [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md)
- [`POW_CURRENT_PATH.md`](POW_CURRENT_PATH.md)

Production v3 operator instructions must ultimately be bound to the exact frozen v3.0.0 artifact and separate mainnet/testnet identities.

## Evidence and launch gates

- [`RELEASE_EVIDENCE.md`](RELEASE_EVIDENCE.md)
- [`BURN_IN_GATE.md`](BURN_IN_GATE.md) — historical/private evidence policy input, not a standalone-public-testnet launch authority.
- [`ROADMAP_V3_0_LONG_LIVED_CORE.md`](ROADMAP_V3_0_LONG_LIVED_CORE.md) — supplemental engineering philosophy; its old staged public-testnet sequence is superseded by `ROADMAP_V3_0_0.md`.

Legacy v2.4 compatibility markers remain fail-closed in old tooling:

- Task31 marker: `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- default high cadence separately gated/disabled;
- `contracts_enabled=false`.

These markers are not the v3.0.0 launch state.

## Historical/future planning documents

- [`ROADMAP_V2_5_0.md`](ROADMAP_V2_5_0.md)
- [`ROADMAP_V2_6_0.md`](ROADMAP_V2_6_0.md)

Their useful requirements should be absorbed into #794 as needed; their prior version-by-version public-testnet sequence is not mandatory after the v3 rebaseline.

## Maintenance and history

- [`REPOSITORY_STANDARDS.md`](REPOSITORY_STANDARDS.md)
- [`archive/README.md`](archive/README.md)
- [`codex_tasks/`](codex_tasks/)

Historical v2.x evidence remains immutable provenance. Consensus/network identity changes require a fresh, explicitly versioned activation boundary and exact-candidate evidence; they must never be inferred from an old release branch, old private-testnet database or superseded launch plan.
