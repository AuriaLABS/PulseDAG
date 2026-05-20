#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

fail() {
  echo "[FAIL] $1" >&2
  exit 1
}

pass() {
  echo "[PASS] $1"
}

tracked_files="$(mktemp)"
git ls-files > "$tracked_files"

# Disallowed tracked file patterns
if rg -n '\.(log|tmp|bak|old|orig|swp)$' "$tracked_files" >/dev/null; then
  rg -n '\.(log|tmp|bak|old|orig|swp)$' "$tracked_files" || true
  fail "tracked temporary/log/editor files detected"
fi
pass "no tracked *.log/*.tmp/*.bak/*.old/*.orig/*.swp files"

if rg -n '(^|/)target/' "$tracked_files" >/dev/null; then
  rg -n '(^|/)target/' "$tracked_files" || true
  fail "tracked target/ paths detected"
fi
pass "no tracked target/ paths"

if rg -n '^artifacts/.*\.tar\.gz$' "$tracked_files" >/dev/null; then
  rg -n '^artifacts/.*\.tar\.gz$' "$tracked_files" || true
  fail "tracked artifacts/*.tar.gz bundles detected"
fi
pass "no tracked artifacts/*.tar.gz bundles"

if rg -n '^evidence/.*\.(zip|tar|tar\.gz|tgz)$' "$tracked_files" >/dev/null; then
  rg -n '^evidence/.*\.(zip|tar|tar\.gz|tgz)$' "$tracked_files" || true
  fail "tracked packaged evidence bundles detected"
fi
pass "no tracked packaged evidence bundles"

# Core docs/version consistency checks
version_raw="$(tr -d '[:space:]' < VERSION)"
version_no_v="${version_raw#v}"
workspace_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"

[[ -n "$workspace_version" ]] || fail "could not parse workspace version from Cargo.toml"
[[ "$workspace_version" == "$version_no_v" ]] || fail "VERSION ($version_raw) and Cargo.toml version ($workspace_version) mismatch"
pass "VERSION matches workspace Cargo.toml version"

rg -n "${version_raw}|${version_no_v}" README.md >/dev/null || fail "README.md does not reference current version ${version_raw}"
pass "README references current version"

[ -f docs/VERSION_MATRIX.md ] || fail "docs/VERSION_MATRIX.md missing"
pass "docs/VERSION_MATRIX.md exists"

# Required release docs for current stabilization cycle
required_docs=(
  "docs/RELEASE_NOTES_V2_2_17.md"
  "docs/CLOSING_CHECKLIST_V2_2_17.md"
  "docs/RPC_ENDPOINT_INVENTORY_V2_2_17.md"
  "docs/OPERATOR_SECURITY_RUNBOOK_V2_2_17.md"
)
for d in "${required_docs[@]}"; do
  [ -f "$d" ] || fail "required doc missing: $d"
done
pass "required v2.2.17 stabilization docs exist"

# v2.2.18 docs may still be in-progress; warn only.
if [ -f docs/RELEASE_NOTES_V2_2_18.md ]; then
  pass "docs/RELEASE_NOTES_V2_2_18.md exists"
else
  echo "[WARN] docs/RELEASE_NOTES_V2_2_18.md not found (acceptable if not authored yet)"
fi

if [ -f docs/CLOSING_CHECKLIST_V2_2_18.md ]; then
  pass "docs/CLOSING_CHECKLIST_V2_2_18.md exists"
else
  echo "[WARN] docs/CLOSING_CHECKLIST_V2_2_18.md not found (acceptable if not authored yet)"
fi

rm -f "$tracked_files"
pass "repository cleanup validation completed"
