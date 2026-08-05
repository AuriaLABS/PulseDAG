# v2.4.0 RustSec warning disposition

Status: active, temporary and fail-closed

Owner: `kalekoi`

Recorded: `2026-08-04 UTC`

Review deadline: `2026-08-31 UTC`

Hard expiry: before any public-testnet GO decision, or immediately when any
listed package version, direct parent, supported target, allocator policy,
Kaspa/workflow dependency line or warning set changes.

## Decision boundary

This record permits the exact v2.4.0 dependency graph to enter a **valueless,
private, non-public burn-in only**. It does not approve public exposure,
public-testnet GO, Day 0, the 30-day clock, contracts, production custody or
mainnet claims.

The two reachable unsound warnings (`atty` and `linkme`) remain public-testnet
blockers. They must be removed through a supported parent-stack upgrade or an
upstream-reviewed fix before public-testnet GO. They are not ignored by
`.cargo/audit.toml` and remain visible in every pinned audit report.

## Evidence scope

The analyzer `scripts/analyze_v2_4_0_rustsec_warnings.py` compiled
`pulsedagd` and `pulsedag-miner` independently from empty Cargo target
directories on Ubuntu 24.04 and Windows Server 2022 using Rust 1.88.0. It
captured Cargo `compiler-artifact` messages, metadata and reverse dependency
trees.

Evidence was produced for branch head
`0516420770f87e27eafa6771e58e6c6e6e4aa01f` on GitHub's exact pull-request
merge candidate `74e54026043e4f2aebd43e5cb4bbeb80f2e67be9`:

- Linux artifact digest:
  `sha256:6b8ea3d4e1e2dc7843826739355513f6187c266254dbd32c24d58f8ac0ac20de`;
- Windows artifact digest:
  `sha256:0464fe820156b8655c566509ad5cfb2a588a071f24351c4a4642cf59dc9400f8`.

The warning inventory is therefore classified by actual native compilation,
not merely by presence in `Cargo.lock`.

## Exact warning inventory and disposition

| Advisory | Package | Native evidence | Dependency owner/path | Disposition |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2025-0052` | `async-std 1.13.2` | Compiled into node and miner on Linux and Windows | `workflow-core 0.18.0`, reached through the supported Kaspa `0.15.0` / workflow `0.18.0` graph | Unmaintained only; accepted temporarily for private burn-in. Replace through a supported Kaspa/workflow upgrade. Do not force a transitive runtime substitution. |
| `RUSTSEC-2024-0375` | `atty 0.2.14` | Compiled into node and miner on Linux and Windows | `hexplay 0.3.0` via `workflow-log 0.18.0` | Unmaintained; same removal gate as the unsound advisory below. |
| `RUSTSEC-2021-0145` | `atty 0.2.14` | Windows-reachable in node and miner | `hexplay 0.3.0` via `workflow-log 0.18.0` | Unsound on Windows. PulseDAG defines no custom global allocator, so the advisory's exceptional unaligned-allocation precondition is not present in the reviewed tree. Accepted only for valueless private burn-in; public-testnet blocker until the supported workflow line removes `hexplay/atty`. |
| `RUSTSEC-2025-0141` | `bincode 1.3.3` | Compiled into the node; not the standalone miner | Direct persistence dependency of `pulsedagd` and `pulsedag-storage` | Unmaintained, with no reported vulnerability in this advisory. Retained for v2.4.0 storage/schema compatibility. Replacement requires an explicit format migration, dual-read or restore proof and a new burn-in candidate. |
| `RUSTSEC-2024-0384` | `instant 0.1.13` | Compiled into node and miner on Linux and Windows | `workflow-core 0.18.0` through Kaspa/workflow | Unmaintained only; accepted temporarily for private burn-in and removed through a supported Kaspa/workflow upgrade. |
| `RUSTSEC-2024-0407` | `linkme 0.2.10` | Compiled into node and miner on Linux and Windows | `intertrait 0.2.2` through `kaspa-core 0.15.0` | Reachable unsoundness. Patched `linkme >=0.3.24` is not semver-compatible with the reviewed `intertrait 0.2.2` contract, which explicitly uses the `0.2` line. No transitive override is authorized. Private burn-in only; public-testnet blocker until a supported Kaspa/intertrait migration passes full consensus and miner validation. |
| `RUSTSEC-2024-0436` | `paste 1.0.15` | Build/proc-macro artifact for the Linux node; absent from Windows and both miner builds | Linux-only transitive build graph | Unmaintained build-time dependency. Locked checksum and reproducible CI limit supply drift. Replace through the owning upstream dependency line; no runtime exception is inferred. |
| `RUSTSEC-2024-0370` | `proc-macro-error 1.0.4` | Build/proc-macro artifacts for node and miner on Linux and Windows | `workflow-core-macros 0.18.0` and `workflow-wasm-macros 0.18.0` | Unmaintained build-time dependency. Accepted for the pinned toolchain and locked source; remove through the supported workflow upgrade. |

## Why unsupported point patches are rejected

- `intertrait 0.2.2` uses the `linkme 0.2` API and generated macros. Forcing
  `linkme 0.3` under it would be an unreviewed API/ABI change in consensus
  dependencies.
- `atty` is owned by `hexplay`, which is owned by `workflow-log 0.18.0`.
  Replacing only the leaf would fork upstream behavior without a supported
  release contract.
- `async-std`, `instant` and `proc-macro-error` are owned by the workflow/Kaspa
  dependency family. Moving only selected crates risks mixed-version macro and
  runtime behavior.
- `bincode` encodes persisted v2.4 state. Replacing it without an explicit
  migration would risk unreadable snapshots or silent recovery divergence.

## Mandatory controls

The permanent dependency-security workflow must:

1. use pinned Rust `1.88.0` and pinned `cargo-audit 0.22.2`;
2. prove the raw vulnerability set remains exactly the separately documented
   Hickory pair;
3. prove the raw informational warning set remains exactly the eight advisory
   records in this document;
4. run `scripts/validate_v2_4_0_rustsec_warning_disposition.py`;
5. fail after `2026-08-31 UTC` or when any package/parent/version/invariant
   changes;
6. keep every warning visible in the uploaded raw and configured audit JSON;
7. preserve evidence hashes and exact candidate provenance.

No warning ID is added to `.cargo/audit.toml`.

## Removal plan

Before public-testnet GO:

1. migrate the supported Kaspa/workflow stack so `atty 0.2.14` is absent;
2. migrate the supported Kaspa/intertrait stack so the affected `linkme 0.2.10`
   is absent and the replacement is at least the patched `0.3.24` line, or use
   another upstream-reviewed casting registry;
3. rerun consensus, storage/replay, P2P, RPC, miner, Windows/Linux release,
   packaged-smoke and RustSec matrices on one exact SHA;
4. freeze a new private candidate and restart its burn-in clock from zero.

The maintenance-only dependencies should be removed in the same supported
stack upgrade where possible. `bincode` requires a separately reviewed storage
format migration.

## Authorization statement

Disposition means the risks are identified, owned and bounded. It does **not**
mean the crates are maintained or sound, and it does not convert warning-only
RustSec findings into a public readiness approval.
