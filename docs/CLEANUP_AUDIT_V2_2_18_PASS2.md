# Cleanup Audit v2.2.18 - Pass 2

## Repository inventory method
- Enumerated tracked files with `git ls-files`.
- Enumerated all files with `find . -type f`.
- Searched stale/versioned terms via `rg -n` across docs/scripts/workflows.
- Checked references with `rg`/`git grep` for moved paths and stale script names.

## Exact commands used
- `git ls-files`
- `find . -type f`
- `rg -n -i 'obsolete|deprecated|legacy|stale|TODO|FIXME|backup|copy|tmp|log|artifact|evidence' docs scripts .github`
- `rg -n 'scripts/rpc_security_smoke\.sh|V2_2_|V2_3_0|V3_READINESS' docs .github scripts`
- `bash -n scripts/*.sh`
- `bash scripts/list_cleanup_candidates.sh`
- `bash scripts/validate_repo_cleanup.sh --strict`

| Path | Action | Reason | Replacement | Risk | Validation |
|---|---|---|---|---|---|
| docs/RELEASE_NOTES_V2_2_7.md .. docs/RELEASE_NOTES_V2_2_16.md | MOVE | historical but not current workflow | docs/archive/v2_2_history/* | low | links/reference scans pass |
| docs/CLOSING_CHECKLIST_V2_2_7.md .. docs/CLOSING_CHECKLIST_V2_2_16.md | MOVE | historical closeout artifacts | docs/archive/v2_2_history/* | low | strict old-doc-root check passes |
| docs/ROADMAP_V2_2_7.md .. docs/ROADMAP_V2_2_16.md | MOVE | stale iteration roadmaps cluttering root | docs/archive/v2_2_history/* | low | strict old-doc-root check passes |
| docs/SMOKE_TEST_V2_2_7.md .. docs/SMOKE_TEST_V2_2_12.md | MOVE | completed test snapshots, not active runbooks | docs/archive/v2_2_history/* | low | list-candidate output reviewed |
| docs/ROADMAP_V2_3_0.md | MOVE | superseded planning doc | docs/archive/v2_2_history/ROADMAP_V2_3_0.md | medium | no root readiness claims |
| docs/V3_READINESS.md | MOVE | stale readiness framing | docs/archive/v2_2_history/V3_READINESS.md | medium | no root readiness claims |
| docs/REPO_CLEANUP_V2_2_16.md | MOVE | superseded by v2.2.18 cleanup audits | docs/archive/v2_2_history/REPO_CLEANUP_V2_2_16.md | low | cleanup docs updated |
| .github/workflows/v2_2_17_ci_gate.yml | UPDATE | stale script path + brittle exact-text version checks | robust regex check and new script path | low | workflow now references existing script |
| scripts/validate_repo_cleanup.sh | UPDATE | add strict mode + stale ref checks + archive checks | n/a | low | `--strict` run |
| scripts/list_cleanup_candidates.sh | UPDATE (new) | repeatable candidate discovery | n/a | low | script executed |
| docs/archive/README.md | ADD | archive landing page | n/a | low | link check pass |
| docs/archive/v2_2_history/README.md | ADD | scoped archive index | n/a | low | link check pass |
| scripts/*.ps1 legacy helpers | PENDING_REVIEW | may be used by operators; not safe to remove blindly | future deprecation/migration plan | medium | tracked in candidate report |

## Files actually deleted
- No tracked runtime junk files (`*.log`, `*.tmp`, `*.bak`, `*.old`, `*.orig`, `*.swp`, `*.swo`, `*.zip`, `*.tar.gz`, `target/`, `logs/`, `run/`, `artifacts/`) were present at cleanup time.

## Files moved/archived
- See table above and `docs/archive/v2_2_history/`.

## Files kept intentionally
- Core protocol/spec/release docs and v2.2.17 closeout set kept in root docs.

## Stale references fixed
- Workflow optional smoke step now checks `scripts/v2_2_17_rpc_security_smoke.sh`.
- Version drift gate no longer depends on one exact sentence in `docs/VERSION_MATRIX.md`.

## Files pending maintainer review
- Pass 2 intentionally left PowerShell legacy helper scripts (`scripts/*.ps1`) as pending review.
- Resolution completed in final pass: see `docs/CLEANUP_AUDIT_V2_2_18_FINAL.md`.

## Validation results
- `bash -n scripts/*.sh` -> pass
- `bash scripts/list_cleanup_candidates.sh` -> pass
- `bash scripts/validate_repo_cleanup.sh --strict --allow-pending-review` -> pass (pass-2 state)
- `cargo fmt --check` -> pass
- `cargo test --workspace` -> pass
- `cargo build --workspace --release` -> pass
