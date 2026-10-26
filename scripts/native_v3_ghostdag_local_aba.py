#!/usr/bin/env python3
"""Compare dormant native-v3 local CPU probes A(new) / B(old) / A(new).

The OLD checkout must use the SAME test-only probe fixture from A (copied only
into its test path at CI runtime), but its v3 policy algorithm implementation
MUST remain pinned to OLD_SHA. Never compare different deep-DAG workloads.

This is a SAME-RUNNER, SEQUENTIAL comparison of generated synthetic-time
full-PoW headers. It does not control CPU load, thermals, frequency or VM
contention. Never interpret timing ratios as network/DoS/security/K evidence.
"""
from __future__ import annotations

import argparse
import json
import re
from decimal import Decimal
from pathlib import Path
from statistics import median

OLD_SHA = "f26cf7e976e09334a9035535d355041e21b0866c"
COMMON = {
    "linear_16_500ms": (17, 32),
    "linear_64_500ms": (65, 32),
    "linear_120_500ms": (121, 32),
    "eight_parent_K0": (10, 0),
    "eight_parent_K6": (10, 6),
    "eight_parent_K7": (10, 7),
    "eight_parent_K32": (10, 32),
}
NEW_ONLY = {
    "deep_eight_parent_12x8_K0": (98, 0),
    "deep_eight_parent_12x8_K32": (98, 32),
}
PATTERN = re.compile(
    r"NATIVE_V3_PERF_PROBE scope=local_not_network "
    r"case=(?P<case>[A-Za-z0-9_]+) headers=(?P<headers>[0-9]+) "
    r"K=(?P<k>[0-9]+) samples_us=\[(?P<samples>[0-9,\s]+)\] "
    r"median_wall_us=(?P<median>[0-9]+) "
    r"process_vmhwm_kib=(?P<vmhwm>Some\([0-9]+\)|None)"
)
META_PATTERN = re.compile(
    r"NATIVE_V3_ABA_META_DIGEST case=(?P<case>[A-Za-z0-9_]+) "
    r"sha256=(?P<digest>[0-9a-f]{64})"
)


def require(value: bool, reason: str) -> None:
    if not value:
        raise ValueError(reason)


def parse(raw: str, cases: dict[str, tuple[int, int]]) -> dict[str, dict]:
    out: dict[str, dict] = {}
    digests: dict[str, str] = {}
    for line in raw.splitlines():
        if "NATIVE_V3_ABA_META_DIGEST case=" in line:
            match = META_PATTERN.fullmatch(line)
            require(match is not None, "malformed metadata fingerprint")
            case = match["case"]
            require(case in cases and case not in digests,
                    f"unrecognized or duplicate metadata fingerprint {case}")
            digests[case] = match["digest"]
            continue
        if "NATIVE_V3_PERF_PROBE scope=local_not_network case=" not in line:
            continue
        # Unlike grep of CI commands, match ONLY real probe emissions.
        result = PATTERN.fullmatch(line)
        require(result is not None, "malformed local probe output")
        case = result["case"]
        require(case in cases and case not in out,
                f"unexpected or duplicate profiler case {case}")
        headers, k = cases[case]
        require(int(result["headers"]) == headers and int(result["k"]) == k,
                f"wrong graph identity / K for {case}")
        samples = [int(x.strip()) for x in result["samples"].split(",")]
        require(len(samples) == 3 and all(x > 0 for x in samples),
                f"expected three positive samples for {case}")
        require(int(result["median"]) == median(samples),
                f"median mismatch for {case}")
        peak = result["vmhwm"]
        require(peak.startswith("Some(") and int(peak[5:-1]) > 0,
                f"missing Linux process VmHWM in {case}")
        out[case] = {
            "median_wall_us": int(result["median"]),
            "three_samples_us": samples,
            "process_high_water_kib_so_far": int(peak[5:-1]),
        }
    require(set(out) == set(cases),
            f"missing expected local profile cases: {sorted(set(cases) - set(out))}")
    require(set(digests) == set(cases),
            f"missing full-metadata fingerprints: {sorted(set(cases) - set(digests))}")
    for case in out:
        out[case]["full_metadata_sha256"] = digests[case]
    return out


def compare(old: str, new_first: str, new_repeat: str,
            candidate_sha: str) -> dict:
    require(re.fullmatch(r"[0-9a-f]{40}", candidate_sha) is not None,
            "exact full candidate SHA required")
    require(candidate_sha != OLD_SHA, "candidate must differ from baseline")
    # Same test harness is overlaid onto the old checkout at CI runtime,
    # while the old CORE ALGORITHM SOURCE remains pinned and unmodified.
    # This produces nine IDENTICAL native-header workloads for fair comparison.
    common_cases = COMMON | NEW_ONLY
    baseline = parse(old, common_cases)
    first, repeat = parse(new_first, common_cases), parse(new_repeat, common_cases)
    comparison = {}
    for name in sorted(common_cases):
        expected_metadata = baseline[name]["full_metadata_sha256"]
        require(expected_metadata == first[name]["full_metadata_sha256"]
                == repeat[name]["full_metadata_sha256"],
                f"old/new derived native GHOSTDAG metadata diverges for {name}")
        b, a, a2 = (sample[name]["median_wall_us"]
                    for sample in (baseline, first, repeat))
        comparison[name] = {
            "matching_full_metadata_sha256": expected_metadata,
            "baseline_median_us_B": b,
            "new_median_us_A1": a,
            "new_median_us_A2": a2,
            "ratio_A1_over_B": f"{Decimal(a) / Decimal(b):.4f}",
            "ratio_A2_over_B": f"{Decimal(a2) / Decimal(b):.4f}",
            "A1_A2_span_us": abs(a - a2),
        }
    return {
        "status": "SAME_RUNNER_SEQUENTIAL_A_B_A_ONLY_NOT_SPEEDUP_PROOF",
        "candidate_sha": candidate_sha,
        "baseline_sha": OLD_SHA,
        "measurement_order": ["candidate_A1", "prior_B", "candidate_A2"],
        "common_case_count": len(common_cases),
        "same_harness_overlaid_only_in_baseline_worktree": True,
        "baseline_algorithm_source_sha_is_pinned_not_modified": True,
        "deep_cases_compared_on_both_algorithms": sorted(NEW_ONLY),
        "all_nine_full_derived_metadata_digests_identical_A1_B_A2": True,
        "fingerprints_are_test_only_not_consensus_authority": True,
        "network_propagation_measured": False,
        "same_runner_does_not_control_cloud_host_cpu_noise": True,
        "not_a_consensus_500ms_latency_or_K_security_decision": True,
        "per_process_VmHWM_not_per_fixture_RSS": True,
        "numeric_K32_AND_eight_parent_cap_UNAPPROVED": True,
        "monetary_mining_and_smart_contracts_UNTOUCHED": True,
        "comparison": comparison,
    }


def self_test() -> None:
    def make(cases: dict[str, tuple[int, int]], delta: int = 0) -> str:
        return "\n".join(
            (
                f"NATIVE_V3_PERF_PROBE scope=local_not_network case={case} "
                f"headers={h} K={k} samples_us=[{100+delta}, {110+delta}, "
                f"{120+delta}] median_wall_us={110+delta} "
                "process_vmhwm_kib=Some(8192)\n"
                f"NATIVE_V3_ABA_META_DIGEST case={case} sha256={i+1:064x}"
            )
            for i, (case, (h, k)) in enumerate(cases.items())
        )
    all_cases = COMMON | NEW_ONLY
    old, first, second = make(all_cases), make(all_cases, 5), (
        make(all_cases, 10)
    )
    r = compare(old, first, second, "a" * 40)
    require(len(r["comparison"]) == 9, "comparison cardinality")
    require(r["all_nine_full_derived_metadata_digests_identical_A1_B_A2"],
            "metadata equivalence was not enforced")
    require(len(r["deep_cases_compared_on_both_algorithms"]) == 2,
            "deep cases are required for both algorithms")
    require(r["comparison"]["eight_parent_K32"]["ratio_A1_over_B"] ==
            f"{Decimal(115) / Decimal(110):.4f}", "ratio arithmetic")
    bad_inputs = [
        (old + "\n" + old.splitlines()[0], first, second, "a" * 40),
        ("\n".join(old.splitlines()[:-1]), first, second, "a" * 40),
        (old, first.replace("median_wall_us=115", "median_wall_us=116", 1),
         second, "a" * 40),
        (old, first, second.replace("K=32", "K=33", 1), "a" * 40),
        (old, first, second, OLD_SHA),
        (old, first, second, "badsha"),
        (old, first.replace("NATIVE_V3_ABA_META_DIGEST case=linear_16_500ms sha256=000", "NATIVE_V3_ABA_META_DIGEST case=linear_16_500ms sha256=fff", 1),
         second, "a" * 40),
        (old, first.replace("NATIVE_V3_ABA_META_DIGEST", "OTHER_KIND", 1),
         second, "a" * 40),
        (old + "\n" + old.splitlines()[1], first, second, "a" * 40),
    ]
    for old_b, a_b, a2_b, sha_b in bad_inputs:
        try:
            compare(old_b, a_b, a2_b, sha_b)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid local A/B/A evidence accepted")
    print("PASS: nine full-metadata-equivalent local PoW workload comparisons")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--baseline", type=Path)
    p.add_argument("--candidate-first", type=Path)
    p.add_argument("--candidate-repeat", type=Path)
    p.add_argument("--candidate-sha")
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    if args.self_test:
        self_test()
        return
    require(all([args.baseline, args.candidate_first, args.candidate_repeat,
                 args.candidate_sha, args.output]), "missing explicit A/B/A inputs")
    result = compare(
        args.baseline.read_text(encoding="utf-8"),
        args.candidate_first.read_text(encoding="utf-8"),
        args.candidate_repeat.read_text(encoding="utf-8"),
        args.candidate_sha,
    )
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("PASS: nine cases with exact full-metadata A/B/A parity; "
          "A/B/A NOT network/speedup/K approval")


if __name__ == "__main__":
    main()
