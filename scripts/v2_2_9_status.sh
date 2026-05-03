#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_9_common.sh"

check_endpoint() {
  local base="$1"
  local ep="$2"
  local url="http://$base$ep"
  local code
  if ! code="$(curl -s -m 3 -o /tmp/pulsedag_status_body.$$ -w '%{http_code}' "$url")"; then
    code="000"
  fi
  if [[ "$code" =~ ^2 ]]; then
    echo "  [ok] $ep ($code)"
  elif [[ "$code" == "404" ]]; then
    echo "  [skip] $ep not exposed (404)"
  elif [[ "$code" == "000" ]]; then
    echo "  [down] $ep unreachable"
  else
    echo "  [warn] $ep returned HTTP $code"
  fi
  rm -f /tmp/pulsedag_status_body.$$ || true
}

for node in a b c; do
  rpc="$(node_rpc "$node")"
  pid_file="$(node_pid_file "$node")"
  echo "node-$node @ $rpc"

  if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "  [ok] process running (pid=$(cat "$pid_file"))"
  else
    echo "  [warn] process not running (or not tracked)"
  fi

  for ep in /health /status /tips /pow /runtime /metrics /p2p/status; do
    check_endpoint "$rpc" "$ep"
  done
  echo

done
