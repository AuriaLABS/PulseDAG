#!/usr/bin/env python3
"""Normalize the offline catch-up fixture for v2.4 compact-target consensus."""

from pathlib import Path
import re

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()
pattern = re.compile(
    r"(?ms)^    #\[test\]\n    fn restart_rejoin_offline_node_converges_after_selected_segment_catch_up\(\) \{.*?^    \}\n"
)
replacement = '''    #[test]
    fn restart_rejoin_offline_node_converges_after_selected_segment_catch_up() {
        let path = temp_db_path("restart-rejoin-offline-catch-up");
        let storage = Storage::open(&path).expect("open storage");
        let offline_state = build_test_chain("testnet", 8);
        persist_chain_blocks(&storage, &offline_state);
        storage
            .persist_chain_state(&offline_state)
            .expect("persist offline node snapshot");
        let offline_boundary_height = offline_state.dag.best_height;

        let report = storage
            .prune_blocks_with_retained_set(
                &offline_state,
                offline_boundary_height.saturating_sub(3),
            )
            .expect("non-zero prune before offline window");
        assert!(
            report.blocks_pruned_total > 0,
            "restart_rejoin offline requires non-zero pruning"
        );
        drop(storage);

        use pulsedag_core::{
            accept_block, build_candidate_block, build_coinbase_transaction,
            pow::dev_mine_header, refresh_block_consensus_ids,
            refresh_block_consensus_ids_with_state, AcceptSource,
        };
        let mut network_state = offline_state.clone();
        for i in (offline_boundary_height + 1)..=(offline_boundary_height + 4) {
            let parent = chain_best_tip(&network_state);
            let parent_timestamp = network_state
                .dag
                .blocks
                .get(&parent)
                .map(|block| block.header.timestamp)
                .unwrap_or(0);
            let mut block = build_candidate_block(
                vec![parent],
                i,
                pulsedag_core::expected_difficulty(&network_state),
                vec![build_coinbase_transaction("miner", 50, i * 1000)],
            );
            block.header.timestamp = parent_timestamp.saturating_add(60).max(1);
            refresh_block_consensus_ids_with_state(&mut block, &network_state)
                .expect("prepare catch-up block state root");
            let (header, mined, _, _) = dev_mine_header(block.header.clone(), 1_000_000);
            assert!(mined, "failed to mine catch-up block at height {i}");
            block.header = header;
            refresh_block_consensus_ids(&mut block);
            accept_block(block, &mut network_state, AcceptSource::P2p)
                .expect("accept catch-up block");
        }

        let rejoined = Storage::open(&path).expect("reopen offline node for rejoin");
        for block in network_state
            .dag
            .blocks
            .values()
            .filter(|b| b.header.height > offline_boundary_height)
        {
            rejoined
                .persist_block(block)
                .expect("persist catch-up block during rejoin");
        }
        rejoined
            .persist_chain_state(&network_state)
            .expect("persist converged snapshot after rejoin");
        let caught_up = rejoined
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("replay after rejoin");

        assert_eq!(
            chain_best_tip(&caught_up),
            chain_best_tip(&network_state),
            "rejoined node selected tip must match network after catch-up"
        );
        assert_eq!(
            caught_up.dag.ordered_dag_tip, network_state.dag.ordered_dag_tip,
            "rejoined node ordered DAG tip must match network"
        );
        assert_eq!(
            caught_up.dag.best_height, network_state.dag.best_height,
            "rejoined node height must match network after catch-up"
        );
        assert_eq!(
            caught_up.utxo.compute_state_root().expect("caught-up root"),
            network_state
                .utxo
                .compute_state_root()
                .expect("network root"),
            "rejoined node state root must match network after catch-up"
        );

        let _ = std::fs::remove_dir_all(path);
    }
'''
updated, count = pattern.subn(replacement, text, count=1)
if count != 1:
    raise SystemExit(f"expected one offline catch-up test, found {count}")
path.write_text(updated)
