#!/usr/bin/env python3
"""Normalize legacy storage fixtures for v2.4 compact-target consensus."""

from pathlib import Path
import re

PATH = Path("crates/pulsedag-storage/src/lib.rs")


def replace_function(name: str, replacement: str) -> None:
    text = PATH.read_text()
    pattern = re.compile(
        rf"(?ms)^    fn {re.escape(name)}\(.*?^    \}}\n(?=\n    (?:fn |#\[test\]))"
    )
    updated, count = pattern.subn(replacement.rstrip() + "\n", text, count=1)
    if count != 1:
        raise SystemExit(f"expected one function {name}, found {count}")
    PATH.write_text(updated)


def replace_test(name: str, replacement: str) -> None:
    text = PATH.read_text()
    pattern = re.compile(
        rf"(?ms)^    #\[test\]\n    fn {re.escape(name)}\(\) \{{.*?^    \}}\n"
    )
    updated, count = pattern.subn(replacement.rstrip() + "\n", text, count=1)
    if count != 1:
        raise SystemExit(f"expected one test {name}, found {count}")
    PATH.write_text(updated)


replace_function(
    "build_linear_chain",
    '''    fn build_linear_chain(chain_id: &str, blocks_to_add: usize) -> pulsedag_core::ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        for i in 1..=blocks_to_add {
            let parent = best_tip_hash(&state);
            let parent_timestamp = state
                .dag
                .blocks
                .get(&parent)
                .map(|block| block.header.timestamp)
                .unwrap_or(0);
            let mut block = build_candidate_block(
                vec![parent],
                i as u64,
                pulsedag_core::expected_difficulty(&state),
                vec![build_coinbase_transaction("miner", 50, i as u64)],
            );
            block.header.timestamp = parent_timestamp.saturating_add(60).max(1);
            refresh_block_consensus_ids_with_state(&mut block, &state)
                .expect("prepare and mine state-aware test block");
            accept_block(block, &mut state, AcceptSource::LocalMining).expect("accept mined block");
        }
        state
    }
''',
)

replace_function(
    "append_test_block",
    '''    fn append_test_block(
        state: &mut pulsedag_core::ChainState,
        parents: Vec<Hash>,
        height: u64,
    ) -> Block {
        let newest_parent_timestamp = parents
            .iter()
            .filter_map(|parent| state.dag.blocks.get(parent))
            .map(|block| block.header.timestamp)
            .max()
            .unwrap_or(0);
        let coinbase_nonce = height
            .saturating_mul(1_000)
            .saturating_add(state.dag.blocks.len() as u64);
        let mut block = build_candidate_block(
            parents,
            height,
            pulsedag_core::expected_difficulty(state),
            vec![build_coinbase_transaction("miner", 50, coinbase_nonce)],
        );
        block.header.timestamp = newest_parent_timestamp.saturating_add(60).max(1);
        refresh_block_consensus_ids_with_state(&mut block, state)
            .expect("prepare and mine state-aware test block");
        accept_block(block.clone(), state, AcceptSource::LocalMining).expect("accept block");
        block
    }
''',
)

replace_test(
    "accepted_block_atomic_persistence_path_has_no_regression",
    '''    #[test]
    fn accepted_block_atomic_persistence_path_has_no_regression() {
        let path = temp_db_path("atomic-path-regression");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&best_tip_hash(&state))
            .cloned()
            .expect("genesis block");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");

        for height in 1..=3 {
            let parent = best_tip_hash(&state);
            let parent_timestamp = state
                .dag
                .blocks
                .get(&parent)
                .map(|block| block.header.timestamp)
                .unwrap_or(0);
            let mut block = build_candidate_block(
                vec![parent],
                height,
                pulsedag_core::expected_difficulty(&state),
                vec![build_coinbase_transaction("miner", 50, height)],
            );
            block.header.timestamp = parent_timestamp.saturating_add(60).max(1);
            refresh_block_consensus_ids_with_state(&mut block, &state)
                .expect("prepare and mine state-aware test block");
            accept_block(block.clone(), &mut state, AcceptSource::LocalMining)
                .expect("accept block");
            storage
                .persist_block_and_chain_state(&block, &state)
                .expect("persist block + snapshot");
        }

        drop(storage);
        let reopened = Storage::open(&path).expect("reopen storage");
        let snapshot = reopened
            .load_chain_state()
            .expect("load snapshot")
            .expect("snapshot present");
        let blocks = reopened.list_blocks().expect("list blocks");
        assert_eq!(snapshot.dag.best_height, 3);
        assert_eq!(
            blocks.len(),
            4,
            "genesis + 3 accepted blocks should persist"
        );
        assert!(blocks
            .iter()
            .any(|b| b.hash == best_tip_hash(&snapshot)
                && b.header.height == snapshot.dag.best_height));

        let _ = std::fs::remove_dir_all(path);
    }
''',
)

replace_test(
    "snapshot_verification_newer_delta_side_dag_is_not_snapshot_corruption",
    '''    #[test]
    fn snapshot_verification_newer_delta_side_dag_is_not_snapshot_corruption() {
        let path = temp_db_path("snapshot-newer-side-dag");
        let storage = Storage::open(&path).expect("open storage");
        let snapshot = build_linear_chain("testnet", 2);
        let mut final_state = snapshot.clone();
        let parent = best_tip_hash(&snapshot);
        let parent_timestamp = snapshot
            .dag
            .blocks
            .get(&parent)
            .map(|block| block.header.timestamp)
            .unwrap_or(0);
        let mut side = build_candidate_block(
            vec![parent],
            3,
            pulsedag_core::expected_difficulty(&snapshot),
            vec![build_coinbase_transaction("side", 50, 30)],
        );
        side.header.timestamp = parent_timestamp.saturating_add(60).max(1);
        refresh_block_consensus_ids_with_state(&mut side, &final_state)
            .expect("prepare and mine side block");
        accept_block(side.clone(), &mut final_state, AcceptSource::LocalMining)
            .expect("accept side block");
        let bundle = Storage::snapshot_bundle_for_state(
            snapshot.clone(),
            vec![side],
            1,
            Some(1),
            Storage::snapshot_metadata_for_state(&snapshot, 1),
            storage
                .accepted_storage_generation()
                .expect("storage generation"),
        );

        let report = storage.verify_snapshot_bundle(&bundle, Some("testnet"));
        assert!(!report
            .issues
            .iter()
            .any(|issue| issue.code == "SNAPSHOT_BUNDLE_DELTA_NOT_IN_SNAPSHOT"));
        let _ = std::fs::remove_dir_all(path);
    }
''',
)
