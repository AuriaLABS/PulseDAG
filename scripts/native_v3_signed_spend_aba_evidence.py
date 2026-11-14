#!/usr/bin/env python3
"""Record exact-SHA test-only signed-spend A/B/A evidence; NOT a replay SLO.

The existing opt-in Rust test signs and validates 128 spends, then proves equal
UTXO state roots from clone / in-place / clone application on the same corpus.
This parser refuses ambiguous or oversized untrusted test output. No timing SLO.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path

MAX_LOG_BYTES = 8 * 1024 * 1024
MAX_LOG_LINES = 100_000
MAX_LINE_CHARS = 16_384
MAX_TIMING_DIGITS = 16

OBSERVATION = re.compile(
    r"^v3_trial_accepted_apply_time_us "
    r"clone_a=([0-9]{1,16}) in_place=([0-9]{1,16}) "
    r"clone_b=([0-9]{1,16}) signed_spends=([0-9]{1,16}) "
    r"funded_utxos=([0-9]{1,16}) validation_once_us=([0-9]{1,16}) "
    r"identical_root=([0-9a-f]{64})$"
)
PASSED_TEST = (
    "test state_replay_v3::tests::accepted_spend_clone_vs_in_place_trial_probe ... ok"
)
PASSED_SUMMARY = re.compile(
    r"^test result: ok\. 1 passed; 0 failed; "
    r"([0-9]{1,8}) ignored; 0 measured; ([0-9]{1,8}) filtered out; "
    r"finished in [0-9]{1,6}(?:\.[0-9]{1,9})?s$"
)


def bounded_log_bytes(path):
    """Read at most one byte past the cap; never materialize an unlimited file."""
    with path.open("rb") as handle:
        raw = handle.read(MAX_LOG_BYTES + 1)
    if len(raw) > MAX_LOG_BYTES:
        raise ValueError("Rust test log exceeds the 8 MiB evidence budget")
    return raw


def parse_observation(raw_log, candidate_sha):
    """Validate the complete raw log bytes and one unambiguous passing test."""
    if not re.fullmatch(r"[0-9a-f]{40}", candidate_sha):
        raise ValueError("candidate SHA must be an exact lowercase 40-hex commit ID")
    if not isinstance(raw_log, bytes) or len(raw_log) > MAX_LOG_BYTES:
        raise ValueError("untrusted evidence must be <=8 MiB raw bytes")
    try:
        log_text = raw_log.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise ValueError("Rust log must be UTF-8") from error
    lines = log_text.splitlines()
    if len(lines) > MAX_LOG_LINES or any(len(line) > MAX_LINE_CHARS for line in lines):
        raise ValueError("untrusted evidence exceeds log line or line-length budget")

    observations = [match for line in lines if (match := OBSERVATION.fullmatch(line))]
    if len(observations) != 1:
        raise ValueError("require exactly one complete signed-spend observation")
    if lines.count(PASSED_TEST) != 1:
        raise ValueError("require exactly one matching Rust passing test line")
    summaries = [line for line in lines if line.startswith("test result:")]
    if len(summaries) != 1 or PASSED_SUMMARY.fullmatch(summaries[0]) is None:
        raise ValueError("require exactly one complete Rust harness pass summary")
    a, direct, b, spends, funded, validation = (
        int(value) for value in observations[0].groups()[:6]
    )
    root = observations[0].group(7)
    if min(a, direct, b, validation) <= 0 or spends != 128 or funded != 512:
        raise ValueError("invalid timing or signed-spend fixture dimensions")

    return {
        "schema": "pulsedag-test-only-accepted-apply-aba-v1",
        "scope": "isolated_legacy_carrier_utxo_apply_not_full_native_v3_replay",
        "candidate_sha": candidate_sha,
        "source_log_sha256": hashlib.sha256(raw_log).hexdigest(),
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
    root = "b" * 64
    summary = (
        "test result: ok. 1 passed; 0 failed; 0 ignored; "
        "0 measured; 0 filtered out; finished in 0.01s"
    )
    sample = (
        "running 1 test\n"
        "v3_trial_accepted_apply_time_us clone_a=100 in_place=20 clone_b=110 "
        "signed_spends=128 funded_utxos=512 validation_once_us=900 "
        "identical_root=" + root + "\n"
        + PASSED_TEST + "\n"
        + summary + "\n"
    )
    data = parse_observation(sample.encode("utf-8"), sha)
    if data["observed_clone_mean_to_direct_ratio"] != 5.25:
        raise AssertionError("positive A/B/A ratio invariant failed")
    if data["final_utxo_root_equal"] is not True or data["final_utxo_root"] != root:
        raise AssertionError("positive final-state equality invariant failed")
    if data["source_log_sha256"] != hashlib.sha256(sample.encode()).hexdigest():
        raise AssertionError("raw log digest invariant failed")
    crlf = sample.replace("\n", "\r\n").encode("utf-8")
    if parse_observation(crlf, sha)["source_log_sha256"] != hashlib.sha256(crlf).hexdigest():
        raise AssertionError("original CRLF bytes must be hashed")

    malformed = [
        sample.replace("signed_spends=128", "signed_spends=129").encode(),
        sample.replace("identical_root=" + root, "identical_root=broken").encode(),
        sample.replace(PASSED_TEST, "test failed").encode(),
        (sample + sample).encode(),
        sample.replace("in_place=20", "in_place=0").encode(),
        sample.replace(summary, "cargo warning: " + summary).encode(),
        (sample + PASSED_TEST + "\n").encode(),
        (sample + summary + "\n").encode(),
        (sample + "test result: FAILED. 0 passed; 1 failed\n").encode(),
        sample.replace("clone_a=100", "clone_a=" + "1" * (MAX_TIMING_DIGITS + 1)).encode(),
        (sample + "x" * (MAX_LINE_CHARS + 1)).encode(),
        b"x" * (MAX_LOG_BYTES + 1),
        sample.encode() + b"\xff",
    ]
    for invalid in malformed:
        try:
            parse_observation(invalid, sha)
        except ValueError:
            continue
        raise AssertionError("malformed or unbounded Rust evidence accepted")
    for invalid_sha in ("wrong", "A" * 40, "a" * 39):
        try:
            parse_observation(sample.encode(), invalid_sha)
        except ValueError:
            continue
        raise AssertionError("invalid source commit accepted")


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
    result = parse_observation(bounded_log_bytes(args.input), args.sha)
    args.output.write_text(
        json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    print("signed-spend A/B/A local evidence recorded (no speed SLO)")


if __name__ == "__main__":
    main()
