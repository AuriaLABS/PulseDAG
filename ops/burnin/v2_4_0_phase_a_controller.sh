#!/usr/bin/env bash
set -euo pipefail

: "${SSH_KEY:?missing protected SSH key}"
: "${KNOWN_HOSTS:?missing protected known_hosts}"
: "${INVENTORY_B64:?missing protected rehearsal inventory}"
: "${GH_TOKEN:?missing GitHub token}"
: "${GITHUB_RUN_ID:?missing GitHub run id}"
: "${GITHUB_RUN_ATTEMPT:?missing GitHub run attempt}"

CANDIDATE_SHA="8a1a5f74e03eae695e76bf8a84ddc9d48f94db34"
REPO_URL="https://github.com/AuriaLABS/PulseDAG.git"
WORK="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/pulsedag-v2.4.0-phase-a"
OUT="${GITHUB_WORKSPACE:-$PWD}/ops-output/phase-a"
rm -rf "${WORK}" "${OUT}"
install -d -m 0700 "${WORK}" "${OUT}" "$HOME/.ssh"

printf '%s\n' "${SSH_KEY}" > "$HOME/.ssh/id_ed25519"
printf '%s\n' "${KNOWN_HOSTS}" > "$HOME/.ssh/known_hosts"
chmod 0600 "$HOME/.ssh/id_ed25519" "$HOME/.ssh/known_hosts"

printf '%s' "${INVENTORY_B64}" | base64 --decode > "${WORK}/inventory.json"
chmod 0600 "${WORK}/inventory.json"

node_index="$(jq -r '[.nodes | to_entries[] | select(.value.transport_mode == "ssh") | .key][0] // empty' "${WORK}/inventory.json")"
test -n "${node_index}"
jq -r --argjson i "${node_index}" '.nodes[$i].transport[]' "${WORK}/inventory.json" > "${WORK}/transport.txt"
host_label="$(jq -r --argjson i "${node_index}" '.nodes[$i].name' "${WORK}/inventory.json")"
test -n "${host_label}"
mapfile -t ssh_argv < "${WORK}/transport.txt"

"${ssh_argv[@]}" 'set -e; command -v git; command -v cargo; command -v curl; command -v python3; printf "remote-preflight-ok\n"'

run_id="${CANDIDATE_SHA:0:12}-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
printf '%s\n' "${run_id}" > "${OUT}/run-id.txt"
printf '%s\n' "${host_label}" > "${OUT}/host-label.txt"

printf -v remote_cmd 'bash -s -- %q %q %q %q' "${CANDIDATE_SHA}" "${run_id}" "${host_label}" "${REPO_URL}"
"${ssh_argv[@]}" "${remote_cmd}" < ops/burnin/v2_4_0_phase_a_remote.sh 2>&1 | tee "${OUT}/launch.log"

printf -v start_cmd 'cat "$HOME/pulsedag-burnin-v2.4.0/%s/state/start.json"' "${run_id}"
"${ssh_argv[@]}" "${start_cmd}" > "${OUT}/start.json"
jq -e --arg sha "${CANDIDATE_SHA}" '
  .gate == "v2.4.0-private-burnin-phase-a-start" and
  .candidate_sha == $sha and
  .network_profile == "private-testnet-v2.4.0" and
  .chain_id == "pulsedag-private-v2.4.0" and
  .public_testnet_ready == false and
  .thirty_day_public_testnet_clock_started == false and
  .contracts_enabled == false
' "${OUT}/start.json" >/dev/null

printf -v tar_cmd 'cd "$HOME/pulsedag-burnin-v2.4.0/%s" && tar -czf - evidence state/start.json state/clock-start-utc.txt state/candidate.sha state/host-label.txt' "${run_id}"
"${ssh_argv[@]}" "${tar_cmd}" > "${OUT}/initial-evidence.tar.gz"
sha256sum "${OUT}/initial-evidence.tar.gz" > "${OUT}/initial-evidence.tar.gz.sha256"

python3 - "${OUT}/start.json" "${OUT}/issue-comment.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    start = json.load(handle)
body = f"""## v2.4.0 private burn-in Phase A — CLOCK STARTED

The operational 24-hour clock has started on a protected remote host after the exact frozen candidate passed the final gate and the Phase A host passed its fail-closed preflight.

- Candidate SHA: `{start['candidate_sha']}`
- Run ID: `{start['run_id']}`
- Host label: `{start['host_label']}`
- Actual UTC clock start: **`{start['clock_start_utc']}`**
- Network profile: `{start['network_profile']}`
- Chain ID: `{start['chain_id']}`
- Consensus mode: `{start['consensus_mode']}`
- Sanitized config SHA-256: `{start['config_sha256']}`
- Node binary SHA-256: `{start['node_sha256']}`
- Miner binary SHA-256: `{start['miner_sha256']}`
- Node/miner/exporter/monitor processes: verified alive at the clock boundary
- Fresh isolated RocksDB path: unique to this run
- Public-testnet readiness: `false`
- 30-day public-testnet clock: `false`
- Contracts: disabled

Phase A monitoring is recording RPC snapshots, exporter health/metrics, process resource state, disk usage and RocksDB size every five minutes on the remote host. Do not combine evidence from any other candidate SHA or burn-in run. Any invalidating source/config/database drift resets this clock.
"""
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump({"body": body}, handle)
PY

curl -fsS \
  -X POST \
  -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  https://api.github.com/repos/AuriaLABS/PulseDAG/issues/789/comments \
  --data-binary "@${OUT}/issue-comment.json" \
  > "${OUT}/issue-comment-response.json"
jq -e '.id and .html_url' "${OUT}/issue-comment-response.json" >/dev/null

echo "Phase A clock started and recorded for ${CANDIDATE_SHA}."
