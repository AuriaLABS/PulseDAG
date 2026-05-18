#!/usr/bin/env bash
set -euo pipefail

# Lightweight v2.2.16 miner performance evidence harness.
#
# This script is intentionally optional and non-consensus. It runs the standalone
# external miner for a bounded number of short attempts, parses miner stdout, and
# writes JSON/CSV evidence under artifacts/. It does not add pool/share logic and
# does not require GPU hardware.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

NODE_URL="${NODE_URL:-http://127.0.0.1:8080}"
MINER_ADDRESS="${MINER_ADDRESS:-bench-v2_2_16-local}"
ITERATIONS="${ITERATIONS:-1}"
MAX_TRIES="${MAX_TRIES:-50000}"
THREADS="${THREADS:-}"
TIMEOUT_SECS="${TIMEOUT_SECS:-45}"
RUN_GPU="${RUN_GPU:-auto}"
GPU_DEVICE="${GPU_DEVICE:-}"
BENCH_STRICT="${BENCH_STRICT:-0}"
BENCH_BUILD="${BENCH_BUILD:-0}"
BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
ARTIFACT_BASE="${ARTIFACT_DIR:-$ROOT_DIR/artifacts/v2.2.16/miner-benchmark}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$ARTIFACT_BASE/$STAMP"
mkdir -p "$OUT_DIR"

if [[ -z "$THREADS" ]]; then
  if command -v nproc >/dev/null 2>&1; then
    THREADS="$(nproc)"
  else
    THREADS="1"
  fi
  # Keep default local/VPS runs conservative; operators can raise THREADS.
  if [[ "$THREADS" -gt 2 ]]; then
    THREADS="2"
  fi
fi

if [[ "$BENCH_BUILD" == "1" ]]; then
  cargo build -p pulsedag-miner --release
fi

CSV_PATH="$OUT_DIR/miner_benchmark.csv"
JSON_PATH="$OUT_DIR/miner_benchmark.json"
SUMMARY_PATH="$OUT_DIR/summary.md"

printf 'backend,status,iterations,hashes_per_sec_avg,accepted_submits,rejected_submits,stale_skips,avg_template_age_secs,log_path\n' > "$CSV_PATH"

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/ }"
  printf '%s' "$value"
}

append_csv_row() {
  local backend="$1" status="$2" iterations="$3" hps="$4" accepted="$5" rejected="$6" stale="$7" age="$8" log_path="$9"
  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$backend" "$status" "$iterations" "$hps" "$accepted" "$rejected" "$stale" "$age" "$log_path" >> "$CSV_PATH"
}

parse_log() {
  local log_path="$1"
  awk '
    BEGIN { hps_sum = 0; hps_count = 0; accepted = 0; rejected = 0; stale = 0; age_sum = 0; age_count = 0; created = ""; }
    /template received:/ {
      if (match($0, /created_at=[0-9]+/)) {
        created = substr($0, RSTART + 11, RLENGTH - 11);
      }
    }
    /mining:/ {
      if (match($0, /hashes_per_sec=[0-9]+(\.[0-9]+)?/)) {
        value = substr($0, RSTART + 15, RLENGTH - 15);
        hps_sum += value;
        hps_count += 1;
      }
    }
    /submit_result:/ {
      if ($0 ~ /accepted=true/) { accepted += 1; }
      if ($0 ~ /rejected=true/ || $0 ~ /accepted=false/) { rejected += 1; }
      if (created != "") {
        submit_epoch = $1 + 0;
        age = submit_epoch - created;
        if (age >= 0) {
          age_sum += age;
          age_count += 1;
        }
      }
    }
    /submit_rejected:/ { if ($0 !~ /submit_result:/) { rejected += 1; } }
    /stale-template safety: skip submit:/ { stale += 1; }
    END {
      hps_avg = (hps_count > 0) ? hps_sum / hps_count : 0;
      age_avg = (age_count > 0) ? age_sum / age_count : 0;
      printf "%.2f %d %d %d %.2f %d\n", hps_avg, accepted, rejected, stale, age_avg, hps_count;
    }
  ' "$log_path"
}

run_backend() {
  local backend="$1"
  local log_path="$OUT_DIR/${backend}.log"
  local status="pass"
  local hps="0.00" accepted="0" rejected="0" stale="0" age="0.00" hps_count="0"

  : > "$log_path"

  if [[ ! -x "$BIN" ]]; then
    status="skip_binary_missing"
    printf '%s binary_missing path=%s\n' "$(date +%s)" "$BIN" >> "$log_path"
    append_csv_row "$backend" "$status" "0" "$hps" "$accepted" "$rejected" "$stale" "$age" "$log_path"
    return 0
  fi

  for ((i = 1; i <= ITERATIONS; i++)); do
    printf '%s benchmark_iteration backend=%s iteration=%s node=%s max_tries=%s threads=%s\n' \
      "$(date +%s)" "$backend" "$i" "$NODE_URL" "$MAX_TRIES" "$THREADS" >> "$log_path"

    local args=("--node" "$NODE_URL" "--miner-address" "$MINER_ADDRESS" "--backend" "$backend" "--max-tries" "$MAX_TRIES" "--threads" "$THREADS" "--no-heartbeat")
    if [[ "$backend" == "gpu" && -n "$GPU_DEVICE" ]]; then
      args+=("--gpu-device" "$GPU_DEVICE")
    fi

    set +e
    timeout "$TIMEOUT_SECS" "$BIN" "${args[@]}" 2>&1 | while IFS= read -r line; do
      printf '%s %s\n' "$(date +%s)" "$line"
    done >> "$log_path"
    local cmd_status=${PIPESTATUS[0]}
    set -e

    if [[ "$cmd_status" -eq 124 ]]; then
      status="timeout"
      printf '%s benchmark_timeout backend=%s iteration=%s timeout_secs=%s\n' "$(date +%s)" "$backend" "$i" "$TIMEOUT_SECS" >> "$log_path"
      break
    elif [[ "$cmd_status" -ne 0 ]]; then
      if [[ "$backend" == "gpu" ]] && grep -Eq "built without the gpu feature|not implemented yet|OpenCL GPU backend initialization failed|no OpenCL GPU devices|OpenCL runtime library not found" "$log_path"; then
        status="skip_gpu_unavailable_or_not_implemented"
      else
        status="error"
      fi
      printf '%s benchmark_command_exit backend=%s iteration=%s status=%s\n' "$(date +%s)" "$backend" "$i" "$cmd_status" >> "$log_path"
      break
    fi
  done

  read -r hps accepted rejected stale age hps_count < <(parse_log "$log_path")
  if [[ "$status" == "pass" && "$hps_count" == "0" ]]; then
    status="no_mining_sample"
  fi

  append_csv_row "$backend" "$status" "$ITERATIONS" "$hps" "$accepted" "$rejected" "$stale" "$age" "$log_path"
}

run_backend cpu

case "$RUN_GPU" in
  1|true|yes|auto)
    run_backend gpu
    ;;
  0|false|no)
    append_csv_row gpu not_requested 0 0.00 0 0 0 0.00 "$OUT_DIR/gpu.log"
    printf '%s gpu_not_requested\n' "$(date +%s)" > "$OUT_DIR/gpu.log"
    ;;
  *)
    echo "invalid RUN_GPU=$RUN_GPU (expected auto, true, or false)" >&2
    exit 2
    ;;
esac

{
  echo "["
  tail -n +2 "$CSV_PATH" | awk -F, '
    BEGIN { first = 1 }
    {
      if (!first) { print "," }
      first = 0
      printf "  {\"backend\":\"%s\",\"status\":\"%s\",\"iterations\":%s,\"hashes_per_sec_avg\":%s,\"accepted_submits\":%s,\"rejected_submits\":%s,\"stale_skips\":%s,\"avg_template_age_secs\":%s,\"log_path\":\"%s\"}", $1, $2, $3, $4, $5, $6, $7, $8, $9
    }
    END { if (NR > 0) { print "" } }
  '
  echo "]"
} > "$JSON_PATH"

{
  echo "# v2.2.16 miner benchmark evidence"
  echo
  echo "- UTC timestamp: \`$STAMP\`"
  echo "- Node URL: \`$NODE_URL\`"
  echo "- Miner address: \`$MINER_ADDRESS\`"
  echo "- Iterations: \`$ITERATIONS\`"
  echo "- Max tries per iteration: \`$MAX_TRIES\`"
  echo "- Threads: \`$THREADS\`"
  echo "- Timeout seconds: \`$TIMEOUT_SECS\`"
  echo "- CSV: \`$CSV_PATH\`"
  echo "- JSON: \`$JSON_PATH\`"
  echo
  echo "## Results"
  echo
  echo '| backend | status | hashes/sec avg | accepted | rejected | stale skips | avg template age at submit (s) |'
  echo '| --- | --- | ---: | ---: | ---: | ---: | ---: |'
  tail -n +2 "$CSV_PATH" | awk -F, '{ printf "| `%s` | `%s` | %s | %s | %s | %s | %s |\n", $1, $2, $4, $5, $6, $7, $8 }'
  echo
  echo "This harness is optional, non-consensus, bounded by default, and does not add pool/share logic. GPU results may be \`not_requested\` or \`skip_gpu_unavailable_or_not_implemented\` on hosts without a canonical feature-gated GPU backend."
} > "$SUMMARY_PATH"

echo "wrote miner benchmark artifacts:"
echo "  $CSV_PATH"
echo "  $JSON_PATH"
echo "  $SUMMARY_PATH"

if [[ "$BENCH_STRICT" == "1" ]] && grep -Eq ',(error|timeout|no_mining_sample|skip_binary_missing),' "$CSV_PATH"; then
  echo "BENCH_STRICT=1 and benchmark did not produce complete samples" >&2
  exit 1
fi
