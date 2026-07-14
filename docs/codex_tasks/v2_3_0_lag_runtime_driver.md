# Task: implement real selected-segment lag-injection evidence

## Context

Candidate run `29296544878` passed workspace validation and staged `5N/1M -> 5N/2M -> 5N/4M` on commit `a4439259d08c0b8f98add607e5d0290e69b6ee90`.

The lag gate's coverage and selected-segment/lag tests pass. Runtime-closeout fails because `scripts/v2_3_0_lag_injection_selected_segment.sh` still exits in `CI_MODE=0` with an operator-only instruction instead of running the drill. Do not weaken `.github/workflows/v2_3_0_lag_injection_gate.yml`.

## Deliverables

1. Make `scripts/v2_3_0_lag_injection_selected_segment.sh` self-contained in `CI_MODE=0` on GitHub-hosted Ubuntu.
2. Reuse the runtime harness introduced by PR #739 after rebasing, or extract compatible helpers without changing existing 5N rehearsal behavior.
3. Preserve `CI_MODE=1` as clearly synthetic schema-only evidence that is never closeout-eligible.
4. Add shell regression tests for runtime manifest validation, gap calculation, process cleanup and rejection of synthetic/zero-counter evidence.
5. Remove this task file in the implementation commit.

## Real runtime drill

The script must accept:

```text
CI_MODE=0
OUT_DIR=<absolute output directory>
MIN_SELECTED_GAP=<integer, normally 96>
```

The runtime path must:

- use the exact checked-out release binaries;
- start five `libp2p-real` private nodes and four miners with stable four-peer topology;
- identify `n5` and record its pre-isolation selected tip/height/state root;
- genuinely isolate `n5` from block/tip propagation. Stopping `n5` while preserving its data directory and identity is acceptable and preferred over host firewall mutation on GitHub runners;
- keep `n1..n4` and miners running until the canonical selected-height gap is at least `MIN_SELECTED_GAP` and never below 64;
- record both harness-observed and canonical endpoint-derived gap and require equality;
- restart/reconnect `n5` with the same identity/data directory;
- prove remote selected-tip inventory is received and a best remote selected height above local is selected;
- prove correlated locator request/response, selected-segment header acceptance, parent-first block requests, blocks applied and chunks completed using endpoint counters and logs attributable to `n5`;
- prove no broadcast-only shortcut is counted as selected-segment recovery;
- wait for all five nodes to converge to identical selected tip, ordered DAG tip, state root and retained accepted-hash digest;
- require five ready nodes, four compatible peers each, zero active orphans, zero blocking missing parents and storage/memory equality;
- capture pre-isolation, isolated, reconnecting and final endpoints/logs;
- terminate all processes and close all ports on success/failure.

Do not fabricate counters, edit RocksDB state, copy block files, or set `CI_MODE=1` in runtime-closeout.

## Required runtime manifest

Write `${OUT_DIR}/evidence_manifest.json` with at least:

```json
{
  "result": "PASS",
  "ci_mode": false,
  "evidence_kind": "runtime",
  "candidate_commit": "<full sha>",
  "node_count": 5,
  "isolated_node": "n5",
  "configured_min_gap": 96,
  "observed_network_selected_height_gap": 96,
  "canonical_network_selected_height_gap": 96,
  "remote_tip_inventory_received_total": 1,
  "locator_requests_sent_total": 1,
  "locator_responses_correlated_total": 1,
  "selected_segment_block_requests_total": 1,
  "selected_segment_blocks_applied_total": 1,
  "selected_segment_chunks_completed_total": 1,
  "primary_session_path": "correlated_selected_segment",
  "final_convergence": true,
  "storage_memory_consistent": true,
  "public_testnet_ready": false
}
```

Use real values, not the illustrative numbers. Include per-node final state, transition timeline, session id/peer attribution, pending selected-segment requests, final orphan/missing-parent counts and failure reasons. Return non-zero on any failed invariant.

## Evidence files

Store under `OUT_DIR`:

- node/miner logs and PIDs;
- endpoint snapshots for every phase;
- topology samples and gap timeline;
- selected-segment transition/counter summary;
- final convergence table;
- command log;
- `SHA256SUMS`.

## Validation

Run and pass:

```bash
bash -n scripts/v2_3_0_lag_injection_selected_segment.sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedagd selected_segment --locked -- --nocapture
cargo test -p pulsedagd lag_injection --locked -- --nocapture
bash scripts/tests/<new-lag-driver-tests>.sh
```

Then run `.github/workflows/v2_3_0_lag_injection_gate.yml` with `evidence_mode=runtime-closeout`, `candidate_sha=<implementation sha>` and `min_selected_gap=96`. The driver, evidence-semantics and enforcement steps must all pass.

## Guardrails

- Keep `VERSION` and workspace version at `2.2.20`.
- Keep `public_testnet_ready=false`.
- Do not change consensus/PoW or start the public-testnet clock.
- Do not relax jq conditions or accept schema-only evidence for closeout.