"""Experimental Task39 cadence evidence for 1000/500/250 ms rehearsal runs."""
from __future__ import annotations

import argparse
import hashlib
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
NODES = (
    ("a", "rehearsal-a", "http://127.0.0.1:18080", 18181),
    ("b", "rehearsal-b", "http://127.0.0.1:18081", 18182),
    ("c", "rehearsal-c", "http://127.0.0.1:18082", 18183),
)
SCHEMA = "task39-cadence-evidence-v1"
SUBMIT_FINALITY_UNKNOWN_CODE = "submit_finality_unknown"
PLACEHOLDER_BLOCK_HASHES = {"", "-", "null", "none", "unknown", "n/a"}
FIELD_RE = re.compile(r"([A-Za-z0-9_]+)=([^\s]+)")
EFFECTIVE_CADENCE_RE = re.compile(r"active at ([0-9]+)ms")
EXPERIMENTAL_LIMITS = {
    "PULSEDAG_MAX_PARALLEL_TIPS": "8",
    "PULSEDAG_MAX_MERGE_SET_SIZE": "64",
    "PULSEDAG_MAX_ORPHAN_COUNT": "2048",
    "PULSEDAG_MAX_PENDING_MISSING_PARENTS": "1024",
    "PULSEDAG_MAX_BLOCK_MASS": "2000000",
    "PULSEDAG_MAX_TEMPLATE_AGE_MS": "5000",
}


def dist(values):
    values = sorted(float(value) for value in values)
    if not values:
        return {
            "count": 0,
            "min": None,
            "max": None,
            "mean": None,
            "p50": None,
            "p95": None,
            "p99": None,
        }

    def percentile(q):
        index = max(0, min(len(values) - 1, (len(values) * q + 99) // 100 - 1))
        return values[index]

    return {
        "count": len(values),
        "min": values[0],
        "max": values[-1],
        "mean": statistics.fmean(values),
        "p50": percentile(50),
        "p95": percentile(95),
        "p99": percentile(99),
    }


def jain(values):
    values = [int(value) for value in values]
    total = sum(values)
    if not values or total == 0:
        return None
    return total * total / (len(values) * sum(value * value for value in values))


def api_data(url, path):
    with urllib.request.urlopen(url + path, timeout=1) as response:
        payload = json.load(response)
    data = payload.get("data")
    if not isinstance(data, dict):
        raise RuntimeError(f"invalid {path} response")
    return data


def status(url):
    return api_data(url, "/status")


def readiness(url):
    return api_data(url, "/readiness")


def p2p_status(url):
    return api_data(url, "/p2p/status")


def peer_id_from_status(data):
    peer_id = data.get("peer_id") or data.get("local_peer_id")
    if not isinstance(peer_id, str) or not peer_id.strip():
        return None
    return peer_id.strip()


def real_block_hash(value):
    if not isinstance(value, str):
        return None
    normalized = value.strip()
    if normalized.lower() in PLACEHOLDER_BLOCK_HASHES:
        return None
    return normalized


def observed_p2p_status_ok(data):
    return (
        data.get("mode") == "libp2p-real"
        and data.get("mdns") is False
        and data.get("p2p_status_stale") is False
        and data.get("p2p_status_degraded") is False
        and data.get("connected_peers_are_real_network") is True
        and isinstance(data.get("connected_peers"), list)
    )


def full_mesh_matches(p2p_by_node, peer_ids):
    if set(p2p_by_node) != set(peer_ids) or len(peer_ids) != len(NODES):
        return False
    if any(not isinstance(peer_id, str) or not peer_id for peer_id in peer_ids.values()):
        return False
    if len(set(peer_ids.values())) != len(peer_ids):
        return False
    all_peer_ids = set(peer_ids.values())
    for name, own_peer_id in peer_ids.items():
        data = p2p_by_node.get(name)
        if not isinstance(data, dict) or not observed_p2p_status_ok(data):
            return False
        connected = data.get("connected_peers")
        if not isinstance(connected, list) or any(not isinstance(peer, str) for peer in connected):
            return False
        if set(connected) != all_peer_ids - {own_peer_id}:
            return False
    return True


def effective_cadence_ms(readiness_data):
    category = readiness_data.get("categories", {}).get("high_cadence", {})
    reasons = category.get("reasons", [])
    for reason in reasons if isinstance(reasons, list) else []:
        match = EFFECTIVE_CADENCE_RE.search(str(reason))
        if match:
            return int(match.group(1))
    return None


def size(path):
    total = 0
    if path.exists():
        for candidate in path.rglob("*"):
            try:
                if candidate.is_file() and not candidate.is_symlink():
                    total += candidate.stat().st_size
            except FileNotFoundError:
                pass
    return total


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def node_environment(profile, cadence, db, bootstrap=""):
    env = os.environ.copy()
    env.update(EXPERIMENTAL_LIMITS)
    env.update(
        {
            "PULSEDAG_CONFIG_PROFILE": profile,
            "PULSEDAG_EXPERIMENTAL_GHOSTDAG_SELECTION": "true",
            "PULSEDAG_EXPERIMENTAL_FAST_CADENCE": "true",
            "PULSEDAG_TARGET_BLOCK_INTERVAL_MS": str(cadence),
            "PULSEDAG_CONSENSUS_MODE": "ghostdag_dev",
            "PULSEDAG_PROTOCOL_CONSENSUS_MODE": "legacy",
            "PULSEDAG_ROCKSDB_PATH": str(db),
            "PULSEDAG_P2P_ENABLED": "true",
            "PULSEDAG_P2P_MODE": "libp2p-real",
            "PULSEDAG_P2P_MDNS": "false",
            "PULSEDAG_P2P_BOOTSTRAP": bootstrap,
            "PULSEDAG_PUBLIC_TESTNET_READY": "false",
            "PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED": "false",
        }
    )
    return env


class Proc:
    def __init__(self, cmd, env, log):
        self.records = []
        self.file = open(log, "w", encoding="utf-8", buffering=1)
        self.process = subprocess.Popen(
            cmd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()
        self._closed = False

    def _read(self):
        assert self.process.stdout is not None
        for raw in self.process.stdout:
            mono_ns = time.monotonic_ns()
            line = raw.rstrip("\n")
            self.records.append((mono_ns, line))
            self.file.write(f"{mono_ns} {line}\n")

    def terminate(self):
        if self.process.poll() is None:
            self.process.terminate()

    def drain(self):
        if self._closed:
            return
        if self.process.poll() is None:
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.thread.join(timeout=2)
        if self.thread.is_alive():
            raise RuntimeError("process log reader failed to drain")
        self.file.close()
        self._closed = True

    def stop(self):
        self.terminate()
        self.drain()


def in_measurement_window(mono_ns, start_ns, end_ns):
    if start_ns is not None and mono_ns < start_ns:
        return False
    if end_ns is not None and mono_ns > end_ns:
        return False
    return True


def miner_metrics(records, start_ns=None, end_ns=None):
    last_template_ns = None
    template_to_submit_ms = []
    final_outcomes = {}
    pending_unknown = set()
    pending_unknown_unkeyed = set()
    reconciliation_reasons = defaultdict(int)
    metrics = {
        "templates": 0,
        "submits": 0,
        "accepted": 0,
        "rejected": 0,
        "stale": 0,
        "stale_skipped_before_submit": 0,
        "stale_submit_results": 0,
        "unknown_finality": 0,
        "unknown_finality_initial": 0,
        "reconciled_accepted": 0,
        "reconciled_rejected": 0,
        "reconciled_unmatched": 0,
        "reasons": defaultdict(int),
    }

    for mono_ns, line in records:
        lower = line.lower()
        within_window = in_measurement_window(mono_ns, start_ns, end_ns)

        if within_window and "miner_telemetry event=template_received" in lower:
            metrics["templates"] += 1
            last_template_ns = mono_ns

        if within_window and "stale-template safety: skip submit:" in lower:
            metrics["stale_skipped_before_submit"] += 1
            metrics["stale"] += 1

        if "submit_reconciled:" in lower:
            fields = dict(FIELD_RE.findall(line))
            block_hash = real_block_hash(fields.get("block_hash"))
            outcome = fields.get("outcome", "").lower()
            if block_hash is None or block_hash not in pending_unknown:
                if within_window:
                    metrics["reconciled_unmatched"] += 1
                continue
            if not within_window:
                continue
            if outcome == "accepted":
                final_outcomes[block_hash] = "accepted"
                pending_unknown.remove(block_hash)
                metrics["reconciled_accepted"] += 1
            elif outcome == "rejected":
                final_outcomes[block_hash] = "rejected"
                pending_unknown.remove(block_hash)
                metrics["reconciled_rejected"] += 1
                reconciliation_reasons[fields.get("reason_code", "unknown")] += 1
            continue

        if not within_window or "submit_result:" not in lower:
            continue

        metrics["submits"] += 1
        fields = dict(FIELD_RE.findall(line))
        accepted = fields.get("accepted", "false").lower() == "true"
        rejected_field = fields.get("rejected", "false").lower() == "true"
        reason = fields.get("reason_code", "unknown")
        block_hash = real_block_hash(fields.get("block_hash"))
        outcome_key = block_hash or f"submit-{metrics['submits']}"
        unknown_finality = reason == SUBMIT_FINALITY_UNKNOWN_CODE
        stale_submit = fields.get("stale_template", "false").lower() == "true"

        if accepted:
            final_outcomes[outcome_key] = "accepted"
        elif unknown_finality:
            if block_hash is not None:
                pending_unknown.add(block_hash)
            else:
                pending_unknown_unkeyed.add(outcome_key)
            metrics["unknown_finality_initial"] += 1
        elif rejected_field:
            final_outcomes[outcome_key] = "rejected"

        metrics["stale_submit_results"] += int(stale_submit)
        metrics["stale"] += int(stale_submit)
        metrics["reasons"][reason] += 1
        if last_template_ns is not None and mono_ns >= last_template_ns:
            template_to_submit_ms.append((mono_ns - last_template_ns) / 1_000_000.0)

    metrics["accepted"] = sum(1 for outcome in final_outcomes.values() if outcome == "accepted")
    metrics["rejected"] = sum(1 for outcome in final_outcomes.values() if outcome == "rejected")
    metrics["unknown_finality"] = len(pending_unknown) + len(pending_unknown_unkeyed)
    metrics["reasons"] = dict(sorted(metrics["reasons"].items()))
    metrics["reconciliation_reasons"] = dict(sorted(reconciliation_reasons.items()))
    metrics["template_to_submit_ms"] = dist(template_to_submit_ms)
    return metrics


def sample(urls, sink):
    for name, url in urls.items():
        started = time.monotonic_ns()
        try:
            data = status(url)
        except Exception:
            continue
        completed = time.monotonic_ns()
        sink.append(
            {
                "node": name,
                "t": completed,
                "rpc_ms": (completed - started) / 1_000_000.0,
                "height": int(data.get("selected_height") or data.get("best_height") or 0),
                "tip": data.get("selected_tip"),
                "root": data.get("ordered_dag_state_root"),
                "ordered_tip": data.get("ordered_dag_tip"),
                "tips": int(data.get("tip_count") or 0),
                "orphans": int(data.get("orphan_count") or 0),
                "peers": int(data.get("peer_count") or 0),
                "sync": data.get("sync_state"),
                "persisted": int(data.get("persisted_block_count") or 0),
            }
        )


def observed_poll_gaps_ms(samples):
    by_node = defaultdict(list)
    for point in samples:
        by_node[point["node"]].append(int(point["t"]))
    gaps = []
    for timestamps in by_node.values():
        timestamps.sort()
        gaps.extend(
            (later - earlier) / 1_000_000.0
            for earlier, later in zip(timestamps, timestamps[1:])
            if later >= earlier
        )
    return gaps


def wait_peer_id(proc, url, seconds=45):
    deadline = time.monotonic() + seconds
    last_error = None
    while time.monotonic() < deadline:
        if proc.process.poll() is not None:
            raise RuntimeError(f"node exited before p2p identity rc={proc.process.returncode}")
        try:
            data = p2p_status(url)
            peer_id = peer_id_from_status(data)
            if peer_id:
                return peer_id
            last_error = "p2p status omitted peer_id/local_peer_id"
        except Exception as exc:
            last_error = str(exc)
        time.sleep(0.25)
    raise RuntimeError(f"p2p peer id unavailable for {url}: {last_error}")


def wait_ready(procs, urls, expected_cadence_ms, seconds=90):
    deadline = time.monotonic() + seconds
    last_status = {}
    last_readiness = {}
    while time.monotonic() < deadline:
        for name, proc in procs.items():
            if proc.process.poll() is not None:
                raise RuntimeError(f"node {name} exited rc={proc.process.returncode}")

        current_status = {}
        current_readiness = {}
        for name, url in urls.items():
            try:
                current_status[name] = status(url)
                current_readiness[name] = readiness(url)
            except Exception:
                pass

        if len(current_status) == 3 and len(current_readiness) == 3:
            last_status = current_status
            last_readiness = current_readiness
            status_ok = all(
                data.get("chain_id") == "pulsedag-rehearsal"
                and data.get("consensus_mode") == "ghostdag_dev"
                and data.get("protocol_consensus_mode") == "legacy"
                and data.get("high_cadence_allowed") is True
                and data.get("rpc_response_stale") is False
                for data in current_status.values()
            )
            readiness_ok = all(
                data.get("metrics", {}).get("consensus_mode") == "ghostdag_dev"
                and data.get("metrics", {}).get("high_cadence_allowed") is True
                and data.get("categories", {}).get("high_cadence", {}).get("status") == "warn"
                and effective_cadence_ms(data) == expected_cadence_ms
                for data in current_readiness.values()
            )
            if status_ok and readiness_ok:
                return current_status, current_readiness
        time.sleep(0.25)

    diagnostic = {
        "status": last_status,
        "readiness": {
            name: {
                "consensus_mode": data.get("metrics", {}).get("consensus_mode"),
                "high_cadence_allowed": data.get("metrics", {}).get("high_cadence_allowed"),
                "high_cadence_category": data.get("categories", {}).get("high_cadence"),
                "effective_cadence_ms": effective_cadence_ms(data),
            }
            for name, data in last_readiness.items()
        },
    }
    raise RuntimeError(f"high-cadence readiness failed: {diagnostic}")


def wait_mesh(urls, peer_ids, seconds=45):
    deadline = time.monotonic() + seconds
    last = {}
    while time.monotonic() < deadline:
        current = {}
        for name, url in urls.items():
            try:
                current[name] = p2p_status(url)
            except Exception:
                pass
        if len(current) == len(peer_ids):
            last = current
            if full_mesh_matches(current, peer_ids):
                return current
        time.sleep(0.5)
    diagnostic = {
        name: {
            "mode": data.get("mode"),
            "mdns": data.get("mdns"),
            "stale": data.get("p2p_status_stale"),
            "degraded": data.get("p2p_status_degraded"),
            "connected_peers": data.get("connected_peers"),
        }
        for name, data in last.items()
    }
    raise RuntimeError(f"full peer mesh failed: {diagnostic}")


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
            tips = {data.get("selected_tip") for data in current.values()}
            roots = {data.get("ordered_dag_state_root") for data in current.values()}
            heights = {data.get("selected_height") for data in current.values()}
            if (
                len(tips) == 1
                and len(roots) == 1
                and len(heights) == 1
                and None not in tips
                and None not in roots
            ):
                return True, current
        time.sleep(0.25)
    return False, last


def propagation(samples):
    seen = defaultdict(dict)
    for point in samples:
        if point["tip"] and point["node"] not in seen[point["tip"]]:
            seen[point["tip"]][point["node"]] = point["t"]
    return [
        (max(per_node.values()) - min(per_node.values())) / 1_000_000.0
        for per_node in seen.values()
        if len(per_node) == 3
    ]


def start_rehearsal_nodes(node, run_dir, cadence):
    nodes = {}
    peer_ids = {}
    bootstraps = {}
    by_name = {name: (profile, url, port) for name, profile, url, port in NODES}

    try:
        profile_a, url_a, port_a = by_name["a"]
        db_a = run_dir / "node-a" / "rocksdb"
        db_a.parent.mkdir(parents=True)
        bootstraps["a"] = ""
        nodes["a"] = Proc(
            [str(node)],
            node_environment(profile_a, cadence, db_a, bootstraps["a"]),
            run_dir / "node-a.log",
        )
        peer_ids["a"] = wait_peer_id(nodes["a"], url_a)
        boot_a = f"/ip4/127.0.0.1/tcp/{port_a}/p2p/{peer_ids['a']}"

        profile_b, url_b, port_b = by_name["b"]
        db_b = run_dir / "node-b" / "rocksdb"
        db_b.parent.mkdir(parents=True)
        bootstraps["b"] = boot_a
        nodes["b"] = Proc(
            [str(node)],
            node_environment(profile_b, cadence, db_b, bootstraps["b"]),
            run_dir / "node-b.log",
        )
        peer_ids["b"] = wait_peer_id(nodes["b"], url_b)
        boot_b = f"/ip4/127.0.0.1/tcp/{port_b}/p2p/{peer_ids['b']}"

        profile_c, url_c, _port_c = by_name["c"]
        db_c = run_dir / "node-c" / "rocksdb"
        db_c.parent.mkdir(parents=True)
        bootstraps["c"] = f"{boot_a},{boot_b}"
        nodes["c"] = Proc(
            [str(node)],
            node_environment(profile_c, cadence, db_c, bootstraps["c"]),
            run_dir / "node-c.log",
        )
        peer_ids["c"] = wait_peer_id(nodes["c"], url_c)
        return nodes, peer_ids, bootstraps
    except Exception:
        for proc in list(nodes.values()):
            try:
                proc.stop()
            except Exception:
                pass
        raise


def run(root, node, miner, out, sha, tree, cadence, duration, sample_ms, max_tries, threads):
    del root
    run_dir = out / f"cadence-{cadence}ms"
    shutil.rmtree(run_dir, ignore_errors=True)
    run_dir.mkdir(parents=True)
    urls = {name: url for name, _, url, _ in NODES}
    nodes = {}
    miners = {}
    samples = []
    before_size = {}
    shutdown_overhead_ms = None
    peer_ids = {}
    bootstraps = {}
    try:
        nodes, peer_ids, bootstraps = start_rehearsal_nodes(node, run_dir, cadence)
        start, readiness_start = wait_ready(nodes, urls, cadence)
        mesh_state = wait_mesh(urls, peer_ids)
        for name, _, _, _ in NODES:
            before_size[name] = size(run_dir / f"node-{name}" / "rocksdb")
        sample(urls, samples)

        measured_start = time.monotonic_ns()
        deadline = measured_start + duration * 1_000_000_000
        sleep_ms = cadence * 3
        for index, (name, _, url, _) in enumerate(NODES):
            cmd = [
                str(miner),
                "--node",
                url,
                "--miner-address",
                f"task39-{cadence}-{name}",
                "--backend",
                "cpu",
                "--threads",
                str(threads),
                "--max-tries",
                str(max_tries),
                "--loop",
                "--sleep-ms",
                str(sleep_ms),
                "--no-heartbeat",
            ]
            miners[name] = Proc(cmd, os.environ.copy(), run_dir / f"miner-{name}.log")
            if index < 2:
                time.sleep(cadence / 3000)

        while time.monotonic_ns() < deadline:
            for kind, processes in (("node", nodes), ("miner", miners)):
                for name, proc in processes.items():
                    if proc.process.poll() is not None:
                        raise RuntimeError(f"{kind} {name} exited rc={proc.process.returncode}")
            sample(urls, samples)
            time.sleep(max(0.01, sample_ms / 1000))

        shutdown_started = time.monotonic_ns()
        for proc in miners.values():
            proc.terminate()
        for proc in miners.values():
            proc.drain()
        measured_end = time.monotonic_ns()
        shutdown_overhead_ms = (measured_end - shutdown_started) / 1_000_000.0

        miner_data = {
            name: miner_metrics(proc.records, measured_start, measured_end)
            for name, proc in miners.items()
        }
        miners.clear()

        converged, final = convergence(urls, samples)
        if not converged:
            raise RuntimeError("post-mining convergence failed")

        start_heights = {
            name: int(data.get("selected_height") or data.get("best_height") or 0)
            for name, data in start.items()
        }
        end_heights = {
            name: int(data.get("selected_height") or data.get("best_height") or 0)
            for name, data in final.items()
        }
        if len(set(start_heights.values())) != 1:
            raise RuntimeError(f"nodes did not start from one selected height: {start_heights}")
        if len(set(end_heights.values())) != 1:
            raise RuntimeError(f"nodes did not end on one selected height: {end_heights}")

        blocks = next(iter(end_heights.values())) - next(iter(start_heights.values()))
        if blocks < 1:
            raise RuntimeError("no selected-height advance")

        tips = {data.get("selected_tip") for data in final.values()}
        roots = {data.get("ordered_dag_state_root") for data in final.values()}
        ordered = {data.get("ordered_dag_tip") for data in final.values()}
        accepted = {name: int(data["accepted"]) for name, data in miner_data.items()}
        accepted_total = sum(accepted.values())
        if accepted_total < blocks:
            raise RuntimeError(
                "miner acceptance telemetry is smaller than selected-height advance "
                f"accepted_total={accepted_total} selected_height_advance={blocks}"
            )

        storage = {}
        for name, _, _, _ in NODES:
            db = run_dir / f"node-{name}" / "rocksdb"
            after = size(db)
            begin_count = int(start[name].get("persisted_block_count") or 0)
            end_count = int(final[name].get("persisted_block_count") or 0)
            delta = max(0, end_count - begin_count)
            bytes_delta = max(0, after - before_size[name])
            storage[name] = {
                "bytes_delta": bytes_delta,
                "persisted_block_delta": delta,
                "bytes_per_block_proxy": bytes_delta / delta if delta else None,
                "proxy_note": (
                    "filesystem-size delta from post-readiness baseline to final live "
                    "RocksDB size; not exact write amplification"
                ),
            }

        missing = [
            "canonical_block_acceptance_and_state_apply_latency_distribution",
            "selection_digest",
            "ordered_dag_digest",
        ]
        configured_limits = {key: int(value) for key, value in EXPERIMENTAL_LIMITS.items()}
        effective_cadences = {
            name: effective_cadence_ms(data) for name, data in readiness_start.items()
        }
        poll_gaps = observed_poll_gaps_ms(samples)
        expected_mesh = {
            name: sorted(set(peer_ids.values()) - {peer_ids[name]}) for name in peer_ids
        }
        connected_mesh = {
            name: sorted(data.get("connected_peers") or []) for name, data in mesh_state.items()
        }
        manifest = {
            "schema": SCHEMA,
            "candidate_sha": sha,
            "candidate_tree_sha": tree,
            "configured_cadence_ms": cadence,
            "configured_experimental_limits": configured_limits,
            "runtime_observation": {
                "consensus_mode": "ghostdag_dev",
                "protocol_consensus_mode": "legacy",
                "effective_cadence_ms_by_node": effective_cadences,
                "effective_cadence_source": "/readiness high_cadence category",
                "experimental_limit_source": (
                    "process environment plus pulsedagd config guard regressions; current RPC "
                    "does not export numeric max_* limits"
                ),
                "configured_p2p": {"mode": "libp2p-real", "mdns": False},
                "observed_p2p_mode_by_node": {
                    name: data.get("mode") for name, data in mesh_state.items()
                },
                "observed_p2p_mdns_by_node": {
                    name: data.get("mdns") for name, data in mesh_state.items()
                },
                "observed_p2p_fresh_by_node": {
                    name: (
                        data.get("p2p_status_stale") is False
                        and data.get("p2p_status_degraded") is False
                    )
                    for name, data in mesh_state.items()
                },
                "observed_connected_peers_are_real_network_by_node": {
                    name: data.get("connected_peers_are_real_network")
                    for name, data in mesh_state.items()
                },
                "peer_ids_by_node": peer_ids,
                "bootstrap_multiaddrs_by_node": bootstraps,
                "connected_peer_ids_by_node": connected_mesh,
                "full_mesh_expected_peer_ids_by_node": expected_mesh,
                "full_mesh_verified": full_mesh_matches(mesh_state, peer_ids),
                "mesh_peer_counts_after_startup": {
                    name: len(data.get("connected_peers") or [])
                    for name, data in mesh_state.items()
                },
            },
            "experimental_only": True,
            "production_cadence_selected": False,
            "consensus_timestamp_precision_changed": False,
            "consensus_target_semantics_changed": False,
            "measurement": {
                "configured_active_duration_ms": duration * 1000,
                "elapsed_monotonic_ms": (measured_end - measured_start) / 1_000_000.0,
                "shutdown_and_log_drain_overhead_ms": shutdown_overhead_ms,
                "window_boundary_note": (
                    "starts before first miner launch; closes only after terminate-all then "
                    "drain-all, so elapsed time conservatively includes shutdown/drain overhead"
                ),
                "configured_post_sample_sleep_ms": sample_ms,
                "observed_same_node_poll_gap_ms": dist(poll_gaps),
                "selected_height_advance": blocks,
                "per_node_start_height": start_heights,
                "per_node_end_height": end_heights,
                "observed_blocks_per_second": blocks
                / ((measured_end - measured_start) / 1_000_000_000.0),
                "rpc_status_latency_ms": dist(point["rpc_ms"] for point in samples),
            },
            "propagation_and_dag": {
                "sampled_selected_tip_convergence_latency_ms": dist(propagation(samples)),
                "observed_same_node_poll_gap_ms": dist(poll_gaps),
                "propagation_note": (
                    "polling-derived proxy; first-to-last observation includes sequential RPC "
                    "sampling skew. Configured sleep is not treated as a sampling bound; actual "
                    "same-node gaps are reported separately"
                ),
                "peak_parallel_tip_count_proxy": max([point["tips"] for point in samples] or [0]),
                "dag_width_proxy_note": (
                    "status tip_count proxy; not a full historical DAG-width integral"
                ),
                "peak_orphan_count": max([point["orphans"] for point in samples] or [0]),
                "orphan_nonzero_sample_fraction": (
                    sum(1 for point in samples if point["orphans"] > 0) / len(samples)
                    if samples
                    else None
                ),
                "orphan_rate_note": (
                    "sample occupancy proxy; not authoritative orphan-arrival rate"
                ),
                "final_peer_counts": {
                    name: int(data.get("peer_count") or 0) for name, data in final.items()
                },
            },
            "acceptance_state_apply_latency": {
                "available": False,
                "reason": (
                    "not exposed by current runtime/status surfaces; no synthetic value recorded"
                ),
            },
            "storage_db_amplification_proxy": storage,
            "mining": {
                "per_miner": miner_data,
                "accepted_by_miner": accepted,
                "accepted_total": accepted_total,
                "rejected_total": sum(int(data["rejected"]) for data in miner_data.values()),
                "stale_total": sum(int(data["stale"]) for data in miner_data.values()),
                "unknown_finality_initial_total": sum(
                    int(data["unknown_finality_initial"]) for data in miner_data.values()
                ),
                "unknown_finality_unresolved_total": sum(
                    int(data["unknown_finality"]) for data in miner_data.values()
                ),
                "reconciled_accepted_total": sum(
                    int(data["reconciled_accepted"]) for data in miner_data.values()
                ),
                "reconciled_rejected_total": sum(
                    int(data["reconciled_rejected"]) for data in miner_data.values()
                ),
                "rejection_taxonomy_note": (
                    "submit_finality_unknown remains separate while pending; reconciled outcomes "
                    "are folded into definitive accepted/rejected totals by real block_hash; "
                    "placeholder/hashless definitive outcomes use unique per-submit identities"
                ),
                "jain_accepted_block_fairness": jain(accepted.values()),
            },
            "sync_finality": {
                "final_sync_states": {name: data.get("sync_state") for name, data in final.items()},
                "final_converged": True,
            },
            "canonical_end_state": {
                "selected_tip": next(iter(tips)) if len(tips) == 1 else None,
                "ordered_dag_tip": next(iter(ordered)) if len(ordered) == 1 else None,
                "ordered_dag_state_root": next(iter(roots)) if len(roots) == 1 else None,
                "selection_digest": {
                    "available": False,
                    "reason": "not exposed by current rehearsal RPC",
                },
                "ordered_dag_digest": {
                    "available": False,
                    "reason": "not exposed by current rehearsal RPC",
                },
            },
            "coverage": {
                "completion_eligible": False,
                "missing_required_measurements": missing,
            },
            "runtime_gate_result": "PASS",
            "fail_reasons": [],
        }
    except Exception as exc:
        manifest = {
            "schema": SCHEMA,
            "candidate_sha": sha,
            "candidate_tree_sha": tree,
            "configured_cadence_ms": cadence,
            "experimental_only": True,
            "production_cadence_selected": False,
            "coverage": {"completion_eligible": False},
            "runtime_gate_result": "FAIL",
            "fail_reasons": [str(exc)],
        }
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

    (run_dir / "evidence.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    return manifest


def selftest():
    assert CADENCES == (1000, 500, 250)
    assert EXPERIMENTAL_LIMITS["PULSEDAG_MAX_PARALLEL_TIPS"] == "8"
    assert dist([1, 2, 3, 4])["p50"] == 2
    assert abs(jain([2, 2, 2]) - 1) < 1e-12
    assert effective_cadence_ms(
        {
            "categories": {
                "high_cadence": {
                    "status": "warn",
                    "reasons": ["controlled experimental high cadence active at 250ms; test"],
                }
            }
        }
    ) == 250

    env = node_environment(
        "rehearsal-b",
        500,
        Path("/tmp/task39-selftest"),
        "/ip4/127.0.0.1/tcp/18181/p2p/peer-a",
    )
    assert env["PULSEDAG_CONSENSUS_MODE"] == "ghostdag_dev"
    assert env["PULSEDAG_PROTOCOL_CONSENSUS_MODE"] == "legacy"
    assert env["PULSEDAG_TARGET_BLOCK_INTERVAL_MS"] == "500"
    assert env["PULSEDAG_P2P_MODE"] == "libp2p-real"
    assert env["PULSEDAG_P2P_MDNS"] == "false"
    assert env["PULSEDAG_P2P_BOOTSTRAP"].endswith("/p2p/peer-a")
    assert peer_id_from_status({"peer_id": "preferred", "local_peer_id": "fallback"}) == "preferred"
    assert peer_id_from_status({"local_peer_id": "fallback"}) == "fallback"
    assert peer_id_from_status({}) is None
    assert real_block_hash("abc") == "abc"
    assert real_block_hash("-") is None
    assert real_block_hash(None) is None

    peer_ids = {"a": "peer-a", "b": "peer-b", "c": "peer-c"}
    good_p2p = {
        "mode": "libp2p-real",
        "mdns": False,
        "p2p_status_stale": False,
        "p2p_status_degraded": False,
        "connected_peers_are_real_network": True,
    }
    full_mesh = {
        "a": {**good_p2p, "connected_peers": ["peer-b", "peer-c"]},
        "b": {**good_p2p, "connected_peers": ["peer-a", "peer-c"]},
        "c": {**good_p2p, "connected_peers": ["peer-a", "peer-b"]},
    }
    assert observed_p2p_status_ok(full_mesh["a"])
    assert full_mesh_matches(full_mesh, peer_ids)
    chain_only = {
        "a": {**good_p2p, "connected_peers": ["peer-b"]},
        "b": {**good_p2p, "connected_peers": ["peer-a", "peer-c"]},
        "c": {**good_p2p, "connected_peers": ["peer-b"]},
    }
    assert not full_mesh_matches(chain_only, peer_ids)
    assert not observed_p2p_status_ok({**full_mesh["a"], "mdns": True})
    assert not observed_p2p_status_ok({**full_mesh["a"], "mode": "memory-simulated"})
    assert not observed_p2p_status_ok({**full_mesh["a"], "p2p_status_stale": True})
    assert not observed_p2p_status_ok({**full_mesh["a"], "p2p_status_degraded": True})

    import tempfile

    with tempfile.NamedTemporaryFile("wb", delete=False) as fixture:
        fixture.write(b"task39")
        fixture_path = fixture.name
    try:
        assert sha256_file(fixture_path) == hashlib.sha256(b"task39").hexdigest()
    finally:
        os.unlink(fixture_path)

    base = time.monotonic_ns()
    accepted = miner_metrics(
        [
            (base, "miner_telemetry event=template_received block_template_hash=t1"),
            (base + 500_000, "template received: created_at=1"),
            (
                base + 1_000_000,
                "submit_result: accepted=true rejected=false reason_code=accepted "
                "block_hash=b1 stale_template=false",
            ),
        ]
    )
    assert accepted["templates"] == 1
    assert accepted["accepted"] == 1
    assert accepted["rejected"] == 0
    assert accepted["template_to_submit_ms"]["count"] == 1
    assert accepted["template_to_submit_ms"]["p50"] == 1.0

    reconciled_accept = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true "
                "reason_code=submit_finality_unknown block_hash=b2 stale_template=false",
            ),
            (base + 1, "submit_reconciled: outcome=accepted block_hash=b2 height=9"),
        ]
    )
    assert reconciled_accept["unknown_finality_initial"] == 1
    assert reconciled_accept["unknown_finality"] == 0
    assert reconciled_accept["accepted"] == 1
    assert reconciled_accept["reconciled_accepted"] == 1

    reconciled_reject = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true "
                "reason_code=submit_finality_unknown block_hash=b3 stale_template=false",
            ),
            (
                base + 1,
                "submit_reconciled: outcome=rejected block_hash=b3 "
                "reason_code=stale_template reason=stale",
            ),
        ]
    )
    assert reconciled_reject["unknown_finality"] == 0
    assert reconciled_reject["rejected"] == 1
    assert reconciled_reject["reconciled_rejected"] == 1

    unresolved = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true "
                "reason_code=submit_finality_unknown block_hash=b4 stale_template=false",
            )
        ]
    )
    assert unresolved["accepted"] == 0
    assert unresolved["rejected"] == 0
    assert unresolved["unknown_finality"] == 1

    rejected = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true reason_code=stale_template "
                "block_hash=b5 stale_template=true",
            )
        ]
    )
    assert rejected["rejected"] == 1
    assert rejected["unknown_finality"] == 0
    assert rejected["stale"] == 1

    hashless_rejected = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true reason_code=stale_template_error "
                "block_hash=- stale_template=true",
            ),
            (
                base + 1,
                "submit_result: accepted=false rejected=true reason_code=stale_template_error "
                "block_hash=- stale_template=true",
            ),
        ]
    )
    assert hashless_rejected["submits"] == 2
    assert hashless_rejected["rejected"] == 2
    assert hashless_rejected["stale"] == 2

    hashless_unknown = miner_metrics(
        [
            (
                base,
                "submit_result: accepted=false rejected=true "
                "reason_code=submit_finality_unknown block_hash=- stale_template=false",
            )
        ]
    )
    assert hashless_unknown["unknown_finality"] == 1
    assert hashless_unknown["rejected"] == 0

    windowed = miner_metrics(
        [
            (base, "miner_telemetry event=template_received block_template_hash=t0"),
            (
                base + 10,
                "submit_result: accepted=true rejected=false reason_code=accepted "
                "block_hash=before stale_template=false",
            ),
            (base + 20, "miner_telemetry event=template_received block_template_hash=t1"),
            (
                base + 30,
                "submit_result: accepted=true rejected=false reason_code=accepted "
                "block_hash=inside stale_template=false",
            ),
            (
                base + 40,
                "submit_result: accepted=true rejected=false reason_code=accepted "
                "block_hash=during-drain stale_template=false",
            ),
        ],
        start_ns=base + 15,
        end_ns=base + 45,
    )
    assert windowed["templates"] == 1
    assert windowed["submits"] == 2
    assert windowed["accepted"] == 2

    gaps = observed_poll_gaps_ms(
        [
            {"node": "a", "t": base},
            {"node": "b", "t": base + 2_000_000},
            {"node": "a", "t": base + 75_000_000},
            {"node": "b", "t": base + 90_000_000},
        ]
    )
    assert sorted(gaps) == [75.0, 88.0]

    events = []

    class FakeProc:
        def terminate(self):
            events.append("terminate")

        def drain(self):
            events.append("drain")

    fake = [FakeProc(), FakeProc(), FakeProc()]
    for proc in fake:
        proc.terminate()
    for proc in fake:
        proc.drain()
    assert events == ["terminate", "terminate", "terminate", "drain", "drain", "drain"]

    print("task39 cadence evidence self-test: PASS")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-sha")
    parser.add_argument("--root", default=".")
    parser.add_argument("--node-bin", default="target/release/pulsedagd")
    parser.add_argument("--miner-bin", default="target/release/pulsedag-miner")
    parser.add_argument("--out-dir", default="artifacts/task39-cadence")
    parser.add_argument("--duration-secs", type=int, default=20)
    parser.add_argument("--sample-ms", type=int, default=50)
    parser.add_argument("--max-tries", type=int, default=1000000)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        selftest()
        return 0
    if not args.candidate_sha or not re.fullmatch("[0-9a-f]{40}", args.candidate_sha):
        parser.error("--candidate-sha must be 40 lowercase hex")

    root = Path(args.root).resolve()
    sha = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    tree = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD^{tree}"], text=True
    ).strip()
    if sha != args.candidate_sha:
        raise SystemExit(f"candidate mismatch expected={args.candidate_sha} actual={sha}")
    if subprocess.check_output(
        ["git", "-C", str(root), "status", "--porcelain"], text=True
    ).strip():
        raise SystemExit("candidate checkout must be clean")

    node = (root / args.node_bin).resolve()
    miner = (root / args.miner_bin).resolve()
    out = (root / args.out_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not os.access(node, os.X_OK) or not os.access(miner, os.X_OK):
        raise SystemExit("release node/miner binaries are required")

    runs = [
        run(
            root,
            node,
            miner,
            out,
            sha,
            tree,
            cadence,
            args.duration_secs,
            args.sample_ms,
            args.max_tries,
            args.threads,
        )
        for cadence in CADENCES
    ]
    aggregate = {
        "schema": SCHEMA,
        "candidate_sha": sha,
        "candidate_tree_sha": tree,
        "required_cadence_points_ms": list(CADENCES),
        "runtime_gate_result": (
            "PASS" if all(run["runtime_gate_result"] == "PASS" for run in runs) else "FAIL"
        ),
        "completion_eligible": all(
            run.get("coverage", {}).get("completion_eligible") for run in runs
        ),
        "production_cadence_selected": False,
        "runs": [
            {
                "cadence_ms": run["configured_cadence_ms"],
                "runtime_gate_result": run["runtime_gate_result"],
                "completion_eligible": run.get("coverage", {}).get(
                    "completion_eligible", False
                ),
                "evidence_path": f"cadence-{run['configured_cadence_ms']}ms/evidence.json",
                "evidence_sha256": sha256_file(
                    out / f"cadence-{run['configured_cadence_ms']}ms" / "evidence.json"
                ),
            }
            for run in runs
        ],
    }
    (out / "aggregate.json").write_text(
        json.dumps(aggregate, indent=2, sort_keys=True) + "\n"
    )
    print(json.dumps(aggregate, indent=2, sort_keys=True))
    return 0 if aggregate["runtime_gate_result"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
