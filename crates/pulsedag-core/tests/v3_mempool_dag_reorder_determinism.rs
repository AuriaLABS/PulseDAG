use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::retarget::expected_difficulty_for_parent;
use pulsedag_core::validation::block_subsidy;
use pulsedag_core::{
    accept_transaction_for_protocol, address_from_public_key, build_candidate_block_v2,
    build_coinbase_transaction_v2, canonical_mempool_txids, canonicalize_block_parents_v2,
    classify_merge_set_v1, commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2,
    compute_txid_v2, current_ts, drive_activated_v2_p2p_block_atomically,
    prepare_activated_v2_p2p_block_state, rebuild_authoritative_state_v2, signing_message_v2,
    validate_pow_for_protocol, AcceptSource, ActivatedV2P2pRuntime, ActivatedV2P2pRuntimeOutcome,
    Block, CandidateBlockV2Spec, ChainState, Hash, OutPoint, ProtocolActivationIdentity,
    Transaction, TxInput, TxOutput, GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V2,
};

const CHAIN_ID: &str = "v3-mempool-dag-reorder";

fn identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn finalized_block(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    parents: Vec<Hash>,
    coinbase_nonce: u64,
    coinbase_address: &str,
) -> Block {
    let parents = canonicalize_block_parents_v2(&parents).unwrap();
    let height = parents
        .iter()
        .map(|parent| state.dag.blocks[parent].header.height.saturating_add(1))
        .max()
        .unwrap();
    let timestamp = parents
        .iter()
        .map(|parent| state.dag.blocks[parent].header.timestamp)
        .max()
        .unwrap()
        .max(current_ts().saturating_sub(1));
    let coinbase = build_coinbase_transaction_v2(
        coinbase_address,
        block_subsidy(height),
        coinbase_nonce,
        &identity.chain_id,
    )
    .unwrap();
    let mut block = build_candidate_block_v2(
        CandidateBlockV2Spec {
            parents,
            timestamp,
            height,
            blue_score: 0,
            difficulty: 1,
            state_root: "00".repeat(32),
        },
        vec![coinbase],
        &identity.chain_id,
    )
    .unwrap();
    let classification = classify_merge_set_v1(&block, state).unwrap();
    block.header.blue_score = classification.blue_score;
    block.header.difficulty =
        expected_difficulty_for_parent(state, classification.selected_parent.as_ref().unwrap())
            .unwrap();
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();

    let mut first_state = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut first_state, identity).unwrap();
    let first = rebuild_authoritative_state_v2(&first_state).unwrap();
    block.header.state_root = first.diagnostics.state_root;
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();

    let mut final_state = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut final_state, identity).unwrap();
    let final_replay = rebuild_authoritative_state_v2(&final_state).unwrap();
    assert_eq!(final_replay.diagnostics.state_root, block.header.state_root);
    assert_eq!(
        final_replay.diagnostics.ordered_dag_tip,
        Some(block.hash.clone())
    );

    for nonce in 0..=200_000_u64 {
        block.header.nonce = nonce;
        block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
        if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
            return block;
        }
    }
    panic!("expected PoW-limit fixture to find a valid nonce");
}

fn signed_spend_of_parent_coinbase(
    state: &ChainState,
    parent: &Block,
    signing_key: &SigningKey,
) -> Transaction {
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let outpoint = OutPoint {
        txid: parent.transactions[0].txid.clone(),
        index: 0,
    };
    let funding = state
        .utxo
        .utxos
        .get(&outpoint)
        .expect("parent coinbase UTXO exists")
        .amount;
    assert!(funding > 1);

    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: outpoint,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1dagreorderrecipient".to_string(),
            amount: funding - 1,
        }],
        fee: 1,
        nonce: 91,
    };
    let message = signing_message_v2(&tx, &state.chain_id).unwrap();
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, &state.chain_id).unwrap();
    tx
}

fn drive(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
) -> pulsedag_core::ActivatedV2P2pDriveResult {
    drive_activated_v2_p2p_block_atomically(
        block,
        state,
        runtime,
        identity,
        |_, _| Ok(()),
        |_, _| Ok(()),
        |_| Ok(()),
    )
    .unwrap()
}

#[test]
fn equivalent_parent_arrival_orders_converge_to_identical_mempool() {
    let base = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let identity = identity(&base);
    let signing_key = SigningKey::from_bytes(&[73; 32]);
    let funding_address =
        address_from_public_key(&hex::encode(signing_key.verifying_key().to_bytes()));
    let genesis = base.dag.genesis_hash.clone();
    let parent = finalized_block(&base, &identity, vec![genesis], 11, &funding_address);

    let mut live = prepare_activated_v2_p2p_block_state(&parent, &base, &identity).unwrap();
    let spend = signed_spend_of_parent_coinbase(&live, &parent, &signing_key);
    accept_transaction_for_protocol(spend.clone(), &mut live, AcceptSource::Rpc, &identity)
        .unwrap();
    let expected_mempool = vec![spend.txid.clone()];
    assert_eq!(canonical_mempool_txids(&live), expected_mempool);

    let child = finalized_block(
        &live,
        &identity,
        vec![parent.hash.clone()],
        12,
        "pulse1dagreorderchild",
    );
    let child_state = prepare_activated_v2_p2p_block_state(&child, &live, &identity).unwrap();
    assert_eq!(canonical_mempool_txids(&child_state), expected_mempool);
    let grandchild = finalized_block(
        &child_state,
        &identity,
        vec![child.hash.clone()],
        13,
        "pulse1dagreordergrandchild",
    );

    let mut ordered = live.clone();
    let mut ordered_runtime = ActivatedV2P2pRuntime::default();
    let first = drive(child.clone(), &mut ordered, &mut ordered_runtime, &identity);
    assert!(matches!(
        first.primary,
        ActivatedV2P2pRuntimeOutcome::Accepted { ref block_hash, .. }
            if block_hash == &child.hash
    ));
    let second = drive(
        grandchild.clone(),
        &mut ordered,
        &mut ordered_runtime,
        &identity,
    );
    assert!(matches!(
        second.primary,
        ActivatedV2P2pRuntimeOutcome::Accepted { ref block_hash, .. }
            if block_hash == &grandchild.hash
    ));

    let mut reordered = live;
    let mut reordered_runtime = ActivatedV2P2pRuntime::default();
    let queued = drive(
        grandchild.clone(),
        &mut reordered,
        &mut reordered_runtime,
        &identity,
    );
    assert!(matches!(
        queued.primary,
        ActivatedV2P2pRuntimeOutcome::MissingParents { .. }
    ));
    assert!(reordered_runtime.pending_contains(&grandchild.hash));

    let recovered = drive(
        child.clone(),
        &mut reordered,
        &mut reordered_runtime,
        &identity,
    );
    assert!(matches!(
        recovered.primary,
        ActivatedV2P2pRuntimeOutcome::Accepted { ref block_hash, .. }
            if block_hash == &child.hash
    ));
    assert!(recovered.retried.iter().any(|outcome| matches!(
        outcome,
        ActivatedV2P2pRuntimeOutcome::Accepted { block_hash, .. }
            if block_hash == &grandchild.hash
    )));

    assert!(ordered_runtime.pending_is_empty());
    assert!(reordered_runtime.pending_is_empty());
    assert!(ordered_runtime.staging().is_empty());
    assert!(reordered_runtime.staging().is_empty());
    assert_eq!(ordered.dag.ordered_dag, reordered.dag.ordered_dag);
    assert_eq!(ordered.dag.ordered_dag_tip, reordered.dag.ordered_dag_tip);
    assert_eq!(
        ordered.utxo.compute_state_root().unwrap(),
        reordered.utxo.compute_state_root().unwrap()
    );
    assert_eq!(canonical_mempool_txids(&ordered), expected_mempool);
    assert_eq!(canonical_mempool_txids(&reordered), expected_mempool);
    assert_eq!(ordered.mempool.first_seen, reordered.mempool.first_seen);
    assert_eq!(
        ordered.mempool.next_first_seen,
        reordered.mempool.next_first_seen
    );
    assert_eq!(
        ordered.mempool.spent_outpoints,
        reordered.mempool.spent_outpoints
    );
    assert!(ordered.mempool.transactions.contains_key(&spend.txid));
    assert!(reordered.mempool.transactions.contains_key(&spend.txid));
}
