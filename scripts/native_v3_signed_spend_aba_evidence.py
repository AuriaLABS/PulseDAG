#!/usr/bin/env python3
"""Record an exact-SHA test-only signed-spend A/B/A observation; NOT a replay SLO.

No threshold is imposed on timing. The Rust test verifies signatures once,
then asserts equal UTXO roots for both state-application methods.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path

OBSERVATION = re.compile(
    r"^v3_trial_accepted_apply_time_us "
    r"clone_a=([0-9]+) in_place=([0-9]+) clone_b=([0-9]+) "
    r"signed_spends=([0-9]+) funded_utxos=([0-9]+) "
    r"validation_once_us=([0-9]+) identical_root=([0-9a-f]{64})$"
)
PASSED_TEST = (
    "test state_replay_v3::tests::accepted_spend_clone_vs_in_place_trial_probe ... ok"
)
PASSED_SUMMARY = "test result: ok. 1 passed; 0 failed;"


def parse_observation(log_text, candidate_sha):
    if not re.fullmatch(r"[0-9a-f]{40}", candidate_sha):
        raise ValueError("candidate SHA must be an exact 40-character lowercase commit ID")
    lines = log_text.splitlines()
    observations = [match for line in lines if (match := OBSERVATION.fullmatch(line))]
    if len(observations) != 1:
        raise ValueError("require exactly one complete benchmark observation")
    if PASSED_TEST not in lines or not any(PASSED_SUMMARY in line for line in lines):
        raise ValueError("matching Rust UTXO-root equality test must have passed")
    a, direct, b, spends, funded, validation = (
        int(v) for v in observations[0].groups()[:6]
    )
    root = observations[0].group(7)
    if min(a, direct, b, validation) == 0 or spends != 128 or funded != 512:
        raise ValueError("missing timing or unexpected signed-spend corpus dimensions")
    return {
        "schema": "pulsedag-test-only-accepted-apply-aba-v1",
        "scope": "isolated_legacy_carrier_utxo_apply_not_full_native_v3_replay",
        "candidate_sha": candidate_sha,
        "source_log_sha256": hashlib.sha256(log_text.encode("utf-8")).hexdigest(),
        "signed_spends": spends,
        "initial_funded_utxos": funded,
        "final_utxo_root_equal": True,
        "final_utxo_root": root,
        "timing_us": {
            "signature_validation_once": validation,
            "clone_a_apply": a,
            "in_place_apply": direct,
            "clone_b_apply": b,
        },
        "observed_clone_mean_to_direct_ratio": round((a + b) / (2 * direct), 3),
        "limitations": [
            "One noisy same-runner A/B/A observation; never a speed pass threshold",
            "Excludes signature validation from application times",
            "No peak RSS, native-v3 consensus replay, network or 500ms cadence claim",
        ],
    }


def self_test():
    sha = "a" * 40
    sample = (
        "running 1 test\n"
        "v3_trial_accepted_apply_time_us clone_a=100 in_place=20 clone_b=110 "
        "signed_spends=128 funded_utxos=512 validation_once_us=900 "
        "identical_root=" + "b" * 64 + "\n"
        + PASSED_TEST + "\n"
        + PASSED_SUMMARY + " 0 ignored; 0 measured; 0 filtered out\n"
    )
    data = parse_observation(sample, sha)
    assert data["observed_clone_mean_to_direct_ratio"] == 5.25
    assert data["final_utxo_root_equal"]
    for malformed in [
        sample.replace("signed_spends=128", "signed_spends=129"),
        sample.replace("identical_root=" + "b" * 64, "identical_root=nope"),
        sample.replace(PASSED_TEST, "test failed"),
        sample + sample,
        sample.replace("in_place=20", "in_place=0"),
    ]:
        try:
            parse_observation(malformed, sha)
        except ValueError:
            continue
        raise AssertionError("invalid A/B/A input accepted")
    try:
        parse_observation(sample, "wrong")
    except ValueError:
        pass
    else:
        raise AssertionError("invalid commit ID accepted")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sha")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("signed-spend A/B/A evidence parser self-test PASS")
        return
    if not (args.input and args.output and args.sha):
        parser.error("--input, --output and --sha required unless --self-test")
    result = parse_observation(args.input.read_text(encoding="utf-8"), args.sha)
    args.output.write_text(
        json.dumps(result, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    print("signed-spend A/B/A local evidence recorded (no speed SLO)")


if __name__ == "__main__":
    main()
