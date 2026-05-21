#!/usr/bin/env bash
set -euo pipefail
RUN_DIR=${1:-}
[[ -n "$RUN_DIR" && -d "$RUN_DIR" ]] || { echo "Usage: $0 <run_dir>"; exit 1; }
OUT_DIR="$RUN_DIR/evidence"
mkdir -p "$OUT_DIR"
git rev-parse HEAD > "$OUT_DIR/git-commit.txt"
git rev-parse --abbrev-ref HEAD > "$OUT_DIR/git-ref.txt"
cat VERSION > "$OUT_DIR/version.txt"
awk '/^version\s*=/{print $3; exit}' Cargo.toml | tr -d '"' > "$OUT_DIR/cargo-workspace-version.txt"
cp -f "$RUN_DIR"/summary.md "$OUT_DIR"/ 2>/dev/null || true
cp -f "$RUN_DIR"/command-log.txt "$OUT_DIR"/ 2>/dev/null || true
find "$RUN_DIR" -maxdepth 1 -type f \( -name '*log*' -o -name '*endpoint*' -o -name '*manifest*' \) -exec cp -f {} "$OUT_DIR"/ \;
rg -n "token|apikey|api_key|secret|password|Authorization:" "$OUT_DIR" > "$OUT_DIR/secret-scan.txt" || true
sed -i -E 's/(Authorization: ).*/\1[REDACTED]/I;s/(token|apikey|api_key|secret|password)=\S+/\1=[REDACTED]/Ig' "$OUT_DIR"/* 2>/dev/null || true
( cd "$RUN_DIR" && find evidence -type f | sort ) > "$OUT_DIR/manifest.txt"
( cd "$RUN_DIR" && tar --exclude='target' --exclude='*.tmp' -czf evidence.tar.gz evidence )
( cd "$RUN_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256 )
printf "# Evidence Summary\n\n- run_dir: %s\n- tarball: %s/evidence.tar.gz\n- sha256: %s/evidence.tar.gz.sha256\n" "$RUN_DIR" "$RUN_DIR" "$RUN_DIR" > "$RUN_DIR/evidence-summary.md"
echo "Evidence collected: $RUN_DIR/evidence.tar.gz"
