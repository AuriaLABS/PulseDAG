#!/usr/bin/env python3
"""Normalize known Bash defects in the v2.3.0 lag runtime harness.

The candidate driver may point at the repository harness or an externally supplied
copy. This patcher is deliberately fail-closed: every expected unsafe or fixed
form must be present exactly once before the normalized harness is emitted.
"""

from __future__ import annotations

import sys
from pathlib import Path


def replace_once_or_confirm(text: str, unsafe: str, fixed: str, label: str) -> str:
    unsafe_count = text.count(unsafe)
    fixed_count = text.count(fixed)
    if unsafe_count == 1 and fixed_count == 0:
        return text.replace(unsafe, fixed, 1)
    if unsafe_count == 0 and fixed_count == 1:
        return text
    raise SystemExit(
        f"runtime harness patch precondition failed for {label}: "
        f"unsafe={unsafe_count} fixed={fixed_count}"
    )


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} INPUT OUTPUT", file=sys.stderr)
        return 64

    source_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    text = source_path.read_text()

    text = replace_once_or_confirm(
        text,
        '    local idx="$1" boot="$2" data="$out_dir/data/n$idx"',
        '    local idx="$1"\n'
        '    local boot="$2"\n'
        '    local data="$out_dir/data/n$idx"',
        "node local declarations",
    )
    text = replace_once_or_confirm(
        text,
        '    local event="$1" node="${2:-}" details="${3:-{}}"',
        '    local event="$1"\n'
        '    local node="${2:-}"\n'
        '    local details="${3:-}"\n'
        "    [[ -n \"$details\" ]] || details='{}'",
        "event JSON default",
    )
    text = replace_once_or_confirm(
        text,
        "  trap '_v230_lag_unexpected_exit $?' EXIT",
        "  trap '_v230_lag_unexpected_exit $?' ERR",
        "unexpected-exit trap",
    )
    text = replace_once_or_confirm(
        text,
        "\n    trap - EXIT INT TERM\n",
        "\n    trap - ERR INT TERM\n",
        "abort trap cleanup",
    )
    text = replace_once_or_confirm(
        text,
        "\n  trap - EXIT INT TERM\n",
        "\n  trap - ERR INT TERM\n",
        "success trap cleanup",
    )

    unsafe_socket_helper = r'''  _v230_lag_kill_stopped_node_sockets() {
    local pid="$1" endpoint port killed=0
    local -a _v230_lag_ports=()
    command -v ss >/dev/null 2>&1 || return 1
    mapfile -t _v230_lag_ports < <(
      ss -Htnp state established 2>/dev/null | awk -v token="pid=$pid," 'index($0, token) {print $4}' | sed -E 's/.*:([0-9]+)$/\1/' | awk '/^[0-9]+$/' | sort -u
    )
    ((${#_v230_lag_ports[@]})) || return 1
    for port in "${_v230_lag_ports[@]}"; do
      if ss -K state established sport = ":$port" >/dev/null 2>&1; then
        killed=$((killed + 1))
      elif command -v sudo >/dev/null 2>&1 && sudo -n ss -K state established sport = ":$port" >/dev/null 2>&1; then
        killed=$((killed + 1))
      fi
    done
    sleep 1
    endpoint="$(ss -Htnp state established 2>/dev/null | awk -v token="pid=$pid," 'index($0, token) {print; exit}')"
    [[ -z "$endpoint" && "$killed" -gt 0 ]]
  }'''
    fixed_socket_helper = r'''  _v230_lag_kill_stopped_node_sockets() {
    local pid="$1" row local_endpoint peer_endpoint remaining deadline
    local -a socket_rows=()
    local -a ss_cmd=(ss)
    command -v ss >/dev/null 2>&1 || return 1
    if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
      ss_cmd=(sudo -n ss)
    fi
    mapfile -t socket_rows < <(
      "${ss_cmd[@]}" -Htnp state established 2>/dev/null |
        awk -v token="pid=$pid," 'index($0, token) {print $(NF-2) "\t" $(NF-1)}' |
        sort -u
    )
    ((${#socket_rows[@]})) || return 1
    for row in "${socket_rows[@]}"; do
      IFS=$'\t' read -r local_endpoint peer_endpoint <<< "$row"
      [[ -n "$local_endpoint" && -n "$peer_endpoint" ]] || continue
      "${ss_cmd[@]}" -K state established src "$local_endpoint" dst "$peer_endpoint" >/dev/null 2>&1 || true
    done
    deadline=$(( $(date +%s) + 5 ))
    while (( $(date +%s) < deadline )); do
      remaining="$("${ss_cmd[@]}" -Htnp state established 2>/dev/null |
        awk -v token="pid=$pid," 'index($0, token) {print; exit}')"
      [[ -z "$remaining" ]] && return 0
      sleep 1
    done
    return 1
  }'''
    text = replace_once_or_confirm(
        text,
        unsafe_socket_helper,
        fixed_socket_helper,
        "stopped-node socket destruction",
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
