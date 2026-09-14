#!/usr/bin/env python3
# Fail-closed Task39 evidence-integrity adapter for the experimental cadence harness.

from __future__ import annotations

import argparse
from pathlib import Path


MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE = 2


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Task39 adapter expected exactly one {label}; found={count}")
    return text.replace(old, new, 1)


def patch_source(text: str) -> str:
    text = replace_once(
        text,
        'SCHEMA = "task39-cadence-evidence-v1"\n',
        'SCHEMA = "task39-cadence-evidence-v1"\n'
        f'MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE = {MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE}\n',
        "schema anchor",
    )

    old_sample = '''def sample(urls, sink):
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
'''
    new_sample = '''def sample(urls, sink, coverage=None, active_window=False):
    for name, url in urls.items():
        if active_window and coverage is not None:
            coverage[name]["attempts"] += 1
        started = time.monotonic_ns()
        try:
            data = status(url)
        except Exception:
            if active_window and coverage is not None:
                coverage[name]["failures"] += 1
            continue
        completed = time.monotonic_ns()
        if active_window and coverage is not None:
            coverage[name]["successes"] += 1
        sink.append(
            {
                "node": name,
                "t": completed,
                "active_window": bool(active_window),
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


def sampling_coverage_ok(coverage):
    expected = {name for name, _, _, _ in NODES}
    if set(coverage) != expected:
        return False
    return all(
        int(values.get("attempts", 0))
        == int(values.get("successes", 0)) + int(values.get("failures", 0))
        and int(values.get("successes", 0)) >= MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE
        for values in coverage.values()
    )


def convergence_matches(current):
    if len(current) != len(NODES):
        return False
    tips = {data.get("selected_tip") for data in current.values()}
    roots = {data.get("ordered_dag_state_root") for data in current.values()}
    heights = {data.get("selected_height") for data in current.values()}
    ordered = {data.get("ordered_dag_tip") for data in current.values()}
    return (
        len(tips) == 1
        and len(roots) == 1
        and len(heights) == 1
        and len(ordered) == 1
        and None not in tips
        and None not in roots
        and None not in ordered
    )
'''
    text = replace_once(text, old_sample, new_sample, "sample function")

    old_convergence = '''def convergence(urls, samples, seconds=20):
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
'''
    new_convergence = '''def convergence(urls, samples, seconds=20):
    deadline = time.monotonic() + seconds
    last = {}
    while time.monotonic() < deadline:
        sample(urls, samples, active_window=False)
        current = {}
        for name, url in urls.items():
            try:
                current[name] = status(url)
            except Exception:
                pass
        if len(current) == len(NODES):
            last = current
            if convergence_matches(current):
                return True, current
        time.sleep(0.25)
    return False, last
'''
    text = replace_once(text, old_convergence, new_convergence, "convergence function")

    old_propagation = '''def propagation(samples):
    seen = defaultdict(dict)
    for point in samples:
        if point["tip"] and point["node"] not in seen[point["tip"]]:
            seen[point["tip"]][point["node"]] = point["t"]
    return [
        (max(per_node.values()) - min(per_node.values())) / 1_000_000.0
        for per_node in seen.values()
        if len(per_node) == 3
    ]
'''
    new_propagation = '''def propagation(samples):
    seen = defaultdict(dict)
    for point in samples:
        if point.get("active_window") is not True:
            continue
        if point["tip"] and point["node"] not in seen[point["tip"]]:
            seen[point["tip"]][point["node"]] = point["t"]
    return [
        (max(per_node.values()) - min(per_node.values())) / 1_000_000.0
        for per_node in seen.values()
        if len(per_node) == len(NODES)
    ]
'''
    text = replace_once(text, old_propagation, new_propagation, "propagation function")

    text = replace_once(
        text,
        '''    peer_ids = {}
    bootstraps = {}
    try:
''',
        '''    peer_ids = {}
    bootstraps = {}
    sampling_coverage = {
        name: {"attempts": 0, "successes": 0, "failures": 0}
        for name, _, _, _ in NODES
    }
    try:
''',
        "sampling coverage initialization",
    )

    text = replace_once(
        text,
        '''        for name, _, _, _ in NODES:
            before_size[name] = size(run_dir / f"node-{name}" / "rocksdb")
        sample(urls, samples)

        measured_start = time.monotonic_ns()
''',
        '''        for name, _, _, _ in NODES:
            before_size[name] = size(run_dir / f"node-{name}" / "rocksdb")
        sample(urls, samples, active_window=False)

        measured_start = time.monotonic_ns()
''',
        "baseline sample",
    )

    text = replace_once(
        text,
        '''            sample(urls, samples)
            time.sleep(max(0.01, sample_ms / 1000))
''',
        '''            sample(
                urls,
                samples,
                coverage=sampling_coverage,
                active_window=True,
            )
            time.sleep(max(0.01, sample_ms / 1000))
''',
        "active-window sample",
    )

    text = replace_once(
        text,
        '''        converged, final = convergence(urls, samples)
        if not converged:
            raise RuntimeError("post-mining convergence failed")

        start_heights = {
''',
        '''        converged, final = convergence(urls, samples)
        if not converged:
            raise RuntimeError("post-mining convergence failed")
        if not sampling_coverage_ok(sampling_coverage):
            raise RuntimeError(
                "insufficient per-node active-window status sampling coverage: "
                f"{sampling_coverage}"
            )
        active_samples = [
            point for point in samples if point.get("active_window") is True
        ]
        propagation_values = propagation(samples)
        if not propagation_values:
            raise RuntimeError(
                "no active-window three-node selected-tip propagation observation"
            )

        start_heights = {
''',
        "post-convergence evidence checks",
    )

    text = replace_once(
        text,
        '        poll_gaps = observed_poll_gaps_ms(samples)\n',
        '        poll_gaps = observed_poll_gaps_ms(active_samples)\n',
        "active-window poll gaps",
    )

    text = replace_once(
        text,
        '''                "observed_same_node_poll_gap_ms": dist(poll_gaps),
                "selected_height_advance": blocks,
''',
        '''                "observed_same_node_poll_gap_ms": dist(poll_gaps),
                "active_window_sampling_by_node": {
                    name: {
                        "attempts": int(values["attempts"]),
                        "successes": int(values["successes"]),
                        "failures": int(values["failures"]),
                        "success_fraction": (
                            values["successes"] / values["attempts"]
                            if values["attempts"]
                            else None
                        ),
                    }
                    for name, values in sampling_coverage.items()
                },
                "selected_height_advance": blocks,
''',
        "sampling manifest evidence",
    )

    text = replace_once(
        text,
        '                "rpc_status_latency_ms": dist(point["rpc_ms"] for point in samples),\n',
        '                "rpc_status_latency_ms": dist(point["rpc_ms"] for point in active_samples),\n',
        "active-window rpc distribution",
    )

    text = replace_once(
        text,
        '                "sampled_selected_tip_convergence_latency_ms": dist(propagation(samples)),\n',
        '                "sampled_selected_tip_convergence_latency_ms": dist(propagation_values),\n',
        "active-window propagation distribution",
    )

    text = replace_once(
        text,
        '                "peak_parallel_tip_count_proxy": max([point["tips"] for point in samples] or [0]),\n',
        '                "peak_parallel_tip_count_proxy": max([point["tips"] for point in active_samples] or [0]),\n',
        "active-window tip peak",
    )
    text = replace_once(
        text,
        '                "peak_orphan_count": max([point["orphans"] for point in samples] or [0]),\n',
        '                "peak_orphan_count": max([point["orphans"] for point in active_samples] or [0]),\n',
        "active-window orphan peak",
    )
    text = replace_once(
        text,
        '''                    sum(1 for point in samples if point["orphans"] > 0) / len(samples)
                    if samples
''',
        '''                    sum(1 for point in active_samples if point["orphans"] > 0)
                    / len(active_samples)
                    if active_samples
''',
        "active-window orphan occupancy",
    )

    selftest_anchor = '''    gaps = observed_poll_gaps_ms(
        [
            {"node": "a", "t": base},
            {"node": "b", "t": base + 2_000_000},
            {"node": "a", "t": base + 75_000_000},
            {"node": "b", "t": base + 90_000_000},
        ]
    )
    assert sorted(gaps) == [75.0, 88.0]

    events = []
'''
    selftest_replacement = '''    gaps = observed_poll_gaps_ms(
        [
            {"node": "a", "t": base},
            {"node": "b", "t": base + 2_000_000},
            {"node": "a", "t": base + 75_000_000},
            {"node": "b", "t": base + 90_000_000},
        ]
    )
    assert sorted(gaps) == [75.0, 88.0]

    good_coverage = {
        name: {"attempts": 3, "successes": 2, "failures": 1}
        for name, _, _, _ in NODES
    }
    assert sampling_coverage_ok(good_coverage)
    weak_coverage = {name: dict(values) for name, values in good_coverage.items()}
    weak_coverage["c"] = {"attempts": 2, "successes": 1, "failures": 1}
    assert not sampling_coverage_ok(weak_coverage)

    converged = {
        "a": {
            "selected_tip": "tip-final",
            "selected_height": 42,
            "ordered_dag_state_root": "root-final",
            "ordered_dag_tip": "ordered-final",
        },
        "b": {
            "selected_tip": "tip-final",
            "selected_height": 42,
            "ordered_dag_state_root": "root-final",
            "ordered_dag_tip": "ordered-final",
        },
        "c": {
            "selected_tip": "tip-final",
            "selected_height": 42,
            "ordered_dag_state_root": "root-final",
            "ordered_dag_tip": "ordered-final",
        },
    }
    assert convergence_matches(converged)
    ordered_divergence = {name: dict(data) for name, data in converged.items()}
    ordered_divergence["c"]["ordered_dag_tip"] = "ordered-other"
    assert not convergence_matches(ordered_divergence)
    missing_ordered = {name: dict(data) for name, data in converged.items()}
    missing_ordered["b"]["ordered_dag_tip"] = None
    assert not convergence_matches(missing_ordered)

    propagation_fixture = [
        {"node": "a", "tip": "baseline", "t": base, "active_window": False},
        {"node": "b", "tip": "baseline", "t": base + 1, "active_window": False},
        {"node": "c", "tip": "baseline", "t": base + 2, "active_window": False},
        {"node": "a", "tip": "mined", "t": base + 10_000_000, "active_window": True},
        {"node": "b", "tip": "mined", "t": base + 12_000_000, "active_window": True},
        {"node": "c", "tip": "mined", "t": base + 15_000_000, "active_window": True},
    ]
    assert propagation(propagation_fixture) == [5.0]

    events = []
'''
    text = replace_once(
        text, selftest_anchor, selftest_replacement, "evidence integrity self-tests"
    )

    return text


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("target", type=Path)
    args = parser.parse_args()

    source_text = args.source.read_text(encoding="utf-8")
    patched = patch_source(source_text)
    args.target.parent.mkdir(parents=True, exist_ok=True)
    args.target.write_text(patched, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
