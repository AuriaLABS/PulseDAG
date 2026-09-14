#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


MIN_ACTIVE_WINDOW_SUCCESS_FRACTION = 0.80


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"Task39 evidence adapter expected exactly one {label}; found={count}")
    return text.replace(old, new, 1)


def patch_source(text: str) -> str:
    text = replace_once(
        text,
        'MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE = 2\n',
        'MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE = 2\n'
        f'MIN_ACTIVE_WINDOW_SUCCESS_FRACTION = {MIN_ACTIVE_WINDOW_SUCCESS_FRACTION!r}\n',
        "active-window sampling policy anchor",
    )

    text = replace_once(
        text,
        '''def sampling_coverage_ok(coverage):
    expected = {name for name, _, _, _ in NODES}
    if set(coverage) != expected:
        return False
    return all(
        int(values.get("attempts", 0))
        == int(values.get("successes", 0)) + int(values.get("failures", 0))
        and int(values.get("successes", 0)) >= MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE
        for values in coverage.values()
    )
''',
        '''def sampling_coverage_ok(coverage):
    expected = {name for name, _, _, _ in NODES}
    if set(coverage) != expected:
        return False
    for values in coverage.values():
        attempts = int(values.get("attempts", 0))
        successes = int(values.get("successes", 0))
        failures = int(values.get("failures", 0))
        if attempts != successes + failures:
            return False
        if attempts <= 0 or successes < MIN_ACTIVE_WINDOW_SAMPLES_PER_NODE:
            return False
        if successes / attempts < MIN_ACTIVE_WINDOW_SUCCESS_FRACTION:
            return False
    return True
''',
        "per-node sampling coverage gate",
    )

    text = replace_once(
        text,
        '''def convergence_matches(current):
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
''',
        '''def convergence_matches(current):
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
        and all(isinstance(value, str) and bool(value) for value in tips)
        and all(isinstance(value, str) and bool(value) for value in roots)
        and all(isinstance(value, int) and value >= 0 for value in heights)
        and all(isinstance(value, str) and bool(value) for value in ordered)
    )
''',
        "strict final convergence gate",
    )

    text = replace_once(
        text,
        '''                "active_window_sampling_by_node": {
''',
        '''                "active_window_sampling_min_success_fraction": MIN_ACTIVE_WINDOW_SUCCESS_FRACTION,
                "active_window_sampling_by_node": {
''',
        "sampling policy manifest field",
    )

    text = replace_once(
        text,
        '''    good_coverage = {
        name: {"attempts": 3, "successes": 2, "failures": 1}
        for name, _, _, _ in NODES
    }
    assert sampling_coverage_ok(good_coverage)
    weak_coverage = {name: dict(values) for name, values in good_coverage.items()}
    weak_coverage["c"] = {"attempts": 2, "successes": 1, "failures": 1}
    assert not sampling_coverage_ok(weak_coverage)
''',
        '''    good_coverage = {
        name: {"attempts": 5, "successes": 4, "failures": 1}
        for name, _, _, _ in NODES
    }
    assert sampling_coverage_ok(good_coverage)
    weak_coverage = {name: dict(values) for name, values in good_coverage.items()}
    weak_coverage["c"] = {"attempts": 5, "successes": 3, "failures": 2}
    assert not sampling_coverage_ok(weak_coverage)
    inconsistent_coverage = {name: dict(values) for name, values in good_coverage.items()}
    inconsistent_coverage["b"] = {"attempts": 5, "successes": 4, "failures": 0}
    assert not sampling_coverage_ok(inconsistent_coverage)
''',
        "sampling coverage self-tests",
    )

    text = replace_once(
        text,
        '''    missing_ordered = {name: dict(data) for name, data in converged.items()}
    missing_ordered["b"]["ordered_dag_tip"] = None
    assert not convergence_matches(missing_ordered)

    propagation_fixture = [
''',
        '''    missing_ordered = {name: dict(data) for name, data in converged.items()}
    missing_ordered["b"]["ordered_dag_tip"] = None
    assert not convergence_matches(missing_ordered)
    empty_ordered = {name: dict(data) for name, data in converged.items()}
    empty_ordered["a"]["ordered_dag_tip"] = ""
    empty_ordered["b"]["ordered_dag_tip"] = ""
    empty_ordered["c"]["ordered_dag_tip"] = ""
    assert not convergence_matches(empty_ordered)
    missing_height = {name: dict(data) for name, data in converged.items()}
    missing_height["a"]["selected_height"] = None
    missing_height["b"]["selected_height"] = None
    missing_height["c"]["selected_height"] = None
    assert not convergence_matches(missing_height)

    propagation_fixture = [
''',
        "strict convergence self-tests",
    )

    text = replace_once(
        text,
        '''        sample(urls, samples, active_window=False)

        measured_start = time.monotonic_ns()
''',
        '''        sample(urls, samples, active_window=False)
        pre_measurement_tips = {
            point["tip"]
            for point in samples
            if point.get("active_window") is not True and point.get("tip")
        }

        measured_start = time.monotonic_ns()
''',
        "pre-measurement tip capture",
    )

    text = replace_once(
        text,
        '''        propagation_values = propagation(samples)
        if not propagation_values:
            raise RuntimeError(
                "no active-window three-node selected-tip propagation observation"
            )
''',
        '''        propagation_values = propagation(
            [
                point
                for point in samples
                if point.get("tip") not in pre_measurement_tips
            ]
        )
        propagation_available = bool(propagation_values)
''',
        "strict propagation availability gate",
    )

    text = replace_once(
        text,
        '''                "sampled_selected_tip_convergence_latency_ms": dist(propagation_values),
                "observed_same_node_poll_gap_ms": dist(poll_gaps),
''',
        '''                "sampled_selected_tip_convergence_latency_ms": dist(propagation_values),
                "propagation_available": propagation_available,
                "propagation_unavailable_reason": (
                    None
                    if propagation_available
                    else "no non-baseline selected tip was observed on all three nodes during active measurement window"
                ),
                "propagation_exclusion_policy": "exclude-pre-measurement-selected-tips",
                "propagation_excluded_pre_measurement_tips": sorted(pre_measurement_tips),
                "observed_same_node_poll_gap_ms": dist(poll_gaps),
''',
        "propagation manifest fields",
    )

    text = replace_once(
        text,
        '''    propagation_fixture = [
        {"node": "a", "tip": "baseline", "t": base, "active_window": False},
        {"node": "b", "tip": "baseline", "t": base + 1, "active_window": False},
        {"node": "c", "tip": "baseline", "t": base + 2, "active_window": False},
        {"node": "a", "tip": "mined", "t": base + 10_000_000, "active_window": True},
        {"node": "b", "tip": "mined", "t": base + 12_000_000, "active_window": True},
        {"node": "c", "tip": "mined", "t": base + 15_000_000, "active_window": True},
    ]
    assert propagation(propagation_fixture) == [5.0]

    events = []
''',
        '''    propagation_fixture = [
        {"node": "a", "tip": "baseline", "t": base, "active_window": False},
        {"node": "b", "tip": "baseline", "t": base + 1, "active_window": False},
        {"node": "c", "tip": "baseline", "t": base + 2, "active_window": False},
        {"node": "a", "tip": "baseline", "t": base + 1_000_000, "active_window": True},
        {"node": "b", "tip": "baseline", "t": base + 2_000_000, "active_window": True},
        {"node": "c", "tip": "baseline", "t": base + 3_000_000, "active_window": True},
        {"node": "a", "tip": "mined", "t": base + 10_000_000, "active_window": True},
        {"node": "b", "tip": "mined", "t": base + 12_000_000, "active_window": True},
        {"node": "c", "tip": "mined", "t": base + 15_000_000, "active_window": True},
    ]
    fixture_pre_measurement_tips = {
        point["tip"]
        for point in propagation_fixture
        if point.get("active_window") is not True and point.get("tip")
    }
    eligible_propagation_fixture = [
        point
        for point in propagation_fixture
        if point.get("tip") not in fixture_pre_measurement_tips
    ]
    assert fixture_pre_measurement_tips == {"baseline"}
    assert propagation(eligible_propagation_fixture) == [5.0]
    assert propagation([point for point in propagation_fixture if point["tip"] == "baseline"]) == [2.0]

    events = []
''',
        "propagation baseline exclusion self-test",
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
