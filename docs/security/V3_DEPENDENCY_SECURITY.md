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
- clean compiler-artifact reachability is captured for `pulsedag-p2p`, `pulsedagd` and `pulsedag-miner`;
- optional `dns`, `mdns`, `quic` and `upnp` libp2p packages must not be compiler-reachable from the selected PulseDAG feature set.

## Remaining launch blockers

This remediation does **not** close #803. The following known reachable blockers remain visible and require separate supported remediation or exact-final-candidate reviewed disposition before `GO_V3_DUAL_LAUNCH`:

- `atty 0.2.14` — `RUSTSEC-2024-0375`, `RUSTSEC-2021-0145`;
- `linkme 0.2.10` — `RUSTSEC-2024-0407`.

Other informational warnings remain visible and must be owned by the final v3 security matrix. No warning is hidden merely to obtain a green audit.

## Final launch boundary

A PASS of the active dependency workflow means the current development candidate satisfies this dependency-remediation checkpoint. It does not mean `security_ready=true`, does not authorize mainnet/testnet launch and does not replace the final exact-candidate security review required by #803/#781.
