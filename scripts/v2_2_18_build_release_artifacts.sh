#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-${ROOT_DIR}/artifacts/v2_2_18_release_artifacts}"
OUT_DIR="${ARTIFACT_ROOT}/${RUN_ID}"
BIN_DIR="${OUT_DIR}/bin"
RELEASE_NOTES_PATH="${RELEASE_NOTES_PATH:-${ROOT_DIR}/docs/RELEASE_NOTES_V2_2_18.md}"
EVIDENCE_CHECKLIST_PATH="${EVIDENCE_CHECKLIST_PATH:-${ROOT_DIR}/docs/CLOSING_CHECKLIST_V2_2_18.md}"

mkdir -p "${BIN_DIR}"

pushd "${ROOT_DIR}" >/dev/null

echo "[1/6] building release binaries (workspace)"
cargo build --workspace --release

echo "[2/6] collecting pulsedagd and pulsedag-miner binaries"
cp "${ROOT_DIR}/target/release/pulsedagd" "${BIN_DIR}/pulsedagd"
cp "${ROOT_DIR}/target/release/pulsedag-miner" "${BIN_DIR}/pulsedag-miner"

echo "[3/6] writing metadata"
if [[ -f "${ROOT_DIR}/VERSION" ]]; then
  cp "${ROOT_DIR}/VERSION" "${OUT_DIR}/version.txt"
else
  echo "unknown" > "${OUT_DIR}/version.txt"
fi

git rev-parse HEAD > "${OUT_DIR}/git_commit.txt"

{
  echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "uname=$(uname -a)"
  echo "rustc=$(rustc --version)"
  echo "cargo=$(cargo --version)"
} > "${OUT_DIR}/build_environment.txt"

echo "[4/6] copying release references"
cp "${RELEASE_NOTES_PATH}" "${OUT_DIR}/release_notes_copy.md"
cp "${EVIDENCE_CHECKLIST_PATH}" "${OUT_DIR}/evidence_checklist_copy.md"

echo "[5/6] generating checksums"
(
  cd "${OUT_DIR}"
  sha256sum \
    "bin/pulsedagd" \
    "bin/pulsedag-miner" \
    "version.txt" \
    "git_commit.txt" \
    "build_environment.txt" \
    "release_notes_copy.md" \
    "evidence_checklist_copy.md" \
    > "checksums.txt"
)

echo "[6/6] done"
echo "Artifact directory: ${OUT_DIR}"
echo "NOTE: this script does not publish artifacts automatically."
echo "NOTE: this script does not sign artifacts when signing keys are unavailable."
echo "NOTE: do not include secrets in artifact contents."
echo "NOTE: artifact generation alone does not claim production readiness."

popd >/dev/null
