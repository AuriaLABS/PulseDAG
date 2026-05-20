# Cleanup Candidates v2.2.18 (Final)

Methods used: `git ls-files`, `find . -type f`, `rg` keyword/version scans, and targeted `git grep` reference checks.

## Classification

### delete now
- None.

### archive then delete original
- None (all required archival moves completed in pass 2 and final pass).

### keep historical
- Archived docs only under `docs/archive/`.
- Archived legacy helper scripts only under `scripts/archive/v2_2_history/`.

### keep current
- Current root docs for v2.2.17/v2.2.18 closeout.
- Current supported scripts in `scripts/` only (`*.sh` plus active PowerShell runbook helpers).

### needs maintainer review
- None.
