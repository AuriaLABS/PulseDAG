# PulseDAG version matrix

## Current active baseline

| Surface | Active value |
| --- | --- |
| VERSION file | `v2.4.0` |
| Cargo workspace version | `2.4.0` |
| Current milestone | v2.4.0 software release |
| Release decision | `APPROVE_TAG_AND_PUBLICATION` after final exact-SHA release gates |
| Authoritative release identity | immutable commit referenced by tag `v2.4.0` |
| `public_testnet_ready` | `false` |
| `thirty_day_public_testnet_clock_started` | `false` |
| `contracts_enabled` | `false` |

The pre-bump implementation baseline was validated on exact SHA `8a1a5f74e03eae695e76bf8a84ddc9d48f94db34`. That evidence remains historical provenance. The published v2.4.0 release identity is defined by the immutable `v2.4.0` tag and its attached artifact provenance.

## Version progression

| Version | Status | Scope |
| --- | --- | --- |
| v2.2.x | Historical | Earlier private-testnet development and evidence |
| v2.3.0 | Historical release baseline | Previous private-testnet release and operational evidence |
| v2.4.0 | Active software release | Consensus retarget hardening, sync/recovery, public-safe RPC, security gates, professional wallet boundary, release/infrastructure rehearsal |

## v2.4.0 identity

Private v2.4.0 validation uses the dedicated identity:

- network profile: `private-testnet-v2.4.0`;
- chain ID: `pulsedag-private-v2.4.0`;
- consensus mode: `legacy` where required by the supported v2.4 single-parent/tip policy.

Final public-testnet chain identity, genesis/configuration digests, bootnodes, launch timestamp and Day 0 must be frozen and recorded by the separate launch process. They must not be inferred from a private candidate or from the software release tag alone.

## Release authorization boundary

The v2.4.0 software tag and GitHub Release are authorized once the final exact-SHA software-release gates pass. This authorization does not imply:

- `GO_PUBLIC_TESTNET`;
- public ingress or Day 0;
- start or backdating of the 30-day public-testnet clock;
- smart-contract activation;
- production/mainnet custody claims.

Public launch authorization remains a separate explicit decision after the required private burn-in, recovery drills, 5-node/4-miner rehearsal, security disposition, release identity freeze and infrastructure evidence are complete.

## Repository version rule

Active release surfaces must identify v2.4.0. References to v2.3.0 or earlier are allowed only when they are clearly historical, compatibility-related, immutable evidence, or named legacy helpers/workflows whose identity is intentionally retained.
