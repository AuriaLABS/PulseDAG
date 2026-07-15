use pulsedag_core::{
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
