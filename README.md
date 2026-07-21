# PulseDAG v2.3.0

PulseDAG is in the **v2.3.0 private-testnet release-candidate** stage.

## Current state

- Repository version: `v2.3.0`.
- Cargo workspace version: `2.3.0`.
- External standalone miner: supported and packaged separately from `pulsedagd`.
- Release-candidate validation: completed on the exact approved candidate.
- Final private-testnet release decision: `PENDING_FINAL_CANDIDATE_EVIDENCE`.
- `v2.3.0` tag: not created.
- GitHub Release publication: not authorized.
- `public_testnet_ready=false`.
- The 30-day public-testnet clock has not started.
- Smart contracts and pool logic remain outside the current scope.

## Start here

- [Documentation index](docs/README.md)
- [v2.3.0 roadmap](docs/ROADMAP_V2_3_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
- [Operator runbook](docs/RUNBOOK.md)
- [Private-testnet operations](docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md)
- [Binary installation and verification](docs/INSTALL_BINARIES_V2_3_0.md)
- [Release notes](docs/release/V2_3_0_RELEASE_NOTES.md)
- [Release decision](docs/release/V2_3_0_RELEASE_DECISION.md)
- [Historical archive](docs/archive/README.md)

## Development

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Repository structure and stale-version checks are enforced by:

```bash
bash scripts/repository_hygiene.sh --strict
```

Historical v2.2.x evidence is retained for traceability under the archive policy. It is not current operator guidance and must not be presented as the active repository version.
