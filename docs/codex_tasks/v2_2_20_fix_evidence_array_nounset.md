# Fix false-negative staged evidence exit under `set -u`

## Evidence

GitHub Actions run `29234971228`, job `86767437140`, evaluated commit:

`d1ff295a8dc080c6bac4423936aadf377b684770`

The 5N/1M rehearsal itself completed successfully:

- five nodes reached selected height 390;
- all selected tips and ordered-DAG tips matched;
- all ordered-DAG state roots matched;
- every node had four peers;
- every node was ready;
- storage consistency passed with memory=391 and persisted=391;
- miner-1 submitted 390 blocks, accepted 390 and rejected 0;
- post-quiescence distinct tips=1 and worst lag=0;
- the command log printed `FINAL_RESULT=PASS`.

Immediately afterwards the script aborted:

```text
scripts/v2_2_20_private_5n_4m_rehearsal.sh: line 1424: EVIDENCE_CONSISTENCY_FAILURES: unbound variable
```

The workflow then read the non-zero shell exit and failed the strict baseline gate.

## Root cause

The shared script runs with:

```bash
set -euo pipefail
```

but initializes array-like values as scalars:

```bash
MINER_EVIDENCE_CONSISTENCY_FAILURES=[]
EVIDENCE_CONSISTENCY_FAILURES=[]
```

Later it performs an array-length expansion:

```bash
${#EVIDENCE_CONSISTENCY_FAILURES[@]}
```

With Bash `nounset`, an empty scalar initialized as `[]` is not a safely declared empty array for this expansion and produces the observed false-negative exit.

## Required implementation

1. Initialize true Bash arrays:

```bash
declare -a EVIDENCE_CONSISTENCY_FAILURES=()
```

2. Keep miner parser JSON and Bash-array concepts separate. Either:

- retain `MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON='[]'` as a JSON string; or
- declare a real Bash array and populate it element-by-element.

Do not reuse one variable as both JSON text and a Bash array.

3. Audit every variable initialized with `=[]` in the shared rehearsal script. Classify each as either:

- JSON string, named with a `_JSON` suffix; or
- Bash array, initialized with `declare -a name=()`.

4. Make evidence-summary generation safe under `set -u` when all arrays are empty.

5. Ensure a successful rehearsal cannot be converted into a non-zero process exit by summary generation, packaging, checksum generation or cleanup.

6. The packaged archive must contain, at minimum:

- `evidence_manifest.json`;
- `evidence-summary.md`;
- `p2p_convergence.json`;
- `final-convergence-table.json`;
- `quiescence-metrics.json`;
- `restart_rejoin.log`;
- endpoint captures;
- complete node/miner logs;
- checksum file.

The failed run fell back to archiving the raw run directory and omitted the first four structured artifacts.

## Regression tests

Add a shell test that runs with `set -euo pipefail` and verifies:

1. an empty evidence-consistency array reports count 0;
2. an empty miner-evidence JSON array remains valid JSON;
3. summary generation succeeds with no failures or warnings;
4. manifest generation and packaged-evidence assertions succeed;
5. the wrapper exit code remains 0 after `FINAL_RESULT=PASS`;
6. a real failure still returns non-zero.

Suggested test file:

`scripts/tests/test_v2_2_20_evidence_array_nounset.sh`

## Validation

```bash
bash -n scripts/v2_2_20_private_5n_4m_rehearsal.sh
bash scripts/tests/test_v2_2_20_evidence_array_nounset.sh
bash scripts/tests/test_v2_2_20_evidence_parser_semantics.sh
bash scripts/tests/test_v2_2_20_multi_failure_classification_fixture.sh
bash scripts/tests/test_v2_2_20_miner_log_parser_fixtures.sh
```

Then rerun the strict workflows on the same new commit:

1. 5N/1M baseline — must finish job `success` and manifest `PASS`;
2. 5N/2M intermediate — must finish job `success` and manifest `PASS`;
3. 5N/4M stress — inspect manifest, not only workflow color.

## Release-control impact

This is an evidence-harness defect, not a consensus or network failure. Nevertheless it blocks closeout because the current candidate commit lacks a complete, correctly packaged strict 5N/1M artifact.

Keep:

- `VERSION=v2.2.20`;
- Cargo workspace version `2.2.20`;
- `public_testnet_ready=false`;
- PR #732 in draft.
