# v2.4.0 Task31 Hickory RustSec reachability exception

Status: active, temporary and fail-closed

Owner: `kalekoi`

Review deadline: `2026-08-31 UTC`

Hard expiry: before any public-testnet GO decision, or immediately when the `libp2p` version, selected feature set, P2P transport construction, ignored advisory set or Hickory package version changes.

## Decision

PulseDAG temporarily ignores only `RUSTSEC-2026-0119` for the locked `hickory-proto 0.24.4`, and only because the exact Task31 release build does not compile the DNS/mDNS/QUIC feature path that owns it.

The companion Hickory advisories do not justify broader ignores for this graph: `RUSTSEC-2026-0118` marks releases before 0.25.0-alpha.3 unaffected, and `RUSTSEC-2025-0006` is patched on the 0.24 line beginning at 0.24.3. The repository therefore does not ignore either advisory.

This is a version-specific non-reachability exception, not a claim that Hickory is generally safe and not permission to ignore other RustSec output. Every other vulnerability remains blocking.

## Why Hickory remains in Cargo.lock

PulseDAG uses the `libp2p 0.54` umbrella crate. Cargo may retain optional component dependencies in the lock graph even when their features are not selected for the build. The Task31 P2P manifest uses `default-features = false` and selects only Tokio, TCP, Noise, Yamux, Kademlia, gossipsub, identify, macros and ping. It does not select `dns`, `mdns` or `quic`.

The v2.4 candidate profiles explicitly keep `PULSEDAG_P2P_MDNS=false`. The runtime source contains no mDNS behaviour construction.

## Exact-candidate non-reachability evidence

`scripts/validate_v2_4_0_hickory_exception.py` compiles both `pulsedag-p2p` and `pulsedagd` independently from empty Cargo target directories and parses Cargo `compiler-artifact` messages. It fails if `hickory-proto`, `hickory-resolver`, `libp2p-dns` or `libp2p-mdns` is actually compiled.

The validator also fails unless:

- `.cargo/audit.toml` ignores exactly `RUSTSEC-2026-0119` and no other vulnerability;
- libp2p remains on the reviewed 0.54 line with exactly the approved feature set;
- `hickory-proto` remains locked exactly at 0.24.4;
- release candidate profiles keep mDNS disabled;
- no mDNS behaviour is instantiated;
- the review deadline remains unexpired.

The dependency workflow records the exact candidate SHA, compiler messages, package lists, audit JSON and checksums. Evidence from older release-branch SHAs is not substituted.

## Removal plan

Remove this exception and the ignored ID as soon as an upstream-supported dependency graph no longer retains an affected optional Hickory release, or if PulseDAG changes its networking stack so the dependency becomes compiled. Any networking dependency change must rerun P2P, daemon, workspace, packaged-smoke and dependency security matrices on one exact candidate SHA.

## Authorization boundary

This exception authorizes only private technical candidate validation. It does not authorize public exposure, public-testnet GO, Day 0, the 30-day clock, contracts, high cadence or production custody. If the record expires or any invariant changes, dependency security returns to a blocking state until re-reviewed.
