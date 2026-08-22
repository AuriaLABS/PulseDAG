use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_activated_v2_mined_block_atomically, accept_activated_v2_p2p_block_atomically,
    accept_transaction_with_result_for_protocol, address_from_public_key,
    build_activated_v2_mining_template, compute_block_hash_v2, compute_submission_id_v2,
    compute_txid_v2, current_ts, genesis::init_chain_state, signing_message_v2,
    validate_pow_for_protocol, AcceptSource, ActivatedV2MiningTemplateSpec, Block,
    BlockAcceptanceResult, ChainState, OutPoint, ProtocolActivationIdentity, Transaction,
    TxAcceptanceResult, TxInput, TxOutput, GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V2,
};

const CHAIN_ID: &str = "task28-multi-node-tx-parity";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn mine_v2_block(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    miner_address: &str,
    coinbase_nonce: u64,
    transactions: Vec<Transaction>,
) -> Block {
    let template = build_activated_v2_mining_template(
        state,
        identity,
        ActivatedV2MiningTemplateSpec {
            miner_address: miner_address.to_string(),
            timestamp: current_ts(),
            coinbase_nonce,
            transactions,
        },
    )
    .unwrap();
    let mut block = template.block;
    for nonce in 0..=200_000_u64 {
        block.header.nonce = nonce;
        block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
        if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
            return block;
        }
    }
    panic!("expected Task 28 dev-PoW fixture to find a valid nonce");
}

fn accept_local_mined(state: &mut ChainState, identity: &ProtocolActivationIdentity, block: Block) {
    let accepted = accept_activated_v2_mined_block_atomically(
        block,
        state,
        AcceptSource::LocalMining,
        identity,
        |_, _| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(accepted.result, BlockAcceptanceResult::Accepted);
    assert!(accepted.persisted && accepted.committed && accepted.broadcast);
}

fn accept_from_peer(state: &mut ChainState, identity: &ProtocolActivationIdentity, block: Block) {
    let accepted = accept_activated_v2_p2p_block_atomically(
        block,
        state,
        AcceptSource::P2p,
        identity,
        |_, _| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(accepted.result, BlockAcceptanceResult::Accepted);
    assert!(accepted.persisted && accepted.committed && accepted.broadcast);
}

fn signed_v2_spend(
    signing_key: &SigningKey,
    previous_output: OutPoint,
    input_amount: u64,
    nonce: u64,
) -> Transaction {
    assert!(input_amount > 1);
    let public_key = public_key_hex(signing_key);
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1task28multinoderecipient".to_string(),
            amount: input_amount - 1,
        }],
        fee: 1,
        nonce,
    };
    let message = signing_message_v2(&tx, CHAIN_ID).unwrap();
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, CHAIN_ID).unwrap();
    tx
}

fn assert_final_parity(states: [&ChainState; 3], block: &Block, tx: &Transaction) {
    let expected_state_root = states[0].utxo.compute_state_root().unwrap();
    assert_eq!(expected_state_root, block.header.state_root);

    let recipient_outpoint = OutPoint {
        txid: tx.txid.clone(),
        index: 0,
    };

    for state in states {
        assert_eq!(state.chain_id, CHAIN_ID);
        assert_eq!(
            state.dag.ordered_dag_tip.as_deref(),
            Some(block.hash.as_str())
        );
        assert_eq!(
            state.dag.selected_chain.last().map(String::as_str),
            Some(block.hash.as_str())
        );
        assert_eq!(
            state.dag.ordered_dag_state_root.as_deref(),
            Some(block.header.state_root.as_str())
        );
        assert_eq!(
            state.utxo.compute_state_root().unwrap(),
            expected_state_root
        );
        assert!(state.utxo.utxos.contains_key(&recipient_outpoint));
        assert!(!state.mempool.transactions.contains_key(&tx.txid));
        assert!(!state
            .mempool
            .spent_outpoints
            .contains(&tx.inputs[0].previous_output));

        let committed = state.dag.blocks.get(&block.hash).unwrap();
        assert!(committed
            .transactions
            .iter()
            .any(|candidate| candidate.txid == tx.txid));
    }
}

#[test]
fn propagated_v2_transaction_reaches_canonical_parity_across_three_nodes() {
    let mut node_a = init_chain_state(CHAIN_ID.to_string());
    let mut node_b = init_chain_state(CHAIN_ID.to_string());
    let mut node_c = init_chain_state(CHAIN_ID.to_string());
    let identity = activated_identity(&node_a);

    assert_eq!(identity, activated_identity(&node_b));
    assert_eq!(identity, activated_identity(&node_c));

    // Establish a canonical spendable UTXO through the activated-v2 block
    // boundaries on every independent node. This deliberately avoids injecting
    // test-only UTXO state that authoritative ordered-DAG replay would discard.
    let source_key = signing_key(41);
    let source_address = address_from_public_key(&public_key_hex(&source_key));
    let funding_block = mine_v2_block(&node_a, &identity, &source_address, 1, Vec::new());
    accept_local_mined(&mut node_a, &identity, funding_block.clone());
    accept_from_peer(&mut node_b, &identity, funding_block.clone());
    accept_from_peer(&mut node_c, &identity, funding_block.clone());

    let funding_tx = &funding_block.transactions[0];
    let funding_output = &funding_tx.outputs[0];
    assert_eq!(funding_output.address, source_address);
    let funding_outpoint = OutPoint {
        txid: funding_tx.txid.clone(),
        index: 0,
    };

    // Submit on node A and relay the exact chain-bound transaction across the
    // P2P admission boundary to B and C. All nodes must agree on canonical txid
    // and stable submission identity before mining.
    let tx = signed_v2_spend(&source_key, funding_outpoint, funding_output.amount, 77);
    let txid = tx.txid.clone();
    let submission_id = compute_submission_id_v2(&tx, CHAIN_ID).unwrap();
    assert_ne!(submission_id, txid);

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            tx.clone(),
            &mut node_a,
            AcceptSource::Rpc,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert_eq!(
        accept_transaction_with_result_for_protocol(
            tx.clone(),
            &mut node_b,
            AcceptSource::P2p,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert_eq!(
        accept_transaction_with_result_for_protocol(
            tx.clone(),
            &mut node_c,
            AcceptSource::P2p,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );

    for state in [&node_a, &node_b, &node_c] {
        let propagated = state.mempool.transactions.get(&txid).unwrap();
        assert_eq!(compute_txid_v2(propagated, CHAIN_ID).unwrap(), txid);
        assert_eq!(
            compute_submission_id_v2(propagated, CHAIN_ID).unwrap(),
            submission_id
        );
    }

    // Mine the propagated tx on A, then deliver the exact block through the
    // activated-v2 P2P acceptance path on B/C. Canonical state, inclusion and
    // mempool cleanup must converge identically on all three nodes.
    let tx_block = mine_v2_block(
        &node_a,
        &identity,
        "pulse1task28multinodeminer",
        2,
        vec![tx.clone()],
    );
    assert!(tx_block
        .transactions
        .iter()
        .any(|candidate| candidate.txid == txid));

    accept_local_mined(&mut node_a, &identity, tx_block.clone());
    accept_from_peer(&mut node_b, &identity, tx_block.clone());
    accept_from_peer(&mut node_c, &identity, tx_block.clone());

    assert_final_parity([&node_a, &node_b, &node_c], &tx_block, &tx);
}
