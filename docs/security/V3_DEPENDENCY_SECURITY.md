# PulseDAG v3 dependency-security boundary

Status: **active development gate; not final launch security approval**.

Issue authority: #803. Launch authority: #781. Integrated program: #794.

## Historical v2.4 exception retirement

The v2.4 lock-only vulnerability exception expired on **2026-08-31 UTC** and is not renewed for the active v3 development line. The active `.cargo/audit.toml` contains no vulnerability advisory ignores.

The libp2p 0.56 migration removes the historical lock-only vulnerable versions that were retained by the v2.4 libp2p 0.54 graph:

- `ring 0.16.20`;
- `rustls-webpki 0.101.7`;
- `hickory-proto 0.24.4`;
- `h2 0.3.27`.

## #803 lru remediation

`lru 0.12.5` was reachable through the old libp2p stack (`libp2p-identify 0.45.0` and `libp2p-swarm 0.45.1`). The supported parent-stack migration in PR #1018 moves PulseDAG P2P to `libp2p 0.56.x`, where the selected identify/swarm line no longer resolves `lru 0.12.5`.

The remediation rule is fail-closed:

- no direct or transitive `lru` leaf override;
- `lru 0.12.5` must be absent from the resolved lock graph;
- historical v2.4 lock-only vulnerable versions above must be absent;
- PulseDAG's selected libp2p feature set remains explicit with default features disabled;
- clean compiler-artifact reachability is captured for `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet`;
- optional `dns`, `mdns`, `quic` and `upnp` libp2p packages must not be compiler-reachable from the selected PulseDAG feature set.

## #803 linkme remediation

`linkme 0.2.10` was reachable through the legacy chain `kaspa-core 0.15.0 -> intertrait 0.2.2 -> linkme 0.2.10`. PulseDAG does not patch the `linkme` leaf or carry a private `intertrait` fork for the active remediation.

The supported parent-stack migration moves the direct PoW dependencies to the official Rusty Kaspa `v2.0.1` source, pinned to exact upstream commit:

`cfafeb4c093fa37a303f1b9f19c58f986b870ce3`

The active direct dependencies are `kaspa-hashes 2.0.1` and `kaspa-pow 2.0.1` from `https://github.com/kaspanet/rusty-kaspa` at that exact revision. This upstream line removes the legacy `intertrait 0.2.2` dependency path, so `linkme 0.2.10` and `linkme-impl 0.2.10` no longer resolve.

Because Rusty Kaspa 2.0.1 declares Rust 1.91.0 / edition 2024, the active v3 lint and dependency-security gates move to Rust 1.91.0. Historical frozen v2.4 workflows remain historical and are not reinterpreted by this migration.

The linkme remediation rule is fail-closed:

- `kaspa-hashes` and `kaspa-pow` must remain pinned to the reviewed official upstream revision above;
- the resolved direct Kaspa packages must be version `2.0.1` from that upstream revision;
- `intertrait 0.2.2` must be absent;
- `linkme 0.2.10` must be absent;
- existing `pulsedag-core` consensus/PoW tests must pass without changing PoW vectors or adapter logic;
- `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet` must compile against the same exact lock graph.

## Hickory 0.25.2 lock-only disposition

The Kaspa 2.0.1 lock graph currently contains `hickory-proto 0.25.2`, covered by `RUSTSEC-2026-0118` and `RUSTSEC-2026-0119`. These advisories are not hidden: the raw `cargo audit` report must contain exactly those vulnerability records and CI preserves the raw failing audit evidence.

The temporary development disposition is valid only while clean compiler-artifact evidence proves `hickory-proto 0.25.2` is absent from **all** launch roots: `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet`. If the package becomes compiler-reachable from any root, the gate fails closed. The CLI audit uses explicit per-run ignores only after that reachability proof; `.cargo/audit.toml` remains free of vulnerability ignores.

This is not a final v3 launch disposition. The exact candidate security review in #803 must either remove this lock-only residue through a supported parent migration or explicitly renew the reviewed unreachable disposition for the final candidate.

## Remaining launch blocker and warning inventory

This remediation does **not** close #803. The known reachable blocker remaining from the inherited dependency pair is:

- `atty 0.2.14` — `RUSTSEC-2024-0375`, `RUSTSEC-2021-0145`.

The Kaspa 2.0.1 graph also makes `derivative 2.2.0` (`RUSTSEC-2024-0388`, unmaintained) compiler-reachable. It remains visible in the raw warning inventory and requires owner/disposition in the final v3 security matrix. Other informational warnings likewise remain visible; no warning is hidden merely to obtain a green audit.

## Final launch boundary

A PASS of the active dependency workflow means the current development candidate satisfies this dependency-remediation checkpoint. It does not mean `security_ready=true`, does not authorize mainnet/testnet launch and does not replace the final exact-candidate security review required by #803/#781.
