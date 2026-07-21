#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

bash -n \
  scripts/v2_3_0_private_rehearsal_compat.sh \
  scripts/v2_3_0_private_5n_1m_rehearsal.sh \
  scripts/v2_3_0_private_5n_2m_rehearsal.sh \
  scripts/v2_3_0_private_5n_4m_rehearsal.sh

for script in \
  scripts/v2_3_0_private_5n_1m_rehearsal.sh \
  scripts/v2_3_0_private_5n_2m_rehearsal.sh \
  scripts/v2_3_0_private_5n_4m_rehearsal.sh; do
  grep -Fq 'v2_3_0_private_rehearsal_compat.sh' "$script"
  if grep -Fq 'v2_2_20_private_5n_' "$script"; then
    echo "current wrapper calls a v2.2.20 stage entrypoint directly: $script" >&2
    exit 1
  fi
done

for miners in 1 2 4; do
  MINER_COUNT="$miners" \
    STAGE_NAME="identity-check" \
    bash scripts/v2_3_0_private_rehearsal_compat.sh --verify-template
done

grep -Fq 'artifacts/v2_3_0/private_5n_1m_rehearsal' scripts/v2_3_0_private_5n_1m_rehearsal.sh
grep -Fq 'artifacts/v2_3_0/private_5n_2m_rehearsal' scripts/v2_3_0_private_5n_2m_rehearsal.sh
grep -Fq 'artifacts/v2_3_0/private_5n_4m_rehearsal' scripts/v2_3_0_private_5n_4m_rehearsal.sh

echo "PASS: v2.3.0 rehearsal identity regression"
