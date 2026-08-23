# v2.4.0 Task31 RustSec warning disposition

Status: active, temporary and fail-closed

Owner: `kalekoi`

Review deadline: `2026-08-31 UTC`

Hard expiry: before any public-testnet GO decision, or immediately when any listed package version, direct parent, supported target, allocator policy, Kaspa/workflow dependency line or warning set changes.

## Decision boundary

This disposition permits only the exact Task31 **technical node + miner candidate** and private, valueless validation. It does not authorize public exposure, public-testnet GO, Day 0, the 30-day clock, contracts, production custody or mainnet claims.

The two reachable unsound warnings (`atty` and `linkme`) remain **public-testnet blockers**. They must be removed through a supported parent-stack upgrade or an upstream-reviewed fix before public-testnet GO. They are not ignored by `.cargo/audit.toml` and remain visible in every pinned audit report.

## Exact-candidate evidence rule

The dependency workflow regenerates Linux compiler-artifact reachability from empty Cargo target directories for `pulsedagd` and `pulsedag-miner` on every candidate. The evidence JSON records the exact PR head SHA rather than reusing a historical release-branch SHA.

The earlier release-line Windows evidence is useful historical classification only. **Windows exact-candidate revalidation remains pending** and is mandatory before any public-testnet GO or claim that packaged Windows artifacts satisfy the final security matrix.

## Warning inventory and current Linux expectation

| Advisory | Package | Exact-candidate Linux expectation | Disposition |
| --- | --- | --- | --- |
| `RUSTSEC-2025-0052` | `async-std 1.13.2` | compiled into node and miner | Unmaintained; private-only temporary acceptance through the supported workflow/Kaspa line. |
| `RUSTSEC-2024-0375` | `atty 0.2.14` | compiled into node and miner | Unmaintained; removal tracks the same supported workflow/hexplay migration as the unsound advisory. |
| `RUSTSEC-2021-0145` | `atty 0.2.14` | package compiled into node and miner | Windows-specific unsoundness remains a public-testnet blocker; PulseDAG must preserve the no-custom-global-allocator invariant until removal/re-review. |
| `RUSTSEC-2025-0141` | `bincode 1.3.3` | node yes; standalone miner no | Unmaintained; retained for storage/schema compatibility. Replacement requires an explicit migration and new candidate evidence. |
| `RUSTSEC-2024-0384` | `instant 0.1.13` | compiled into node and miner | Unmaintained; remove through supported workflow/Kaspa upgrade. |
| `RUSTSEC-2024-0407` | `linkme 0.2.10` | compiled into node and miner | Reachable unsoundness; public-testnet blocker until a supported Kaspa/intertrait migration passes the full consensus/miner matrix. |
| `RUSTSEC-2024-0436` | `paste 1.0.15` | Linux node build artifact; miner absent | Unmaintained build-time dependency; no runtime exception is inferred. |
| `RUSTSEC-2024-0370` | `proc-macro-error 1.0.4` | build/proc-macro artifacts in node and miner | Unmaintained build-time dependency; remove through supported workflow upgrade. |

## Mandatory controls

The permanent dependency-security workflow must:

1. use pinned Rust `1.88.0` and pinned `cargo-audit 0.22.2`;
2. prove the raw vulnerability set remains exactly the separately documented Hickory pair;
3. prove the raw informational warning set remains exactly the eight advisory records above;
4. regenerate Linux compiler-artifact reachability on the exact candidate SHA and fail on reachability drift;
5. run `scripts/validate_v2_4_0_rustsec_warning_disposition.py`;
6. fail after `2026-08-31 UTC` or when any package/parent/version/invariant changes;
7. keep every warning visible in uploaded raw and configured audit JSON;
8. preserve checksummed exact-candidate provenance.

No warning ID is added to `.cargo/audit.toml`.

## Removal / public-GO plan

Before public-testnet GO:

1. migrate the supported workflow stack so `atty 0.2.14` is absent;
2. migrate the supported Kaspa/intertrait stack so affected `linkme 0.2.10` is absent using an upstream-supported line;
3. rerun Linux and Windows dependency/reachability, consensus, storage/replay, P2P, RPC, miner and packaged-smoke matrices on one exact SHA;
4. complete the remaining #803 disposition and exact-candidate public security review.

Unsupported transitive leaf patches are not authorized. This record bounds known risk; it does not convert it into public readiness approval.
