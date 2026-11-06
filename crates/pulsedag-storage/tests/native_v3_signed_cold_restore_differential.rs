//! TEST ONLY: conditional two-block signed-spend equivalence before/after
//! *cold* RocksDB reopen of strictly verified native-v3 envelope frames.
//! The original trusted synthetic UTXO prestate and explicit block order are
//! supplied by this test in BOTH runs. This is NOT a pruning checkpoint,
//! canonical GD-08 ordering, finality, real monetary issuance or mainnet GO.

use std::{
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
    native_v3_signed_prestate_trial::inspect_dormant_native_v3_explicit_order_trial,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    signing_message_v2,
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
};
use pulsedag_storage::{
    verify_dormant_v3_genesis_anchored_linear_segment, NativeV3EnvelopeRestoreRecordV1,
};
use rocksdb::{Options, WriteBatch, WriteOptions, DB};

const CHAIN: &str = "TEST-ONLY-native-v3-signed-cold-frame-replay";
const GENESIS_NS: u64 = 1_800_000_101_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;
const FIRST_KEY: &[u8] = b"__TEST_ONLY_V3_SIGNED_FRAME_A";
const SECOND_KEY: &[u8] = b"__TEST_ONLY_V3_SIGNED_FRAME_B";

struct TemporaryDb(PathBuf);

impl TemporaryDb {
    fn new() -> Self {
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-v3-signed-cold-differential-{}-{ns}",
            std::process::id()
        )))
    }

    fn open(&self) -> DB {
        let mut options = Options::default();
        options.create_if_missing(true);
        DB::open(&options, &self.0).unwrap()
    }
}

impl Drop for TemporaryDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn signer() -> SigningKey {
    SigningKey::from_bytes(&[53; 32])
}

fn address() -> String {
    address_from_public_key(&hex::encode(signer().verifying_key().to_bytes()))
}

fn synthetic_prestate() -> (ChainState, OutPoint) {
    let mut state = init_chain_state_v3(CHAIN.into(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(!state.contracts.config.enabled);
    let source = OutPoint {
        txid: "explicitly-synthetic-funded-input".into(),
        index: 0,
    };
    state.utxo.utxos.insert(
        source.clone(),
        Utxo {
            outpoint: source.clone(),
            address: address(),
            amount: 100,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(address())
        .or_default()
        .push(source.clone());
    (state, source)
}

fn signed(input: OutPoint, output: u64, fee: u64, nonce: u64) -> Transaction {
    let key = signer();
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: input,
            public_key: hex::encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: address(),
            amount: output,
        }],
        fee,
        nonce,
    };
    tx.inputs[0].signature = hex::encode(
        key.sign(&signing_message_v2(&tx, CHAIN).unwrap())
            .to_bytes(),
    );
    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
    tx
}

fn mine(parent: &str, height: u64, tx: Transaction) -> BlockEnvelopeV3 {
    let transactions = vec![
        build_reward_claim_transaction_v3("pulse1synthetic-miner", height, CHAIN).unwrap(),
        tx,
    ];
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.to_owned()],
        timestamp_ns: GENESIS_NS + height * 500_000_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "33".repeat(32), // Deliberately FALSE monetary root.
        blue_score: height,
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        }
    }
    panic!("easy real-PoW test-only nonce search exhausted");
}

fn fixture() -> (ChainState, String, [BlockEnvelopeV3; 2]) {
    let (state, funded) = synthetic_prestate();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first_spend = signed(funded, 90, 10, 1);
    let second_spend = signed(
        OutPoint {
            txid: first_spend.txid.clone(),
            index: 0,
        },
        83,
        7,
        2,
    );
    let first = mine(&genesis.hash, 1, first_spend);
    let second = mine(&first.hash, 2, second_spend);
    (state, genesis.hash, [first, second])
}

fn encode_frames(genesis: &str, blocks: &[BlockEnvelopeV3; 2]) -> [Vec<u8>; 2] {
    blocks.clone().map(|block| {
        NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(block, CHAIN, GENESIS_NS, genesis)
            .unwrap()
            .encode_verified(CHAIN, GENESIS_NS, genesis)
            .unwrap()
    })
}

fn write_frames(db: &DB, frames: &[Vec<u8>; 2]) {
    let mut batch = WriteBatch::default();
    batch.put(FIRST_KEY, &frames[0]);
    batch.put(SECOND_KEY, &frames[1]);
    let mut options = WriteOptions::default();
    options.set_sync(true);
    db.write_opt(batch, &options).unwrap();
}

fn reload_frames(db: &DB, genesis: &str) -> Result<[BlockEnvelopeV3; 2], String> {
    let mut verified = Vec::new();
    for key in [FIRST_KEY, SECOND_KEY] {
        let raw = db
            .get(key)
            .map_err(|err| format!("private test RocksDB read: {err}"))?
            .ok_or("missing required signed v3 frame")?;
        let record =
            NativeV3EnvelopeRestoreRecordV1::decode_verified(&raw, CHAIN, GENESIS_NS, genesis)
                .map_err(|err| format!("strict frame decode: {err}"))?;
        verified.push(record);
    }
    let linkage =
        verify_dormant_v3_genesis_anchored_linear_segment(&verified, CHAIN, GENESIS_NS, genesis)
            .map_err(|err| format!("strict near-genesis linkage: {err}"))?;
    if linkage.linkage_checked_records != 2 {
        return Err("unexpected signed frame count".to_owned());
    }
    let mut envelopes = verified.into_iter().map(|record| record.envelope().clone());
    Ok([envelopes.next().unwrap(), envelopes.next().unwrap()])
}

#[test]
fn cold_reopen_signed_utxo_root_and_fees_match_full_explicit_replay() {
    let (trusted_test_state, genesis, original) = fixture();
    let initial_root = trusted_test_state.utxo.compute_state_root().unwrap();
    let baseline =
        inspect_dormant_native_v3_explicit_order_trial(&original, &trusted_test_state).unwrap();
    assert_eq!(baseline.signed_transactions, 2);
    assert_eq!(baseline.candidate_fees, 17);
    assert_eq!(baseline.fees_by_block.get(&original[0].hash), Some(&10));
    assert_eq!(baseline.fees_by_block.get(&original[1].hash), Some(&7));
    // The block headers never committed to actual UTXO state.
    assert_ne!(baseline.resulting_utxo_root, original[1].header.state_root);

    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &encode_frames(&genesis, &original));
    }
    {
        let db = disk.open();
        let reloaded = reload_frames(&db, &genesis).unwrap();
        let restored =
            inspect_dormant_native_v3_explicit_order_trial(&reloaded, &trusted_test_state).unwrap();
        assert_eq!(restored, baseline);
        assert_eq!(reloaded[0].hash, original[0].hash);
        assert_eq!(reloaded[1].hash, original[1].hash);
        assert!(inspect_dormant_native_v3_explicit_order_trial(
            &[reloaded[1].clone(), reloaded[0].clone()],
            &trusted_test_state,
        )
        .is_err());
    }
    assert_eq!(
        trusted_test_state.utxo.compute_state_root().unwrap(),
        initial_root
    );
}

#[test]
fn persisted_missing_or_corrupted_later_signed_frame_fails_before_fee_claim() {
    let (trusted_test_state, genesis, blocks) = fixture();
    let original_root = trusted_test_state.utxo.compute_state_root().unwrap();
    let frames = encode_frames(&genesis, &blocks);
    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &frames);
        let mut corrupted = frames[1].clone();
        let flip = corrupted.len() / 2;
        corrupted[flip] ^= 1;
        db.put(SECOND_KEY, corrupted).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(reload_frames(&db, &genesis).is_err());
        db.delete(SECOND_KEY).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(reload_frames(&db, &genesis).is_err());
    }
    assert_eq!(
        trusted_test_state.utxo.compute_state_root().unwrap(),
        original_root
    );
}

#[test]
fn cold_reopen_rejects_distinct_signed_double_spend_without_crediting_later_fee() {
    let (trusted_test_state, original_input) = synthetic_prestate();
    let original_root = trusted_test_state.utxo.compute_state_root().unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine(&genesis.hash, 1, signed(original_input.clone(), 90, 10, 1));
    let second = mine(&first.hash, 2, signed(original_input, 80, 20, 2));
    assert_ne!(first.transactions[1].txid, second.transactions[1].txid);
    let blocks = [first, second];

    // The earlier signed spend is valid by itself, but the complete order
    // must fail rather than silently skipping the conflicting second spend.
    let first_only =
        inspect_dormant_native_v3_explicit_order_trial(&blocks[..1], &trusted_test_state).unwrap();
    assert_eq!(first_only.signed_transactions, 1);
    assert_eq!(first_only.candidate_fees, 10);

    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &encode_frames(&genesis.hash, &blocks));
    }
    {
        let db = disk.open();
        let recovered = reload_frames(&db, &genesis.hash).unwrap();
        assert_eq!(recovered[0].hash, blocks[0].hash);
        assert_eq!(recovered[1].hash, blocks[1].hash);
        let result =
            inspect_dormant_native_v3_explicit_order_trial(&recovered, &trusted_test_state);
        assert!(result.is_err());
    }
    assert_eq!(
        trusted_test_state.utxo.compute_state_root().unwrap(),
        original_root
    );
}

#[test]
fn cold_reopen_authentic_pow_frame_with_forged_signature_rejects_signed_replay() {
    let (trusted_test_state, genesis, original) = fixture();
    let original_root = trusted_test_state.utxo.compute_state_root().unwrap();
    let mut forged_spend = original[1].transactions[1].clone();
    forged_spend.inputs[0].signature = "00".repeat(64);
    forged_spend.txid = compute_txid_v2(&forged_spend, CHAIN).unwrap();

    // Re-mine the new Merkle commitment: even a PoW-valid, strictly persisted
    // v3 frame must not authorize a transfer with a forged Ed25519 signature.
    let forged_second = mine(&original[0].hash, 2, forged_spend);
    let blocks = [original[0].clone(), forged_second];
    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &encode_frames(&genesis, &blocks));
    }
    {
        let db = disk.open();
        let recovered = reload_frames(&db, &genesis).unwrap();
        assert_eq!(recovered[1].hash, blocks[1].hash);
        let result =
            inspect_dormant_native_v3_explicit_order_trial(&recovered, &trusted_test_state);
        assert!(result.is_err());
    }
    assert_eq!(
        trusted_test_state.utxo.compute_state_root().unwrap(),
        original_root
    );
}

#[test]
fn cold_reopen_signed_frames_do_not_authenticate_missing_or_tampered_funding_prestate() {
    // The same valid PoW/Ed25519 frames may be recovered without recovering
    // the UTXO that a caller had injected into the full-history hypothesis.
    // Authenticated framing MUST NOT invent an independently trusted prestate.
    let (trusted_test_state, genesis, blocks) = fixture();
    let original_root = trusted_test_state.utxo.compute_state_root().unwrap();
    let no_funding = init_chain_state_v3(CHAIN.into(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(!no_funding.contracts.config.enabled);
    assert!(no_funding.utxo.utxos.is_empty());

    let input = blocks[0].transactions[1].inputs[0].previous_output.clone();
    let mut tampered_funding = trusted_test_state.clone();
    tampered_funding.utxo.utxos.get_mut(&input).unwrap().amount = 101;
    let altered_root = tampered_funding.utxo.compute_state_root().unwrap();
    assert_ne!(altered_root, original_root);

    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &encode_frames(&genesis, &blocks));
    }
    {
        let db = disk.open();
        let restored = reload_frames(&db, &genesis).unwrap();
        assert_eq!(restored[0].hash, blocks[0].hash);
        assert_eq!(restored[1].hash, blocks[1].hash);

        let baseline =
            inspect_dormant_native_v3_explicit_order_trial(&restored, &trusted_test_state).unwrap();
        assert_eq!(baseline.signed_transactions, 2);
        assert_eq!(baseline.candidate_fees, 17);
        assert_ne!(baseline.resulting_utxo_root, restored[1].header.state_root);

        // No funding UTXO in the real genesis-derived state: signed frames
        // cannot prove the previously injected synthetic 100-unit coin.
        assert!(inspect_dormant_native_v3_explicit_order_trial(&restored, &no_funding).is_err());
        // A plausible but different balance for that same outpoint must not
        // let an attacker claim the original 17-unit signed fee sequence.
        assert!(
            inspect_dormant_native_v3_explicit_order_trial(&restored, &tampered_funding).is_err()
        );
    }
    assert_eq!(
        trusted_test_state.utxo.compute_state_root().unwrap(),
        original_root
    );
    assert_eq!(
        tampered_funding.utxo.compute_state_root().unwrap(),
        altered_root
    );
}

#[test]
fn cold_reopen_conditional_signed_utxo_roots_detect_pow_valid_false_commitment() {
    use pulsedag_core::apply::apply_transaction;

    // This is intentionally a caller-ordered, synthetic-prestate diagnostic;
    // a matching root proves neither canonical GD-08 nor checkpoint trust.
    fn remine(block: &BlockEnvelopeV3, parent: &str, committed_root: &str) -> BlockEnvelopeV3 {
        let mut header = block.header.clone();
        header.parents = vec![parent.to_owned()];
        header.state_root = committed_root.to_owned();
        for nonce in 0..4096_u64 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                return build_block_envelope_v3(CHAIN, header, block.transactions.clone()).unwrap();
            }
        }
        panic!("TEST-ONLY easy-target remine exceeded nonce budget");
    }

    fn check_conditional_roots(
        blocks: &[BlockEnvelopeV3; 2],
        prestate: &ChainState,
    ) -> Result<u64, String> {
        let mut total_fees = 0;
        for (index, block) in blocks.iter().enumerate() {
            let hypothesis =
                inspect_dormant_native_v3_explicit_order_trial(&blocks[..=index], prestate)
                    .map_err(|err| err.to_string())?;
            if hypothesis.resulting_utxo_root != block.header.state_root {
                return Err("caller-ordered signed UTXO root differs from v3 header".to_owned());
            }
            total_fees = hypothesis.candidate_fees;
        }
        Ok(total_fees)
    }

    let (trusted_prestate, genesis, originals) = fixture();
    let initial_root = trusted_prestate.utxo.compute_state_root().unwrap();
    let mut independent = trusted_prestate.clone();
    apply_transaction(&originals[0].transactions[1], &mut independent, 1).unwrap();
    let first_root = independent.utxo.compute_state_root().unwrap();
    apply_transaction(&originals[1].transactions[1], &mut independent, 2).unwrap();
    let second_root = independent.utxo.compute_state_root().unwrap();
    assert_ne!(first_root, second_root);

    let first = remine(&originals[0], &genesis, &first_root);
    let second = remine(&originals[1], &first.hash, &second_root);
    let committed = [first, second];
    assert_eq!(
        check_conditional_roots(&committed, &trusted_prestate),
        Ok(17)
    );

    let disk = TemporaryDb::new();
    {
        let db = disk.open();
        write_frames(&db, &encode_frames(&genesis, &committed));
    }
    {
        let db = disk.open();
        let recovered = reload_frames(&db, &genesis).unwrap();
        assert_eq!(recovered[0].hash, committed[0].hash);
        assert_eq!(recovered[1].hash, committed[1].hash);
        assert_eq!(
            check_conditional_roots(&recovered, &trusted_prestate),
            Ok(17)
        );

        // The attacker recomputes block hash, Merkle-consistent v3 header and
        // genuine PoW with a fake state root; frame authentication still passes.
        let bad_root = "ff".repeat(32);
        assert_ne!(bad_root, second_root);
        let falsified = [
            committed[0].clone(),
            remine(&originals[1], &committed[0].hash, &bad_root),
        ];
        write_frames(&db, &encode_frames(&genesis, &falsified));
    }
    {
        let db = disk.open();
        let recovered = reload_frames(&db, &genesis).unwrap();
        assert!(check_conditional_roots(&recovered, &trusted_prestate).is_err());
    }
    assert_eq!(
        trusted_prestate.utxo.compute_state_root().unwrap(),
        initial_root
    );
}
