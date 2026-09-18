#!/usr/bin/env python3
"""Fail-closed evidence guards layered over the Task39 cadence engine.

The underlying engine stays intentionally stable. This launcher installs three
strict evidence-integrity guards before delegating to it:
* every active-window /status attempt is accounted for per node;
* propagation excludes baseline/pre-measurement tips and only uses active samples;
* final convergence requires a non-empty identical ordered_dag_tip on all nodes.
"""
from __future__ import annotations

import json
import sys
import time
from collections import defaultdict
from pathlib import Path

MIN_ACTIVE_SUCCESSES = 2
MIN_ACTIVE_SUCCESS_RATIO = 0.50
MAX_RECORDED_FAILURES_PER_NODE = 20


class SamplingState:
    def __init__(self):
        self.reset()

    def reset(self):
        self.phase = "baseline"
        self.attempts = defaultdict(lambda: defaultdict(int))
        self.successes = defaultdict(lambda: defaultdict(int))
        self.failures = defaultdict(lambda: defaultdict(int))
        self.failure_details = defaultdict(lambda: defaultdict(list))
        self.baseline_tips = set()


STATE = SamplingState()


def active_propagation_ms(samples, baseline_tips):
    seen = defaultdict(dict)
    for point in samples:
        if point.get("phase") != "active":
            continue
        tip = point.get("tip")
        node = point.get("node")
        if not tip or tip in baseline_tips or node in seen[tip]:
            continue
        seen[tip][node] = int(point["t"])
    return [
        (max(per_node.values()) - min(per_node.values())) / 1_000_000.0
        for per_node in seen.values()
        if len(per_node) == 3
    ]


def convergence_contract(current):
    if len(current) != 3:
        return False
    tips = [data.get("selected_tip") for data in current.values()]
    roots = [data.get("ordered_dag_state_root") for data in current.values()]
    heights = [data.get("selected_height") for data in current.values()]
    ordered = [data.get("ordered_dag_tip") for data in current.values()]
    selection_digests = [data.get("selection_digest") for data in current.values()]
    ordered_dag_digests = [data.get("ordered_dag_digest") for data in current.values()]
    return (
        len(set(tips)) == 1
        and len(set(roots)) == 1
        and len(set(heights)) == 1
        and all(value is not None for value in tips)
        and all(value is not None for value in roots)
        and all(isinstance(value, str) and value for value in ordered)
        and len(set(ordered)) == 1
        and all(isinstance(value, str) and value for value in selection_digests)
        and len(set(selection_digests)) == 1
        and all(isinstance(value, str) and value for value in ordered_dag_digests)
        and len(set(ordered_dag_digests)) == 1
    )


def per_node_poll_gaps(samples):
    by_node = defaultdict(list)
    for point in samples:
        if point.get("phase") == "active":
            by_node[point["node"]].append(int(point["t"]))
    result = {}
    for name in ("a", "b", "c"):
        timestamps = sorted(by_node.get(name, []))
        result[name] = [
            (later - earlier) / 1_000_000.0
            for earlier, later in zip(timestamps, timestamps[1:])
            if later >= earlier
        ]
    return result


def sampling_coverage(base):
    gaps = per_node_poll_gaps(_LAST_SAMPLES)
    result = {}
    for name in ("a", "b", "c"):
        attempts = int(STATE.attempts["active"][name])
        successes = int(STATE.successes["active"][name])
        failures = int(STATE.failures["active"][name])
        ratio = successes / attempts if attempts else 0.0
        result[name] = {
            "active_attempts": attempts,
            "active_successes": successes,
            "active_failures": failures,
            "active_success_ratio": ratio,
            "minimum_successes": MIN_ACTIVE_SUCCESSES,
            "minimum_success_ratio": MIN_ACTIVE_SUCCESS_RATIO,
            "adequate": (
                successes >= MIN_ACTIVE_SUCCESSES
                and ratio >= MIN_ACTIVE_SUCCESS_RATIO
                and len(gaps[name]) >= 1
            ),
            "observed_same_node_poll_gap_ms": base.dist(gaps[name]),
            "recorded_failures": list(STATE.failure_details["active"][name]),
        }
    return result


_LAST_SAMPLES = []


def install(base):
    original_proc = base.Proc
    original_run = base.run
    original_wait_ready = base.wait_ready

    class StrictProc(original_proc):
        def __init__(self, cmd, env, log):
            executable = Path(str(cmd[0])).name if cmd else ""
            if "pulsedag-miner" in executable:
                STATE.phase = "active"
            super().__init__(cmd, env, log)

    def strict_wait_ready(procs, urls, expected_cadence_ms, seconds=90):
        status_by_node, readiness_by_node = original_wait_ready(
            procs, urls, expected_cadence_ms, seconds
        )
        for data in status_by_node.values():
            tip = data.get("selected_tip")
            if isinstance(tip, str) and tip:
                STATE.baseline_tips.add(tip)
        return status_by_node, readiness_by_node

    def strict_sample(urls, sink):
        global _LAST_SAMPLES
        _LAST_SAMPLES = sink
        phase = STATE.phase
        for name, url in urls.items():
            started = time.monotonic_ns()
            STATE.attempts[phase][name] += 1
            try:
                data = base.status(url)
            except Exception as exc:
                completed = time.monotonic_ns()
                STATE.failures[phase][name] += 1
                details = STATE.failure_details[phase][name]
                if len(details) < MAX_RECORDED_FAILURES_PER_NODE:
                    details.append(
                        {
                            "t": completed,
                            "rpc_ms": (completed - started) / 1_000_000.0,
                            "error": f"{type(exc).__name__}: {exc}",
                        }
                    )
                continue
            completed = time.monotonic_ns()
            STATE.successes[phase][name] += 1
            point = {
                "node": name,
                "phase": phase,
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
            sink.append(point)
            if phase == "baseline" and point["tip"]:
                STATE.baseline_tips.add(point["tip"])

    def strict_convergence(urls, samples, seconds=20):
        STATE.phase = "post"
        deadline = time.monotonic() + seconds
        last = {}
        while time.monotonic() < deadline:
            strict_sample(urls, samples)
            current = {}
            for name, url in urls.items():
                try:
                    current[name] = base.status(url)
                except Exception:
                    pass
            if len(current) == 3:
                last = current
                if convergence_contract(current):
                    return True, current
            time.sleep(0.25)
        return False, last

    def strict_propagation(samples):
        return active_propagation_ms(samples, STATE.baseline_tips)

    def strict_run(root, node, miner, out, sha, tree, cadence, duration, sample_ms, max_tries, threads):
        global _LAST_SAMPLES
        STATE.reset()
        _LAST_SAMPLES = []
        manifest = original_run(
            root,
            node,
            miner,
            out,
            sha,
            tree,
            cadence,
            duration,
            sample_ms,
            max_tries,
            threads,
        )

        coverage = sampling_coverage(base)
        reasons = []
        if not STATE.baseline_tips:
            reasons.append("no verified pre-measurement baseline tip was captured")
        for name, data in coverage.items():
            if not data["adequate"]:
                reasons.append(
                    "insufficient active-window /status coverage "
                    f"node={name} successes={data['active_successes']} "
                    f"attempts={data['active_attempts']} ratio={data['active_success_ratio']:.3f}"
                )

        propagation_count = int(
            manifest.get("propagation_and_dag", {})
            .get("sampled_selected_tip_convergence_latency_ms", {})
            .get("count")
            or 0
        )
        if manifest.get("runtime_gate_result") == "PASS" and propagation_count < 1:
            reasons.append("no genuine active-window three-node propagation observation")

        ordered_tip = manifest.get("canonical_end_state", {}).get("ordered_dag_tip")
        if manifest.get("runtime_gate_result") == "PASS" and not ordered_tip:
            reasons.append("canonical ordered_dag_tip missing after convergence")

        canonical = manifest.get("canonical_end_state", {})
        for digest_name in ("selection_digest", "ordered_dag_digest"):
            digest = canonical.get(digest_name, {})
            if (
                manifest.get("runtime_gate_result") == "PASS"
                and (
                    digest.get("available") is not True
                    or not isinstance(digest.get("value"), str)
                    or not digest.get("value")
                )
            ):
                reasons.append(f"{digest_name} unavailable after convergence")

        latency = manifest.get("acceptance_state_apply_latency", {})
        if manifest.get("runtime_gate_result") == "PASS":
            if latency.get("available") is not True or latency.get("unit") != "microseconds":
                reasons.append("canonical state-apply latency distribution unavailable")
            by_node = latency.get("by_node", {})
            for name in ("a", "b", "c"):
                summary = by_node.get(name, {})
                if int(summary.get("count") or 0) < 1:
                    reasons.append(
                        f"canonical state-apply latency has no accepted sample node={name}"
                    )

        measurement = manifest.setdefault("measurement", {})
        measurement["sampling_coverage_by_node"] = coverage
        measurement["sampling_coverage_contract"] = {
            "minimum_successes_per_node": MIN_ACTIVE_SUCCESSES,
            "minimum_success_ratio_per_node": MIN_ACTIVE_SUCCESS_RATIO,
            "requires_observed_poll_gap_per_node": True,
        }
        propagation = manifest.setdefault("propagation_and_dag", {})
        propagation["propagation_active_window_only"] = True
        propagation["excluded_baseline_tips"] = sorted(STATE.baseline_tips)
        propagation["requires_genuine_three_node_observation"] = True

        if reasons:
            manifest["runtime_gate_result"] = "FAIL"
            manifest.setdefault("fail_reasons", []).extend(reasons)
            manifest.setdefault("coverage", {})["completion_eligible"] = False

        run_dir = Path(out) / f"cadence-{cadence}ms"
        (run_dir / "evidence.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return manifest

    base.Proc = StrictProc
    base.wait_ready = strict_wait_ready
    base.sample = strict_sample
    base.convergence = strict_convergence
    base.propagation = strict_propagation
    base.run = strict_run


def selftest():
    baseline = "startup-tip"
    samples = [
        {"node": "a", "phase": "baseline", "tip": baseline, "t": 1},
        {"node": "b", "phase": "baseline", "tip": baseline, "t": 2},
        {"node": "c", "phase": "baseline", "tip": baseline, "t": 3},
        {"node": "a", "phase": "active", "tip": baseline, "t": 10},
        {"node": "b", "phase": "active", "tip": baseline, "t": 11},
        {"node": "c", "phase": "active", "tip": baseline, "t": 12},
    ]
    assert active_propagation_ms(samples, {baseline}) == []
    samples.extend(
        [
            {"node": "a", "phase": "active", "tip": "new-tip", "t": 20_000_000},
            {"node": "b", "phase": "active", "tip": "new-tip", "t": 23_000_000},
            {"node": "c", "phase": "active", "tip": "new-tip", "t": 27_000_000},
        ]
    )
    assert active_propagation_ms(samples, {baseline}) == [7.0]

    good = {
        "a": {"selected_tip": "s", "ordered_dag_state_root": "r", "selected_height": 9, "ordered_dag_tip": "o", "selection_digest": "sd", "ordered_dag_digest": "od"},
        "b": {"selected_tip": "s", "ordered_dag_state_root": "r", "selected_height": 9, "ordered_dag_tip": "o", "selection_digest": "sd", "ordered_dag_digest": "od"},
        "c": {"selected_tip": "s", "ordered_dag_state_root": "r", "selected_height": 9, "ordered_dag_tip": "o", "selection_digest": "sd", "ordered_dag_digest": "od"},
    }
    assert convergence_contract(good)
    assert not convergence_contract({**good, "c": {**good["c"], "ordered_dag_tip": "different"}})
    assert not convergence_contract({**good, "c": {**good["c"], "ordered_dag_tip": None}})
    assert not convergence_contract({**good, "c": {**good["c"], "selection_digest": "different"}})
    assert not convergence_contract({**good, "c": {**good["c"], "ordered_dag_digest": None}})
    print("task39 strict evidence guards self-test: PASS")


def main():
    if "--self-test" in sys.argv:
        selftest()
        return 0
    import task39_cadence_evidence as base

    install(base)
    return base.main()


if __name__ == "__main__":
    raise SystemExit(main())
