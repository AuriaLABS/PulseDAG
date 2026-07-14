# Task: implement real non-zero prune, restart and offline rejoin evidence

## Context

Candidate run `29296544878` passed workspace validation and staged `5N/1M -> 5N/2M -> 5N/4M` on commit `a4439259d08c0b8f98add607e5d0290e69b6ee90`.

The retained-set test gate and focused test discovery pass. Runtime-closeout fails because the required executable does not exist:

```text
scripts/v2_3_0_prune_restart_rejoin_runtime.sh
```

Cargo tests cannot substitute for an operational drill. Do not weaken `.github/workflows/v2_3_0_prune_restart_rejoin_gate.yml`.

## Deliverables

1. Add executable `scripts/v2_3_0_prune_restart_rejoin_runtime.sh`.
2. Reuse the runtime harness from PR #739 after rebasing, or extract compatible lifecycle helpers without changing existing rehearsal behavior.
3. Add shell regression tests for manifest semantics, non-zero prune enforcement, restart/rejoin state comparisons, cleanup and failure propagation.
4. Remove this task file in the implementation commit.

## Runtime drill

The script must be self-contained on GitHub-hosted Ubuntu and accept:

```text
OUT_DIR=<absolute output directory>
```

Optional bounded overrides may include base ports, keep-recent blocks, minimum pre-prune height, offline advance blocks and convergence timeouts. Defaults must complete reliably in CI.

The drill must:

- run the exact checked-out release binaries;
- start five `libp2p-real` private nodes with unique RPC/P2P ports, identities and data directories, plus enough miners to advance the network;
- establish stable four-peer topology and identical selected tip/state before maintenance;
- configure a private/rehearsal pruning window small enough to produce real historical candidates while preserving production defaults;
- advance the network beyond the retained/finality/rollback window;
- capture and persist a valid snapshot anchor;
- invoke the real operator prune path on a selected node and require `blocks_pruned_total > 0`;
- capture the retained-set report and prove storage and memory retained hash digests are identical with no storage-only/memory-only retained hashes;
- stop that node cleanly and restart it from the same identity/data directory;
- prove snapshot+delta restart actually occurred and that selected tip, ordered DAG tip and state root match the pre-restart values or the current canonical network state as appropriate;
- stop/offline the node again, keep the other four nodes/miners advancing by a positive configured block count, then restart/rejoin it;
- prove it catches up and all five nodes converge to identical selected tip, ordered DAG tip, state root and retained accepted-set digest;
- require five ready nodes, four compatible peers per node, zero active orphans/blocking missing parents and final storage/memory consistency;
- capture endpoint data before prune, after prune, after restart, during offline advance and after final rejoin;
- terminate all processes and close ports on every exit path.

Do not copy RocksDB directories between nodes, edit persisted state, fake `pruned` counts or use unit-test JSON as runtime evidence.

## Required runtime manifest

Write `${OUT_DIR}/evidence_manifest.json` with at least:

```json
{
  "result": "PASS",
  "evidence_kind": "runtime",
  "candidate_commit": "<full sha>",
  "node_count": 5,
  "blocks_pruned_total": 1,
  "retained_storage_hash_digest": "...",
  "retained_memory_hash_digest": "...",
  "snapshot_delta_restart_executed": true,
  "restart_selected_tip_matches": true,
  "restart_state_root_matches": true,
  "offline_advance_blocks": 1,
  "rejoin_executed": true,
  "rejoin_converged": true,
  "final_storage_memory_consistent": true,
  "public_testnet_ready": false
}
```

Use actual values. Also include prune boundary, considered/pruned counts, retained categories, snapshot generation/anchor, pre/post restart tips and roots, offline height interval, per-node final state, timing and failure reasons. Any failed invariant must write `result=FAIL` and return non-zero.

## Evidence files

Store under `OUT_DIR`:

- node/miner logs;
- endpoint captures for all phases, including checks, snapshot, sync, p2p and status;
- prune request/response and retained-set report;
- restart and rejoin timelines;
- sorted accepted/retained hash digests per node;
- final convergence table;
- command log;
- `SHA256SUMS`.

## Validation

Run and pass:

```bash
bash -n scripts/v2_3_0_prune_restart_rejoin_runtime.sh
cargo fmt --all -- --check
cargo check --workspace --locked
scripts/v2_3_0_04_prune_restart_rejoin_evidence.sh
cargo test -p pulsedag-storage non_zero --locked -- --nocapture
cargo test -p pulsedag-storage restart --locked -- --nocapture
cargo test -p pulsedag-storage offline --locked -- --nocapture
bash scripts/tests/<new-prune-runtime-tests>.sh
```

Then run `.github/workflows/v2_3_0_prune_restart_rejoin_gate.yml` with `evidence_mode=runtime-closeout` and the implementation SHA. Runtime driver, semantic validation and enforcement must all pass.

## Guardrails

- Keep `VERSION` and workspace version at `2.2.20`.
- Keep `public_testnet_ready=false`.
- Do not change consensus/PoW or start the public-testnet clock.
- Do not relax the existing workflow semantics or treat the test gate as runtime evidence.