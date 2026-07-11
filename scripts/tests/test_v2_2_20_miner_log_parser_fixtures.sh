#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PARSER="$ROOT_DIR/scripts/lib/miner_evidence_parser.awk"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
run_fixture(){ awk -f "$PARSER" "$1"; }
assert_metric(){ local parsed="$1" key="$2" expected="$3" actual; actual="$(printf '%s\n' "$parsed" | awk -F= -v k="$key" '$1==k{gsub(/^'\''|'\''$/, "", $2); print $2}')"; [[ "$actual" == "$expected" ]] || { echo "expected $key=$expected got $actual" >&2; echo "$parsed" >&2; return 1; }; }

cat > "$TMP_DIR/accepted_heartbeat.log" <<'EOF'
miner_telemetry event=heartbeat submits_total=100 submits_accepted=99 submits_rejected=1 templates_received=10 stale_template=false
INFO template_received block_template_hash=aaa stale_template=false
INFO submit_result: accepted=true rejected=false reason_code=none block_hash=abc001 height=101 pow_accepted_dev=true stale_template=false
miner_telemetry event=heartbeat submits_total=100 submits_accepted=99 submits_rejected=1
EOF
parsed="$(run_fixture "$TMP_DIR/accepted_heartbeat.log")"
assert_metric "$parsed" local_miner_templates_received 1
assert_metric "$parsed" local_miner_submits_total 1
assert_metric "$parsed" local_miner_submits_accepted 1
assert_metric "$parsed" local_miner_submits_rejected 0
assert_metric "$parsed" unique_submitted_block_hashes 1

cat > "$TMP_DIR/stale_submit.log" <<'EOF'
INFO template_received block_template_hash=bbb stale_parent_template=old
WARN submit_result: accepted=false rejected=true reason_code=stale_template block_hash=def002 height=102 pow_accepted_dev=false stale_template=true
miner_telemetry event=heartbeat submits_total=999 submits_accepted=500 submits_rejected=499 last_reject_code=stale_template
EOF
parsed="$(run_fixture "$TMP_DIR/stale_submit.log")"
assert_metric "$parsed" local_miner_submits_total 1
assert_metric "$parsed" local_miner_submits_accepted 0
assert_metric "$parsed" local_miner_submits_rejected 1
printf '%s\n' "$parsed" | rg -q "stale_template"

cat > "$TMP_DIR/repeated_telemetry.log" <<'EOF'
miner_telemetry event=heartbeat submits_total=10 submits_accepted=8 submits_rejected=2
miner_telemetry event=heartbeat submits_total=11 submits_accepted=9 submits_rejected=2
miner_telemetry event=heartbeat stale_template=false templates_received=100
EOF
parsed="$(run_fixture "$TMP_DIR/repeated_telemetry.log")"
assert_metric "$parsed" local_miner_submits_total 0
assert_metric "$parsed" local_miner_submits_accepted 0
assert_metric "$parsed" local_miner_submits_rejected 0

python3 - <<'PY' "$TMP_DIR/5n1m.log" "$TMP_DIR/5n2m.log"
import sys
p1,p2=sys.argv[1:]
with open(p1,'w') as f:
    for i in range(391):
        f.write(f"INFO template_received block_template_hash=t{i:04x}\n")
        f.write(f"INFO submit_result: accepted=true rejected=false reason_code=none block_hash=a{i:04x} height={i}\n")
with open(p2,'w') as f:
    for i in range(702): f.write(f"INFO template_received block_template_hash=u{i:04x}\n")
    for i in range(696): f.write(f"INFO submit_result: accepted=true rejected=false reason_code=none block_hash=b{i:04x} height={i}\n")
    for i in range(6): f.write(f"WARN submit_result: accepted=false rejected=true reason_code=stale_template block_hash=c{i:04x} height={i}\n")
PY
parsed="$(run_fixture "$TMP_DIR/5n1m.log")"
assert_metric "$parsed" local_miner_templates_received 391
assert_metric "$parsed" local_miner_submits_total 391
assert_metric "$parsed" local_miner_submits_accepted 391
assert_metric "$parsed" local_miner_submits_rejected 0
parsed="$(run_fixture "$TMP_DIR/5n2m.log")"
assert_metric "$parsed" local_miner_templates_received 702
assert_metric "$parsed" local_miner_submits_total 702
assert_metric "$parsed" local_miner_submits_accepted 696
assert_metric "$parsed" local_miner_submits_rejected 6
printf '%s\n' "$parsed" | rg -q 'stale_template'


python3 - <<'PY' "$TMP_DIR/5n4m.log"
import sys
p=sys.argv[1]
with open(p,'w') as f:
    for i in range(1227): f.write(f"INFO template_received block_template_hash=v{i:04x}\n")
    for i in range(1033): f.write(f"INFO submit_result: accepted=true rejected=false reason_code=none block_hash=d{i:04x} height={i}\n")
    for i in range(194): f.write(f"WARN submit_result: accepted=false rejected=true reason_code=stale_template block_hash=e{i:04x} height={i}\n")
PY
parsed="$(run_fixture "$TMP_DIR/5n4m.log")"
assert_metric "$parsed" local_miner_templates_received 1227
assert_metric "$parsed" local_miner_submits_total 1227
assert_metric "$parsed" local_miner_submits_accepted 1033
assert_metric "$parsed" local_miner_submits_rejected 194
