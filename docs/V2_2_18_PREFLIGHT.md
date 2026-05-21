# v2.2.18 Preflight Verification

Use this before running any server/miner evidence workflow.

## Required commands
```bash
git fetch --all --tags
git describe --tags --always
cat VERSION
awk '/^version\s*=/{print $3; exit}' Cargo.toml | tr -d '"'
cargo metadata --no-deps --format-version 1 | jq -r '.workspace_members[0] as $m | .packages[] | select(.id==$m) | .version'
```

## Pass criteria
- `VERSION == v2.2.18`
- Cargo workspace version == `2.2.18`
- `README.md` and `docs/VERSION_MATRIX.md` identify v2.2.18 as current preflight/RC preparation
- no v2.3.0 readiness claim
- no v3.0 readiness claim

## Detect accidental checkout
- If `cat VERSION` shows `v2.2.17`, stop immediately.
- If Cargo version is `2.2.17`, stop immediately.
- Do not assume `main` is correct unless version files prove alignment.
- Do not run v2.2.18 evidence from a v2.2.17 checkout.
