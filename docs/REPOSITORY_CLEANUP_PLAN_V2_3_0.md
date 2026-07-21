# v2.3.0 repository cleanup and organization

Date: 2026-07-21 UTC

## Objective

Keep active repository surfaces aligned with `v2.3.0`, separate historical evidence from current operator guidance, and prevent stale-version regressions without changing protocol behavior.

## Completed in this cleanup

- Replaced the stale v2.2.20 repository README with a current v2.3.0 entrypoint.
- Rebuilt `docs/VERSION_MATRIX.md` around the active v2.3.0 candidate state.
- Added a canonical documentation index at `docs/README.md`.
- Repaired the historical archive index and bound it to the immutable pre-cleanup commit.
- Updated the current runbook, release-evidence policy, and standalone-miner documentation.
- Added v2.3.0 staged-rehearsal and Docker entrypoints.
- Updated active workflows and container surfaces to use v2.3.0 names and evidence paths.
- Removed confirmed v2.2.9-v2.2.20 documents from the active `docs/` root.
- Removed completed v2.2.20 evidence workflows from the active workflow directory.
- Classified retained version-pinned scripts and private-testnet configurations as compatibility or historical evidence.
- Added a fail-closed active-version surface audit to repository hygiene.

## Historical preservation

The complete repository state before this cleanup is preserved at:

`4bee533d97708d5166024839c02277d913438448`

The historical index is `docs/archive/README.md`. Historical material is never relabelled as v2.3.0 evidence.

## Active v2.3.0 surfaces

- `README.md`
- `docs/README.md`
- `docs/ROADMAP_V2_3_0.md`
- `docs/VERSION_MATRIX.md`
- `docs/RUNBOOK.md`
- `docs/RELEASE_EVIDENCE.md`
- `docs/INSTALL_BINARIES_V2_3_0.md`
- `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`
- `docs/release/`
- active `.github/workflows/v2_3_0_*` gates
- current or neutral script entrypoints documented in `scripts/README.md`

## Compatibility surfaces

Some v2.2.x scripts and configurations remain because they reproduce accepted evidence or are implementation engines behind current v2.3.0 wrappers.

- Script classification: `scripts/LEGACY_COMPATIBILITY_V2_3_0.md`.
- Configuration classification: `configs/private-testnet/LEGACY_COMPATIBILITY_V2_3_0.md`.

Active documentation and workflows must not invoke those legacy paths directly.

## Enforced repository rules

`bash scripts/repository_hygiene.sh --strict` now verifies:

1. `VERSION=v2.3.0` and Cargo workspace version `2.3.0`;
2. primary active documents contain exact v2.3.0 markers;
3. active documents do not advertise a stale v2.2.x baseline;
4. historical v2.2.x documents do not remain in the active docs root;
5. v2.2.x workflows do not remain active;
6. retained legacy script/configuration families are explicitly classified;
7. local links and referenced current paths resolve;
8. generated files, secret-like paths, path collisions, and non-English technical comments are rejected.

## Follow-up refactoring debt

The following are behavior-preserving refactoring candidates, not cleanup deletions:

- extraction of the accepted staged-rehearsal engines from version-pinned v2.2.20 files into neutral modules;
- decomposition of `crates/pulsedag-p2p/src/lib.rs`;
- decomposition of `apps/pulsedagd/src/main.rs`;
- decomposition of large storage and RPC modules.

Each refactoring requires dedicated regression coverage and must not be mixed with protocol changes.

## Completion boundary

This cleanup changes repository organization and active documentation only. It does not authorize:

- creating the `v2.3.0` tag;
- publishing a GitHub Release;
- launching a public testnet;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day public-testnet clock;
- smart contracts or pool logic.
