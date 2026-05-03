#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_9_common.sh"

ensure_dirs

for node in a b c; do
  pid_file="$(node_pid_file "$node")"
  if [[ ! -f "$pid_file" ]]; then
    echo "[info] node-$node not tracked (missing pid file)"
    continue
  fi

  pid="$(cat "$pid_file")"
  if kill -0 "$pid" 2>/dev/null; then
    echo "[info] stopping node-$node (pid=$pid)"
    kill -TERM "$pid" || true
    for _ in {1..20}; do
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      sleep 0.5
    done
    if kill -0 "$pid" 2>/dev/null; then
      echo "[warn] node-$node did not exit after TERM; sending KILL"
      kill -KILL "$pid" || true
    else
      echo "[ok] node-$node stopped"
    fi
  else
    echo "[info] node-$node pid file exists but process is not running"
  fi
  rm -f "$pid_file"
done
