#!/usr/bin/env bash
set -euo pipefail

evaluate(){
  required_failures=0
  gate_3_nodes_launched=0; gate_miner_launched=0; gate_nodes_healthy=0; gate_nodes_ready=0; gate_templates_seen=0; gate_submissions_seen=0; gate_accepted_blocks=0; gate_heights_gt_genesis=0; gate_p2p_sustained=0; gate_duplicate_sync=0; gate_final_convergence=0; gate_timeline_samples=0; gate_evidence_collection=0
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
  for gatev in $gate_3_nodes_launched $gate_miner_launched $gate_nodes_healthy $gate_nodes_ready $gate_templates_seen $gate_submissions_seen $gate_accepted_blocks $gate_heights_gt_genesis $gate_p2p_sustained $gate_duplicate_sync $gate_final_convergence $gate_timeline_samples $gate_evidence_collection; do (( gatev == 1 )) || ((required_failures+=1)); done
  if (( required_failures > 0 )); then echo FAIL; else echo PASS; fi
}
base(){ NODE_A_LAUNCHED=1; NODE_B_LAUNCHED=1; NODE_C_LAUNCHED=1; MINER_LAUNCHED=1; healthy_nodes=3; ready_nodes=3; miner_templates=1; miner_submits=1; accepted_count=1; WAIVE_ACCEPTED_BLOCK_GATE=0; ha=1; hb=1; hc=1; final_peers_ok=1; pa=2; pb=1; pc=1; duplicate_sync_degraded_blocker=0; final_converged=1; premining_timeline_missing_samples=0; timeline_sample_count=2; evidence_collection_failed=0; }
check(){ local name=$1 exp=$2 mut=$3; base; eval "$mut"; got=$(evaluate); echo "$name => $got"; [[ $got == $exp ]]; }
check all_pass PASS ':'
check miner_fail FAIL 'MINER_LAUNCHED=0'
check p2p_fail FAIL 'final_peers_ok=0; pa=0; pb=0; pc=0'
check accepted_fail FAIL 'accepted_count=0'
check heights_fail FAIL 'ha=0'
check timeline_header_only FAIL 'timeline_sample_count=0'
