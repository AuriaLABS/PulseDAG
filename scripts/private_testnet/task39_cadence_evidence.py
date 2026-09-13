#!/usr/bin/env python3
"""Experimental Task39 cadence evidence for 1000/500/250 ms rehearsal runs."""
from __future__ import annotations

import argparse, json, os, re, shutil, statistics, subprocess, threading, time
import urllib.request
from collections import defaultdict
from pathlib import Path

CADENCES=(1000,500,250)
NODES=(('a','rehearsal-a','http://127.0.0.1:18080'),('b','rehearsal-b','http://127.0.0.1:18081'),('c','rehearsal-c','http://127.0.0.1:18082'))
SCHEMA='task39-cadence-evidence-v1'
FIELD_RE=re.compile(r'([A-Za-z0-9_]+)=([^\s]+)')

def dist(v):
    v=sorted(float(x) for x in v)
    if not v:return {'count':0,'min':None,'max':None,'mean':None,'p50':None,'p95':None,'p99':None}
    def p(q):return v[max(0,min(len(v)-1,(len(v)*q+99)//100-1))]
    return {'count':len(v),'min':v[0],'max':v[-1],'mean':statistics.fmean(v),'p50':p(50),'p95':p(95),'p99':p(99)}

def jain(v):
    v=[int(x) for x in v]; s=sum(v)
    return None if not v or s==0 else (s*s)/(len(v)*sum(x*x for x in v))

def status(url):
    with urllib.request.urlopen(url+'/status',timeout=1) as r:p=json.load(r)
    d=p.get('data')
    if not isinstance(d,dict):raise RuntimeError('invalid status response')
    return d

def size(path):
    total=0
    if path.exists():
        for p in path.rglob('*'):
            try:
                if p.is_file() and not p.is_symlink():total+=p.stat().st_size
            except FileNotFoundError:pass
    return total

class Proc:
    def __init__(self,cmd,env,log):
        self.records=[]; self.f=open(log,'w',encoding='utf-8',buffering=1)
        self.p=subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,bufsize=1)
        self.t=threading.Thread(target=self._read,daemon=True); self.t.start()
    def _read(self):
        assert self.p.stdout is not None
        for raw in self.p.stdout:
            mono=time.monotonic_ns(); line=raw.rstrip('\n'); self.records.append((mono,line)); self.f.write(f'{mono} {line}\n')
    def stop(self):
        if self.p.poll() is None:
            self.p.terminate()
            try:self.p.wait(timeout=5)
            except subprocess.TimeoutExpired:self.p.kill(); self.p.wait(timeout=5)
        self.t.join(timeout=2); self.f.close()

def miner_metrics(records):
    last=None; lats=[]; out={'templates':0,'submits':0,'accepted':0,'rejected':0,'stale':0,'unknown_finality':0,'reasons':defaultdict(int)}
    for mono,line in records:
        low=line.lower()
        if 'template received:' in low or 'template_received' in low:out['templates']+=1; last=mono
        if 'stale-template safety: skip submit:' in low:out['stale']+=1
        if 'submit_result:' not in low:continue
        out['submits']+=1; f=dict(FIELD_RE.findall(line)); a=f.get('accepted','false').lower()=='true'; r=f.get('rejected','false').lower()=='true'
        out['accepted']+=int(a); out['rejected']+=int(r or not a); out['stale']+=int(f.get('stale_template','false').lower()=='true')
        reason=f.get('reason_code','unknown'); out['reasons'][reason]+=1; out['unknown_finality']+=int('unknown' in reason.lower() and 'final' in reason.lower())
        if last is not None and mono>=last:lats.append((mono-last)/1e6)
    out['reasons']=dict(sorted(out['reasons'].items())); out['template_to_submit_ms']=dist(lats); return out

def sample(urls,sink):
    for name,url in urls.items():
        t0=time.monotonic_ns()
        try:d=status(url)
        except Exception:continue
        t=time.monotonic_ns(); sink.append({'node':name,'t':t,'rpc_ms':(t-t0)/1e6,'height':int(d.get('selected_height') or d.get('best_height') or 0),'tip':d.get('selected_tip'),'root':d.get('ordered_dag_state_root'),'ordered_tip':d.get('ordered_dag_tip'),'tips':int(d.get('tip_count') or 0),'orphans':int(d.get('orphan_count') or 0),'peers':int(d.get('peer_count') or 0),'sync':d.get('sync_state'),'persisted':int(d.get('persisted_block_count') or 0)})

def wait_ready(procs,urls,seconds=90):
    end=time.monotonic()+seconds; last={}
    while time.monotonic()<end:
        for n,p in procs.items():
            if p.p.poll() is not None:raise RuntimeError(f'node {n} exited rc={p.p.returncode}')
        cur={}
        for n,u in urls.items():
            try:cur[n]=status(u)
            except Exception:pass
        if len(cur)==3:
            last=cur
            if all(x.get('chain_id')=='pulsedag-rehearsal' and x.get('high_cadence_allowed') is True and x.get('rpc_response_stale') is False for x in cur.values()):return cur
        time.sleep(.25)
    raise RuntimeError(f'high-cadence readiness failed: {last}')

def wait_mesh(urls,seconds=45):
    end=time.monotonic()+seconds; last={}
    while time.monotonic()<end:
        cur={}
        for n,u in urls.items():
            try:cur[n]=status(u)
            except Exception:pass
        if len(cur)==3:
            last=cur
            if all(int(x.get('peer_count') or 0)>=1 for x in cur.values()):return
        time.sleep(.5)
    raise RuntimeError('peer mesh failed '+str({n:x.get('peer_count') for n,x in last.items()}))

def convergence(urls,samples,seconds=20):
    end=time.monotonic()+seconds; last={}
    while time.monotonic()<end:
        sample(urls,samples); cur={}
        for n,u in urls.items():
            try:cur[n]=status(u)
            except Exception:pass
        if len(cur)==3:
            last=cur; tips={x.get('selected_tip') for x in cur.values()}; roots={x.get('ordered_dag_state_root') for x in cur.values()}; heights={x.get('selected_height') for x in cur.values()}
            if len(tips)==len(roots)==len(heights)==1 and None not in tips and None not in roots:return True,cur
        time.sleep(.25)
    return False,last

def propagation(samples):
    seen=defaultdict(dict)
    for s in samples:
        if s['tip'] and s['node'] not in seen[s['tip']]:seen[s['tip']][s['node']]=s['t']
    return [(max(x.values())-min(x.values()))/1e6 for x in seen.values() if len(x)==3]

def run(root,node,miner,out,sha,tree,cadence,duration,sample_ms,max_tries,threads):
    d=out/f'cadence-{cadence}ms'; shutil.rmtree(d,ignore_errors=True); d.mkdir(parents=True)
    urls={n:u for n,_,u in NODES}; nodes={}; miners={}; samples=[]; before_size={}
    try:
        for n,profile,_ in NODES:
            db=d/f'node-{n}'/'rocksdb'; db.parent.mkdir(parents=True); before_size[n]=size(db); env=os.environ.copy(); env.update({'PULSEDAG_CONFIG_PROFILE':profile,'PULSEDAG_EXPERIMENTAL_GHOSTDAG_SELECTION':'true','PULSEDAG_EXPERIMENTAL_FAST_CADENCE':'true','PULSEDAG_TARGET_BLOCK_INTERVAL_MS':str(cadence),'PULSEDAG_CONSENSUS_MODE':'ghostdag_dev','PULSEDAG_PROTOCOL_CONSENSUS_MODE':'ghostdag_v1','PULSEDAG_ROCKSDB_PATH':str(db),'PULSEDAG_PUBLIC_TESTNET_READY':'false','PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED':'false'}); nodes[n]=Proc([str(node)],env,d/f'node-{n}.log')
        start=wait_ready(nodes,urls); wait_mesh(urls); sample(urls,samples)
        sleep_ms=cadence*3
        for i,(n,_,u) in enumerate(NODES):
            cmd=[str(miner),'--node',u,'--miner-address',f'task39-{cadence}-{n}','--backend','cpu','--threads',str(threads),'--max-tries',str(max_tries),'--loop','--sleep-ms',str(sleep_ms),'--no-heartbeat']; miners[n]=Proc(cmd,os.environ.copy(),d/f'miner-{n}.log')
            if i<2:time.sleep(cadence/3000)
        m0=time.monotonic_ns(); end=m0+duration*1_000_000_000
        while time.monotonic_ns()<end:
            for kind,ps in (('node',nodes),('miner',miners)):
                for n,p in ps.items():
                    if p.p.poll() is not None:raise RuntimeError(f'{kind} {n} exited rc={p.p.returncode}')
            sample(urls,samples); time.sleep(max(.01,sample_ms/1000))
        m1=time.monotonic_ns(); mm={n:miner_metrics(p.records) for n,p in miners.items()}
        for p in miners.values():p.stop()
        miners.clear(); ok,final=convergence(urls,samples)
        if not ok:raise RuntimeError('post-mining convergence failed')
        start_h=[int(x.get('selected_height') or x.get('best_height') or 0) for x in start.values()]; end_h=[int(x.get('selected_height') or x.get('best_height') or 0) for x in final.values()]; blocks=max(end_h)-min(start_h)
        if blocks<1:raise RuntimeError('no selected-height advance')
        tips={x.get('selected_tip') for x in final.values()}; roots={x.get('ordered_dag_state_root') for x in final.values()}; ordered={x.get('ordered_dag_tip') for x in final.values()}; accepted={n:int(x['accepted']) for n,x in mm.items()}
        storage={}
        for n,_,_ in NODES:
            db=d/f'node-{n}'/'rocksdb'; after=size(db); b=int(start[n].get('persisted_block_count') or 0); e=int(final[n].get('persisted_block_count') or 0); delta=max(0,e-b); storage[n]={'bytes_delta':max(0,after-before_size[n]),'persisted_block_delta':delta,'bytes_per_block_proxy':(max(0,after-before_size[n])/delta if delta else None)}
        missing=['canonical_block_acceptance_and_state_apply_latency_distribution','selection_digest','ordered_dag_digest']
        manifest={'schema':SCHEMA,'candidate_sha':sha,'candidate_tree_sha':tree,'configured_cadence_ms':cadence,'experimental_only':True,'production_cadence_selected':False,'consensus_timestamp_precision_changed':False,'consensus_target_semantics_changed':False,'measurement':{'elapsed_monotonic_ms':(m1-m0)/1e6,'sample_interval_ms':sample_ms,'selected_height_advance':blocks,'observed_blocks_per_second':blocks/((m1-m0)/1e9),'rpc_status_latency_ms':dist([s['rpc_ms'] for s in samples])},'propagation_and_dag':{'sampled_selected_tip_convergence_latency_ms':dist(propagation(samples)),'sampling_bound_ms':sample_ms,'peak_tip_count':max([s['tips'] for s in samples] or [0]),'peak_orphan_count':max([s['orphans'] for s in samples] or [0]),'final_peer_counts':{n:int(x.get('peer_count') or 0) for n,x in final.items()}},'acceptance_state_apply_latency':{'available':False,'reason':'not exposed by current runtime/status surfaces; no synthetic value recorded'},'storage_db_amplification_proxy':storage,'mining':{'per_miner':mm,'accepted_by_miner':accepted,'accepted_total':sum(accepted.values()),'rejected_total':sum(int(x['rejected']) for x in mm.values()),'stale_total':sum(int(x['stale']) for x in mm.values()),'unknown_finality_total':sum(int(x['unknown_finality']) for x in mm.values()),'jain_accepted_block_fairness':jain(accepted.values())},'sync_finality':{'final_sync_states':{n:x.get('sync_state') for n,x in final.items()},'final_converged':True},'canonical_end_state':{'selected_tip':next(iter(tips)) if len(tips)==1 else None,'ordered_dag_tip':next(iter(ordered)) if len(ordered)==1 else None,'ordered_dag_state_root':next(iter(roots)) if len(roots)==1 else None,'selection_digest':{'available':False,'reason':'not exposed by current rehearsal RPC'},'ordered_dag_digest':{'available':False,'reason':'not exposed by current rehearsal RPC'}},'coverage':{'completion_eligible':False,'missing_required_measurements':missing},'runtime_gate_result':'PASS','fail_reasons':[]}
    except Exception as e:
        manifest={'schema':SCHEMA,'candidate_sha':sha,'candidate_tree_sha':tree,'configured_cadence_ms':cadence,'experimental_only':True,'production_cadence_selected':False,'coverage':{'completion_eligible':False},'runtime_gate_result':'FAIL','fail_reasons':[str(e)]}
    finally:
        for p in list(miners.values()):
            try:p.stop()
            except Exception:pass
        for p in list(nodes.values()):
            try:p.stop()
            except Exception:pass
    (d/'evidence.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n'); return manifest

def selftest():
    assert CADENCES==(1000,500,250); assert dist([1,2,3,4])['p50']==2; assert abs(jain([2,2,2])-1)<1e-12
    b=time.monotonic_ns(); m=miner_metrics([(b,'template received: created_at=1'),(b+1_000_000,'submit_result: accepted=true rejected=false reason_code=accepted stale_template=false')]); assert m['accepted']==1 and m['template_to_submit_ms']['count']==1; print('task39 cadence evidence self-test: PASS')

def main():
    a=argparse.ArgumentParser(); a.add_argument('--candidate-sha'); a.add_argument('--root',default='.'); a.add_argument('--node-bin',default='target/release/pulsedagd'); a.add_argument('--miner-bin',default='target/release/pulsedag-miner'); a.add_argument('--out-dir',default='artifacts/task39-cadence'); a.add_argument('--duration-secs',type=int,default=20); a.add_argument('--sample-ms',type=int,default=50); a.add_argument('--max-tries',type=int,default=1_000_000); a.add_argument('--threads',type=int,default=1); a.add_argument('--self-test',action='store_true'); x=a.parse_args()
    if x.self_test:selftest(); return 0
    if not x.candidate_sha or not re.fullmatch(r'[0-9a-f]{40}',x.candidate_sha):a.error('--candidate-sha must be 40 lowercase hex')
    root=Path(x.root).resolve(); sha=subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip(); tree=subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD^{tree}'],text=True).strip()
    if sha!=x.candidate_sha:raise SystemExit(f'candidate mismatch expected={x.candidate_sha} actual={sha}')
    if subprocess.check_output(['git','-C',str(root),'status','--porcelain'],text=True).strip():raise SystemExit('candidate checkout must be clean')
    node=(root/x.node_bin).resolve(); miner=(root/x.miner_bin).resolve(); out=(root/x.out_dir).resolve(); out.mkdir(parents=True,exist_ok=True)
    if not os.access(node,os.X_OK) or not os.access(miner,os.X_OK):raise SystemExit('release node/miner binaries are required')
    runs=[run(root,node,miner,out,sha,tree,c,x.duration_secs,x.sample_ms,x.max_tries,x.threads) for c in CADENCES]
    agg={'schema':SCHEMA,'candidate_sha':sha,'candidate_tree_sha':tree,'required_cadence_points_ms':list(CADENCES),'runtime_gate_result':'PASS' if all(r['runtime_gate_result']=='PASS' for r in runs) else 'FAIL','completion_eligible':all(r.get('coverage',{}).get('completion_eligible') for r in runs),'production_cadence_selected':False,'runs':[{'cadence_ms':r['configured_cadence_ms'],'runtime_gate_result':r['runtime_gate_result'],'completion_eligible':r.get('coverage',{}).get('completion_eligible',False),'evidence_path':f"cadence-{r['configured_cadence_ms']}ms/evidence.json"} for r in runs]}; (out/'aggregate.json').write_text(json.dumps(agg,indent=2,sort_keys=True)+'\n'); print(json.dumps(agg,indent=2,sort_keys=True)); return 0 if agg['runtime_gate_result']=='PASS' else 1

if __name__=='__main__':raise SystemExit(main())
