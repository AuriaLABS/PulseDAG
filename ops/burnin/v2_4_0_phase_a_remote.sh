#!/usr/bin/env bash
set -euo pipefail

CANDIDATE_SHA="${1:?candidate SHA required}"
RUN_ID="${2:?run id required}"
HOST_LABEL="${3:?host label required}"
REPO_URL="${4:-https://github.com/AuriaLABS/PulseDAG.git}"

if [[ ! "${CANDIDATE_SHA}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "invalid candidate SHA" >&2
  exit 2
fi

for tool in git cargo curl python3 sha256sum nohup awk ps df du; do
  command -v "${tool}" >/dev/null 2>&1 || {
    echo "missing required tool: ${tool}" >&2
    exit 3
  }
done

ROOT="${HOME}/pulsedag-burnin-v2.4.0/${RUN_ID}"
SRC="${ROOT}/source"
EVIDENCE="${ROOT}/evidence"
LOGS="${ROOT}/logs"
STATE="${ROOT}/state"
CONFIG="${ROOT}/single-node.env"
ROCKSDB="${ROOT}/rocksdb"
NODE_URL="http://127.0.0.1:8280"
EXPORTER_URL="http://127.0.0.1:19108"
MINER_ADDRESS="burnin-${CANDIDATE_SHA:0:12}"

if [[ -e "${ROOT}" ]]; then
  echo "refusing to reuse existing burn-in root: ${ROOT}" >&2
  exit 4
fi
mkdir -p "${ROOT}" "${EVIDENCE}/preflight" "${EVIDENCE}/samples" "${LOGS}" "${STATE}"
chmod 0700 "${ROOT}" "${STATE}"

cleanup_failed_launch() {
  rc=$?
  if [[ $rc -ne 0 ]]; then
    for name in monitor exporter miner node; do
      pid_file="${STATE}/${name}.pid"
      if [[ -s "${pid_file}" ]]; then
        pid="$(cat "${pid_file}")"
        if kill -0 "${pid}" 2>/dev/null; then
          kill "${pid}" 2>/dev/null || true
        fi
      fi
    done
  fi
  exit $rc
}
trap cleanup_failed_launch EXIT

if curl -fsS --max-time 2 "${NODE_URL}/status" >/dev/null 2>&1; then
  echo "RPC ${NODE_URL} is already occupied; refusing to disturb an existing node" >&2
  exit 5
fi
if curl -fsS --max-time 2 "${EXPORTER_URL}/health" >/dev/null 2>&1; then
  echo "exporter ${EXPORTER_URL} is already occupied; refusing to disturb it" >&2
  exit 6
fi

printf '%s\n' "${CANDIDATE_SHA}" > "${STATE}/candidate.sha"
printf '%s\n' "${HOST_LABEL}" > "${STATE}/host-label.txt"
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "${STATE}/prepare-started-utc.txt"

# Use a dedicated source checkout. Never mutate an existing operator checkout.
git clone --quiet --no-checkout "${REPO_URL}" "${SRC}"
git -C "${SRC}" checkout --quiet --detach "${CANDIDATE_SHA}"
test "$(git -C "${SRC}" rev-parse HEAD)" = "${CANDIDATE_SHA}"
test -z "$(git -C "${SRC}" status --porcelain)"

cd "${SRC}"

# Build the exact frozen candidate with the locked dependency graph.
cargo build --locked --release -p pulsedagd --bin pulsedagd
cargo build --locked --release -p pulsedag-miner --bin pulsedag-miner

NODE_BIN="${SRC}/target/release/pulsedagd"
MINER_BIN="${SRC}/target/release/pulsedag-miner"
test -x "${NODE_BIN}"
test -x "${MINER_BIN}"

# Render the official single-node profile changing only the deployment path.
awk -v rocks="${ROCKSDB}" '
  /^PULSEDAG_ROCKSDB_PATH=/ { print "PULSEDAG_ROCKSDB_PATH=" rocks; next }
  { print }
' configs/single-node/single-node.env.example > "${CONFIG}"
chmod 0600 "${CONFIG}"

# Fail closed before runtime startup and preserve preflight evidence.
OUT_DIR="${EVIDENCE}/preflight" bash scripts/v2_4_0_single_node_preflight.sh "${CONFIG}"

# Freeze sanitized operator inputs and binary digests before the clock.
cp "${CONFIG}" "${EVIDENCE}/single-node.env.sanitized"
sha256sum "${EVIDENCE}/single-node.env.sanitized" > "${EVIDENCE}/config.sha256"
sha256sum "${NODE_BIN}" "${MINER_BIN}" > "${EVIDENCE}/binaries.sha256"
git rev-parse HEAD > "${EVIDENCE}/candidate.sha"
git rev-parse HEAD^{tree} > "${EVIDENCE}/tree.sha"

set -a
# shellcheck disable=SC1090
source "${CONFIG}"
set +a

# Launch the exact candidate node.
nohup "${NODE_BIN}" >"${LOGS}/node.log" 2>&1 </dev/null &
NODE_PID=$!
printf '%s\n' "${NODE_PID}" > "${STATE}/node.pid"

ready=false
for _ in $(seq 1 120); do
  if curl -fsS --max-time 3 "${NODE_URL}/status" > "${EVIDENCE}/initial-status.json" 2>/dev/null; then
    ready=true
    break
  fi
  if ! kill -0 "${NODE_PID}" 2>/dev/null; then
    echo "node exited before readiness" >&2
    tail -n 200 "${LOGS}/node.log" >&2 || true
    exit 7
  fi
  sleep 1
done
[[ "${ready}" == true ]] || {
  echo "node did not become ready" >&2
  tail -n 200 "${LOGS}/node.log" >&2 || true
  exit 8
}

# Launch the external standalone miner continuously.
nohup "${MINER_BIN}" \
  --node "${NODE_URL}" \
  --miner-address "${MINER_ADDRESS}" \
  --threads 2 \
  --max-tries 25000 \
  --loop \
  --sleep-ms 1200 \
  >"${LOGS}/miner.log" 2>&1 </dev/null &
MINER_PID=$!
printf '%s\n' "${MINER_PID}" > "${STATE}/miner.pid"
sleep 3
kill -0 "${MINER_PID}" 2>/dev/null || {
  echo "miner exited during startup" >&2
  tail -n 200 "${LOGS}/miner.log" >&2 || true
  exit 9
}

# Run the repository exporter on a loopback-only non-default port.
nohup python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url "${NODE_URL}" \
  --listen 127.0.0.1:19108 \
  --instance v2.4.0-private-burnin \
  >"${LOGS}/exporter.log" 2>&1 </dev/null &
EXPORTER_PID=$!
printf '%s\n' "${EXPORTER_PID}" > "${STATE}/exporter.pid"

exporter_ready=false
for _ in $(seq 1 30); do
  if curl -fsS --max-time 3 "${EXPORTER_URL}/health" > "${EVIDENCE}/initial-exporter-health.json" 2>/dev/null; then
    exporter_ready=true
    break
  fi
  if ! kill -0 "${EXPORTER_PID}" 2>/dev/null; then
    echo "exporter exited before readiness" >&2
    tail -n 100 "${LOGS}/exporter.log" >&2 || true
    exit 10
  fi
  sleep 1
done
[[ "${exporter_ready}" == true ]] || {
  echo "exporter did not become healthy" >&2
  tail -n 100 "${LOGS}/exporter.log" >&2 || true
  exit 11
}

cat > "${ROOT}/monitor.sh" <<'MONITOR'
#!/usr/bin/env bash
set -euo pipefail
ROOT="${1:?root required}"
NODE_URL="http://127.0.0.1:8280"
EXPORTER_URL="http://127.0.0.1:19108"
STATE="${ROOT}/state"
SAMPLES="${ROOT}/evidence/samples"
LOGS="${ROOT}/logs"

while [[ ! -s "${STATE}/clock-start-epoch.txt" ]]; do
  sleep 1
done
start_epoch="$(cat "${STATE}/clock-start-epoch.txt")"
deadline=$((start_epoch + 86400))

sample_once() {
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  dir="${SAMPLES}/${ts}"
  mkdir -p "${dir}"
  for endpoint in status health runtime 'runtime/events?limit=50' 'runtime/events/summary?limit=200' dag/consistency sync/status orphans mempool; do
    safe="$(printf '%s' "${endpoint}" | tr '/?=&' '____')"
    curl -fsS --max-time 5 "${NODE_URL}/${endpoint}" > "${dir}/${safe}.json" 2>"${dir}/${safe}.err" || true
  done
  curl -fsS --max-time 5 "${EXPORTER_URL}/health" > "${dir}/exporter-health.json" 2>"${dir}/exporter-health.err" || true
  curl -fsS --max-time 5 "${EXPORTER_URL}/metrics" > "${dir}/exporter.metrics" 2>"${dir}/exporter-metrics.err" || true
  {
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for name in node miner exporter monitor; do
      if [[ -s "${STATE}/${name}.pid" ]]; then
        pid="$(cat "${STATE}/${name}.pid")"
        ps -p "${pid}" -o pid=,etime=,rss=,vsz=,args= || true
      fi
    done
  } > "${dir}/processes.txt"
  df -B1 "${ROOT}" > "${dir}/disk.txt" 2>&1 || true
  du -sb "${ROOT}/rocksdb" > "${dir}/rocksdb-size.txt" 2>&1 || true
}

sample_once
while (( $(date +%s) < deadline )); do
  sleep 300
  sample_once
done
sample_once
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "${STATE}/phase-a-24h-completed-utc.txt"
sha256sum "${LOGS}/node.log" "${LOGS}/miner.log" "${LOGS}/exporter.log" > "${ROOT}/evidence/runtime-logs.sha256" || true
MONITOR
chmod 0700 "${ROOT}/monitor.sh"

# Start the monitor before the clock; it waits on the clock-start marker.
nohup "${ROOT}/monitor.sh" "${ROOT}" >"${LOGS}/monitor.log" 2>&1 </dev/null &
MONITOR_PID=$!
printf '%s\n' "${MONITOR_PID}" > "${STATE}/monitor.pid"
sleep 1
kill -0 "${MONITOR_PID}" 2>/dev/null || {
  echo "monitor exited before clock start" >&2
  tail -n 100 "${LOGS}/monitor.log" >&2 || true
  exit 12
}

# Record only the burn-in processes; do not expose unrelated host command lines.
{
  echo "host_label=${HOST_LABEL}"
  echo "candidate_sha=${CANDIDATE_SHA}"
  for name in node miner exporter monitor; do
    pid="$(cat "${STATE}/${name}.pid")"
    printf '%s ' "${name}"
    ps -p "${pid}" -o pid=,lstart=,args=
  done
} > "${EVIDENCE}/process-inventory.txt"

# Prove all four processes are alive immediately before starting the clock.
for name in node miner exporter monitor; do
  pid="$(cat "${STATE}/${name}.pid")"
  kill -0 "${pid}" 2>/dev/null || {
    echo "${name} is not alive at clock boundary" >&2
    exit 13
  }
done
curl -fsS --max-time 3 "${NODE_URL}/status" > "${EVIDENCE}/clock-boundary-status.json"
curl -fsS --max-time 3 "${EXPORTER_URL}/health" > "${EVIDENCE}/clock-boundary-exporter-health.json"

START_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
START_EPOCH="$(date +%s)"
printf '%s\n' "${START_EPOCH}" > "${STATE}/clock-start-epoch.txt"
printf '%s\n' "${START_UTC}" > "${STATE}/clock-start-utc.txt"

CONFIG_DIGEST="$(awk '{print $1; exit}' "${EVIDENCE}/config.sha256")"
NODE_DIGEST="$(awk 'NR==1 {print $1}' "${EVIDENCE}/binaries.sha256")"
MINER_DIGEST="$(awk 'NR==2 {print $1}' "${EVIDENCE}/binaries.sha256")"

python3 - "${STATE}/start.json" <<PY
import json, sys
path = sys.argv[1]
payload = {
    "gate": "v2.4.0-private-burnin-phase-a-start",
    "candidate_sha": "${CANDIDATE_SHA}",
    "run_id": "${RUN_ID}",
    "host_label": "${HOST_LABEL}",
    "network_profile": "private-testnet-v2.4.0",
    "chain_id": "pulsedag-private-v2.4.0",
    "consensus_mode": "legacy",
    "clock_start_utc": "${START_UTC}",
    "clock_start_epoch": ${START_EPOCH},
    "config_sha256": "${CONFIG_DIGEST}",
    "node_sha256": "${NODE_DIGEST}",
    "miner_sha256": "${MINER_DIGEST}",
    "node_pid": ${NODE_PID},
    "miner_pid": ${MINER_PID},
    "exporter_pid": ${EXPORTER_PID},
    "monitor_pid": ${MONITOR_PID},
    "public_testnet_ready": False,
    "thirty_day_public_testnet_clock_started": False,
    "contracts_enabled": False,
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

cp "${STATE}/start.json" "${EVIDENCE}/start.json"
sha256sum "${EVIDENCE}/start.json" "${EVIDENCE}/process-inventory.txt" "${EVIDENCE}/clock-boundary-status.json" > "${EVIDENCE}/start-evidence.sha256"

# A successful return means the clock is genuinely running on the remote host.
trap - EXIT
cat "${STATE}/start.json"
