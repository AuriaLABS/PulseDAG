#!/usr/bin/env bash
set -euo pipefail

evaluate(){
  required_failures=0
  gate_3_nodes_launched=0; gate_miner_launched=0; gate_nodes_healthy=0; gate_nodes_ready=0; gate_templates_seen=0; gate_submissions_seen=0; gate_accepted_blocks=0; gate_heights_gt_genesis=0; gate_p2p_sustained=0; gate_duplicate_sync=0; gate_final_convergence=0; gate_timeline_samples=0; gate_evidence_collection=0; gate_not_interrupted=0; gate_script_completed=0
  (( NODE_A_LAUNCHED==1 && NODE_B_LAUNCHED==1 && NODE_C_LAUNCHED==1 )) && gate_3_nodes_launched=1
  (( MINER_LAUNCHED==1 )) && gate_miner_launched=1
  (( healthy_nodes==3 )) && gate_nodes_healthy=1
  (( ready_nodes==3 )) && gate_nodes_ready=1
  (( miner_templates>=1 )) && gate_templates_seen=1
  (( miner_submits>=1 )) && gate_submissions_seen=1
  (( accepted_count>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && gate_accepted_blocks=1
  (( ha>0 && hb>0 && hc>0 )) && gate_heights_gt_genesis=1
  (( final_peers_ok==1 && pa>=2 && pb>=1 && pc>=1 )) && gate_p2p_sustained=1
  (( duplicate_sync_degraded_blocker==0 )) && gate_duplicate_sync=1
  (( final_converged==1 )) && gate_final_convergence=1
  (( premining_timeline_missing_samples==0 && timeline_sample_count>=1 )) && gate_timeline_samples=1
  (( evidence_collection_failed==0 )) && gate_evidence_collection=1
  (( interrupted==0 )) && gate_not_interrupted=1
  (( script_completed==1 )) && gate_script_completed=1
  for gate in gate_3_nodes_launched gate_miner_launched gate_nodes_healthy gate_nodes_ready gate_templates_seen gate_submissions_seen gate_accepted_blocks gate_heights_gt_genesis gate_p2p_sustained gate_duplicate_sync gate_final_convergence gate_timeline_samples gate_evidence_collection gate_not_interrupted gate_script_completed; do
    (( ${!gate} == 1 )) || ((required_failures+=1))
  done
  if (( required_failures > 0 )); then echo FAIL; else echo PASS; fi
}
base(){ NODE_A_LAUNCHED=1; NODE_B_LAUNCHED=1; NODE_C_LAUNCHED=1; MINER_LAUNCHED=1; healthy_nodes=3; ready_nodes=3; miner_templates=1; miner_submits=1; accepted_count=1; WAIVE_ACCEPTED_BLOCK_GATE=0; ha=1; hb=1; hc=1; final_peers_ok=1; pa=2; pb=1; pc=1; duplicate_sync_degraded_blocker=0; final_converged=1; premining_timeline_missing_samples=0; timeline_sample_count=2; evidence_collection_failed=0; interrupted=0; script_completed=1; }
check(){ local name=$1 exp=$2 mut=$3; base; eval "$mut"; got=$(evaluate); echo "$name => $got"; [[ $got == $exp ]]; }
check all_pass PASS ':'
check interrupted_fail FAIL 'interrupted=1'
check script_completed_fail FAIL 'script_completed=0'
check p2p_fail FAIL 'final_peers_ok=0; pa=0; pb=0; pc=0'
check partial_p2p_a1_b0_c1 FAIL 'final_peers_ok=0; pa=1; pb=0; pc=1'
check evidence_collection_failed FAIL 'evidence_collection_failed=1'
check accepted_fail FAIL 'accepted_count=0'
check timeline_header_only FAIL 'timeline_sample_count=0'
check internal_deadline_exceeded FAIL 'interrupted=1; script_completed=0'

SMOKE_SCRIPT="scripts/v2_2_19_local_3n_1m_smoke.sh"
[[ -f "$SMOKE_SCRIPT" ]]
rg -q 'trap cleanup EXIT' "$SMOKE_SCRIPT"
rg -q 'trap .*TERM' "$SMOKE_SCRIPT"
rg -q 'cleanup_ran=0' "$SMOKE_SCRIPT"
rg -q 'interrupted=0' "$SMOKE_SCRIPT"
rg -q 'script_completed=0' "$SMOKE_SCRIPT"
rg -q 'current-run-dir.txt' "$SMOKE_SCRIPT"
rg -q 'result_source: gate-driven' "$SMOKE_SCRIPT"

tmp_out=$(mktemp -d /tmp/pulsedag-smoke-fail.XXXXXX)
set +e
timeout 30s env P2P_CONNECT_WAIT_SECS=10 DURATION_SECS=30 SAMPLE_INTERVAL_SECS=2 OUT_DIR="$tmp_out" NODE_BIN=/bin/false MINER_BIN=/bin/false bash "$SMOKE_SCRIPT"
rc=$?
set -e
(( rc != 0 ))
run_dir=$(cat "$tmp_out/current-run-dir.txt")
[[ -f "$run_dir/evidence-summary.md" ]]
[[ -f "$run_dir/current-run-dir.txt" ]]
[[ -f "$tmp_out/current-run-dir.txt" ]]
rg -q '^- result: FAIL' "$run_dir/evidence-summary.md"
rg -q '^- result_source: gate-driven' "$run_dir/evidence-summary.md"
rg -q '^- script_completed: 0' "$run_dir/evidence-summary.md"
rg -q '^- timeline_sample_count:' "$run_dir/evidence-summary.md"
