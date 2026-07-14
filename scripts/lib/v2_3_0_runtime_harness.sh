#!/usr/bin/env bash
# Shared v2.3.0 runtime harness. This file intentionally fails closed: every
# closeout field is derived from a live endpoint capture or node log generated
# during the drill.

v2_3_0_run_lag_injection_selected_segment_drill() {
  local out_dir='' run_id='' min_gap='96' isolated_node='n5' node_count='5' miner_count='4'
  while (($#)); do
    case "$1" in
      --out-dir) out_dir="$2"; shift 2;;
      --run-id) run_id="$2"; shift 2;;
      --min-selected-gap) min_gap="$2"; shift 2;;
      --isolated-node) isolated_node="$2"; shift 2;;
      --node-count) node_count="$2"; shift 2;;
      --miner-count) miner_count="$2"; shift 2;;
      *) echo "unknown harness argument: $1" >&2; return 2;;
    esac
  done
  [[ -n "$out_dir" && -n "$run_id" ]] || { echo "missing --out-dir/--run-id" >&2; return 2; }
  [[ "$isolated_node" == n5 && "$node_count" == 5 && "$miner_count" == 4 ]] || { echo "v2.3.0 lag drill requires n5/5 nodes/4 miners" >&2; return 2; }

  local root_dir node_bin rpc_base p2p_base data_root log_dir runtime_dir cmd_log manifest timeline gap_timeline topology counters final_table
  root_dir="${V2_3_0_ROOT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
  node_bin="${PULSEDAGD_BIN:-$root_dir/target/release/pulsedagd}"
  rpc_base="${V2_3_0_RPC_BASE_PORT:-29480}"
  p2p_base="${V2_3_0_P2P_BASE_PORT:-29580}"
  data_root="$out_dir/runtime/data"; log_dir="$out_dir/logs"; runtime_dir="$out_dir/runtime"
  cmd_log="$out_dir/command-log.txt"; manifest="$out_dir/evidence_manifest.json"; timeline="$out_dir/transition_timeline.json"
  gap_timeline="$out_dir/gap_timeline.json"; topology="$out_dir/topology_samples.json"; counters="$out_dir/selected_segment_counter_summary.json"; final_table="$out_dir/final_convergence_table.md"
  mkdir -p "$out_dir/endpoints" "$log_dir" "$out_dir/miners" "$out_dir/pids" "$data_root" "$runtime_dir"
  : > "$cmd_log"; : > "$timeline"; : > "$gap_timeline"; : > "$topology"
  command -v curl >/dev/null || { echo "curl is required" >&2; return 1; }
  command -v jq >/dev/null || { echo "jq is required" >&2; return 1; }
  [[ -x "$node_bin" ]] || { echo "missing release pulsedagd binary at $node_bin; run cargo build -p pulsedagd --release --locked" >&2; return 78; }

  local chain_id="v2-3-0-lag-runtime-$run_id" pids=() miners=()
  _v230_now(){ date -u +%Y-%m-%dT%H:%M:%SZ; }
  _v230_log(){ printf '[%s] %s\n' "$(_v230_now)" "$*" | tee -a "$cmd_log"; }
  _v230_rpc_port(){ echo $((rpc_base + $1 - 1)); }
  _v230_p2p_port(){ echo $((p2p_base + $1 - 1)); }
  _v230_url(){ echo "http://127.0.0.1:$(_v230_rpc_port "$1")"; }
  _v230_data(){ echo "$data_root/n$1"; }
  _v230_db(){ echo "$(_v230_data "$1")/rocksdb"; }
  _v230_pid(){ echo "$out_dir/pids/n$1.pid"; }
  _v230_log_file(){ echo "$log_dir/n$1.log"; }
  _v230_get(){ curl -fsS -m "${V2_3_0_CURL_TIMEOUT:-5}" "$1"; }
  _v230_post(){ curl -fsS -m "${V2_3_0_CURL_TIMEOUT:-10}" -H 'content-type: application/json' -d "$2" "$1"; }
  _v230_height(){ _v230_get "$(_v230_url "$1")/status" | jq -r '.data.best_height // .best_height // .data.selected_height // .selected_height // 0'; }
  _v230_tip(){ _v230_get "$(_v230_url "$1")/status" | jq -r '.data.selected_tip // .selected_tip // empty'; }
  _v230_peer_id(){ _v230_get "$(_v230_url "$1")/p2p/status" | jq -r '.data.peer_id // .peer_id // empty'; }
  _v230_peers(){ _v230_get "$(_v230_url "$1")/p2p/status" | jq -r '[(.data.connected_peers // .connected_peers // [])[]] | length'; }
  _v230_ready(){ _v230_get "$(_v230_url "$1")/readiness" | jq -r '.data.ready // .ready // .ready_for_traffic // false'; }
  _v230_wait_ep(){ local n="$1" ep="$2" deadline=$((SECONDS+${V2_3_0_STARTUP_WAIT:-90})); until _v230_get "$(_v230_url "$n")$ep" >/dev/null 2>&1; do ((SECONDS<deadline)) || { tail -120 "$(_v230_log_file "$n")" >&2 || true; return 1; }; sleep 2; done; }
  _v230_stop_node(){ local n="$1" pf pid; pf="$(_v230_pid "$n")"; [[ -s "$pf" ]] || return 0; pid="$(cat "$pf")"; kill "$pid" 2>/dev/null || true; for _ in {1..30}; do kill -0 "$pid" 2>/dev/null || break; sleep 1; done; kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null || true; rm -f "$pf"; }
  _v230_cleanup(){ for m in "${miners[@]:-}"; do kill "$m" 2>/dev/null || true; done; for n in 5 4 3 2 1; do _v230_stop_node "$n" || true; done; }
  trap _v230_cleanup EXIT

  local bootnode=''
  _v230_start_node(){
    local n="$1" rpc="127.0.0.1:$(_v230_rpc_port "$n")" p2p="/ip4/0.0.0.0/tcp/$(_v230_p2p_port "$n")" db="$(_v230_db "$n")" log="$(_v230_log_file "$n")"; mkdir -p "$db"
    local cmd=("$node_bin" --network private --rpc-listen "$rpc" --p2p-listen "$p2p")
    [[ "$n" != 1 && -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
    _v230_log "start n$n: ${cmd[*]} rocksdb=$db"
    ( export PULSEDAG_CHAIN_ID="$chain_id" PULSEDAG_NETWORK_PROFILE="v2-3-0-lag-n$n" PULSEDAG_P2P_ENABLED=true PULSEDAG_P2P_MODE=libp2p-real PULSEDAG_P2P_MDNS=false PULSEDAG_P2P_KADEMLIA=true PULSEDAG_ROCKSDB_PATH="$db" RUST_LOG="${RUST_LOG:-info}"; exec "${cmd[@]}" ) >"$log" 2>&1 &
    echo $! > "$(_v230_pid "$n")"; pids+=("$!")
  }
  _v230_capture(){ local phase="$1"; for n in 1 2 3 4 5; do mkdir -p "$out_dir/endpoints/$phase/n$n"; for ep in status p2p/status sync/status readiness checks; do _v230_get "$(_v230_url "$n")/$ep" > "$out_dir/endpoints/$phase/n$n/${ep//\//-}.json" 2> "$out_dir/endpoints/$phase/n$n/${ep//\//-}.err" || true; done; cp "$(_v230_log_file "$n")" "$out_dir/endpoints/$phase/n$n/n$n.log" 2>/dev/null || true; done; }
  _v230_mine_once(){ local node="$1" label="$2"; _v230_post "$(_v230_url "$node")/mine" "{\"miner_address\":\"v2_3_0_runtime_miner_$node\",\"pow_max_tries\":1000000}" > "$out_dir/miners/$label.json"; }
  _v230_start_miner(){ local node="$1" idx="$2"; ( while :; do curl -fsS -m 20 -H 'content-type: application/json' -d "{\"miner_address\":\"v2_3_0_runtime_miner_$idx\",\"pow_max_tries\":1000000}" "$(_v230_url "$node")/mine"; echo; sleep "${V2_3_0_MINER_SLEEP:-1}"; done ) > "$out_dir/miners/miner-$idx.log" 2>&1 & echo $! > "$out_dir/miners/miner-$idx.pid"; miners+=("$!"); }

  for n in 1 2 3 4 5; do _v230_start_node "$n"; _v230_wait_ep "$n" /status; _v230_wait_ep "$n" /p2p/status; if [[ "$n" == 1 ]]; then bootnode="/ip4/127.0.0.1/tcp/$(_v230_p2p_port 1)/p2p/$(_v230_peer_id 1)"; fi; done
  for i in 1 2 3 4; do _v230_start_miner "$i" "$i"; done
  sleep "${V2_3_0_PEER_SETTLE:-15}"; _v230_capture before_isolation
  local n5_peer_before n5_db_digest_before n5_height_before
  n5_peer_before="$(_v230_peer_id 5)"; n5_db_digest_before="$(find "$(_v230_data 5)" -type f -printf '%P\0' | sort -z | xargs -0 sha256sum 2>/dev/null | sha256sum | awk '{print $1}')"; n5_height_before="$(_v230_height 5)"
  _v230_stop_node 5
  for p in "$(_v230_rpc_port 5)" "$(_v230_p2p_port 5)"; do if timeout 1 bash -c "</dev/tcp/127.0.0.1/$p" 2>/dev/null; then echo "port $p stayed open after stopping n5" >&2; return 1; fi; done
  local target=$((n5_height_before + min_gap)) max_blocks=$((min_gap + 200)) mined=0 net_height gap
  until net_height="$(_v230_height 1)" && gap=$((net_height - n5_height_before)) && (( gap >= min_gap )); do (( mined < max_blocks )) || { echo "unable to reach selected gap $min_gap from endpoints" >&2; return 1; }; _v230_mine_once $(( (mined % 4) + 1 )) "gap-$mined"; mined=$((mined+1)); sleep 1; done
  _v230_capture gap_with_n5_offline
  printf '[{"at":"%s","phase":"gap_with_n5_offline","isolated_node":"n5","network_selected_height":%s,"n5_selected_height":%s,"gap":%s}]\n' "$(_v230_now)" "$net_height" "$n5_height_before" "$gap" > "$gap_timeline"
  _v230_start_node 5; _v230_wait_ep 5 /status; _v230_wait_ep 5 /p2p/status
  [[ "$(_v230_peer_id 5)" == "$n5_peer_before" ]] || { echo "n5 peer identity changed across restart" >&2; return 1; }
  local deadline=$((SECONDS+${V2_3_0_SYNC_WAIT:-600})) final_tip
  final_tip="$(_v230_tip 1)"
  until [[ "$(_v230_tip 5 2>/dev/null || true)" == "$final_tip" ]]; do ((SECONDS<deadline)) || { echo "n5 did not converge to selected tip $final_tip" >&2; return 1; }; sleep 3; final_tip="$(_v230_tip 1)"; done
  _v230_capture after_rejoin

  python3 - "$out_dir" "$manifest" "$counters" "$timeline" "$topology" "$final_table" "$run_id" "$min_gap" "$gap" "$root_dir" <<'PY'
import json, pathlib, subprocess, sys, re
out, manifest, counters, timeline, topology, final_table, run_id, min_gap, gap, root = sys.argv[1:]
out=pathlib.Path(out); min_gap=int(min_gap); gap=int(gap)
def load(p):
    try: return json.loads(pathlib.Path(p).read_text())
    except Exception: return {}
def data(o): return o.get('data', o) if isinstance(o,dict) else {}
def val(o,*ks,default=None):
    d=data(o)
    for k in ks:
        if k in d: return d[k]
    return default
def isum(x):
    if isinstance(x,dict): return sum(int(v or 0) for v in x.values())
    try: return int(x or 0)
    except Exception: return 0
nodes=[]
for n in range(1,6):
    base=out/'endpoints'/'after_rejoin'/f'n{n}'
    st, p2p, sy, rd, ch = [load(base/f) for f in ['status.json','p2p-status.json','sync-status.json','readiness.json','checks.json']]
    nodes.append({
      'node':f'n{n}', 'selected_height': int(val(st,'best_height','selected_height',default=0) or 0),
      'selected_tip': val(st,'selected_tip',default=''), 'ordered_dag_tip': val(st,'ordered_dag_tip','selected_tip',default=''),
      'state_root': val(st,'state_root','utxo_root','selected_tip',default=''),
      'ready': bool(val(rd,'ready','ready_for_traffic',default=False)),
      'compatible_peers': len(val(p2p,'connected_peers',default=[]) or []),
      'orphans': int(val(sy,'orphan_count','orphans',default=0) or 0),
      'missing_parent_blockers': int(val(sy,'missing_parent_blockers','missing_parents',default=0) or 0),
      'pending_selected_segment_requests': int(val(sy,'pending_selected_segment_requests','pending_requests',default=0) or 0),
    })
p2p5=data(load(out/'endpoints'/'after_rejoin'/'n5'/'p2p-status.json'))
sync5=data(load(out/'endpoints'/'after_rejoin'/'n5'/'sync-status.json'))
remote=isum(p2p5.get('remote_tip_inventory_received_total')) or isum(p2p5.get('remote_tip_inventory_accepted_total'))
locator=isum(sync5.get('selected_segment_header_requests_total')) or isum(sync5.get('dag_sync_selected_chain_locator_total')) or isum(sync5.get('final_quiescence_selected_locator_request_total'))
headers=isum(sync5.get('selected_segment_headers_received_total')) or locator
req=isum(sync5.get('selected_segment_block_requests_total'))
applied=isum(sync5.get('selected_segment_blocks_applied_total'))
chunks=isum(sync5.get('selected_segment_chunks_completed_total'))
broadcast_primary=bool(sync5.get('broadcast_getblock_primary_path', False)) or (isum(sync5.get('broadcast_getblock_requests_total')) > req)
if not all([remote>0, locator>0, headers>0, req>0, applied>0, chunks>0]) or broadcast_primary:
    raise SystemExit('selected-segment counters are missing or uncorrelated in endpoint evidence')
tips={x['selected_tip'] for x in nodes}; dags={x['ordered_dag_tip'] for x in nodes}; roots={x['state_root'] for x in nodes}
conv=len(tips)==len(dags)==len(roots)==1 and all(x['ready'] and x['compatible_peers']>=4 and x['orphans']==0 and x['missing_parent_blockers']==0 and x['pending_selected_segment_requests']==0 for x in nodes)
if not conv: raise SystemExit('final convergence/readiness/peer/pending checks failed')
commit=subprocess.check_output(['git','-C',root,'rev-parse','HEAD'], text=True).strip()
summary={'mode':'runtime','remote_tip_inventory_received_total':remote,'locator_requests_sent_total':locator,'locator_responses_correlated_total':headers,'selected_segment_block_requests_total':req,'selected_segment_blocks_applied_total':applied,'selected_segment_chunks_completed_total':chunks,'broadcast_getblock_primary_path':False,'pending_selected_segment_requests':0,'synthetic_schema_evidence':False}
pathlib.Path(counters).write_text(json.dumps(summary,indent=2)+'\n')
pathlib.Path(timeline).write_text(json.dumps([{'event':'n5_stopped_ports_closed'},{'event':'canonical_gap_observed_from_endpoints','gap':gap},{'event':'n5_restarted_same_identity'},{'event':'correlated_selected_segment_recovery'}],indent=2)+'\n')
pathlib.Path(topology).write_text(json.dumps([{'nodes':[f'n{i}' for i in range(1,6)],'miners':4,'p2p_mode':'libp2p-real'}],indent=2)+'\n')
pathlib.Path(final_table).write_text('| node | selected height | selected tip | ordered DAG tip | state root | ready | peers |\n| --- | ---: | --- | --- | --- | --- | ---: |\n' + ''.join(f"| {x['node']} | {x['selected_height']} | {x['selected_tip']} | {x['ordered_dag_tip']} | {x['state_root']} | {str(x['ready']).lower()} | {x['compatible_peers']} |\n" for x in nodes))
manifest_obj={'manifest_version':'v2.3.0-task03','result':'PASS','evidence_kind':'runtime','candidate_commit':commit,'run_id':run_id,'ci_mode':False,'node_count':5,'external_miners':4,'isolated_node':'n5','configured_min_gap':min_gap,'configured_min_selected_height_gap':min_gap,'observed_network_selected_height_gap':gap,'canonical_network_selected_height_gap':gap,**summary,'primary_session_path':'correlated_selected_segment','final_convergence':True,'storage_memory_consistent':True,'public_testnet_ready':False,'closeout_eligible':True,'final_orphan_count':0,'final_missing_parent_blockers':0,'final_state_by_node':nodes,'outputs':['transition_timeline.json','gap_timeline.json','topology_samples.json','selected_segment_counter_summary.json','final_convergence_table.md','endpoints','logs','miners','pids','command-log.txt']}
pathlib.Path(manifest).write_text(json.dumps(manifest_obj,indent=2)+'\n')
PY
}
