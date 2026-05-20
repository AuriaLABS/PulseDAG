# Cleanup Candidates v2.2.18 (Pass 2)

Methods used: `git ls-files`, `find . -type f`, `rg` keyword/version scans, and targeted `git grep` reference checks.

## Classification

### delete now
- None remaining after pass-2 moves/fixes.

### archive then delete original
- `docs/RELEASE_NOTES_V2_2_{7..16}.md` -> moved to `docs/archive/v2_2_history/`
- `docs/CLOSING_CHECKLIST_V2_2_{7..16}.md` -> moved to `docs/archive/v2_2_history/`
- `docs/ROADMAP_V2_2_{7,8,9,10,11,12,14,15,16}.md` -> moved to `docs/archive/v2_2_history/`
- `docs/SMOKE_TEST_V2_2_{7..12}.md` -> moved to `docs/archive/v2_2_history/`
- `docs/ROADMAP_V2_3_0.md`, `docs/V3_READINESS.md`, `docs/REPO_CLEANUP_V2_2_16.md` -> moved to `docs/archive/v2_2_history/`

### keep historical
- `docs/ROADMAP_V3_0_0.md` (active long-term roadmap)
- `docs/POW_SPEC_FINAL.md` (canonical protocol/spec baseline)

### keep current
- Protected v2.2.17 closeout docs/scripts and core version files.

### needs maintainer review
- Legacy PowerShell helper scripts under `scripts/*.ps1` that may still be used by external operators.
