use std::collections::BTreeSet;

use crate::{
    apply::apply_transaction,
    errors::{
        InvalidStateRootClassification, InvalidStateRootDiagnostics, InvalidStateRootError,
        PulseError,
    },
    expected_difficulty,
    mining::{current_ts, is_coinbase},
    selection::sorted_tip_hashes,
    state::ChainState,
    tx::{compute_txid, verify_transaction_signatures},
    types::{compute_block_hash, compute_merkle_root, Block, OutPoint, Transaction},
    validate_pow_header,
};

pub const INITIAL_BLOCK_SUBSIDY: u64 = 50;
pub const SUBSIDY_HALVING_INTERVAL: u64 = 210_000;

pub fn block_subsidy(height: u64) -> u64 {
    let halvings = height / SUBSIDY_HALVING_INTERVAL;
    if halvings >= u64::BITS as u64 {
        0
    } else {
        INITIAL_BLOCK_SUBSIDY >> halvings
    }
}

pub fn transaction_output_sum(tx: &Transaction) -> Result<u64, PulseError> {
    tx.outputs.iter().try_fold(0_u64, |acc, output| {
        acc.checked_add(output.amount)
            .ok_or(PulseError::RewardOverflow)
    })
}

pub fn total_block_fees(block: &Block) -> Result<u64, PulseError> {
    block
        .transactions
        .iter()
        .filter(|tx| !is_coinbase(tx))
        .try_fold(0_u64, |acc, tx| {
            acc.checked_add(tx.fee).ok_or(PulseError::RewardOverflow)
        })
}

fn validate_coinbase_position(block: &Block) -> Result<(), PulseError> {
    let coinbase_positions = block
        .transactions
        .iter()
        .enumerate()
        .filter_map(|(index, tx)| is_coinbase(tx).then_some(index))
        .collect::<Vec<_>>();

    match coinbase_positions.as_slice() {
        [] => Err(PulseError::MissingCoinbase),
        [0] => Ok(()),
        [_] => Err(PulseError::CoinbaseNotFirst),
        _ => Err(PulseError::MultipleCoinbase),
    }
}

fn outpoint_label(outpoint: &OutPoint) -> String {
    format!("{}:{}", outpoint.txid, outpoint.index)
}

pub fn validate_created_utxo_outpoints(
    block: &Block,
    state: &ChainState,
) -> Result<(), PulseError> {
    let mut created = BTreeSet::new();
    for tx in &block.transactions {
        for (index, _) in tx.outputs.iter().enumerate() {
            let outpoint = OutPoint {
                txid: tx.txid.clone(),
                index: index as u32,
            };
            if state.utxo.utxos.contains_key(&outpoint) || !created.insert(outpoint.clone()) {
                return Err(PulseError::DuplicateUtxoOutpoint(outpoint_label(&outpoint)));
            }
        }
    }
    Ok(())
}

pub fn validate_coinbase_reward(block: &Block) -> Result<(), PulseError> {
    let coinbase = block
        .transactions
        .first()
        .ok_or(PulseError::MissingCoinbase)?;
    let coinbase_outputs = transaction_output_sum(coinbase)?;
    let fees = total_block_fees(block)?;
    let max_reward = block_subsidy(block.header.height)
        .checked_add(fees)
        .ok_or(PulseError::RewardOverflow)?;
    if coinbase_outputs > max_reward {
        return Err(PulseError::ExcessiveCoinbaseReward);
    }
    Ok(())
}

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
    let expected_difficulty = expected_difficulty(state);
    if block.header.difficulty != expected_difficulty {
        return Err(PulseError::InvalidBlock(format!(
            "invalid consensus difficulty {}, expected {}",
            block.header.difficulty, expected_difficulty
        )));
    }
    validate_pow_header(&block.header).map_err(|reason| {
        PulseError::InvalidBlock(format!(
            "invalid proof of work for expected difficulty {}: {}",
            expected_difficulty,
            reason.code()
        ))
    })?;
    if block.header.timestamp < newest_parent_timestamp {
        return Err(PulseError::InvalidBlock(format!(
            "timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }
    validate_coinbase_position(block)?;
    let computed_hash = compute_block_hash(&block.header);
    if computed_hash != block.hash {
        return Err(PulseError::InvalidBlock(format!(
            "block hash mismatch: supplied {}, computed {}",
            block.hash, computed_hash
        )));
    }
    let mut seen_txids = BTreeSet::new();
    for tx in &block.transactions {
        if !seen_txids.insert(tx.txid.clone()) {
            return Err(PulseError::InvalidBlock(
                "duplicate transaction in block".into(),
            ));
        }
    }
    let computed_merkle_root = compute_merkle_root(&block.transactions);
    if computed_merkle_root != block.header.merkle_root {
        return Err(PulseError::InvalidBlock(format!(
            "merkle root mismatch: supplied {}, computed {}",
            block.header.merkle_root, computed_merkle_root
        )));
    }
    for tx in &block.transactions {
        if compute_txid(tx) != tx.txid {
            return Err(PulseError::InvalidTxid);
        }
    }
    validate_created_utxo_outpoints(block, state)?;
    validate_coinbase_reward(block)?;

    let computed_state_root = compute_post_state_root(block, state)?;
    if computed_state_root != block.header.state_root {
        let diagnostics = invalid_state_root_diagnostics(block, state, computed_state_root);
        return Err(invalid_state_root_error(diagnostics));
    }
    Ok(())
}

pub fn invalid_state_root_diagnostics(
    block: &Block,
    state: &ChainState,
    computed_state_root: String,
) -> InvalidStateRootDiagnostics {
    let current_tips = sorted_tip_hashes(state);
    let selected_tip = current_tips.first().cloned();
    let selected_tip_height = selected_tip
        .as_ref()
        .and_then(|hash| state.dag.blocks.get(hash))
        .map(|block| block.header.height);
    let parent_hashes = block.header.parents.clone();
    let stale_template = !parent_hashes.is_empty()
        && (parent_hashes
            .iter()
            .any(|parent| !state.dag.tips.contains(parent))
            || selected_tip
                .as_ref()
                .is_some_and(|tip| !parent_hashes.contains(tip))
            || selected_tip_height.is_some_and(|tip_height| block.header.height <= tip_height));
    let unknown_context = parent_hashes.is_empty()
        || selected_tip.is_none()
        || parent_hashes
            .iter()
            .any(|parent| !state.dag.blocks.contains_key(parent));
    let classification = if unknown_context {
        InvalidStateRootClassification::UnknownContext
    } else if stale_template {
        InvalidStateRootClassification::StaleTemplate
    } else {
        InvalidStateRootClassification::TrueInvalid
    };

    InvalidStateRootDiagnostics {
        block_hash: block.hash.clone(),
        height: block.header.height,
        parent_hashes,
        supplied_state_root: block.header.state_root.clone(),
        computed_state_root,
        tx_count: block.transactions.len(),
        coinbase_miner_address: block
            .transactions
            .first()
            .and_then(|tx| tx.outputs.first())
            .map(|output| output.address.clone()),
        selected_tip,
        selected_tip_height,
        current_tips,
        stale_template,
        unknown_context,
        classification,
    }
}

pub fn invalid_state_root_error(diagnostics: InvalidStateRootDiagnostics) -> PulseError {
    PulseError::InvalidStateRoot(Box::new(InvalidStateRootError::new(diagnostics)))
}

pub fn compute_post_state_root(block: &Block, state: &ChainState) -> Result<String, PulseError> {
    let mut working = state.clone();
    working.mempool.transactions.clear();
    working.mempool.spent_outpoints.clear();

    let Some(coinbase) = block.transactions.first() else {
        return Err(PulseError::InvalidBlock("empty block".into()));
    };
    apply_transaction(coinbase, &mut working, block.header.height)?;

    for tx in block.transactions.iter().skip(1) {
        validate_transaction(tx, &working)?;
        apply_transaction(tx, &mut working, block.header.height)?;
    }
    working.utxo.compute_state_root()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accept::{accept_transaction_with_result, AcceptSource, TxAcceptanceResult},
        apply::apply_block,
        genesis::{genesis_transaction, init_chain_state},
        mining::{
            build_candidate_block, build_coinbase_transaction, refresh_block_consensus_ids,
            refresh_block_consensus_ids_with_state,
        },
        tx::{address_from_public_key, compute_txid, signing_message},
        types::{TxInput, TxOutput, Utxo},
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn coinbase(nonce: u64) -> Transaction {
        build_coinbase_transaction("miner1", 50, nonce)
    }

    fn structurally_valid_block(state: &ChainState) -> Block {
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(parents, 1, 1, vec![coinbase(1)]);
        refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
        block
    }

    fn append_header_only_block(
        state: &mut ChainState,
        height: u64,
        timestamp: u64,
        difficulty: u32,
    ) {
        let parent = if height == 1 {
            state.dag.genesis_hash.clone()
        } else {
            format!("consensus-parent-{}", height - 1)
        };
        let hash = format!("consensus-parent-{height}");
        state.dag.blocks.insert(
            hash.clone(),
            Block {
                hash: hash.clone(),
                header: crate::types::BlockHeader {
                    version: 1,
                    parents: vec![parent],
                    timestamp,
                    difficulty,
                    nonce: 0,
                    merkle_root: format!("merkle-{height}"),
                    state_root: format!("state-{height}"),
                    blue_score: height,
                    height,
                },
                transactions: Vec::new(),
            },
        );
        state.dag.best_height = height;
        state.dag.tips.clear();
        state.dag.tips.insert(hash);
    }

    fn state_with_tip_difficulty(difficulty: u32) -> ChainState {
        let mut state = init_chain_state("test".to_string());
        let start = current_ts().saturating_sub(30 * 60);
        for height in 1..=25 {
            append_header_only_block(&mut state, height, start + height * 60, difficulty);
        }
        state
    }

    fn mined_next_block(state: &ChainState, difficulty: u32, nonce: u64) -> Block {
        let mut parents = state.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let height = state.dag.best_height + 1;
        let mut block = build_candidate_block(parents, height, difficulty, vec![coinbase(nonce)]);
        refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
        let (header, mined, _, _) = crate::dev_mine_header(block.header.clone(), 200_000);
        assert!(
            mined,
            "expected test block to mine at difficulty {difficulty}"
        );
        block.header = header;
        refresh_block_consensus_ids(&mut block);
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

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public_key_hex(signing_key: &SigningKey) -> String {
        hex::encode(signing_key.verifying_key().to_bytes())
    }

    fn fund_address(
        state: &mut ChainState,
        txid: &str,
        index: u32,
        address: String,
        amount: u64,
    ) -> OutPoint {
        let outpoint = OutPoint {
            txid: txid.to_string(),
            index,
        };
        let utxo = Utxo {
            outpoint: outpoint.clone(),
            address: address.clone(),
            amount,
            coinbase: false,
            height: 1,
        };
        state.utxo.utxos.insert(outpoint.clone(), utxo);
        state
            .utxo
            .address_index
            .entry(address)
            .or_default()
            .push(outpoint.clone());
        outpoint
    }

    fn fund_signing_key(
        state: &mut ChainState,
        seed: u8,
        txid: &str,
        index: u32,
        amount: u64,
    ) -> (SigningKey, OutPoint) {
        let key = signing_key(seed);
        let address = address_from_public_key(&public_key_hex(&key));
        let outpoint = fund_address(state, txid, index, address, amount);
        (key, outpoint)
    }

    fn signed_tx(
        signing_key: &SigningKey,
        previous_outputs: Vec<OutPoint>,
        outputs: Vec<TxOutput>,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let public_key = public_key_hex(signing_key);
        let mut tx = Transaction {
            txid: String::new(),
            version: 1,
            inputs: previous_outputs
                .into_iter()
                .map(|previous_output| TxInput {
                    previous_output,
                    public_key: public_key.clone(),
                    signature: String::new(),
                })
                .collect(),
            outputs,
            fee,
            nonce,
        };

        let message = signing_message(&tx);
        let signature = signing_key.sign(&message);
        let signature_hex = hex::encode(signature.to_bytes());
        for input in &mut tx.inputs {
            input.signature = signature_hex.clone();
        }
        tx.txid = compute_txid(&tx);
        tx
    }

    fn output(address: &str, amount: u64) -> TxOutput {
        TxOutput {
            address: address.to_string(),
            amount,
        }
    }

    fn assert_invalid_transaction_contains(result: Result<(), PulseError>, expected: &str) {
        match result {
            Err(PulseError::InvalidTransaction(message)) => assert!(
                message.contains(expected),
                "expected invalid transaction message containing '{expected}', got '{message}'"
            ),
            other => panic!("expected InvalidTransaction containing '{expected}', got {other:?}"),
        }
    }

    fn assert_validation_error(result: Result<(), PulseError>, expected: fn(&PulseError) -> bool) {
        match result {
            Err(err) if expected(&err) => {}
            other => panic!("unexpected validation result: {other:?}"),
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
    fn transaction_validation_rejects_transaction_with_no_outputs() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 1, "fund-no-outputs", 0, 10);
        let tx = signed_tx(&key, vec![outpoint], vec![], 0, 1);

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "no outputs");
    }

    #[test]
    fn transaction_validation_rejects_transaction_with_no_inputs() {
        let state = init_chain_state("test".to_string());
        let mut tx = Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![],
            outputs: vec![output("receiver", 1)],
            fee: 0,
            nonce: 1,
        };
        tx.txid = compute_txid(&tx);

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "no inputs");
    }

    #[test]
    fn transaction_validation_rejects_zero_value_output() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 2, "fund-zero-output", 0, 10);
        let tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 0)], 0, 1);

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "zero-value output");
    }

    #[test]
    fn transaction_validation_rejects_duplicate_input() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 3, "fund-duplicate-input", 0, 10);
        let tx = signed_tx(
            &key,
            vec![outpoint.clone(), outpoint],
            vec![output("receiver", 5)],
            0,
            1,
        );

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "duplicate input");
    }

    #[test]
    fn transaction_validation_rejects_invalid_txid() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 4, "fund-invalid-txid", 0, 10);
        let mut tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 5)], 0, 1);
        tx.txid = "not-the-canonical-txid".to_string();

        assert_validation_error(validate_transaction(&tx, &state), |err| {
            matches!(err, PulseError::InvalidTxid)
        });
    }

    #[test]
    fn transaction_validation_classifies_missing_utxo_as_orphan_missing_input() {
        let mut state = init_chain_state("test".to_string());
        let key = signing_key(5);
        let missing = OutPoint {
            txid: "missing-parent-tx".to_string(),
            index: 7,
        };
        let tx = signed_tx(
            &key,
            vec![missing.clone()],
            vec![output("receiver", 1)],
            0,
            1,
        );
        let txid = tx.txid.clone();

        assert_validation_error(validate_transaction(&tx, &state), |err| {
            matches!(err, PulseError::UtxoNotFound)
        });
        assert_eq!(
            accept_transaction_with_result(tx, &mut state, AcceptSource::P2p),
            TxAcceptanceResult::Orphan
        );
        assert_eq!(
            state.mempool.orphan_missing_outpoints.get(&txid),
            Some(&vec![missing])
        );
    }

    #[test]
    fn transaction_validation_rejects_double_spend_against_mempool_spent_outpoints() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 6, "fund-double-spend", 0, 10);
        state.mempool.spent_outpoints.insert(outpoint.clone());
        let tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 5)], 0, 1);

        assert_validation_error(validate_transaction(&tx, &state), |err| {
            matches!(err, PulseError::DoubleSpend)
        });
    }

    #[test]
    fn transaction_validation_rejects_insufficient_funds() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 7, "fund-insufficient", 0, 10);
        let tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 11)], 0, 1);

        assert_validation_error(validate_transaction(&tx, &state), |err| {
            matches!(err, PulseError::InsufficientFunds)
        });
    }

    #[test]
    fn transaction_validation_rejects_input_overflow() {
        let mut state = init_chain_state("test".to_string());
        let key = signing_key(8);
        let address = address_from_public_key(&public_key_hex(&key));
        let first = fund_address(
            &mut state,
            "fund-input-overflow-a",
            0,
            address.clone(),
            u64::MAX,
        );
        let second = fund_address(&mut state, "fund-input-overflow-b", 0, address, u64::MAX);
        let tx = signed_tx(&key, vec![first, second], vec![output("receiver", 1)], 0, 1);

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "input overflow");
    }

    #[test]
    fn transaction_validation_rejects_output_plus_fee_overflow() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 9, "fund-output-overflow", 0, u64::MAX);
        let tx = signed_tx(
            &key,
            vec![outpoint],
            vec![output("receiver", u64::MAX)],
            1,
            1,
        );

        assert_invalid_transaction_contains(validate_transaction(&tx, &state), "output overflow");
    }

    #[test]
    fn transaction_validation_rejects_invalid_signature() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 10, "fund-invalid-signature", 0, 10);
        let mut tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 5)], 0, 1);
        tx.inputs[0].signature = hex::encode([0_u8; 64]);
        tx.txid = compute_txid(&tx);

        assert_validation_error(validate_transaction(&tx, &state), |err| {
            matches!(err, PulseError::InvalidSignature)
        });
    }

    #[test]
    fn transaction_validation_accepts_valid_signed_transaction() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 11, "fund-valid", 0, 10);
        let tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 9)], 1, 1);
        let txid = tx.txid.clone();

        assert!(validate_transaction(&tx, &state).is_ok());
        assert_eq!(
            accept_transaction_with_result(tx, &mut state, AcceptSource::Rpc),
            TxAcceptanceResult::Accepted
        );
        assert!(state.mempool.transactions.contains_key(&txid));
    }

    #[test]
    fn transaction_validation_missing_transaction_inputs_returns_exact_missing_outpoints() {
        let mut state = init_chain_state("test".to_string());
        let (_key, present) = fund_signing_key(&mut state, 12, "fund-present", 0, 10);
        let missing_a = OutPoint {
            txid: "missing-a".to_string(),
            index: 0,
        };
        let missing_b = OutPoint {
            txid: "missing-b".to_string(),
            index: 2,
        };
        let tx = Transaction {
            txid: "irrelevant".to_string(),
            version: 1,
            inputs: vec![present, missing_a.clone(), missing_b.clone()]
                .into_iter()
                .map(|previous_output| TxInput {
                    previous_output,
                    public_key: String::new(),
                    signature: String::new(),
                })
                .collect(),
            outputs: vec![output("receiver", 1)],
            fee: 0,
            nonce: 1,
        };

        assert_eq!(
            missing_transaction_inputs(&tx, &state),
            vec![missing_a, missing_b]
        );
    }

    #[test]
    fn transaction_validation_tx_output_amount_resolves_utxo_and_mempool_parent_outputs() {
        let mut state = init_chain_state("test".to_string());
        let (key, parent_input) = fund_signing_key(&mut state, 13, "fund-parent", 0, 50);
        let parent = signed_tx(
            &key,
            vec![parent_input.clone()],
            vec![output("child-spendable", 30), output("change", 19)],
            1,
            1,
        );
        let parent_txid = parent.txid.clone();
        state
            .mempool
            .transactions
            .insert(parent_txid.clone(), parent);

        assert_eq!(tx_output_amount(&state, &parent_input), Some(50));
        assert_eq!(
            tx_output_amount(
                &state,
                &OutPoint {
                    txid: parent_txid.clone(),
                    index: 0,
                },
            ),
            Some(30)
        );
        assert_eq!(
            tx_output_amount(
                &state,
                &OutPoint {
                    txid: parent_txid,
                    index: 1,
                },
            ),
            Some(19)
        );
    }

    #[test]
    fn validate_block_accepts_expected_consensus_difficulty() {
        let state = state_with_tip_difficulty(2);
        assert_eq!(crate::expected_difficulty(&state), 2);
        let block = mined_next_block(&state, 2, 90);

        validate_block(&block, &state).expect("expected consensus difficulty block to validate");
    }

    #[test]
    fn validate_block_rejects_lower_than_expected_difficulty() {
        let state = state_with_tip_difficulty(2);
        let block = mined_next_block(&state, 1, 91);

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "invalid consensus difficulty 1, expected 2",
        );
    }

    #[test]
    fn validate_block_rejects_stale_template_difficulty() {
        let state = state_with_tip_difficulty(4);
        let stale_template_difficulty = 2;
        let block = mined_next_block(&state, stale_template_difficulty, 92);

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "invalid consensus difficulty 2, expected 4",
        );
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
    fn validate_block_rejects_missing_coinbase() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions = vec![non_coinbase_tx("regular-tx")];

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::MissingCoinbase)
        });
    }

    #[test]
    fn validate_block_rejects_empty_block_as_missing_coinbase() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions.clear();

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::MissingCoinbase)
        });
    }

    #[test]
    fn validate_block_rejects_when_coinbase_is_not_first() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions = vec![non_coinbase_tx("regular-tx"), coinbase(2)];

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::CoinbaseNotFirst)
        });
    }

    #[test]
    fn validate_block_rejects_multiple_coinbase_transactions() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions = vec![coinbase(1), coinbase(2)];

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::MultipleCoinbase)
        });
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
    fn validate_block_rejects_fake_block_hash() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.hash = "not-the-canonical-block-hash".to_string();

        assert_invalid_block_contains(validate_block(&block, &state), "block hash mismatch");
    }

    #[test]
    fn validate_block_rejects_fake_merkle_root() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.merkle_root = "not-the-canonical-merkle-root".to_string();
        block.hash = compute_block_hash(&block.header);

        assert_invalid_block_contains(validate_block(&block, &state), "merkle root mismatch");
    }

    #[test]
    fn validate_block_rejects_fake_coinbase_txid() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.transactions[0].txid = "not-the-canonical-txid".to_string();
        refresh_block_consensus_ids(&mut block);

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::InvalidTxid)
        });
    }

    #[test]
    fn validate_block_rejects_wrong_state_root() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.state_root = "wrong-state-root".to_string();
        refresh_block_consensus_ids(&mut block);

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::InvalidStateRoot(_))
        });
    }

    #[test]
    fn deterministic_replay_same_block_and_parent_state_computes_same_state_root() {
        let base = init_chain_state("test".to_string());
        let parent = structurally_valid_block(&base);

        let mut node_a = base.clone();
        let mut node_b = base;
        apply_block(&parent, &mut node_a).unwrap();
        apply_block(&parent, &mut node_b).unwrap();

        let parents = vec![parent.hash.clone()];
        let mut block = build_candidate_block(parents, 2, 1, vec![coinbase(22)]);
        refresh_block_consensus_ids_with_state(&mut block, &node_a).unwrap();

        let node_a_root = compute_post_state_root(&block, &node_a).unwrap();
        let node_b_root = compute_post_state_root(&block, &node_b).unwrap();

        assert_eq!(node_a_root, node_b_root);
        assert_eq!(block.header.state_root, node_a_root);
        assert!(validate_block(&block, &node_a).is_ok());
        assert!(validate_block(&block, &node_b).is_ok());
    }

    #[test]
    fn stale_template_invalid_state_root_is_classified_clearly() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];

        let mut stale_template_block =
            build_candidate_block(parents.clone(), 1, 1, vec![coinbase(31)]);
        refresh_block_consensus_ids_with_state(&mut stale_template_block, &state).unwrap();

        let mut competing_tip = build_candidate_block(parents, 1, 1, vec![coinbase(32)]);
        refresh_block_consensus_ids_with_state(&mut competing_tip, &state).unwrap();
        apply_block(&competing_tip, &mut state).unwrap();

        match validate_block(&stale_template_block, &state) {
            Err(PulseError::InvalidStateRoot(error)) => {
                let diagnostics = &error.diagnostics;
                assert_eq!(
                    diagnostics.classification,
                    InvalidStateRootClassification::StaleTemplate
                );
                assert!(diagnostics.stale_template);
                assert!(!diagnostics.unknown_context);
                assert_eq!(diagnostics.block_hash, stale_template_block.hash);
                assert_eq!(diagnostics.height, stale_template_block.header.height);
                assert_eq!(
                    diagnostics.parent_hashes,
                    stale_template_block.header.parents
                );
                assert_eq!(
                    diagnostics.supplied_state_root,
                    stale_template_block.header.state_root
                );
                assert_eq!(
                    diagnostics.tx_count,
                    stale_template_block.transactions.len()
                );
                assert_eq!(
                    diagnostics.coinbase_miner_address.as_deref(),
                    Some("miner1")
                );
                assert_eq!(
                    diagnostics.selected_tip.as_deref(),
                    Some(competing_tip.hash.as_str())
                );
            }
            other => panic!("expected stale-template invalid state root, got {other:?}"),
        }
    }

    #[test]
    fn failed_block_application_leaves_state_unchanged() {
        let mut state = init_chain_state("test".to_string());
        let before_root = state.utxo.compute_state_root().unwrap();
        let before_tips = state.dag.tips.clone();
        let before_block_count = state.dag.blocks.len();
        let mut block = structurally_valid_block(&state);
        block.header.state_root = "wrong-state-root".to_string();
        refresh_block_consensus_ids(&mut block);

        assert!(matches!(
            apply_block(&block, &mut state),
            Err(PulseError::InvalidStateRoot(_))
        ));

        assert_eq!(before_root, state.utxo.compute_state_root().unwrap());
        assert_eq!(before_tips, state.dag.tips);
        assert_eq!(before_block_count, state.dag.blocks.len());
    }

    #[test]
    fn validate_block_rejects_duplicate_outpoint_outputs() {
        let state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(parents, 1, 1, vec![genesis_transaction()]);
        refresh_block_consensus_ids(&mut block);

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::DuplicateUtxoOutpoint(_))
        });
    }

    #[test]
    fn validate_block_rejects_excessive_coinbase_reward() {
        let state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(
            parents,
            1,
            1,
            vec![build_coinbase_transaction(
                "miner1",
                block_subsidy(1) + 1,
                101,
            )],
        );
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::ExcessiveCoinbaseReward)
        });
    }

    #[test]
    fn validate_block_accepts_coinbase_reward_with_fees() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 14, "fund-fee-reward", 0, 100);
        let fee_tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 80)], 20, 1);
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(
            parents,
            1,
            1,
            vec![
                build_coinbase_transaction("miner1", block_subsidy(1) + 20, 102),
                fee_tx,
            ],
        );
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        validate_block(&block, &state).expect("fee-inclusive reward should validate");
    }

    #[test]
    fn validate_block_rejects_coinbase_reward_above_subsidy_plus_fees() {
        let mut state = init_chain_state("test".to_string());
        let (key, outpoint) = fund_signing_key(&mut state, 15, "fund-excess-fee-reward", 0, 100);
        let fee_tx = signed_tx(&key, vec![outpoint], vec![output("receiver", 80)], 20, 1);
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(
            parents,
            1,
            1,
            vec![
                build_coinbase_transaction("miner1", block_subsidy(1) + 21, 103),
                fee_tx,
            ],
        );
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::ExcessiveCoinbaseReward)
        });
    }

    #[test]
    fn validate_block_rejects_coinbase_output_sum_overflow() {
        let state = init_chain_state("test".to_string());
        let mut overflowing_coinbase = Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![],
            outputs: vec![output("miner-a", u64::MAX), output("miner-b", 1)],
            fee: 0,
            nonce: 104,
        };
        overflowing_coinbase.txid = compute_txid(&overflowing_coinbase);
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block = build_candidate_block(parents, 1, 1, vec![overflowing_coinbase]);
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::RewardOverflow)
        });
    }

    #[test]
    fn validate_block_rejects_total_fee_overflow() {
        let state = init_chain_state("test".to_string());
        let mut high_fee = non_coinbase_tx("placeholder-high-fee");
        high_fee.fee = u64::MAX;
        high_fee.txid = compute_txid(&high_fee);
        let mut overflow_fee = non_coinbase_tx("placeholder-overflow-fee");
        overflow_fee.fee = 1;
        overflow_fee.nonce = 1;
        overflow_fee.txid = compute_txid(&overflow_fee);
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut block =
            build_candidate_block(parents, 1, 1, vec![coinbase(105), high_fee, overflow_fee]);
        refresh_block_consensus_ids(&mut block);

        assert_validation_error(validate_block(&block, &state), |err| {
            matches!(err, PulseError::RewardOverflow)
        });
    }

    #[test]
    fn apply_transaction_rejects_utxo_overwrite_without_mutating_state() {
        let mut state = init_chain_state("test".to_string());
        let before_root = state.utxo.compute_state_root().unwrap();
        let duplicate = genesis_transaction();

        assert_validation_error(apply_transaction(&duplicate, &mut state, 1), |err| {
            matches!(err, PulseError::DuplicateUtxoOutpoint(_))
        });
        assert_eq!(before_root, state.utxo.compute_state_root().unwrap());
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
        block.header.timestamp = parent.header.timestamp;
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        assert!(validate_block(&block, &state).is_ok());
    }

    #[test]
    fn validate_block_accepts_well_formed_coinbase_block() {
        let state = init_chain_state("test".to_string());
        let block = structurally_valid_block(&state);

        assert!(validate_block(&block, &state).is_ok());
    }

    #[test]
    fn validate_block_rejects_pow_target_failure_in_consensus() {
        let state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        block.header.difficulty = crate::expected_difficulty(&state);
        block.header.nonce = 0;
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();
        block.header.difficulty = 0x0100_0000;
        refresh_block_consensus_ids(&mut block);

        assert_invalid_block_contains(
            validate_block(&block, &state),
            "invalid consensus difficulty",
        );
    }
}
