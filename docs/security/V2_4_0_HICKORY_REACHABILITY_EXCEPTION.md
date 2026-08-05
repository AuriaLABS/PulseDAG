# v2.4.0 Hickory RustSec reachability exception

Status: active, temporary and fail-closed

Owner: `kalekoi`

Recorded: `2026-08-04 UTC`

Review deadline: `2026-08-31 UTC`

Hard expiry: before any public-testnet GO decision, or immediately when the
`libp2p` version, selected feature set, P2P transport construction, ignored
advisory set or Hickory package version changes.

## Decision

PulseDAG temporarily ignores only these two advisories in `.cargo/audit.toml`:

- `RUSTSEC-2026-0118` for `hickory-proto 0.25.2`;
- `RUSTSEC-2026-0119` for `hickory-proto 0.25.2`.

This is a version-specific non-reachability exception, not a claim that the
Hickory release is safe and not a general permission to ignore RustSec output.
Every other vulnerability remains blocking.

## Why the package remains in Cargo.lock

PulseDAG uses the upstream `libp2p 0.56.0` umbrella crate. Cargo resolves
optional dependency versions into `Cargo.lock` so they remain available if a
feature is selected later, then performs a separate feature-resolution pass for
the actual build. PulseDAG does not select the DNS or mDNS features.

Removing those package records by hand makes the lockfile invalid; regenerating
the complete lockfile introduces unrelated dependency drift in the
Kaspa/workflow stack.

As of 2026-08-04, `libp2p 0.56.0` is the latest upstream rust-libp2p release.
There is therefore no newer supported umbrella release to adopt for this
candidate.

## Non-reachability evidence

The v2.4.0 P2P manifest selects exactly the supported Tokio, TCP, Noise, Yamux,
Kademlia, gossipsub, identify, macros and ping features. It does not select
`dns`, `mdns` or `quic`.

The runtime contract additionally:

- does not instantiate an mDNS `NetworkBehaviour`;
- reports mDNS disabled;
- defaults `PULSEDAG_P2P_MDNS` to false in every profile;
- rejects `PULSEDAG_P2P_MDNS=true` at configuration validation;
- uses explicit bootnodes and Kademlia for discovery.

The repository validator compiles `pulsedag-p2p` and `pulsedagd` independently
from empty Cargo target directories and parses Cargo's `compiler-artifact`
messages. It fails if either build actually compiles `hickory-proto`,
`hickory-resolver`, `libp2p-dns` or `libp2p-mdns`.

This is intentionally stronger than treating every package recorded in
`Cargo.lock` or displayed by an approximate dependency-tree view as executable
reachability.

## Mandatory controls

`scripts/validate_v2_4_0_hickory_exception.py` must pass on the exact candidate
SHA. It enforces:

- the exact two ignored advisory IDs and no others;
- `libp2p 0.56.x` with `mdns`, `dns` and `quic` absent from selected features;
- the expected locked Hickory version `0.25.2`;
- patched `quinn-proto 0.11.15`;
- fail-closed daemon configuration and runtime status;
- clean, locked compiler-artifact evidence proving no Hickory/DNS/mDNS package
  is compiled into the P2P crate or node;
- an unexpired review deadline.

The pinned `cargo-audit 0.22.2` gate must then exit successfully with the
repository configuration. Its raw and configured JSON evidence, exact compiler
messages and provenance remain attached to the exact candidate.

## Removal plan

Remove this exception and both ignored IDs as soon as one of these supported
paths is validated:

1. an upstream rust-libp2p umbrella release no longer locks an affected Hickory
   version for disabled optional features;
2. PulseDAG migrates from the umbrella crate to directly selected libp2p
   component crates without changing the reviewed transport/behaviour contract;
3. the affected Hickory packages are patched within a Cargo-resolvable,
   upstream-supported dependency graph.

Any chosen path must rerun the complete P2P, daemon, workspace, packaged-smoke,
RustSec and pre-burn-in matrices. The private burn-in candidate must then be
refrozen from zero.

## Authorization boundary

This exception does not authorize public exposure, public-testnet GO, Day 0,
the 30-day clock, contracts, high cadence or production custody. If this record
expires or any invariant above changes, dependency security returns to a
blocking state until re-reviewed.
