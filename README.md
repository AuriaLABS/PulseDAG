# PulseDAG v2.4.0

PulseDAG **v2.4.0** is the active software release.

## Release state

- Repository version: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Release decision: `APPROVE_TAG_AND_PUBLICATION`, subject to the final exact-SHA release gates.
- Authoritative release identity: the immutable commit referenced by tag `v2.4.0`.
- External standalone miner: supported and packaged separately from `pulsedagd`.
- Professional wallet boundary: local encrypted deterministic custody/signing with a keyless node and signed-transaction-only public relay.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- `contracts_enabled=false`.

The v2.4.0 software release is separate from public-testnet launch authorization. Publishing the tag and GitHub Release does **not** start Day 0, the 30-day public-testnet acceptance clock, smart-contract activation, or production/mainnet custody.

## Start here

- [Documentation index](docs/README.md)
- [v2.4.0 roadmap](docs/ROADMAP_V2_4_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
- [Operator runbook](docs/RUNBOOK.md)
- [v2.4.0 single-node operations](docs/runbooks/V2_4_0_SINGLE_NODE_OPERATIONS.md)
- [Binary installation and verification](docs/INSTALL_BINARIES_V2_4_0.md)
- [Release notes](docs/release/V2_4_0_RELEASE_NOTES.md)
- [Release decision](docs/release/V2_4_0_RELEASE_DECISION.md)
- [v2.4.0 release closeout](docs/checklists/V2_4_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md)
- [Historical archive](docs/archive/README.md)

## Development

From the repository root:

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Repository structure, stale-version checks, secret/generated-output checks, and release-surface consistency are enforced by repository hygiene CI.

Historical v2.3.x and earlier documentation remains in the repository where it is required for release provenance, compatibility, or immutable evidence. Active release surfaces identify v2.4.0.
