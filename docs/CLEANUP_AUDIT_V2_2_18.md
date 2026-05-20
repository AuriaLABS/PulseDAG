# PulseDAG Cleanup Audit (v2.2.18 RC Prep)

Date: 2026-05-20

## Scope
Cleanup-only repository audit ahead of v2.2.18 private-testnet RC preparation. No protocol/consensus/PoW/miner behavior changes were made.

## Exhaustive search and audit steps executed
- `git ls-files`
- `find . -type f`
- `rg "v2.2.6|v2.2.7|v2.2.8|v2.2.9|v2.2.10|v2.2.11|v2.2.12|v2.2.13|v2.2.14|v2.2.15|v2.2.16|TODO|obsolete|deprecated|legacy|old|stale" -n`
- `rg "artifacts/|evidence/|logs/|target/|tmp/|backup|copy|old" -n`
- `cargo metadata --no-deps`
- `bash -n scripts/*.sh`
- `bash scripts/validate_repo_cleanup.sh`
- `cargo fmt --check`
- `cargo test --workspace` (started; long-running, still compiling in CI-like environment)
- `cargo build --workspace --release` (started; long-running, still compiling in CI-like environment)

## Findings summary
- No clearly safe tracked deletion candidates were found in the high-confidence junk classes (`*.log`, `*.tmp`, `*.bak`, `*.old`, `*.orig`, `*.swp`, tracked `target/`, tracked archive bundles).
- Historical release notes/checklists/roadmaps were intentionally preserved to avoid losing release history.
- Cleanup hardening was applied via `.gitignore` updates and a dedicated local validation script.

## Deletion/retention/update table

| Path | Action | Reason | Replacement | Risk | Validation |
|---|---|---|---|---|---|
| `.gitignore` | UPDATE | Strengthen ignore rules to prevent runtime/generated artifacts and local OS/editor noise from being recommitted. | N/A | low | `bash scripts/validate_repo_cleanup.sh` PASS |
| `scripts/validate_repo_cleanup.sh` | ADD (UPDATE category) | Add local, network-free cleanup guard script for artifact/doc/version hygiene checks. | N/A | low | `bash scripts/validate_repo_cleanup.sh` PASS |
| `docs/RELEASE_NOTES_V2_2_7.md` and other historical release notes/checklists/roadmaps | KEEP | Preserved intentionally as historical evidence; not duplicates by default and can be referenced in release forensics. | N/A | low | Manual audit via `find`, `rg`, and tracked-file scan |
| Tracked runtime/generated junk candidates (`*.log`, `*.tmp`, `*.bak`, `*.old`, `*.orig`, `*.swp`, `target/`, `artifacts/*.tar.gz`, evidence bundles) | KEEP (not present) | No matching tracked files found, so no deletion performed. | N/A | low | `bash scripts/validate_repo_cleanup.sh` PASS |
| Potential stale docs/scripts from pre-v2.2.17 cycles | PENDING_REVIEW | Appear old but may be part of historical rehearsal/evidence lineage; require maintainer policy decision before pruning. | Prefer future archival strategy doc | medium | Pattern search completed; no high-confidence safe deletions made in this pass |

## Files deleted
- None in this pass (no high-confidence safe tracked deletions identified).

## Files kept intentionally
- Release history docs and checklists across v2.2.x were retained intentionally to avoid evidence loss.
- Core protected docs and all Cargo workspace source/code assets retained.

## Files updated instead of deleted
- `.gitignore` updated to block common artifact pollution.
- Added `scripts/validate_repo_cleanup.sh` to enforce cleanup constraints locally.

## Looked stale but preserved
- Multiple old rehearsal/release documents and scripts were preserved due to potential historical/evidence relevance and explicit conservative policy for release history.

## Validation status after cleanup
- `cargo fmt --check`: PASS
- `bash -n scripts/*.sh`: PASS
- `bash scripts/validate_repo_cleanup.sh`: PASS
- `cargo test --workspace`: PENDING (started; long-running compile not completed in this execution window)
- `cargo build --workspace --release`: PENDING (started; long-running compile not completed in this execution window)

## v2.3.0 readiness note
This cleanup audit does **not** claim v2.3.0 readiness; it only records repository hygiene actions and checks.

- Cleanup pass 2 archive completed; see `docs/CLEANUP_AUDIT_V2_2_18_PASS2.md` and `docs/archive/`.
