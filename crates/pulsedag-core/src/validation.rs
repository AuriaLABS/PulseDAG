use std::collections::BTreeSet;

use crate::{
    apply::apply_transaction,
    errors::PulseError,
    mining::{current_ts, is_coinbase},
    state::ChainState,
    tx::{compute_txid, verify_transaction_signatures},
    types::{Block, OutPoint, Transaction},
};

pub fn tx_output_amount(state: &ChainState, outpoint: &OutPoint) -> Option<u64> {
    if let Some(utxo) = state.utxo.utxos.get(outpoint) {
        return Some(utxo.amount);
    }
    state
        .mempool
        .transactions
        .get(&outpoint.txid)
        .and_then(|tx| tx.outputs.get(outpoint.index as usize))
        .map(|output| output.amount)
}

pub fn missing_transaction_inputs(tx: &Transaction, state: &ChainState) -> Vec<OutPoint> {
    tx.inputs
        .iter()
        .filter_map(|input| {
            let previous_output = &input.previous_output;
            if tx_output_amount(state, previous_output).is_some() {
                None
            } else {
                Some(previous_output.clone())
            }
        })
        .collect()
}

pub fn validate_transaction(tx: &Transaction, state: &ChainState) -> Result<(), PulseError> {
    if tx.outputs.is_empty() {
        return Err(PulseError::InvalidTransaction("no outputs".into()));
    }
    if tx.inputs.is_empty() {
        return Err(PulseError::InvalidTransaction("no inputs".into()));
    }
    if tx.outputs.iter().any(|o| o.amount == 0) {
        return Err(PulseError::InvalidTransaction("zero-value output".into()));
    }
    let mut seen_inputs = BTreeSet::new();
    for input in &tx.inputs {
        if !seen_inputs.insert(input.previous_output.clone()) {
            return Err(PulseError::InvalidTransaction("duplicate input".into()));
        }
    }
    if compute_txid(tx) != tx.txid {
        return Err(PulseError::InvalidTxid);
    }

    let total_input = tx.inputs.iter().try_fold(0_u64, |acc, input| {
        let input_amount =
            tx_output_amount(state, &input.previous_output).ok_or(PulseError::UtxoNotFound)?;
        if state
            .mempool
            .spent_outpoints
            .contains(&input.previous_output)
        {
            return Err(PulseError::DoubleSpend);
        }
        acc.checked_add(input_amount)
            .ok_or_else(|| PulseError::InvalidTransaction("input overflow".into()))
    })?;

    let total_output = tx
        .outputs
        .iter()
        .try_fold(0_u64, |acc, output| acc.checked_add(output.amount))
        .ok_or_else(|| PulseError::InvalidTransaction("output overflow".into()))?;
    let required = total_output
        .checked_add(tx.fee)
        .ok_or_else(|| PulseError::InvalidTransaction("output overflow".into()))?;
    if total_input < required {
        return Err(PulseError::InsufficientFunds);
    }

    verify_transaction_signatures(tx, state)?;
    Ok(())
}

pub fn validate_block(block: &Block, state: &ChainState) -> Result<(), PulseError> {
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    if block.header.timestamp == 0 {
        return Err(PulseError::InvalidBlock(
            "timestamp must be greater than zero".into(),
        ));
    }
    let now = current_ts();
    let max_future = crate::dev_max_future_drift_secs();
    if block.header.timestamp > now.saturating_add(max_future) {
        return Err(PulseError::InvalidBlock(format!(
            "timestamp too far in the future: {} > {} + {}",
            block.header.timestamp, now, max_future
        )));
    }
    if block.header.parents.is_empty() {
        return Err(PulseError::InvalidBlock("block has no parents".into()));
    }
    let mut seen_parents = BTreeSet::new();
    let mut expected_height = 0u64;
    let mut newest_parent_timestamp = 0u64;
    for parent in &block.header.parents {
        if !seen_parents.insert(parent.clone()) {
            return Err(PulseError::InvalidBlock("duplicate parent".into()));
        }
        let parent_block = state
            .dag
            .blocks
            .get(parent)
            .ok_or_else(|| PulseError::InvalidBlock(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
        newest_parent_timestamp = newest_parent_timestamp.max(parent_block.header.timestamp);
    }
    if block.header.height != expected_height {
        return Err(PulseError::InvalidBlock(format!(
            "invalid height {}, expected {}",
            block.header.height, expected_height
        )));
    }
    if block.header.timestamp < newest_parent_timestamp {
        return Err(PulseError::InvalidBlock(format!(
            "timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }
    if block.transactions.is_empty() {
        return Err(PulseError::InvalidBlock("empty block".into()));
    }
    if !is_coinbase(&block.transactions[0]) {
        return Err(PulseError::InvalidBlock("first tx must be coinbase".into()));
    }
    if block.transactions.iter().skip(1).any(is_coinbase) {
        return Err(PulseError::InvalidBlock(
            "multiple coinbase transactions".into(),
        ));
    }
    let mut seen_txids = BTreeSet::new();
    for tx in &block.transactions {
        if !seen_txids.insert(tx.txid.clone()) {
            return Err(PulseError::InvalidBlock(
                "duplicate transaction in block".into(),
            ));
        }
    }

    let mut working = state.clone();
    working.mempool.transactions.clear();
    working.mempool.spent_outpoints.clear();

    for tx in block.transactions.iter().skip(1) {
        validate_transaction(tx, &working)?;
        apply_transaction(tx, &mut working, block.header.height)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining::{build_candidate_block, build_coinbase_transaction},
        types::{TxInput, TxOutput},
    };

    fn coinbase(nonce: u64) -> Transaction {
        build_coinbase_transaction("miner1", 50, nonce)
    }

    fn structurally_valid_block(state: &ChainState) -> Block {
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(parents, 1, 1, vec![coinbase(1)]);
        block.hash = "candidate-block".to_string();
        block
    }

    fn non_coinbase_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "missing-input".to_string(),
                    index: 0,
                },
                public_key: "not-a-public-key".to_string(),
                signature: "not-a-signature".to_string(),
            }],
            outputs: vec![TxOutput {
                address: "receiver".to_string(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn assert_invalid_block_contains(result: Result<(), PulseError>, expected: &str) {
        match result {
            Err(PulseError::InvalidBlock(message)) => assert!(
                message.contains(expected),
                "expected invalid block message containing '{expected}', got '{message}'"
            ),
            other => panic!("expected InvalidBlock containing '{expected}', got {other:?}"),
        }
    }

    #[test]
    fn validate_block_rejects_block_with_no_parents() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.parents.clear();

        assert_invalid_block_contains(validate_block(&block, &state), "no parents");
    }

    #[test]
    fn validate_block_rejects_duplicate_parent() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.parents.push(state.dag.genesis_hash.clone());

        assert_invalid_block_contains(validate_block(&block, &state), "duplicate parent");
    }

    #[test]
    fn validate_block_rejects_missing_parent() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.parents = vec!["missing-parent".to_string()];

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "missing parent missing-parent",
        );
    }

    #[test]
    fn validate_block_rejects_invalid_height() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.height = 2;

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "invalid height 2, expected 1",
        );
    }

    #[test]
    fn validate_block_rejects_zero_timestamp() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.timestamp = 0;

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "timestamp must be greater than zero",
        );
    }

    #[test]
    fn validate_block_rejects_timestamp_older_than_newest_parent() {
        let mut state = init_chain_state("test".to_string());
        let mut parent = structurally_valid_block(&state);
        parent.hash = "parent-block".to_string();
        parent.header.timestamp = 100;
        state.dag.blocks.insert(parent.hash.clone(), parent.clone());

        let mut block = build_candidate_block(vec![parent.hash], 2, 1, vec![coinbase(2)]);
        block.hash = "older-than-parent".to_string();
        block.header.timestamp = 99;

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "older than newest parent 100",
        );
    }

    #[test]
    fn validate_block_rejects_timestamp_too_far_in_future() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.timestamp = current_ts()
            .saturating_add(crate::dev_max_future_drift_secs())
            .saturating_add(1);

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "timestamp too far in the future",
        );
    }

    #[test]
    fn validate_block_rejects_empty_block() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions.clear();

        assert_invalid_block_contains(validate_block(&block, &state), "empty block");
    }

    #[test]
    fn validate_block_rejects_when_first_transaction_is_not_coinbase() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions = vec![non_coinbase_tx("regular-tx")];

        assert_invalid_block_contains(validate_block(&block, &state), "first tx must be coinbase");
    }

    #[test]
    fn validate_block_rejects_multiple_coinbase_transactions() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions = vec![coinbase(1), coinbase(2)];

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "multiple coinbase transactions",
        );
    }

    #[test]
    fn validate_block_rejects_duplicate_transaction_in_block() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        let duplicated_txid = block.transactions[0].txid.clone();
        block.transactions.push(non_coinbase_tx(&duplicated_txid));

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "duplicate transaction in block",
        );
    }

    #[test]
    fn validate_block_accepts_valid_multi_parent_block() {
        let mut state = init_chain_state("test".to_string());
        let mut parent = structurally_valid_block(&state);
        parent.hash = "parent-block".to_string();
        parent.header.timestamp = current_ts();
        state.dag.blocks.insert(parent.hash.clone(), parent.clone());

        let parents = vec![state.dag.genesis_hash.clone(), parent.hash.clone()];
        let mut block = build_candidate_block(parents, 2, 1, vec![coinbase(2)]);
        block.hash = "multi-parent-block".to_string();
        block.header.timestamp = parent.header.timestamp;

        assert!(validate_block(&block, &state).is_ok());
    }

    #[test]
    fn validate_block_accepts_well_formed_coinbase_block() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.hash = "block-1".to_string();

        assert!(validate_block(&block, &state).is_ok());
    }

    #[test]
    fn validate_block_keeps_pow_checks_outside_structural_validation() {
        let state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, u32::MAX, txs);
        block.hash = "pow-agnostic-structural-check".to_string();
        block.header.nonce = 0;

        assert!(
            validate_block(&block, &state).is_ok(),
            "structural validation should remain independent from PoW engine evaluation"
        );
    }
}
