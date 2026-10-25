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
    genesis::init_chain_state,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
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

fn funding_state() -> (ChainState, OutPoint, SigningKey) {
    let signing_key = SigningKey::from_bytes(&[27_u8; 32]);
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let address = address_from_public_key(&public_key);
    let outpoint = OutPoint {
        txid: "TEST_ONLY_NON_GENESIS_NON_COINBASE_EXTERNAL_FUNDING".into(),
        index: 0,
    };
    let mut state = init_chain_state(CHAIN.to_owned());
    // init_chain_state provides the LEGACY v1 treasury. It is NOT the zero-
    // allocation v3 genesis: discard its UTXOs before seeding this private
    // externally authenticated test-only non-coinbase funding state.
    state.utxo.utxos.clear();
    state.utxo.address_index.clear();
    assert!(state.utxo.utxos.is_empty());
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
    state.utxo.address_index.entry(address).or_default().push(outpoint.clone());
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
fn apply_validated_detached_transfer(
    state: &mut ChainState,
    tx: &Transaction,
    height: u64,
) -> Result<(), String> {
    validate_transaction_v2(tx, state, CHAIN).map_err(|e| e.to_string())?;
    for input in &tx.inputs {
        let source = state.utxo.utxos.remove(&input.previous_output)
            .ok_or("validated input disappeared")?;
        let indexed = state.utxo.address_index.get_mut(&source.address)
            .ok_or("validated source index missing")?;
        indexed.retain(|id| id != &input.previous_output);
        if indexed.is_empty() {
            state.utxo.address_index.remove(&source.address);
        }
    }
    for (index, output) in tx.outputs.iter().enumerate() {
        let id = OutPoint { txid: tx.txid.clone(), index: index as u32 };
        let new_coin = Utxo {
            outpoint: id.clone(),
            address: output.address.clone(),
            amount: output.amount,
            coinbase: false,
            height,
        };
        if state.utxo.utxos.insert(id.clone(), new_coin).is_some() {
            return Err("replay duplicate output".into());
        }
        state.utxo.address_index.entry(output.address.clone()).or_default().push(id);
    }
    state.utxo.validate_deterministic_indexes().map_err(|e| e.to_string())
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
        CHAIN, GENESIS_NS, &genesis.hash,
        std::slice::from_ref(&signed_block.hash), &headers, 1
    ).unwrap();
    assert_eq!(full.blocks.len(), (PREFIX + 2) as usize);
    let selected: BTreeMap<_, _> = full.blocks.iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone())).collect();
    let full_window = selected_parent_retarget_window_v3_ns(
        CHAIN, &signed_block.hash, &headers, &selected
    ).unwrap();
    assert_eq!(full_window.len(), KEEP as usize);
    let full_difficulty = expected_difficulty_for_v3_window_ns(&full_window).unwrap();

    let mut expected = funded.clone();
    apply_validated_detached_transfer(&mut expected, &signed, PREFIX + 1).unwrap();
    let expected_root = expected.utxo.compute_state_root().unwrap();
    assert_ne!(expected_root, funding_root);
    assert_ne!(signed_block.header.state_root, expected_root);
    assert_eq!(expected.utxo.utxos.values().map(|coin| coin.amount).sum::<u64>(), 98_900);

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
                envelope.clone(), CHAIN, GENESIS_NS, &genesis.hash
            ).unwrap();
            batch.put(
                frame_key(height as u64 + 1),
                record.encode_verified(CHAIN, GENESIS_NS, &genesis.hash).unwrap(),
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
        if db.get(MANIFEST_KEY).map_err(|e| e.to_string())?
            .ok_or("missing test-only manifest")? != manifest_bytes
        {
            return Err("checkpoint metadata differs from external full oracle".into());
        }
        let snapshot = db.get(SNAPSHOT_KEY).map_err(|e| e.to_string())?
            .ok_or("missing funding snapshot")?;
        if snapshot != funding_bytes {
            return Err("external UTXO snapshot was altered".into());
        }
        let coins: Vec<Utxo> = serde_json::from_slice(&snapshot).map_err(|e| e.to_string())?;
        let mut reconstructed = init_chain_state(CHAIN.to_owned());
        reconstructed.utxo.utxos.clear();
        reconstructed.utxo.address_index.clear();
        for coin in coins {
            if reconstructed.utxo.utxos.insert(coin.outpoint.clone(), coin.clone()).is_some() {
                return Err("duplicate seeded outpoint".into());
            }
            reconstructed.utxo.address_index.entry(coin.address.clone())
                .or_default().push(coin.outpoint.clone());
        }
        if reconstructed.utxo.compute_state_root().map_err(|e| e.to_string())? != funding_root {
            return Err("initial externally anchored UTXO root changed".into());
        }
        let mut retained = BTreeMap::new();
        let mut selected_map = BTreeMap::new();
        let mut transactions = Vec::new();
        let mut previous_retained = None;
        let first_kept = PREFIX + 2 - KEEP;
        for height in first_kept..=PREFIX + 1 {
            let bytes = db.get(frame_key(height)).map_err(|e| e.to_string())?
                .ok_or("retarget predecessor frame missing")?;
            let record = NativeV3EnvelopeRestoreRecordV1::decode_verified(
                &bytes, CHAIN, GENESIS_NS, &genesis.hash
            ).map_err(|e| e.to_string())?;
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
            CHAIN, &signed_block.hash, &retained, &selected_map
        ).map_err(|e| e.to_string())?;
        if restored_window != full_window
            || expected_difficulty_for_v3_window_ns(&restored_window)
                .map_err(|e| e.to_string())? != full_difficulty
        {
            return Err("retarget differs after pruning".into());
        }
        apply_validated_detached_transfer(&mut reconstructed, &transactions[0], PREFIX + 1)?;
        if reconstructed.utxo.compute_state_root().map_err(|e| e.to_string())? != expected_root {
            return Err("verified signed transaction replay differs from full oracle".into());
        }
        if validate_transaction_v2(&transactions[0], &reconstructed, CHAIN).is_ok() {
            return Err("spent transaction was accepted a second time".into());
        }
        if derive_dormant_native_v3_ghostdag_trial(
            CHAIN, GENESIS_NS, &genesis.hash,
            std::slice::from_ref(&signed_block.hash), &retained, 1
        ).is_ok() {
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
        db.put(MANIFEST_KEY, serde_json::to_vec(&forged_manifest).unwrap()).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(check_reopened(&db).is_err(), "forged fee checkpoint must fail");
        db.put(MANIFEST_KEY, &manifest_bytes).unwrap();
        let mut forged_coins = funding_coins.clone();
        forged_coins[0].amount += 1;
        db.put(SNAPSHOT_KEY, serde_json::to_vec(&forged_coins).unwrap()).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(check_reopened(&db).is_err(), "tampered checkpoint UTXO must fail");
        db.put(SNAPSHOT_KEY, &funding_bytes).unwrap();
        db.delete(frame_key(PREFIX + 1 - KEEP / 2)).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(check_reopened(&db).is_err(), "missing retarget frame must fail");
    }
}
