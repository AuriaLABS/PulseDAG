# v2.4.0 Task31 lock-only RustSec exceptions

Status: active, temporary and fail-closed

Owner: `kalekoi`

Review deadline: `2026-08-31 UTC`

Hard expiry: before any public-testnet GO decision, or immediately when the
libp2p version/feature set, Cargo.lock, any listed package version, dependency
path, supported target, or RustSec advisory set changes.

## Decision boundary

This record permits only the exact Task31 **technical node + miner candidate**
and private, valueless validation. It does not authorize release publication,
public-testnet GO, Day 0, the 30-day clock, high cadence, contracts, production
custody, or mainnet claims.

The following vulnerability records may be ignored by `cargo audit` only because
their exact vulnerable package versions remain in `Cargo.lock` while exact-head
compiler-artifact evidence proves those versions are not compiled into
`pulsedag-p2p`, `pulsedagd`, or `pulsedag-miner`:

| Advisory | Locked package | Retention path / boundary |
| --- | --- | --- |
| `RUSTSEC-2025-0009` | `ring 0.16.20` | retained behind unselected `libp2p-quic`; release builds use the separate patched `ring 0.17.x` path where needed |
| `RUSTSEC-2026-0098` | `rustls-webpki 0.101.7` | retained behind unselected `libp2p-quic` |
| `RUSTSEC-2026-0099` | `rustls-webpki 0.101.7` | retained behind unselected `libp2p-quic` |
| `RUSTSEC-2026-0104` | `rustls-webpki 0.101.7` | retained behind unselected `libp2p-quic` |
| `RUSTSEC-2026-0119` | `hickory-proto 0.24.4` | retained by optional DNS/mDNS dependency graph; release build selects neither feature |
| `RUSTSEC-2026-0258` | `h2 0.3.27` | retained behind unselected `libp2p-upnp` |

This is a **non-reachability exception**, not a claim that the vulnerable
versions are safe.

## Frozen network feature set

`pulsedag-p2p` must keep libp2p default features disabled and select exactly:

`tokio`, `gossipsub`, `identify`, `kad`, `macros`, `tcp`, `noise`, `yamux`,
`ping`.

`dns`, `mdns`, `quic`, and `upnp` must remain unselected. Release profiles keep
`PULSEDAG_P2P_MDNS=false`.

## Exact-candidate proof

`scripts/validate_v2_4_0_lock_only_rustsec_exceptions.py` must run on the exact
candidate SHA and:

1. require exactly the six ignored advisory IDs above and no others;
2. require the exact vulnerable package versions to remain present in the lock;
3. require the patched reachable versions `crossbeam-epoch 0.9.20`,
   `anyhow 1.0.103`, and `event-listener 5.4.2`, while rejecting their prior
   vulnerable versions;
4. compile `pulsedag-p2p`, `pulsedagd`, and `pulsedag-miner` independently from
   empty target directories with `--locked`;
5. reject the candidate if any exact vulnerable package version above appears in
   compiler artifacts;
6. record evidence tied to the exact `CANDIDATE_SHA`;
7. fail after `2026-08-31 UTC`.

The permanent dependency workflow also runs raw and configured `cargo audit`
with pinned `cargo-audit 0.22.2`. Any new vulnerability outside the six exact
records above is blocking until it is patched or separately reviewed with fresh
exact-candidate evidence.

## Authorization boundary

These exceptions do not resolve the reachable warning/public-GO blockers
tracked in `V2_4_0_RUSTSEC_WARNING_DISPOSITION.md` or issue #803. Windows
exact-candidate security revalidation remains pending. No public-testnet GO,
Day 0, clock start, high-cadence default, contracts activation, release tag, or
publication is authorized by this record.
