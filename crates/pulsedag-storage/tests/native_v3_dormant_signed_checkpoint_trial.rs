//! TEST ONLY: transaction-bearing native-v3 checkpoint rehearsal after RocksDB
//! pruning. Uses real v2 chain-bound Ed25519 signatures, canonical v2 txids,
//! v3 Merkle commitment/KHeavyHash and the existing v2 economic validator.
//! This is NOT native-v3 monetary acceptance or finality: a test-only external
//! non-coinbase funding UTXO is seeded, v3 header roots are placeholders,
//! K/ordering remain unapproved and snapshots rely on an in-process oracle.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key,
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    compute_txid_v2,
    genesis_v3::{dormant_native_genesis_envelope_v3_ns, init_chain_state_v3},
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_v3::expected_difficulty_for_v3_window_ns,
    signing_message_v2,
    state::ChainState,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    validation_v2::validate_transaction_v2,
    TRANSACTION_VERSION_V2,
};
use pulsedag_storage::NativeV3EnvelopeRestoreRecordV1;
use rocksdb::{Options, WriteBatch, WriteOptions, DB};

const CHAIN: &str = "native-v3-SIGNED-RESTORE-DIAGNOSTIC-NOT-PRODUCTION";
const GENESIS_NS: u64 = 1_800_000_032_000_000_000;
const PREFIX: u64 = 26;
const KEEP: u64 = 20;
const SNAPSHOT_KEY: &[u8] = b"__TEST_ONLY_SIGNED_V3_EXTERNAL_FUNDING_SNAPSHOT";
const MANIFEST_KEY: &[u8] = b"__TEST_ONLY_SIGNED_V3_REPLAY_ORACLE_MANIFEST";

struct DisposableDb(PathBuf);

impl DisposableDb {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-signed-v3-test-only-{}-{nanos}",
            std::process::id()
        )))
    }

    fn open(&self) -> DB {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        DB::open(&opts, &self.0).unwrap()
    }
}
impl Drop for DisposableDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn frame_key(height: u64) -> Vec<u8> {
    format!("__TEST_ONLY_SIGNED_V3_FRAME_{height:03}").into_bytes()
}

fn mine(parent: &BlockEnvelopeV3, height: u64, transactions: Vec<Transaction>) -> BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.hash.clone()],
        timestamp_ns: GENESIS_NS + height * 500_000_000,
        difficulty: parent.header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "99".repeat(32), // NOT committed real v3 UTXO state.
        blue_score: height,
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        }
    }
    panic!("diagnostic easy-target PoW fixture did not find a nonce");
}

/// Native-v3 zero-allocation genesis economic-state fixture. The separate
/// nanosecond BlockEnvelopeV3 identity is validated by the PoW/DAG tests;
/// this state constructor is for v3's initially EMPTY economic state only.
fn empty_v3_test_state() -> ChainState {
    let state = init_chain_state_v3(CHAIN.to_owned(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(state.utxo.utxos.is_empty());
    assert!(state.utxo.address_index.is_empty());
    assert!(!state.contracts.config.enabled);
    state
}

fn funding_state() -> (ChainState, OutPoint, SigningKey) {
    let signing_key = SigningKey::from_bytes(&[27_u8; 32]);
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let address = address_from_public_key(&public_key);
    let outpoint = OutPoint {
        txid: "TEST_ONLY_NON_GENESIS_NON_COINBASE_EXTERNAL_FUNDING".into(),
        index: 0,
    };
    // Explicitly inject TEST funding only after constructing zero-allocation
    // native-v3 economic state. No legacy treasury is present even briefly.
    let mut state = empty_v3_test_state();
    state.utxo.utxos.insert(
        outpoint.clone(),
        Utxo {
            outpoint: outpoint.clone(),
            address: address.clone(),
            amount: 100_000,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(address)
        .or_default()
        .push(outpoint.clone());
    state.utxo.validate_deterministic_indexes().unwrap();
    (state, outpoint, signing_key)
}

fn signed_transfer(outpoint: OutPoint, key: &SigningKey, nonce: u64) -> Transaction {
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: outpoint,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1TEST_ONLY_RECIPIENT_NOT_A_WALLET".into(),
            amount: 98_900,
        }],
        fee: 1_100,
        nonce,
    };
    let message = signing_message_v2(&tx, CHAIN).unwrap();
    tx.inputs[0].signature = hex::encode(key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
    tx
}

/// Separate, deliberately explicit one-transaction UTXO bookkeeping.
/// Signature/funds/fee validation uses the existing genuine v2 validator,
/// but this state application is NOT a native-v3 monetary replay engine.
/// A self-contained test-only validation/application boundary. The real v2
/// validator allows input totals > output+declared fee; this checkpoint trial
/// explicitly rejects that implicit surplus instead of creating unaccounted
/// monetary burn. Apply to a clone so ANY failure keeps prior state unchanged.
fn apply_validated_detached_transfer(
    state: &mut ChainState,
    tx: &Transaction,
    height: u64,
) -> Result<(), String> {
    validate_transaction_v2(tx, state, CHAIN).map_err(|e| e.to_string())?;
    let input_atoms = tx.inputs.iter().try_fold(0u128, |sum, input| {
        let coin = state
            .utxo
            .utxos
            .get(&input.previous_output)
            .ok_or("validated signed input lacks a settled UTXO")?;
        sum.checked_add(u128::from(coin.amount))
            .ok_or("signed source amount overflow")
    })?;
    let output_atoms = tx.outputs.iter().try_fold(0u128, |sum, output| {
        sum.checked_add(u128::from(output.amount))
            .ok_or("signed destination amount overflow")
    })?;
    let accounted_atoms = output_atoms
        .checked_add(u128::from(tx.fee))
        .ok_or("signed total output plus fee overflow")?;
    if input_atoms != accounted_atoms {
        return Err("detached replay refuses implicit unaccounted fee/burn gap".into());
    }
    let mut next_state = state.clone();
    for input in &tx.inputs {
        let source = next_state
            .utxo
            .utxos
            .remove(&input.previous_output)
            .ok_or("validated input disappeared")?;
        let indexed = next_state
            .utxo
            .address_index
            .get_mut(&source.address)
            .ok_or("validated source index missing")?;
        indexed.retain(|id| id != &input.previous_output);
        if indexed.is_empty() {
            next_state.utxo.address_index.remove(&source.address);
        }
    }
    for (index, output) in tx.outputs.iter().enumerate() {
        let id = OutPoint {
            txid: tx.txid.clone(),
            index: u32::try_from(index).map_err(|_| "signed output index overflow")?,
        };
        let new_coin = Utxo {
            outpoint: id.clone(),
            address: output.address.clone(),
            amount: output.amount,
            coinbase: false,
            height,
        };
        if next_state.utxo.utxos.insert(id.clone(), new_coin).is_some() {
            return Err("replay duplicate output".into());
        }
        next_state
            .utxo
            .address_index
            .entry(output.address.clone())
            .or_default()
            .push(id);
    }
    next_state
        .utxo
        .validate_deterministic_indexes()
        .map_err(|e| e.to_string())?;
    *state = next_state;
    Ok(())
}

/// The real v2 validator allows inputs > outputs + the declared fee. Never
/// use that to silently burn uncommitted surplus in the checkpoint preview;
/// reject despite valid signature and txid, and leave the UTXO unchanged.
/// Signed chain-bound v2 transactions cannot spend from the real v3
/// zero-allocation initial state: test funding is ONLY an explicit fixture.
/// This checks both validator fail-closed behavior and zero mutation.
#[test]
fn zero_allocation_v3_test_start_cannot_accept_synthetic_funding_without_seed() {
    let mut initially_empty = empty_v3_test_state();
    let pristine_root = initially_empty.utxo.compute_state_root().unwrap();
    let (seeded, external_outpoint, signer) = funding_state();
    assert_ne!(seeded.utxo.compute_state_root().unwrap(), pristine_root);
    let signed = signed_transfer(external_outpoint, &signer, 201);
    assert!(validate_transaction_v2(&signed, &initially_empty, CHAIN).is_err());
    assert!(apply_validated_detached_transfer(&mut initially_empty, &signed, PREFIX + 1).is_err());
    assert!(initially_empty.utxo.utxos.is_empty());
    assert!(initially_empty.utxo.address_index.is_empty());
    assert_eq!(
        initially_empty.utxo.compute_state_root().unwrap(),
        pristine_root
    );

    let mut explicitly_seeded = seeded;
    apply_validated_detached_transfer(&mut explicitly_seeded, &signed, PREFIX + 1).unwrap();
    assert_ne!(
        explicitly_seeded.utxo.compute_state_root().unwrap(),
        pristine_root
    );
}

#[test]
fn signed_detached_checkpoint_rejects_unaccounted_surplus_and_preserves_utxo() {
    let (funded, source, key) = funding_state();
    let original_root = funded.utxo.compute_state_root().unwrap();
    let mut gap = signed_transfer(source.clone(), &key, 500);
    gap.outputs[0].amount -= 1;
    gap.inputs[0].signature = hex::encode(
        key.sign(&signing_message_v2(&gap, CHAIN).unwrap())
            .to_bytes(),
    );
    gap.txid = compute_txid_v2(&gap, CHAIN).unwrap();
    validate_transaction_v2(&gap, &funded, CHAIN).unwrap();

    let mut still_funded = funded.clone();
    let err = apply_validated_detached_transfer(&mut still_funded, &gap, PREFIX + 1).unwrap_err();
    assert!(err.contains("unaccounted fee/burn gap"));
    assert_eq!(
        still_funded.utxo.compute_state_root().unwrap(),
        original_root
    );
    assert_eq!(still_funded.utxo.utxos[&source].amount, 100_000);

    // An honest signed transfer still succeeds against the unchanged source.
    let valid = signed_transfer(source, &key, 501);
    apply_validated_detached_transfer(&mut still_funded, &valid, PREFIX + 1).unwrap();
    assert_ne!(
        still_funded.utxo.compute_state_root().unwrap(),
        original_root
    );
    assert_eq!(still_funded.utxo.utxos.len(), 1);
    assert_eq!(
        still_funded.utxo.utxos.values().next().unwrap().amount,
        98_900
    );
}

#[test]
fn pruned_rocksdb_replays_real_signed_v2_transfer_inside_pow_valid_v3_frame_fail_closed() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let (funded, funding_outpoint, key) = funding_state();
    let funding_coins: Vec<Utxo> = funded.utxo.utxos.values().cloned().collect();
    let funding_root = funded.utxo.compute_state_root().unwrap();
    let signed = signed_transfer(funding_outpoint, &key, 7);
    validate_transaction_v2(&signed, &funded, CHAIN).unwrap();

    let mut frames = Vec::new();
    let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header.clone())]);
    let mut previous = genesis.clone();
    for height in 1..=PREFIX {
        let block = mine(&previous, height, vec![]);
        headers.insert(block.hash.clone(), block.header.clone());
        frames.push(block.clone());
        previous = block;
    }
    let signed_block = mine(&previous, PREFIX + 1, vec![signed.clone()]);
    assert_eq!(signed_block.transactions.len(), 1);
    assert_eq!(signed_block.transactions[0].txid, signed.txid);
    headers.insert(signed_block.hash.clone(), signed_block.header.clone());
    frames.push(signed_block.clone());

    // The fully replayed, frozen-genesis chain is an independent test oracle.
    let full = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&signed_block.hash),
        &headers,
        1,
    )
    .unwrap();
    assert_eq!(full.blocks.len(), (PREFIX + 2) as usize);
    let selected: BTreeMap<_, _> = full
        .blocks
        .iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
        .collect();
    let full_window =
        selected_parent_retarget_window_v3_ns(CHAIN, &signed_block.hash, &headers, &selected)
            .unwrap();
    assert_eq!(full_window.len(), KEEP as usize);
    let full_difficulty = expected_difficulty_for_v3_window_ns(&full_window).unwrap();

    let mut expected = funded.clone();
    apply_validated_detached_transfer(&mut expected, &signed, PREFIX + 1).unwrap();
    let expected_root = expected.utxo.compute_state_root().unwrap();
    assert_ne!(expected_root, funding_root);
    assert_ne!(signed_block.header.state_root, expected_root);
    assert_eq!(
        expected
            .utxo
            .utxos
            .values()
            .map(|coin| coin.amount)
            .sum::<u64>(),
        98_900
    );

    let manifest = serde_json::json!({
        "schema": "TEST_ONLY_EXPLICIT_UNAPPROVED_SIGNED_V3_REPLAY",
        "chain": CHAIN,
        "genesis": genesis.hash,
        "tip": signed_block.hash,
        "trial_blue_work": full.blocks[&signed_block.hash].blue_work.0,
        "retarget_bits": full_difficulty.expected_bits,
        "funding_snapshot_root": funding_root,
        "signed_txid": signed.txid,
        "signed_fee": signed.fee,
        "detached_after_fee_root": expected_root
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let funding_bytes = serde_json::to_vec(&funding_coins).unwrap();

    let disk = DisposableDb::new();
    {
        let db = disk.open();
        let mut batch = WriteBatch::default();
        for (height, envelope) in frames.iter().enumerate() {
            let record = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                envelope.clone(),
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .unwrap();
            batch.put(
                frame_key(height as u64 + 1),
                record
                    .encode_verified(CHAIN, GENESIS_NS, &genesis.hash)
                    .unwrap(),
            );
        }
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();
        let mut never_committed = WriteBatch::default();
        never_committed.delete(frame_key(1));
        never_committed.put(MANIFEST_KEY, &manifest_bytes);
        drop(never_committed);
    }
    {
        let db = disk.open();
        assert!(db.get(frame_key(1)).unwrap().is_some());
        assert!(db.get(MANIFEST_KEY).unwrap().is_none());
        let mut batch = WriteBatch::default();
        for height in 1..=(PREFIX + 1 - KEEP) {
            batch.delete(frame_key(height));
        }
        batch.put(MANIFEST_KEY, &manifest_bytes);
        batch.put(SNAPSHOT_KEY, &funding_bytes);
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();
    }

    let check_reopened = |db: &DB| -> Result<(), String> {
        if db
            .get(MANIFEST_KEY)
            .map_err(|e| e.to_string())?
            .ok_or("missing test-only manifest")?
            != manifest_bytes
        {
            return Err("checkpoint metadata differs from external full oracle".into());
        }
        let snapshot = db
            .get(SNAPSHOT_KEY)
            .map_err(|e| e.to_string())?
            .ok_or("missing funding snapshot")?;
        if snapshot != funding_bytes {
            return Err("external UTXO snapshot was altered".into());
        }
        let coins: Vec<Utxo> = serde_json::from_slice(&snapshot).map_err(|e| e.to_string())?;
        let mut reconstructed = empty_v3_test_state();
        for coin in coins {
            if reconstructed
                .utxo
                .utxos
                .insert(coin.outpoint.clone(), coin.clone())
                .is_some()
            {
                return Err("duplicate seeded outpoint".into());
            }
            reconstructed
                .utxo
                .address_index
                .entry(coin.address.clone())
                .or_default()
                .push(coin.outpoint.clone());
        }
        if reconstructed
            .utxo
            .compute_state_root()
            .map_err(|e| e.to_string())?
            != funding_root
        {
            return Err("initial externally anchored UTXO root changed".into());
        }
        let mut retained = BTreeMap::new();
        let mut selected_map = BTreeMap::new();
        let mut transactions = Vec::new();
        let mut previous_retained: Option<String> = None;
        let first_kept = PREFIX + 2 - KEEP;
        for height in first_kept..=PREFIX + 1 {
            let bytes = db
                .get(frame_key(height))
                .map_err(|e| e.to_string())?
                .ok_or("retarget predecessor frame missing")?;
            let record = NativeV3EnvelopeRestoreRecordV1::decode_verified(
                &bytes,
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .map_err(|e| e.to_string())?;
            let block = record.envelope();
            if block.header.height != height {
                return Err("persisted frame height incorrect".into());
            }
            if block.hash != frames[(height - 1) as usize].hash {
                return Err("retained frame differs from full-genesis authenticated oracle".into());
            }
            if let Some(previous_hash) = &previous_retained {
                if block.header.parents != [previous_hash.clone()] {
                    return Err("retained signed replay chain has a parent gap".into());
                }
            }
            previous_retained = Some(block.hash.clone());
            retained.insert(block.hash.clone(), block.header.clone());
            selected_map.insert(block.hash.clone(), Some(block.header.parents[0].clone()));
            if height == PREFIX + 1 {
                transactions = block.transactions.clone();
                if block.hash != signed_block.hash {
                    return Err("persisted last block differs from full genesis oracle".into());
                }
            } else if !block.transactions.is_empty() {
                return Err("unexpected monetary side effects in retained prefix".into());
            }
        }
        if retained.len() != KEEP as usize || transactions.len() != 1 {
            return Err("retained source incomplete".into());
        }
        let restored_window = selected_parent_retarget_window_v3_ns(
            CHAIN,
            &signed_block.hash,
            &retained,
            &selected_map,
        )
        .map_err(|e| e.to_string())?;
        if restored_window != full_window
            || expected_difficulty_for_v3_window_ns(&restored_window).map_err(|e| e.to_string())?
                != full_difficulty
        {
            return Err("retarget differs after pruning".into());
        }
        apply_validated_detached_transfer(&mut reconstructed, &transactions[0], PREFIX + 1)?;
        if reconstructed
            .utxo
            .compute_state_root()
            .map_err(|e| e.to_string())?
            != expected_root
        {
            return Err("verified signed transaction replay differs from full oracle".into());
        }
        if validate_transaction_v2(&transactions[0], &reconstructed, CHAIN).is_ok() {
            return Err("spent transaction was accepted a second time".into());
        }
        if derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&signed_block.hash),
            &retained,
            1,
        )
        .is_ok()
        {
            return Err("pruned history incorrectly accepted by full-genesis verifier".into());
        }
        Ok(())
    };
    {
        let db = disk.open();
        assert!(db.get(frame_key(1)).unwrap().is_none());
        check_reopened(&db).unwrap();
    }

    // Signature corruption is rejected by the REAL v2 signature verifier
    // even if an attacker recomputes txid. It does not create a valid v3
    // header: changing txids would require a newly mined Merkle commitment.
    let mut forged_sig = signed.clone();
    forged_sig.inputs[0].signature = "00".repeat(64);
    forged_sig.txid = compute_txid_v2(&forged_sig, CHAIN).unwrap();
    assert!(validate_transaction_v2(&forged_sig, &funded, CHAIN).is_err());
    assert!(validate_transaction_v2(&signed, &funded, "wrong-chain-id").is_err());
    let conflicting_spend = signed_transfer(signed.inputs[0].previous_output.clone(), &key, 8);
    validate_transaction_v2(&conflicting_spend, &funded, CHAIN).unwrap();
    assert!(validate_transaction_v2(&conflicting_spend, &expected, CHAIN).is_err());

    {
        let db = disk.open();
        let mut forged_manifest: serde_json::Value =
            serde_json::from_slice(&manifest_bytes).unwrap();
        forged_manifest["signed_fee"] = serde_json::json!(0);
        db.put(MANIFEST_KEY, serde_json::to_vec(&forged_manifest).unwrap())
            .unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            check_reopened(&db).is_err(),
            "forged fee checkpoint must fail"
        );
        db.put(MANIFEST_KEY, &manifest_bytes).unwrap();
        let mut forged_coins = funding_coins.clone();
        forged_coins[0].amount += 1;
        db.put(SNAPSHOT_KEY, serde_json::to_vec(&forged_coins).unwrap())
            .unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            check_reopened(&db).is_err(),
            "tampered checkpoint UTXO must fail"
        );
        db.put(SNAPSHOT_KEY, &funding_bytes).unwrap();
        db.delete(frame_key(PREFIX + 1 - KEEP / 2)).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            check_reopened(&db).is_err(),
            "missing retarget frame must fail"
        );
    }
}

/// Additional TEST-ONLY parent-DAG fixture. K and transaction ordering are not
/// approved. PoW and canonical signed-v2 Merkle commitments are genuine.
fn mine_two_parent_join(
    parents: &[BlockEnvelopeV3],
    height: u64,
    score: u64,
    tag: u64,
) -> BlockEnvelopeV3 {
    let mut hashes: Vec<String> = parents.iter().map(|parent| parent.hash.clone()).collect();
    hashes.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: hashes,
        timestamp_ns: GENESIS_NS + height * 500_000_000 + tag * 10_000,
        difficulty: parents[0].header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "ab".repeat(32), // Not the validated UTXO root.
        blue_score: score,
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("easy PoW test fixture exhausted");
}

fn signed_fork_frame_key(hash: &str) -> Vec<u8> {
    format!("__TEST_ONLY_SIGNED_V3_COMPETING_DAG_FRAME_{hash}").into_bytes()
}

/// A parent-DAG test of two *individually valid*, mutually conflicting signed
/// v2 spends in competing PoW-valid v3 blocks. This test makes NO decision
/// about eventual native-v3 blue/red monetary inclusion, fee payout, finality,
/// rollback policy or canonical ordering. The selected-parent-FIRST choice is
/// an explicit isolated hypothesis used only to prove fail-closed double-spend
/// behavior after persistence, pruning, and RocksDB reopen.
#[test]
fn competing_signed_spends_reopen_from_pruned_rocksdb_and_refuse_double_settlement() {
    use std::collections::BTreeSet;

    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let (funded, funding_id, signing_key) = funding_state();
    let initial_root = funded.utxo.compute_state_root().unwrap();
    let funding_coins: Vec<Utxo> = funded.utxo.utxos.values().cloned().collect();
    let funding_bytes = serde_json::to_vec(&funding_coins).unwrap();
    let tx_a = signed_transfer(funding_id.clone(), &signing_key, 301);
    let tx_b = signed_transfer(funding_id.clone(), &signing_key, 302);
    assert_ne!(tx_a.txid, tx_b.txid);
    validate_transaction_v2(&tx_a, &funded, CHAIN).unwrap();
    validate_transaction_v2(&tx_b, &funded, CHAIN).unwrap();

    let mut prefix = Vec::new();
    let mut previous = genesis.clone();
    for height in 1..=PREFIX {
        let env = mine(&previous, height, vec![]);
        prefix.push(env.clone());
        previous = env;
    }
    let branch_a = mine(&previous, PREFIX + 1, vec![tx_a]);
    let branch_b = mine(&previous, PREFIX + 1, vec![tx_b]);
    assert_ne!(branch_a.hash, branch_b.hash);
    let (winner, loser) = if branch_a.hash < branch_b.hash {
        (branch_a, branch_b)
    } else {
        (branch_b, branch_a)
    };

    for trial_k in [0_usize, 1] {
        let join = mine_two_parent_join(
            &[winner.clone(), loser.clone()],
            PREFIX + 2,
            PREFIX + 2 + trial_k as u64,
            91 + trial_k as u64,
        );
        let child = mine_two_parent_join(
            std::slice::from_ref(&join),
            PREFIX + 3,
            PREFIX + 3 + trial_k as u64,
            101 + trial_k as u64,
        );
        let mut all_blocks: BTreeMap<String, BlockEnvelopeV3> = prefix
            .iter()
            .map(|block| (block.hash.clone(), block.clone()))
            .collect();
        for block in [&winner, &loser, &join, &child] {
            all_blocks.insert(block.hash.clone(), block.clone());
        }
        assert_eq!(all_blocks.len(), (PREFIX + 4) as usize);
        let mut all_headers = BTreeMap::from([(genesis.hash.clone(), genesis.header.clone())]);
        all_headers.extend(
            all_blocks
                .iter()
                .map(|(id, block)| (id.clone(), block.header.clone())),
        );
        let full = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&child.hash),
            &all_headers,
            trial_k,
        )
        .unwrap();
        assert_eq!(full.blocks.len(), (PREFIX + 5) as usize);
        assert_eq!(
            full.blocks[&join.hash].selected_parent.as_ref(),
            Some(&winner.hash)
        );
        assert_eq!(
            full.blocks[&join.hash].merge_blues,
            if trial_k == 1 {
                vec![loser.hash.clone()]
            } else {
                vec![]
            }
        );
        assert_eq!(
            full.blocks[&join.hash].merge_reds,
            if trial_k == 0 {
                vec![loser.hash.clone()]
            } else {
                vec![]
            }
        );

        // Both signed transactions are genuine individually. A hypothetical
        // selected-parent-first replay MUST reject the second competing spend.
        let mut complete_winner = funded.clone();
        apply_validated_detached_transfer(
            &mut complete_winner,
            &winner.transactions[0],
            PREFIX + 1,
        )
        .unwrap();
        let winning_root = complete_winner.utxo.compute_state_root().unwrap();
        assert_ne!(winning_root, initial_root);
        assert!(apply_validated_detached_transfer(
            &mut complete_winner,
            &loser.transactions[0],
            PREFIX + 1
        )
        .is_err());
        assert_eq!(
            complete_winner.utxo.compute_state_root().unwrap(),
            winning_root
        );
        let mut alternative = funded.clone();
        apply_validated_detached_transfer(&mut alternative, &loser.transactions[0], PREFIX + 1)
            .unwrap();
        let alternative_root = alternative.utxo.compute_state_root().unwrap();
        assert_ne!(alternative_root, winning_root);
        assert!(apply_validated_detached_transfer(
            &mut alternative,
            &winner.transactions[0],
            PREFIX + 1
        )
        .is_err());
        assert_eq!(
            alternative.utxo.compute_state_root().unwrap(),
            alternative_root
        );
        let conditional_fee_burn = u128::from(winner.transactions[0].fee);
        assert_eq!(conditional_fee_burn, 1_100);
        assert_eq!(winner.transactions[0].fee, loser.transactions[0].fee);
        assert_eq!(
            complete_winner
                .utxo
                .utxos
                .values()
                .map(|coin| u128::from(coin.amount))
                .sum::<u128>()
                + conditional_fee_burn,
            100_000
        );

        let selected_map: BTreeMap<_, _> = full
            .blocks
            .iter()
            .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
            .collect();
        let full_window =
            selected_parent_retarget_window_v3_ns(CHAIN, &child.hash, &all_headers, &selected_map)
                .unwrap();
        assert_eq!(full_window.len(), KEEP as usize);
        let full_retarget = expected_difficulty_for_v3_window_ns(&full_window).unwrap();
        let mut selected20 = Vec::new();
        let mut walk = child.hash.clone();
        for _ in 0..KEEP {
            selected20.push(walk.clone());
            walk = full.blocks[&walk].selected_parent.clone().unwrap();
        }
        assert!(selected20.contains(&winner.hash));
        assert!(!selected20.contains(&loser.hash));
        let mut retained: BTreeSet<String> = selected20.iter().cloned().collect();
        retained.insert(loser.hash.clone());
        assert_eq!(retained.len(), KEEP as usize + 1);
        assert!(!retained.contains(&genesis.hash));
        assert!(!retained.contains(&prefix[0].hash));

        // Each checkpoint manifest is informational; only the independent
        // complete-history oracle retained by this test process authorizes a
        // comparison. No self-signed peer checkpoint is accepted.
        let manifest_key = b"__TEST_ONLY_SIGNED_FORK_MANIFEST";
        let snapshot_key = b"__TEST_ONLY_SIGNED_FORK_EXTERNAL_SEED";
        let manifest = serde_json::json!({
            "schema": "TEST_ONLY_FORK_SIGNED_NO_FINALITY_v1",
            "chain": CHAIN,
            "genesis": genesis.hash,
            "tip": child.hash,
            "trial_k": trial_k,
            "selected_parent": winner.hash,
            "loser": loser.hash,
            "selected_transaction": winner.transactions[0].txid,
            "excluded_double_spend": loser.transactions[0].txid,
            "side_blues": full.blocks[&join.hash].merge_blues,
            "side_reds": full.blocks[&join.hash].merge_reds,
            "full_tip_blue_work": full.blocks[&child.hash].blue_work.0,
            "retarget_bits": full_retarget.expected_bits,
            "seed_root": initial_root,
            "winner_utxo_root": winning_root,
            "other_utxo_root": alternative_root
        });
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let encoded: BTreeMap<String, Vec<u8>> = all_blocks
            .iter()
            .map(|(id, block)| {
                let record = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                    block.clone(),
                    CHAIN,
                    GENESIS_NS,
                    &genesis.hash,
                )
                .unwrap();
                (
                    id.clone(),
                    record
                        .encode_verified(CHAIN, GENESIS_NS, &genesis.hash)
                        .unwrap(),
                )
            })
            .collect();

        let temporary = DisposableDb::new();
        {
            let db = temporary.open();
            let mut batch = WriteBatch::default();
            for (id, frame) in &encoded {
                batch.put(signed_fork_frame_key(id), frame);
            }
            let mut sync = WriteOptions::default();
            sync.set_sync(true);
            db.write_opt(batch, &sync).unwrap();

            let mut abandoned = WriteBatch::default();
            abandoned.put(manifest_key, &manifest_bytes);
            abandoned.delete(signed_fork_frame_key(&prefix[0].hash));
            drop(abandoned); // Not a power-loss simulation.
        }
        {
            let db = temporary.open();
            assert!(db.get(manifest_key).unwrap().is_none());
            assert!(db
                .get(signed_fork_frame_key(&prefix[0].hash))
                .unwrap()
                .is_some());
            let mut batch = WriteBatch::default();
            for id in all_blocks.keys() {
                if !retained.contains(id) {
                    batch.delete(signed_fork_frame_key(id));
                }
            }
            batch.put(manifest_key, &manifest_bytes);
            batch.put(snapshot_key, &funding_bytes);
            let mut sync = WriteOptions::default();
            sync.set_sync(true);
            db.write_opt(batch, &sync).unwrap();
        }
        let check_reopened = |db: &DB| -> Result<(), String> {
            let persisted_manifest = db
                .get(manifest_key)
                .map_err(|e| e.to_string())?
                .ok_or("fork manifest missing")?;
            let persisted_seed = db
                .get(snapshot_key)
                .map_err(|e| e.to_string())?
                .ok_or("fork UTXO seed missing")?;
            if persisted_manifest != manifest_bytes || persisted_seed != funding_bytes {
                return Err("fork trust boundary differs from preauthenticated oracle".into());
            }

            let mut replay = empty_v3_test_state();
            let coins: Vec<Utxo> =
                serde_json::from_slice(&persisted_seed).map_err(|e| e.to_string())?;
            for coin in coins {
                if replay
                    .utxo
                    .utxos
                    .insert(coin.outpoint.clone(), coin.clone())
                    .is_some()
                {
                    return Err("duplicate seeded outpoint".into());
                }
                replay
                    .utxo
                    .address_index
                    .entry(coin.address.clone())
                    .or_default()
                    .push(coin.outpoint.clone());
            }
            if replay
                .utxo
                .compute_state_root()
                .map_err(|e| e.to_string())?
                != initial_root
            {
                return Err("fork seed UTXO root changed".into());
            }

            let mut bounded = BTreeMap::new();
            let mut restored_blocks = BTreeMap::new();
            for id in &retained {
                let frame = db
                    .get(signed_fork_frame_key(id))
                    .map_err(|e| e.to_string())?
                    .ok_or("retained selected or competing fork block missing")?;
                let record = NativeV3EnvelopeRestoreRecordV1::decode_verified(
                    &frame,
                    CHAIN,
                    GENESIS_NS,
                    &genesis.hash,
                )
                .map_err(|e| e.to_string())?;
                let restored = record.envelope();
                if restored.hash != *id
                    || restored.header != all_headers[id]
                    || serde_json::to_vec(restored).map_err(|e| e.to_string())?
                        != serde_json::to_vec(&all_blocks[id]).map_err(|e| e.to_string())?
                {
                    return Err(
                        "untrusted restored fork body differs from complete-history oracle".into(),
                    );
                }
                bounded.insert(id.clone(), restored.header.clone());
                restored_blocks.insert(id.clone(), restored.clone());
            }
            for (id, header) in &bounded {
                if id != selected20.last().unwrap()
                    && header
                        .parents
                        .iter()
                        .any(|parent| !bounded.contains_key(parent))
                {
                    return Err("selected or competing fork lost a required parent".into());
                }
            }
            let restored_selection: BTreeMap<_, _> = selected_map
                .iter()
                .filter(|(id, _)| bounded.contains_key(*id))
                .map(|(id, selected)| (id.clone(), selected.clone()))
                .collect();
            let resumed_window = selected_parent_retarget_window_v3_ns(
                CHAIN,
                &child.hash,
                &bounded,
                &restored_selection,
            )
            .map_err(|e| e.to_string())?;
            if resumed_window != full_window
                || expected_difficulty_for_v3_window_ns(&resumed_window)
                    .map_err(|e| e.to_string())?
                    != full_retarget
            {
                return Err("native multiparent signed retarget changed after trim".into());
            }
            if restored_blocks[&winner.hash].transactions.len() != 1
                || restored_blocks[&loser.hash].transactions.len() != 1
            {
                return Err("signed competing fork body missing".into());
            }
            apply_validated_detached_transfer(
                &mut replay,
                &restored_blocks[&winner.hash].transactions[0],
                PREFIX + 1,
            )?;
            if replay
                .utxo
                .compute_state_root()
                .map_err(|e| e.to_string())?
                != winning_root
            {
                return Err("competing fork chosen detached UTXO root differs after trim".into());
            }
            if apply_validated_detached_transfer(
                &mut replay,
                &restored_blocks[&loser.hash].transactions[0],
                PREFIX + 1,
            )
            .is_ok()
            {
                return Err("duplicate signed fork spend accepted".into());
            }
            if replay
                .utxo
                .compute_state_root()
                .map_err(|e| e.to_string())?
                != winning_root
            {
                return Err("rejected spend mutated test-only monetary state".into());
            }
            // Do not let a truncated DAG pass the production-like full-genesis
            // proof by merely presenting a locally matching snapshot.
            if derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
                std::slice::from_ref(&child.hash),
                &bounded,
                trial_k,
            )
            .is_ok()
            {
                return Err("pruned signed fork became trusted genesis closure".into());
            }
            Ok(())
        };
        {
            let db = temporary.open();
            assert!(db
                .get(signed_fork_frame_key(&prefix[0].hash))
                .unwrap()
                .is_none());
            check_reopened(&db).unwrap();
        }
        {
            let db = temporary.open();
            db.delete(signed_fork_frame_key(&loser.hash)).unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = temporary.open();
            assert!(
                check_reopened(&db).is_err(),
                "missing colored competing frontier must fail"
            );
            db.put(signed_fork_frame_key(&loser.hash), &encoded[&loser.hash])
                .unwrap();
            let mut forged = serde_json::from_slice::<serde_json::Value>(&manifest_bytes).unwrap();
            forged["full_tip_blue_work"][0] = serde_json::json!(0);
            db.put(manifest_key, serde_json::to_vec(&forged).unwrap())
                .unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = temporary.open();
            assert!(
                check_reopened(&db).is_err(),
                "unauthenticated side work must fail"
            );
            db.put(manifest_key, &manifest_bytes).unwrap();
            db.delete(signed_fork_frame_key(selected20.last().unwrap()))
                .unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = temporary.open();
            assert!(
                check_reopened(&db).is_err(),
                "retarget predecessor unavailable must fail"
            );
        }
        {
            // Restore the deliberately deleted oldest window sample before
            // testing an independent atomic *test-state* change. Both the
            // chosen and competing native-v3 frames remain in the pruned DB.
            let db = temporary.open();
            db.put(
                signed_fork_frame_key(selected20.last().unwrap()),
                &encoded[selected20.last().unwrap()],
            )
            .unwrap();
            db.flush_wal(true).unwrap();
            check_reopened(&db).unwrap();
        }

        // A hypothetical post-prune branch choice is externally specified by
        // the test oracle. This is NOT a native-v3 reorg/finality election.
        // Two settlement snapshots are calculated from the SAME original
        // independently authenticated test-only funding UTXO, not by applying
        // a loser transaction on top of already-spent winner funds.
        let checkpoint_choice_key = b"__TEST_ONLY_SIGNED_FORK_COMMITTED_TXID";
        let checkpoint_root_key = b"__TEST_ONLY_SIGNED_FORK_COMMITTED_ROOT";
        let checkpoint_coins_key = b"__TEST_ONLY_SIGNED_FORK_COMMITTED_UTXOS";
        let checkpoint_fee_key = b"__TEST_ONLY_SIGNED_FORK_COMMITTED_ELIGIBLE_FEE";
        let checkpoint_conflict_key = b"__TEST_ONLY_SIGNED_FORK_COMMITTED_SKIPPED_CONFLICTS";
        let snapshot_bytes = |state: &ChainState| {
            let mut coins: Vec<Utxo> = state.utxo.utxos.values().cloned().collect();
            coins.sort_by(|a, b| a.outpoint.cmp(&b.outpoint));
            serde_json::to_vec(&coins).unwrap()
        };
        let winner_bytes = snapshot_bytes(&complete_winner);
        let loser_bytes = snapshot_bytes(&alternative);

        let write_branch_state = |db: &DB, txid: &str, root: &str, coins: &[u8]| {
            let mut atomic = WriteBatch::default();
            atomic.put(checkpoint_choice_key, txid.as_bytes());
            atomic.put(checkpoint_root_key, root.as_bytes());
            atomic.put(checkpoint_coins_key, coins);
            // Test-only sidecar: the chosen spend contributes exactly one
            // fee, while its conflicting sibling contributes no second fee.
            atomic.put(checkpoint_fee_key, winner.transactions[0].fee.to_le_bytes());
            atomic.put(checkpoint_conflict_key, 1_u64.to_le_bytes());
            let mut sync = WriteOptions::default();
            sync.set_sync(true);
            db.write_opt(atomic, &sync).unwrap();
        };
        let read_branch_state = |db: &DB,
                                 chosen_txid: &str,
                                 expected_root: &str,
                                 expected_bytes: &[u8]|
         -> Result<(), String> {
            check_reopened(db)?;
            let tx = if chosen_txid == winner.transactions[0].txid {
                &winner.transactions[0]
            } else if chosen_txid == loser.transactions[0].txid {
                &loser.transactions[0]
            } else {
                return Err("test-only oracle refuses an unknown signed branch".into());
            };
            let selected = db
                .get(checkpoint_choice_key)
                .map_err(|error| error.to_string())?
                .ok_or("committed selected signed transaction missing")?;
            let root = db
                .get(checkpoint_root_key)
                .map_err(|error| error.to_string())?
                .ok_or("committed signed UTXO root missing")?;
            let data = db
                .get(checkpoint_coins_key)
                .map_err(|error| error.to_string())?
                .ok_or("committed signed UTXO contents missing")?;
            if selected.as_slice() != chosen_txid.as_bytes()
                || root.as_slice() != expected_root.as_bytes()
                || data.as_slice() != expected_bytes
            {
                return Err("atomic branch identity, root, and UTXOs diverged".into());
            }
            let persisted_fee = db
                .get(checkpoint_fee_key)
                .map_err(|error| error.to_string())?
                .ok_or("committed eligible fee missing")?;
            let persisted_conflicts = db
                .get(checkpoint_conflict_key)
                .map_err(|error| error.to_string())?
                .ok_or("committed conflict count missing")?;
            if persisted_fee.as_slice() != tx.fee.to_le_bytes().as_slice()
                || persisted_conflicts.as_slice() != 1_u64.to_le_bytes().as_slice()
            {
                return Err("signed fork fee or conflict provenance differs from oracle".into());
            }
            let coins: Vec<Utxo> =
                serde_json::from_slice(&data).map_err(|error| error.to_string())?;
            let mut recovered = empty_v3_test_state();
            for coin in coins {
                if recovered
                    .utxo
                    .utxos
                    .insert(coin.outpoint.clone(), coin.clone())
                    .is_some()
                {
                    return Err("duplicate output in settled fork snapshot".into());
                }
                recovered
                    .utxo
                    .address_index
                    .entry(coin.address.clone())
                    .or_default()
                    .push(coin.outpoint.clone());
            }
            if recovered
                .utxo
                .compute_state_root()
                .map_err(|error| error.to_string())?
                != expected_root
            {
                return Err("persisted fork UTXO state root invalid".into());
            }
            // Recalculate independently from test-only source funding using
            // genuine chain-bound signature, funds and fee validation.
            let mut oracle_replay = funded.clone();
            apply_validated_detached_transfer(&mut oracle_replay, tx, PREFIX + 1)?;
            if oracle_replay
                .utxo
                .compute_state_root()
                .map_err(|error| error.to_string())?
                != expected_root
            {
                return Err("signed branch not supported by external oracle".into());
            }
            let competitor = if tx.txid == winner.transactions[0].txid {
                &loser.transactions[0]
            } else {
                &winner.transactions[0]
            };
            if apply_validated_detached_transfer(&mut recovered, competitor, PREFIX + 1).is_ok() {
                return Err("post-recovery double settlement accepted".into());
            }
            if recovered
                .utxo
                .compute_state_root()
                .map_err(|error| error.to_string())?
                != expected_root
            {
                return Err("rejected post-recovery spend mutated economic state".into());
            }
            Ok(())
        };
        {
            let db = temporary.open();
            write_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            );
        }
        {
            let db = temporary.open();
            read_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            )
            .unwrap();
            // A dropped batch may NOT publish the other branch identity, even
            // if it contained the alternative complete valid UTXO snapshot.
            let mut abandoned = WriteBatch::default();
            abandoned.put(checkpoint_choice_key, loser.transactions[0].txid.as_bytes());
            abandoned.put(checkpoint_root_key, alternative_root.as_bytes());
            abandoned.put(checkpoint_coins_key, &loser_bytes);
            drop(abandoned);
        }
        {
            let db = temporary.open();
            read_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            )
            .unwrap();
            // This transition has no consensus authority; it only asserts
            // that a DIFFERENT externally authenticated trial outcome cannot
            // partially overwrite the prior durable branch/economic state.
            write_branch_state(
                &db,
                &loser.transactions[0].txid,
                &alternative_root,
                &loser_bytes,
            );
        }
        {
            let db = temporary.open();
            read_branch_state(
                &db,
                &loser.transactions[0].txid,
                &alternative_root,
                &loser_bytes,
            )
            .unwrap();
            // Simulate a torn/corrupt *postcommit store*, not a real power-loss
            // write crash: replacing only the selected txid with the old
            // branch must be rejected despite both signatures being valid.
            db.put(
                checkpoint_choice_key,
                winner.transactions[0].txid.as_bytes(),
            )
            .unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = temporary.open();
            assert!(
                read_branch_state(
                    &db,
                    &loser.transactions[0].txid,
                    &alternative_root,
                    &loser_bytes,
                )
                .is_err(),
                "mixed signed-branch identity and root must never replay"
            );
            write_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            );
        }
        {
            let db = temporary.open();
            read_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            )
            .unwrap();
            // A forged fee cannot be accepted merely because the authenticated
            // UTXO root is unchanged. Likewise for omitted conflict records.
            db.put(checkpoint_fee_key, 2_200_u64.to_le_bytes()).unwrap();
            assert!(
                read_branch_state(
                    &db,
                    &winner.transactions[0].txid,
                    &winning_root,
                    &winner_bytes,
                )
                .is_err(),
                "forged persisted fee must fail closed"
            );
            write_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            );
            db.put(checkpoint_conflict_key, 0_u64.to_le_bytes())
                .unwrap();
            assert!(
                read_branch_state(
                    &db,
                    &winner.transactions[0].txid,
                    &winning_root,
                    &winner_bytes,
                )
                .is_err(),
                "omitted competing transaction must fail closed"
            );
            write_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            );
            read_branch_state(
                &db,
                &winner.transactions[0].txid,
                &winning_root,
                &winner_bytes,
            )
            .unwrap();
        }
    }
}
