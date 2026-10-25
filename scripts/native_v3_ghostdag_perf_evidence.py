#!/usr/bin/env python3
"""Parse exact-head native-v3 GHOSTDAG LOCAL CPU probe into auditable JSON.

NOT a network measurement, approval of K=32/8 parents, a 500ms consensus
latency guarantee, a memory-per-case benchmark, or mining/contract activation.
Must never turn CI wall time into a protocol security probability.
"""
from __future__ import annotations

import argparse
import json
import re
from decimal import Decimal
from pathlib import Path
from statistics import median

BLOCK_SLOT_US = 500_000
SAMPLES = 3
# Case -> (native header count including genesis, UNAPPROVED trial K).
CASES = {
    "linear_16_500ms": (17, 32),
    "linear_64_500ms": (65, 32),
    "linear_120_500ms": (121, 32),
    "eight_parent_K0": (10, 0),
    "eight_parent_K6": (10, 6),
    "eight_parent_K7": (10, 7),
    "eight_parent_K32": (10, 32),
}
PATTERN = re.compile(
    r"(?m)^NATIVE_V3_PERF_PROBE scope=local_not_network "
    r"case=(?P<case>[A-Za-z0-9_]+) "
    r"headers=(?P<headers>[0-9]+) K=(?P<k>[0-9]+) "
    r"samples_us=\[(?P<samples>[0-9,\s]+)\] "
    r"median_wall_us=(?P<median>[0-9]+) "
    r"process_vmhwm_kib=(?P<vmhwm>Some\([0-9]+\)|None)\s*$"
)


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def parse(raw: str, exact_head_sha: str) -> dict:
    require(
        re.fullmatch(r"[0-9a-f]{40}", exact_head_sha) is not None,
        "exact-head 40-hex commit SHA required",
    )
    entries: dict[str, dict] = {}
    for line in raw.splitlines():
        if "NATIVE_V3_PERF_PROBE scope=local_not_network " not in line:
            continue
        match = PATTERN.fullmatch(line)
        require(match is not None, "malformed local timing evidence")
        case = match["case"]
        require(case in CASES, f"unrecognized or extra profiler case: {case}")
        require(case not in entries, f"duplicate profiler case: {case}")
        values = [int(x.strip()) for x in match["samples"].split(",")]
        require(len(values) == SAMPLES and all(x > 0 for x in values),
                f"invalid 3-repeat local timings: {case}")
        observed_median = int(match["median"])
        require(observed_median == median(values), f"false median: {case}")
        header_count, trial_k = CASES[case]
        require(
            int(match["headers"]) == header_count and int(match["k"]) == trial_k,
            f"unexpected graph size or trial K: {case}",
        )
        vmhwm = match["vmhwm"]
        require(vmhwm.startswith("Some("),
                f"Ubuntu runner process VmHWM unavailable: {case}")
        peak_kib = int(vmhwm[5:-1])
        require(peak_kib > 0, f"nonpositive process VmHWM: {case}")
        entries[case] = {
            "headers_including_genesis": header_count,
            "trial_K_NOT_APPROVED": trial_k,
            "sample_wall_microseconds": values,
            "median_wall_microseconds": observed_median,
            "median_slot_fraction": format(
                Decimal(observed_median) / Decimal(BLOCK_SLOT_US), ".6f"
            ),
            # VmHWM is the process peak observed SO FAR, not case-specific RSS
            # and not validated production daemon memory consumption.
            "process_high_water_kib_cumulative": peak_kib,
        }
    require(set(entries) == set(CASES),
            f"missing expected profiler cases: {sorted(set(CASES) - set(entries))}")
    return {
        "status": "LOCAL_EXPLORATORY_NOT_PROTOCOL_FREEZE",
        "candidate_sha": exact_head_sha,
        "evidence_source": "native_v3_ghostdag_perf_probe ignored Rust test",
        "500ms_slot_reference_only": BLOCK_SLOT_US,
        "not_network_propagation_measurement": True,
        "not_real_miner_or_node_hot_path": True,
        "not_K_security_or_parent_cap_evidence": True,
        "not_per_case_RSS_peak": True,
        "not_go_no_mining_activation": True,
        "cases": {k: entries[k] for k in sorted(entries)},
    }


def self_test() -> None:
    sha = "a" * 40
    raw = "\n".join(
        f"NATIVE_V3_PERF_PROBE scope=local_not_network case={case} "
        f"headers={count} K={k} samples_us=[7, 9, 11] "
        f"median_wall_us=9 process_vmhwm_kib=Some(4096)"
        for case, (count, k) in CASES.items()
    )
    parsed = parse(raw, sha)
    require(len(parsed["cases"]) == 7, "missing case")
    require(parsed["cases"]["linear_120_500ms"]["median_slot_fraction"]
            == "0.000018", "wrong normalization")
    wrong = [
        raw.replace("median_wall_us=9", "median_wall_us=10", 1),
        raw.replace("K=32", "K=33", 1),
        raw.replace("samples_us=[7, 9, 11]", "samples_us=[7, 9]", 1),
        raw.replace("Some(4096)", "None", 1),
        raw.splitlines()[0] + "\n" + raw,
        "\n".join(raw.splitlines()[1:]),
    ]
    for bad in wrong:
        try:
            parse(bad, sha)
        except ValueError:
            pass
        else:
            raise AssertionError("corrupt or incomplete probe evidence accepted")
    for bad_sha in ("", "a" * 39, "../" + sha, "0" * 41):
        try:
            parse(raw, bad_sha)
        except ValueError:
            pass
        else:
            raise AssertionError("unbound candidate SHA accepted")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sha")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: exact-head local GHOSTDAG profiler evidence parser")
        return
    require(args.input is not None and args.output is not None and args.sha is not None,
            "--input, --output and --sha are mandatory outside --self-test")
    require(args.input.is_file(), "missing local Rust probe file")
    artifact = parse(args.input.read_text(encoding="utf-8"), args.sha)
    args.output.write_text(json.dumps(artifact, indent=2) + "\n", encoding="utf-8")
    print(f"LOCAL_ONLY: {len(artifact['cases'])} cases bound to {args.sha}; "
          "NOT an approved K, network latency estimate or mainnet GO")


if __name__ == "__main__":
    main()
