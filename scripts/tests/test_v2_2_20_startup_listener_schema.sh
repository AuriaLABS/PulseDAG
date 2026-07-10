#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/v2_2_20_private_5n_4m_rehearsal.sh"

rg -q 'extract_p2p_listening_addresses\(\)' "$SCRIPT"
rg -q '.data.listening_addresses' "$SCRIPT"
rg -q '.data.listening' "$SCRIPT"
rg -q '.data.listen_addresses' "$SCRIPT"
rg -q '.data.listeners' "$SCRIPT"
rg -q 'p2p_listener_has_expected_port' "$SCRIPT"
rg -q 'expected_port=\$\(\(BASE_P2P_PORT\+i\)\)' "$SCRIPT"
rg -q 'STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES' "$SCRIPT"
rg -q 'MINERS_STARTED=\$\(\(MINERS_STARTED \+ 1\)\)' "$SCRIPT"

extract_filter='(
  .data.listening_addresses
  // .data.listening
  // .data.listen_addresses
  // .data.listeners
  // []
) as $listeners
| if ($listeners | type) == "array" then
    $listeners
  elif ($listeners | type) == "string" then
    if ($listeners | length) > 0 then [$listeners] else [] end
  else
    []
  end'

has_port_filter='any(.[]?; (tostring | contains("/tcp/32303")))'

for field in listening_addresses listening listen_addresses listeners; do
  got=$(jq -cn --arg field "$field" '{data:{($field):["/ip4/127.0.0.1/tcp/32303"]}}' | jq -c "$extract_filter")
  [[ "$got" == '["/ip4/127.0.0.1/tcp/32303"]' ]]
  jq -e "$has_port_filter" >/dev/null <<<"$got"
done

string_got=$(jq -cn '{data:{listening:"/ip4/127.0.0.1/tcp/32303"}}' | jq -c "$extract_filter")
[[ "$string_got" == '["/ip4/127.0.0.1/tcp/32303"]' ]]
jq -e "$has_port_filter" >/dev/null <<<"$string_got"

wrong_got=$(jq -cn '{data:{listening_addresses:["/ip4/127.0.0.1/tcp/32304"]}}' | jq -c "$extract_filter")
if jq -e "$has_port_filter" >/dev/null <<<"$wrong_got"; then
  echo "wrong TCP port was accepted" >&2
  exit 1
fi

missing_got=$(jq -cn '{data:{peer_count:4}}' | jq -c "$extract_filter")
[[ "$missing_got" == '[]' ]]
if jq -e "$has_port_filter" >/dev/null <<<"$missing_got"; then
  echo "missing listener was accepted" >&2
  exit 1
fi

echo "v2.2.20 startup listener schema validation passed"
