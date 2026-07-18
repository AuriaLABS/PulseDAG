from pathlib import Path

harness_path = Path("scripts/lib/v2_3_0_runtime_harness.sh")
harness = harness_path.read_text()

if "pulsedag_json_counter_total()" not in harness:
    anchor = '''pulsedag_json_txids_sorted() {
  local file="$1"
  jq -r '(.data.txids // [])[]' "$file" | sort -u
}

'''
    helper = '''pulsedag_json_counter_total() {
  local file="$1" expr="$2"
  jq -r "($expr // 0) | if type == \\"object\\" then ([.[] | numbers] | add // 0) elif type == \\"number\\" then . else 0 end" "$file" 2>/dev/null |
    head -n1 | awk '/^[0-9]+$/ {print; found=1} END {if (!found) print 0}'
}

'''
    if anchor not in harness:
        raise SystemExit("top-level JSON helper anchor not found")
    harness = harness.replace(anchor, anchor + helper, 1)

replacements = [
    (
        '''_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-p2p-status.json" '.data.remote_tip_inventory_received_total' '''.strip(),
        '''pulsedag_json_counter_total "$out_dir/endpoints/pre_isolation/n5-p2p-status.json" '.data.remote_tip_inventory_received_total' '''.strip(),
        "baseline remote inventory counter",
    ),
    (
        '''_v230_lag_json_num "$p2p_file" '.data.remote_tip_inventory_received_total' '''.strip(),
        '''pulsedag_json_counter_total "$p2p_file" '.data.remote_tip_inventory_received_total' '''.strip(),
        "recovery remote inventory counter",
    ),
    (
        '''_v230_lag_json_num "$n5_p2p" '.data.remote_tip_inventory_received_total' '''.strip(),
        '''pulsedag_json_counter_total "$n5_p2p" '.data.remote_tip_inventory_received_total' '''.strip(),
        "final remote inventory counter",
    ),
]
for old, new, label in replacements:
    old_count = harness.count(old)
    new_count = harness.count(new)
    if old_count == 1 and new_count == 0:
        harness = harness.replace(old, new, 1)
    elif old_count == 0 and new_count == 1:
        pass
    else:
        raise SystemExit(f"{label} anchor mismatch: old={old_count} new={new_count}")

seen_old = '''    if (( seen_headers == 0 && final_headers_received > baseline_headers_received && final_headers_received - baseline_headers_received > final_uncorrelated_headers - baseline_uncorrelated_headers )); then
'''
seen_new = '''    if (( seen_headers == 0 && final_headers_received > baseline_headers_received )); then
'''
if seen_old in harness:
    harness = harness.replace(seen_old, seen_new, 1)
elif seen_new not in harness:
    raise SystemExit("correlated header observation anchor not found")

delta_old = '''  local correlated_headers_delta=$((headers_received_delta - uncorrelated_headers_delta))
  (( correlated_headers_delta < 0 )) && correlated_headers_delta=0
'''
delta_new = '''  local correlated_headers_delta="$headers_received_delta"
'''
if delta_old in harness:
    harness = harness.replace(delta_old, delta_new, 1)
elif delta_new not in harness:
    raise SystemExit("correlated header delta anchor not found")

harness_path.write_text(harness)

test_path = Path("scripts/tests/test_v2_3_0_lag_runtime_driver.sh")
test = test_path.read_text()
if "remote-tip-counter-map.json" not in test:
    anchor = '''grep -Fq 'remote_tip_inventory_accepted_total' "$HARNESS"
grep -Fq 'closeout_eligible:true' "$HARNESS"
'''
    addition = '''grep -Fq 'remote_tip_inventory_accepted_total' "$HARNESS"
grep -Fq 'pulsedag_json_counter_total' "$HARNESS"
grep -Fq 'local correlated_headers_delta="$headers_received_delta"' "$HARNESS"
if grep -Fq 'headers_received_delta - uncorrelated_headers_delta' "$HARNESS"; then
  echo "correlated selected-segment headers must not subtract unrelated frontier headers" >&2
  exit 1
fi
cat > "$tmp/remote-tip-counter-map.json" <<'JSON'
{"data":{"remote_tip_inventory_received_total":{"GetTips":4,"Tips":16}}}
JSON
[[ "$(pulsedag_json_counter_total "$tmp/remote-tip-counter-map.json" '.data.remote_tip_inventory_received_total')" == 20 ]]
cat > "$tmp/remote-tip-counter-scalar.json" <<'JSON'
{"data":{"remote_tip_inventory_received_total":7}}
JSON
[[ "$(pulsedag_json_counter_total "$tmp/remote-tip-counter-scalar.json" '.data.remote_tip_inventory_received_total')" == 7 ]]
grep -Fq 'closeout_eligible:true' "$HARNESS"
'''
    if anchor not in test:
        raise SystemExit("lag driver contract test anchor not found")
    test = test.replace(anchor, addition, 1)
    test_path.write_text(test)
