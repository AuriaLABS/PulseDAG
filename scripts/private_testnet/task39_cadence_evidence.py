"""Experimental Task39 cadence evidence for 1000/500/250 ms rehearsal runs."""
from __future__ import annotations
import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import threading
import time
import urllib.request
from collections import defaultdict
from pathlib import Path
CADENCES = (1000, 500, 250)
NODES = (('a', 'rehearsal-a', 'http://127.0.0.1:18080'), ('b', 'rehearsal-b', 'http://127.0.0.1:18081'), ('c', 'rehearsal-c', 'http://127.0.0.1:18082'))
SCHEMA = 'task39-cadence-evidence-v1'
SUBMIT_FINALITY_UNKNOWN_CODE = 'submit_finality_unknown'
FIELD_RE = re.compile('([A-Za-z0-9_]+)=([^\\s]+)')

def dist(values):
    values = sorted((float(value) for value in values))
    if not values:
        return {'count': 0, 'min': None, 'max': None, 'mean': None, 'p50': None, 'p95': None, 'p99': None}

    def percentile(q):
        index = max(0, min(len(values) - 1, (len(values) * q + 99) // 100 - 1))
        return values[index]
    return {'count': len(values), 'min': values[0], 'max': values[-1], 'mean': statistics.fmean(values), 'p50': percentile(50), 'p95': percentile(95), 'p99': percentile(99)}

def jain(values):
    values = [int(value) for value in values]
    total = sum(values)
    if not values or total == 0:
        return None
    return total * total / (len(values) * sum((value * value for value in values)))

def status(url):
    with urllib.request.urlopen(url + '/status', timeout=1) as response:
        payload = json.load(response)
    data = payload.get('data')
    if not isinstance(data, dict):
        raise RuntimeError('invalid status response')
    return data

def size(path):
    total = 0
    if path.exists():
        for candidate in path.rglob('*'):
            try:
                if candidate.is_file() and (not candidate.is_symlink()):
                    total += candidate.stat().st_size
            except FileNotFoundError:
                pass
    return total

class Proc:

    def __init__(self, cmd, env, log):
        self.records = []
        self.file = open(log, 'w', encoding='utf-8', buffering=1)
        self.process = subprocess.Popen(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        assert self.process.stdout is not None
        for raw in self.process.stdout:
            mono_ns = time.monotonic_ns()
            line = raw.rstrip('\n')
            self.records.append((mono_ns, line))
            self.file.write(f'{mono_ns} {line}\n')

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.thread.join(timeout=2)
        self.file.close()

def miner_metrics(records):
    last_template_ns = None
    template_to_submit_ms = []
    metrics = {'templates': 0, 'submits': 0, 'accepted': 0, 'rejected': 0, 'stale': 0, 'stale_skipped_before_submit': 0, 'stale_submit_results': 0, 'unknown_finality': 0, 'reasons': defaultdict(int)}
    for mono_ns, line in records:
        lower = line.lower()
        if 'template received:' in lower or 'template_received' in lower:
            metrics['templates'] += 1
            last_template_ns = mono_ns
        if 'stale-template safety: skip submit:' in lower:
            metrics['stale_skipped_before_submit'] += 1
            metrics['stale'] += 1
        if 'submit_result:' not in lower:
            continue
        metrics['submits'] += 1
        fields = dict(FIELD_RE.findall(line))
        accepted = fields.get('accepted', 'false').lower() == 'true'
        rejected_field = fields.get('rejected', 'false').lower() == 'true'
        reason = fields.get('reason_code', 'unknown')
        unknown_finality = reason == SUBMIT_FINALITY_UNKNOWN_CODE
        stale_submit = fields.get('stale_template', 'false').lower() == 'true'
        metrics['accepted'] += int(accepted)
        metrics['rejected'] += int(rejected_field and (not unknown_finality))
        metrics['unknown_finality'] += int(unknown_finality)
        metrics['stale_submit_results'] += int(stale_submit)
        metrics['stale'] += int(stale_submit)
        metrics['reasons'][reason] += 1
        if last_template_ns is not None and mono_ns >= last_template_ns:
            template_to_submit_ms.append((mono_ns - last_template_ns) / 1000000.0)
    metrics['reasons'] = dict(sorted(metrics['reasons'].items()))
    metrics['template_to_submit_ms'] = dist(template_to_submit_ms)
    return metrics

def sample(urls, sink):
    for name, url in urls.items():
        started = time.monotonic_ns()
        try:
            data = status(url)
        except Exception:
            continue
        completed = time.monotonic_ns()
        sink.append({'node': name, 't': completed, 'rpc_ms': (completed - started) / 1000000.0, 'height': int(data.get('selected_height') or data.get('best_height') or 0), 'tip': data.get('selected_tip'), 'root': data.get('ordered_dag_state_root'), 'ordered_tip': data.get('ordered_dag_tip'), 'tips': int(data.get('tip_count') or 0), 'orphans': int(data.get('orphan_count') or 0), 'peers': int(data.get('peer_count') or 0), 'sync': data.get('sync_state'), 'persisted': int(data.get('persisted_block_count') or 0), 'target_ms': int(data.get('target_block_interval_ms') or 0)})

def wait_ready(procs, urls, expected_cadence_ms, seconds=90):
    deadline = time.monotonic() + seconds
    last = {}
    while time.monotonic() < deadline:
        for name, proc in procs.items():
            if proc.process.poll() is not None:
                raise RuntimeError(f'node {name} exited rc={proc.process.returncode}')
        current = {}
        for name, url in urls.items():
            try:
                current[name] = status(url)
            except Exception:
                pass
        if len(current) == 3:
            last = current
            if all((data.get('chain_id') == 'pulsedag-rehearsal' and data.get('high_cadence_allowed') is True and (data.get('rpc_response_stale') is False) and (int(data.get('target_block_interval_ms') or 0) == expected_cadence_ms) for data in current.values())):
                return current
        time.sleep(0.25)
    raise RuntimeError(f'high-cadence readiness failed: {last}')

def wait_mesh(urls, seconds=45):
    deadline = time.monotonic() + seconds
    last = {}
    while time.monotonic() < deadline:
        current = {}
        for name, url in urls.items():
            try:
                current[name] = status(url)
            except Exception:
                pass
        if len(current) == 3:
            last = current
            if all((int(data.get('peer_count') or 0) >= 1 for data in current.values())):
                return
        time.sleep(0.5)
    raise RuntimeError('peer mesh failed ' + str({name: data.get('peer_count') for name, data in last.items()}))

def convergence(urls, samples, seconds=20):
    deadline = time.monotonic() + seconds
    last = {}
    while time.monotonic() < deadline:
        sample(urls, samples)
        current = {}
        for name, url in urls.items():
            try:
                current[name] = status(url)
            except Exception:
                pass
        if len(current) == 3:
            last = current
            tips = {data.get('selected_tip') for data in current.values()}
            roots = {data.get('ordered_dag_state_root') for data in current.values()}
            heights = {data.get('selected_height') for data in current.values()}
            if len(tips) == 1 and len(roots) == 1 and (len(heights) == 1) and (None not in tips) and (None not in roots):
                return (True, current)
        time.sleep(0.25)
    return (False, last)

def propagation(samples):
    seen = defaultdict(dict)
    for point in samples:
        if point['tip'] and point['node'] not in seen[point['tip']]:
            seen[point['tip']][point['node']] = point['t']
    return [(max(per_node.values()) - min(per_node.values())) / 1000000.0 for per_node in seen.values() if len(per_node) == 3]

def run(root, node, miner, out, sha, tree, cadence, duration, sample_ms, max_tries, threads):
    run_dir = out / f'cadence-{cadence}ms'
    shutil.rmtree(run_dir, ignore_errors=True)
    run_dir.mkdir(parents=True)
    urls = {name: url for name, _, url in NODES}
    nodes = {}
    miners = {}
    samples = []
    before_size = {}
    try:
        for name, profile, _ in NODES:
            db = run_dir / f'node-{name}' / 'rocksdb'
            db.parent.mkdir(parents=True)
            before_size[name] = size(db)
            env = os.environ.copy()
            env.update({'PULSEDAG_CONFIG_PROFILE': profile, 'PULSEDAG_EXPERIMENTAL_GHOSTDAG_SELECTION': 'true', 'PULSEDAG_EXPERIMENTAL_FAST_CADENCE': 'true', 'PULSEDAG_TARGET_BLOCK_INTERVAL_MS': str(cadence), 'PULSEDAG_CONSENSUS_MODE': 'ghostdag_dev', 'PULSEDAG_PROTOCOL_CONSENSUS_MODE': 'ghostdag_v1', 'PULSEDAG_ROCKSDB_PATH': str(db), 'PULSEDAG_PUBLIC_TESTNET_READY': 'false', 'PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED': 'false'})
            nodes[name] = Proc([str(node)], env, run_dir / f'node-{name}.log')
        start = wait_ready(nodes, urls, cadence)
        wait_mesh(urls)
        sample(urls, samples)
        sleep_ms = cadence * 3
        for index, (name, _, url) in enumerate(NODES):
            cmd = [str(miner), '--node', url, '--miner-address', f'task39-{cadence}-{name}', '--backend', 'cpu', '--threads', str(threads), '--max-tries', str(max_tries), '--loop', '--sleep-ms', str(sleep_ms), '--no-heartbeat']
            miners[name] = Proc(cmd, os.environ.copy(), run_dir / f'miner-{name}.log')
            if index < 2:
                time.sleep(cadence / 3000)
        measured_start = time.monotonic_ns()
        deadline = measured_start + duration * 1000000000
        while time.monotonic_ns() < deadline:
            for kind, processes in (('node', nodes), ('miner', miners)):
                for name, proc in processes.items():
                    if proc.process.poll() is not None:
                        raise RuntimeError(f'{kind} {name} exited rc={proc.process.returncode}')
            sample(urls, samples)
            time.sleep(max(0.01, sample_ms / 1000))
        measured_end = time.monotonic_ns()
        miner_data = {name: miner_metrics(proc.records) for name, proc in miners.items()}
        for proc in miners.values():
            proc.stop()
        miners.clear()
        converged, final = convergence(urls, samples)
        if not converged:
            raise RuntimeError('post-mining convergence failed')
        start_heights = {name: int(data.get('selected_height') or data.get('best_height') or 0) for name, data in start.items()}
        end_heights = {name: int(data.get('selected_height') or data.get('best_height') or 0) for name, data in final.items()}
        if len(set(start_heights.values())) != 1:
            raise RuntimeError(f'nodes did not start from one selected height: {start_heights}')
        if len(set(end_heights.values())) != 1:
            raise RuntimeError(f'nodes did not end on one selected height: {end_heights}')
        blocks = next(iter(end_heights.values())) - next(iter(start_heights.values()))
        if blocks < 1:
            raise RuntimeError('no selected-height advance')
        tips = {data.get('selected_tip') for data in final.values()}
        roots = {data.get('ordered_dag_state_root') for data in final.values()}
        ordered = {data.get('ordered_dag_tip') for data in final.values()}
        accepted = {name: int(data['accepted']) for name, data in miner_data.items()}
        storage = {}
        for name, _, _ in NODES:
            db = run_dir / f'node-{name}' / 'rocksdb'
            after = size(db)
            begin_count = int(start[name].get('persisted_block_count') or 0)
            end_count = int(final[name].get('persisted_block_count') or 0)
            delta = max(0, end_count - begin_count)
            bytes_delta = max(0, after - before_size[name])
            storage[name] = {'bytes_delta': bytes_delta, 'persisted_block_delta': delta, 'bytes_per_block_proxy': bytes_delta / delta if delta else None, 'proxy_note': 'filesystem-size delta while RocksDB is live; not exact write amplification'}
        missing = ['canonical_block_acceptance_and_state_apply_latency_distribution', 'selection_digest', 'ordered_dag_digest']
        manifest = {'schema': SCHEMA, 'candidate_sha': sha, 'candidate_tree_sha': tree, 'configured_cadence_ms': cadence, 'experimental_only': True, 'production_cadence_selected': False, 'consensus_timestamp_precision_changed': False, 'consensus_target_semantics_changed': False, 'measurement': {'elapsed_monotonic_ms': (measured_end - measured_start) / 1000000.0, 'sample_interval_ms': sample_ms, 'selected_height_advance': blocks, 'per_node_start_height': start_heights, 'per_node_end_height': end_heights, 'observed_blocks_per_second': blocks / ((measured_end - measured_start) / 1000000000.0), 'rpc_status_latency_ms': dist([point['rpc_ms'] for point in samples])}, 'propagation_and_dag': {'sampled_selected_tip_convergence_latency_ms': dist(propagation(samples)), 'sampling_bound_ms': sample_ms, 'propagation_note': 'polling-derived upper-bound proxy; includes sequential RPC sampling skew', 'peak_parallel_tip_count_proxy': max([point['tips'] for point in samples] or [0]), 'dag_width_proxy_note': 'status tip_count proxy; not a full historical DAG-width integral', 'peak_orphan_count': max([point['orphans'] for point in samples] or [0]), 'final_peer_counts': {name: int(data.get('peer_count') or 0) for name, data in final.items()}}, 'acceptance_state_apply_latency': {'available': False, 'reason': 'not exposed by current runtime/status surfaces; no synthetic value recorded'}, 'storage_db_amplification_proxy': storage, 'mining': {'per_miner': miner_data, 'accepted_by_miner': accepted, 'accepted_total': sum(accepted.values()), 'rejected_total': sum((int(data['rejected']) for data in miner_data.values())), 'stale_total': sum((int(data['stale']) for data in miner_data.values())), 'unknown_finality_total': sum((int(data['unknown_finality']) for data in miner_data.values())), 'rejection_taxonomy_note': 'submit_finality_unknown is excluded from definitive rejected counts to match Task29 finality semantics', 'jain_accepted_block_fairness': jain(accepted.values())}, 'sync_finality': {'final_sync_states': {name: data.get('sync_state') for name, data in final.items()}, 'final_converged': True}, 'canonical_end_state': {'selected_tip': next(iter(tips)) if len(tips) == 1 else None, 'ordered_dag_tip': next(iter(ordered)) if len(ordered) == 1 else None, 'ordered_dag_state_root': next(iter(roots)) if len(roots) == 1 else None, 'selection_digest': {'available': False, 'reason': 'not exposed by current rehearsal RPC'}, 'ordered_dag_digest': {'available': False, 'reason': 'not exposed by current rehearsal RPC'}}, 'coverage': {'completion_eligible': False, 'missing_required_measurements': missing}, 'runtime_gate_result': 'PASS', 'fail_reasons': []}
    except Exception as exc:
        manifest = {'schema': SCHEMA, 'candidate_sha': sha, 'candidate_tree_sha': tree, 'configured_cadence_ms': cadence, 'experimental_only': True, 'production_cadence_selected': False, 'coverage': {'completion_eligible': False}, 'runtime_gate_result': 'FAIL', 'fail_reasons': [str(exc)]}
    finally:
        for proc in list(miners.values()):
            try:
                proc.stop()
            except Exception:
                pass
        for proc in list(nodes.values()):
            try:
                proc.stop()
            except Exception:
                pass
    (run_dir / 'evidence.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
    return manifest

def selftest():
    assert CADENCES == (1000, 500, 250)
    assert dist([1, 2, 3, 4])['p50'] == 2
    assert abs(jain([2, 2, 2]) - 1) < 1e-12
    base = time.monotonic_ns()
    accepted = miner_metrics([(base, 'template received: created_at=1'), (base + 1000000, 'submit_result: accepted=true rejected=false reason_code=accepted stale_template=false')])
    assert accepted['accepted'] == 1
    assert accepted['rejected'] == 0
    assert accepted['template_to_submit_ms']['count'] == 1
    unknown = miner_metrics([(base, 'submit_result: accepted=false rejected=true reason_code=submit_finality_unknown stale_template=false')])
    assert unknown['accepted'] == 0
    assert unknown['rejected'] == 0
    assert unknown['unknown_finality'] == 1
    rejected = miner_metrics([(base, 'submit_result: accepted=false rejected=true reason_code=stale_template stale_template=true')])
    assert rejected['rejected'] == 1
    assert rejected['unknown_finality'] == 0
    assert rejected['stale'] == 1
    print('task39 cadence evidence self-test: PASS')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--candidate-sha')
    parser.add_argument('--root', default='.')
    parser.add_argument('--node-bin', default='target/release/pulsedagd')
    parser.add_argument('--miner-bin', default='target/release/pulsedag-miner')
    parser.add_argument('--out-dir', default='artifacts/task39-cadence')
    parser.add_argument('--duration-secs', type=int, default=20)
    parser.add_argument('--sample-ms', type=int, default=50)
    parser.add_argument('--max-tries', type=int, default=1000000)
    parser.add_argument('--threads', type=int, default=1)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        selftest()
        return 0
    if not args.candidate_sha or not re.fullmatch('[0-9a-f]{40}', args.candidate_sha):
        parser.error('--candidate-sha must be 40 lowercase hex')
    root = Path(args.root).resolve()
    sha = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
    tree = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD^{tree}'], text=True).strip()
    if sha != args.candidate_sha:
        raise SystemExit(f'candidate mismatch expected={args.candidate_sha} actual={sha}')
    if subprocess.check_output(['git', '-C', str(root), 'status', '--porcelain'], text=True).strip():
        raise SystemExit('candidate checkout must be clean')
    node = (root / args.node_bin).resolve()
    miner = (root / args.miner_bin).resolve()
    out = (root / args.out_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not os.access(node, os.X_OK) or not os.access(miner, os.X_OK):
        raise SystemExit('release node/miner binaries are required')
    runs = [run(root, node, miner, out, sha, tree, cadence, args.duration_secs, args.sample_ms, args.max_tries, args.threads) for cadence in CADENCES]
    aggregate = {'schema': SCHEMA, 'candidate_sha': sha, 'candidate_tree_sha': tree, 'required_cadence_points_ms': list(CADENCES), 'runtime_gate_result': 'PASS' if all((run['runtime_gate_result'] == 'PASS' for run in runs)) else 'FAIL', 'completion_eligible': all((run.get('coverage', {}).get('completion_eligible') for run in runs)), 'production_cadence_selected': False, 'runs': [{'cadence_ms': run['configured_cadence_ms'], 'runtime_gate_result': run['runtime_gate_result'], 'completion_eligible': run.get('coverage', {}).get('completion_eligible', False), 'evidence_path': f"cadence-{run['configured_cadence_ms']}ms/evidence.json"} for run in runs]}
    (out / 'aggregate.json').write_text(json.dumps(aggregate, indent=2, sort_keys=True) + '\n')
    print(json.dumps(aggregate, indent=2, sort_keys=True))
    return 0 if aggregate['runtime_gate_result'] == 'PASS' else 1
if __name__ == '__main__':
    raise SystemExit(main())
