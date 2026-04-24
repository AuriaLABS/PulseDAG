#!/usr/bin/env bash
set -euo pipefail

REPO_VERSION="$(< VERSION)"
RUN_ID="${1:-${REPO_VERSION}-burnin}"
RUN_DATE="${2:-}"
OUTPUT_DIR="${3:-artifacts/release-evidence/${RUN_ID}}"

if [[ -z "${RUN_DATE}" ]]; then
  RUN_DATE="$(date -u +%F)"
fi

mkdir -p "${OUTPUT_DIR}"/{runtime-alerts,snapshot-cadence,pruning-cadence,p2p-recovery,restart-recovery-notes}

cat > "${OUTPUT_DIR}/README.md" <<EOT
# PulseDAG ${REPO_VERSION} burn-in evidence bundle

- Run ID: ${RUN_ID}
- Run date (UTC): ${RUN_DATE}
- Scope: evidence capture only. This bundle **does not** claim a completed 14-day burn-in by itself.

## Required sections
1. runtime alerts
2. snapshot cadence
3. pruning cadence
4. p2p recovery stats
5. restart / recovery notes

See \
- docs/BURN_IN_14D.md for execution rules.\
- docs/RELEASE_EVIDENCE.md for artifact expectations.
EOT

cat > "${OUTPUT_DIR}/runtime-alerts/alerts.csv" <<'EOT'
timestamp_utc,severity,source,summary,ticket
EOT

cat > "${OUTPUT_DIR}/snapshot-cadence/snapshot-events.csv" <<'EOT'
timestamp_utc,node_id,height,snapshot_path,duration_seconds,result,notes
EOT

cat > "${OUTPUT_DIR}/pruning-cadence/pruning-events.csv" <<'EOT'
timestamp_utc,node_id,from_height,to_height,duration_seconds,reclaimed_bytes,result,notes
EOT

cat > "${OUTPUT_DIR}/p2p-recovery/recovery-events.csv" <<'EOT'
timestamp_utc,node_id,event,recovery_seconds,peer_count_before,peer_count_after,notes
EOT

cat > "${OUTPUT_DIR}/restart-recovery-notes/restart-log.md" <<'EOT'
# Restart and recovery notes

## Event template
- Timestamp (UTC):
- Node:
- Trigger (planned/unplanned):
- Pre-restart height:
- Post-restart healthy timestamp (UTC):
- Recovery duration (seconds):
- Evidence links (logs/metrics):
- Follow-up action:
EOT

cat > "${OUTPUT_DIR}/CHECKLIST.md" <<EOT
# ${REPO_VERSION} burn-in evidence checklist

Run ID: ${RUN_ID}
Run date (UTC): ${RUN_DATE}

- [ ] Runtime alert summary completed (runtime-alerts/alerts.csv)
- [ ] Snapshot cadence events recorded (snapshot-cadence/snapshot-events.csv)
- [ ] Pruning cadence events recorded (pruning-cadence/pruning-events.csv)
- [ ] P2P recovery events recorded (p2p-recovery/recovery-events.csv)
- [ ] Restart/recovery incident notes recorded (restart-recovery-notes/restart-log.md)
- [ ] Linked incident tickets are triaged and closed or explicitly accepted for release
- [ ] Release manager sign-off attached to this bundle

> This checklist captures evidence only. It does not replace the 14-day run execution requirements.
EOT


echo "Generated evidence bundle at ${OUTPUT_DIR}"
