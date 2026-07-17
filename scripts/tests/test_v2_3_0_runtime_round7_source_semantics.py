#!/usr/bin/env python3
from pathlib import Path

root = Path(__file__).resolve().parents[2]
main = (root / "apps/pulsedagd/src/main.rs").read_text()
replay = (root / "crates/pulsedag-core/src/replay.rs").read_text()
prune = (root / "crates/pulsedag-rpc/src/handlers/pruning.rs").read_text()

for token in [
    "correlates_continuation_headers",
    "issued_selected_hashes",
    "fn start_chunk(&mut self, hashes: Vec<String>, now: u64) -> bool",
    "session.start_chunk(issued_selected_hashes, now_unix())",
    "complete_current_chunk_if_applied",
]:
    assert token in main, token
for token in [
    "pub fn compact_snapshot_to_retained_blocks",
    "snapshot.dag.blocks = retained_blocks",
    ".selected_chain",
    ".ordered_dag",
]:
    assert token in replay, token
for token in [
    "compact_snapshot_to_retained_blocks(chain.clone(), &retained_blocks)",
    "verify_accepted_storage_invariants(&compact)",
    "PRUNE_RETAINED_SET_MISMATCH",
    "!invariant_report.is_ok()",
]:
    assert token in prune, token
assert prune.index("verify_accepted_storage_invariants(&compact)") < prune.index("*chain = compact.clone()")
assert "session.current_chunk = candidates.clone()" not in main
