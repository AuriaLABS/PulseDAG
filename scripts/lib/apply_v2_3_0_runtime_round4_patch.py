#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one {label} match, found {count}")
    path.write_text(text.replace(old, new, 1))


root = Path(__file__).resolve().parents[2]
main = root / "apps/pulsedagd/src/main.rs"
apply = root / "crates/pulsedag-core/src/apply.rs"
prune_workflow = root / ".github/workflows/v2_3_0_prune_restart_rejoin_gate.yml"
lag_contract = root / "scripts/tests/test_v2_3_0_lag_runtime_driver.sh"

replace_once(
    apply,
    """    ghostdag::classify_merge_set,
    mining::is_coinbase,
""",
    """    ghostdag::classify_merge_set,
    mempool::reconcile_mempool,
    mining::is_coinbase,
""",
    "mempool reconcile import",
)
replace_once(
    apply,
    """        state.utxo = rebuilt.utxo;
        state.dag.ordered_dag_state_root = state.utxo.compute_state_root().ok();
    }
    Ok(())
}
""",
    """        state.utxo = rebuilt.utxo;
        state.dag.ordered_dag_state_root = state.utxo.compute_state_root().ok();
    }
    // Revalidate the live mempool against the newly committed UTXO view.
    // Rebuilds operate on a fresh state, so their internal transaction removal
    // cannot clean the live mempool unless reconciliation runs after commit.
    reconcile_mempool(state);
    Ok(())
}
""",
    "post-block mempool reconciliation",
)

replace_once(
    main,
    """                            let accepted_tip = pulsedag_core::preferred_tip_hash(&guard)
                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                            {
                                let mut rt = runtime.write().await;
""",
    """                            let accepted_tip = pulsedag_core::preferred_tip_hash(&guard)
                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                            let mut selected_segment_completed = false;
                            {
                                let mut rt = runtime.write().await;
""",
    "selected segment completion flag",
)
replace_once(
    main,
    """                                if selected_segment_session
                                    .as_mut()
                                    .map(|session| session.mark_applied(&block.hash, now_unix()))
                                    .unwrap_or(false)
                                {
                                    rt.sync_state =
                                        DagSyncStage::ApplyingSelectedSegment.as_str().to_string();
                                    rt.selected_segment_blocks_applied_total =
                                        rt.selected_segment_blocks_applied_total.saturating_add(1);
                                    if let Some(session) = selected_segment_session.as_mut() {
                                        let selected_tip =
                                            pulsedag_core::preferred_tip_hash(&guard)
                                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                                        if guard
                                            .dag
                                            .blocks
                                            .contains_key(&session.remote_selected_tip)
                                            && selected_tip == session.remote_selected_tip
                                        {
                                            session.state = SelectedSegmentSessionState::Complete;
                                            rt.sync_state = DagSyncStage::SelectedSegmentComplete
                                                .as_str()
                                                .to_string();
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                        }
                                    }
                                }
""",
    """                                if let Some(session) = selected_segment_session.as_mut() {
                                    if session.mark_applied(&block.hash, now_unix()) {
                                        rt.sync_state = DagSyncStage::ApplyingSelectedSegment
                                            .as_str()
                                            .to_string();
                                        rt.selected_segment_blocks_applied_total = rt
                                            .selected_segment_blocks_applied_total
                                            .saturating_add(1);
                                        rt.active_session_applied_blocks =
                                            session.accepted_applied_hashes.len() as u64;
                                        rt.active_session_remaining_blocks = session
                                            .remote_selected_height
                                            .saturating_sub(guard.dag.best_height);
                                        let selected_tip =
                                            pulsedag_core::preferred_tip_hash(&guard)
                                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                                        if guard
                                            .dag
                                            .blocks
                                            .contains_key(&session.remote_selected_tip)
                                            && selected_tip == session.remote_selected_tip
                                        {
                                            session.state = SelectedSegmentSessionState::Complete;
                                            selected_segment_completed = true;
                                            rt.sync_state = DagSyncStage::SelectedSegmentComplete
                                                .as_str()
                                                .to_string();
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                            rt.active_session_remaining_blocks = 0;
                                            rt.active_session_id = None;
                                            rt.active_session_peer = None;
                                            rt.active_session_remote_tip = None;
                                            rt.active_session_remote_height = 0;
                                            rt.active_session_common_ancestor = None;
                                        }
                                    }
                                }
""",
    "selected segment completion accounting",
)
replace_once(
    main,
    """                                rt.sync_state = if guard.orphan_blocks.is_empty() {
                                    "synced"
                                } else {
                                    "catching_up"
                                }
                                .to_string();
""",
    """                                rt.sync_state = if selected_segment_completed {
                                    DagSyncStage::SelectedSegmentComplete.as_str().to_string()
                                } else if guard.orphan_blocks.is_empty() {
                                    "synced".to_string()
                                } else {
                                    "catching_up".to_string()
                                };
""",
    "preserve selected segment completion state",
)
replace_once(
    main,
    """                                rt.sync_pipeline.complete_cycle(now_unix());
                            }
                            if adopted > 0 {
""",
    """                                rt.sync_pipeline.complete_cycle(now_unix());
                            }
                            if selected_segment_completed {
                                selected_segment_session = None;
                                selected_segment_locator_state.lock().await.pending_locator = None;
                            }
                            if adopted > 0 {
""",
    "clear completed selected segment session",
)

replace_once(
    main,
    """                        let planned_request_count = requests.len() as u64;
                        for hash in requests {
""",
    """                        let planned_request_count = requests.len() as u64;
                        let mut issued_selected_request_count = 0u64;
                        for hash in requests {
""",
    "selected request counter",
)
replace_once(
    main,
    """                                if let Some(ref p2p) = p2p {
                                    let result = if let Some(session) =
                                        selected_segment_session.as_ref().filter(|session| {
                                            session.current_chunk.iter().any(|item| item == &hash)
                                        }) {
                                        p2p.request_block_from(&session.peer_id, &hash).map(|_| ())
                                    } else {
                                        p2p.request_block(&hash)
                                    };
                                    if let Err(e) = result {
                                        warn!(error = %e, block_hash = %hash, "failed issuing header-driven GetBlock request");
                                    }
                                }
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
""",
    """                                let mut peer_addressed_request_succeeded = false;
                                if let Some(ref p2p) = p2p {
                                    let (result, peer_addressed) = if let Some(session) =
                                        selected_segment_session.as_ref().filter(|session| {
                                            session.current_chunk.iter().any(|item| item == &hash)
                                        }) {
                                        (
                                            p2p.request_block_from(&session.peer_id, &hash)
                                                .map(|_| ()),
                                            true,
                                        )
                                    } else {
                                        (p2p.request_block(&hash), false)
                                    };
                                    match result {
                                        Ok(()) => peer_addressed_request_succeeded = peer_addressed,
                                        Err(e) => {
                                            warn!(error = %e, block_hash = %hash, "failed issuing header-driven GetBlock request");
                                        }
                                    }
                                }
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                if peer_addressed_request_succeeded {
                                    issued_selected_request_count =
                                        issued_selected_request_count.saturating_add(1);
                                    rt.peer_addressed_getblock_sent_total = rt
                                        .peer_addressed_getblock_sent_total
                                        .saturating_add(1);
                                }
""",
    "peer-addressed selected block accounting",
)
replace_once(
    main,
    """                            let local_height = common_ancestor_height;
                            let remote_height = headers
                                .iter()
                                .map(|item| item.header.height)
                                .max()
                                .unwrap_or(local_height);
                            rt.selected_segment_gap_blocks =
                                remote_height.saturating_sub(local_height);
""",
    """                            let local_height = common_ancestor_height;
                            let remote_height = headers
                                .iter()
                                .map(|item| item.header.height)
                                .max()
                                .unwrap_or(local_height);
                            let canonical_gap = remote_height.saturating_sub(local_height);
                            rt.selected_segment_gap_blocks = canonical_gap;
                            rt.network_selected_height_gap =
                                rt.network_selected_height_gap.max(canonical_gap);
""",
    "canonical selected segment gap",
)
replace_once(
    main,
    """                                rt.active_session_requested_blocks = rt
                                    .active_session_requested_blocks
                                    .saturating_add(planned_request_count);
""",
    """                                rt.active_session_requested_blocks = rt
                                    .active_session_requested_blocks
                                    .saturating_add(issued_selected_request_count);
""",
    "active session issued request count",
)
replace_once(
    main,
    """                                Some(Ok(())) if planned_request_count > 0 => {
""",
    """                                Some(Ok(())) if issued_selected_request_count > 0 => {
""",
    "selected request success condition",
)
replace_once(
    main,
    """                                    rt.selected_segment_block_requests_total = rt
                                        .selected_segment_block_requests_total
                                        .saturating_add(planned_request_count);
""",
    """                                    rt.selected_segment_block_requests_total = rt
                                        .selected_segment_block_requests_total
                                        .saturating_add(issued_selected_request_count);
""",
    "selected request runtime total",
)
replace_once(
    main,
    """                        rt.final_quiescence_missing_segment_request_total = rt
                            .final_quiescence_missing_segment_request_total
                            .saturating_add(planned_request_count);
""",
    """                        rt.final_quiescence_missing_segment_request_total = rt
                            .final_quiescence_missing_segment_request_total
                            .saturating_add(issued_selected_request_count);
""",
    "final quiescence selected request count",
)

replace_once(
    prune_workflow,
    """          if [[ ! -x "$driver" ]]; then
            echo "Required runtime driver is missing or not executable: $driver" >&2
            echo 'Cargo tests alone are not operational restart/rejoin evidence.' >&2
            exit 44
          fi
          OUT_DIR="$PWD/ci-evidence/prune-restart/runtime" "$driver" 2>&1 | tee ci-evidence/prune-restart/runtime-driver.log
""",
    """          if [[ ! -r "$driver" ]]; then
            echo "Required runtime driver is missing or unreadable: $driver" >&2
            echo 'Cargo tests alone are not operational restart/rejoin evidence.' >&2
            exit 44
          fi
          OUT_DIR="$PWD/ci-evidence/prune-restart/runtime" bash "$driver" 2>&1 | tee ci-evidence/prune-restart/runtime-driver.log
""",
    "Task 04 driver invocation",
)

replace_once(
    lag_contract,
    'PATCHER="scripts/lib/patch_v2_3_0_lag_runtime_harness.py"\n',
    'PATCHER="scripts/lib/patch_v2_3_0_lag_runtime_harness.py"\nNODE_MAIN="apps/pulsedagd/src/main.rs"\n',
    "lag contract node source path",
)
replace_once(
    lag_contract,
    """grep -Fq '_v230_lag_package_failure' "$HARNESS"
""",
    """grep -Fq '_v230_lag_package_failure' "$HARNESS"
grep -Fq 'let mut selected_segment_completed = false;' "$NODE_MAIN"
grep -Fq 'selected_segment_session = None;' "$NODE_MAIN"
grep -Fq 'rt.active_session_remaining_blocks = 0;' "$NODE_MAIN"
grep -Fq 'rt.peer_addressed_getblock_sent_total = rt' "$NODE_MAIN"
grep -Fq 'rt.network_selected_height_gap.max(canonical_gap)' "$NODE_MAIN"
""",
    "lag node lifecycle regressions",
)

integration_test = root / "crates/pulsedag-core/tests/confirmed_mempool_cleanup.rs"
integration_test.parent.mkdir(parents=True, exist_ok=True)
integration_test.write_text(r'''use pulsedag_core::{
    apply::commit_block_to_state,
    build_coinbase_transaction,
    genesis::init_chain_state,
    types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput},
    ConsensusMode, SelectedParentPolicy,
};

fn block(hash: &str, parent: &str, height: u64, transactions: Vec<Transaction>) -> Block {
    Block {
        hash: hash.to_string(),
        header: BlockHeader {
            version: 1,
            parents: vec![parent.to_string()],
            timestamp: height,
            difficulty: 1,
            nonce: height,
            merkle_root: pulsedag_core::types::compute_merkle_root(&transactions),
            state_root: format!("state-{hash}"),
            blue_score: height,
            height,
        },
        transactions,
    }
}

fn assert_confirmed_transaction_cleanup(mode: ConsensusMode) {
    let mut state = init_chain_state(format!("confirmed-cleanup-{mode:?}"));
    state.dag.consensus_mode = mode;
    state.dag.selected_parent_policy = if mode == ConsensusMode::GhostdagDev {
        SelectedParentPolicy::GhostdagInspired
    } else {
        SelectedParentPolicy::LegacyTip
    };

    let genesis = state.dag.genesis_hash.clone();
    let funding_tx = build_coinbase_transaction("funding-owner", 50, 0);
    let funding = block("funding", &genesis, 1, vec![funding_tx.clone()]);
    commit_block_to_state(&funding, &mut state).expect("funding block should commit");

    let spent = OutPoint {
        txid: funding_tx.txid.clone(),
        index: 0,
    };
    let confirmed = Transaction {
        txid: "confirmed-spend".to_string(),
        version: 1,
        inputs: vec![TxInput {
            previous_output: spent.clone(),
            public_key: String::new(),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "destination".to_string(),
            amount: 49,
        }],
        fee: 1,
        nonce: 1,
    };
    state
        .mempool
        .transactions
        .insert(confirmed.txid.clone(), confirmed.clone());
    state.mempool.first_seen.insert(confirmed.txid.clone(), 0);
    state.mempool.spent_outpoints.insert(spent);

    let coinbase = build_coinbase_transaction("block-miner", 51, 0);
    let confirmed_block = block(
        "confirmed-block",
        &funding.hash,
        2,
        vec![coinbase, confirmed.clone()],
    );
    commit_block_to_state(&confirmed_block, &mut state)
        .expect("block containing mempool transaction should commit");

    assert!(!state.mempool.transactions.contains_key(&confirmed.txid));
    assert!(!state.mempool.first_seen.contains_key(&confirmed.txid));
    assert!(state.mempool.spent_outpoints.is_empty());
    assert!(state.mempool.counters.reconcile_removed_total > 0);
}

#[test]
fn confirmed_transactions_are_removed_after_legacy_rebuild() {
    assert_confirmed_transaction_cleanup(ConsensusMode::Legacy);
}

#[test]
fn confirmed_transactions_are_removed_after_ghostdag_rebuild() {
    assert_confirmed_transaction_cleanup(ConsensusMode::GhostdagDev);
}
''')

print("runtime round-4 patch applied")
