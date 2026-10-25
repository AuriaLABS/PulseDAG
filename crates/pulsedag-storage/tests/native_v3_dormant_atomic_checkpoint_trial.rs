//! TEST ONLY: RocksDB atomic snapshot + history trim, compared to a verified
//! full native-v3 PoW DAG. All persisted names are isolated test-private keys.
//! This is NOT a real checkpoint authority, consensus pruning, finality proof,
//! transaction/fee/reward acceptance, crash injection, or live node restart.
//! The *external* test oracle is reconstructed from full genesis BEFORE trim.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    monetary_v3::{subsidy_atoms_for_score, PRODUCTION_CADENCE_V3},
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_v3::{expected_difficulty_for_v3_window_ns, V3RetargetSample},
    state::UtxoState,
    types::{compute_merkle_root, BlockId, OutPoint, Utxo},
};
use pulsedag_storage::NativeV3EnvelopeRestoreRecordV1;
use rocksdb::{Options, WriteBatch, WriteOptions, DB};

#[path = "../../pulsedag-core/tests/support/native_v3_trial_order_preview.rs"]
#[allow(dead_code)]
mod trial_order_preview;

const CHAIN: &str = "dormant-v3-ATOMIC-checkpoint-trial-NOT-PRODUCTION";
const GENESIS_NS: u64 = 1_800_000_030_000_000_000;
const FULL_HEIGHT: usize = 26;
const KEEP: usize = 20;
const MANIFEST_KEY: &[u8] = b"__TEST_ONLY_V3_TRIM_ATOMIC_MANIFEST";
const ECONOMIC_KEY: &[u8] = b"__TEST_ONLY_V3_TRIM_ATOMIC_HYPOTHETICAL_UTXO";
const OLD_MARKER: &[u8] = b"__TEST_ONLY_V3_TRIM_OLD_FULL_PREFIX_MARKER";

fn frame_key(height: usize) -> Vec<u8> {
    format!("__TEST_ONLY_V3_TRIM_FRAME_{height:03}").into_bytes()
}

struct DisposableDb(PathBuf);

impl DisposableDb {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-v3-atomic-trim-test-{}-{nanos}",
            std::process::id()
        )))
    }

    fn open(&self) -> DB {
        let mut opt = Options::default();
        opt.create_if_missing(true);
        DB::open(&opt, &self.0).unwrap()
    }
}
impl Drop for DisposableDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mine(parent: &BlockEnvelopeV3, height: u64) -> BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.hash.clone()],
        timestamp_ns: GENESIS_NS + height * 500_000_000,
        difficulty: parent.header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "77".repeat(32), // NOT the economic root.
        blue_score: height,
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("test-only PoW fixture exhausted");
}

/// Precisely matched hypothetical post-finality credits, not real coinbase
/// settlement. Every record has zero transactions and an unverified header
/// state root. The actual v3 subsidy curve is used ONLY for bookkeeping.
fn hypothetical_coins(records: &[NativeV3EnvelopeRestoreRecordV1]) -> Vec<Utxo> {
    records
        .iter()
        .map(|record| {
            let env = record.envelope();
            Utxo {
                outpoint: OutPoint {
                    txid: env.hash.clone(),
                    index: 0,
                },
                address: format!("trial-only-miner-{}", env.header.height),
                amount: subsidy_atoms_for_score(env.header.blue_score, &PRODUCTION_CADENCE_V3)
                    .unwrap(),
                coinbase: true,
                height: env.header.height,
            }
        })
        .collect()
}

fn reconstructed_state(coins: &[Utxo]) -> Result<(String, u128), String> {
    let mut state = UtxoState::default();
    let mut supply = 0u128;
    for coin in coins {
        if state
            .utxos
            .insert(coin.outpoint.clone(), coin.clone())
            .is_some()
        {
            return Err("duplicate test-only output".into());
        }
        state
            .address_index
            .entry(coin.address.clone())
            .or_default()
            .push(coin.outpoint.clone());
        supply = supply
            .checked_add(u128::from(coin.amount))
            .ok_or("supply overflow")?;
    }
    state
        .validate_deterministic_indexes()
        .map_err(|e| e.to_string())?;
    let root = state.compute_state_root().map_err(|e| e.to_string())?;
    Ok((root, supply))
}

struct FullHistoryOracle {
    genesis: BlockId,
    records: Vec<NativeV3EnvelopeRestoreRecordV1>,
    /// Independently generated after complete native-v3 KHeavyHash/GHOSTDAG
    /// verification, held OUTSIDE RocksDB and never learned from the manifest.
    manifest: Vec<u8>,
    coins: Vec<u8>,
    expected_window: Vec<V3RetargetSample>,
    expected_root: String,
    expected_supply: u128,
}

fn full_history_oracle() -> FullHistoryOracle {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut all = BTreeMap::from([(genesis.hash.clone(), genesis.header.clone())]);
    let mut parent = genesis.clone();
    let mut records = Vec::new();
    for height in 1..=FULL_HEIGHT {
        let envelope = mine(&parent, height as u64);
        all.insert(envelope.hash.clone(), envelope.header.clone());
        records.push(
            NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                envelope.clone(),
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .unwrap(),
        );
        parent = envelope;
    }
    let tip = parent.hash.clone();
    let verified = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&tip),
        &all,
        1,
    )
    .unwrap();
    assert_eq!(verified.blocks.len(), FULL_HEIGHT + 1);
    let parent_map = verified
        .blocks
        .iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
        .collect();
    let window = selected_parent_retarget_window_v3_ns(CHAIN, &tip, &all, &parent_map).unwrap();
    assert_eq!(window.len(), KEEP);
    let decision = expected_difficulty_for_v3_window_ns(&window).unwrap();
    assert_eq!(decision.observed_intervals, KEEP - 1);
    assert_eq!(decision.average_interval_ns, 500_000_000);

    let coins = hypothetical_coins(&records);
    let (root, supply) = reconstructed_state(&coins).unwrap();
    let retained = records
        .iter()
        .skip(FULL_HEIGHT - KEEP)
        .map(|r| r.envelope().hash.clone())
        .collect::<Vec<_>>();
    let manifest = serde_json::json!({
        "schema": "TEST_ONLY_not_trusted_v1",
        "chain": CHAIN,
        "genesis": genesis.hash,
        "tip": tip,
        "trial_k": 1,
        "blue_score": verified.blocks[&tip].blue_score,
        "blue_work_u512_limbs": verified.blocks[&tip].blue_work.0,
        "preview_utxo_root": root,
        "preview_supply_atoms": supply.to_string(),
        "retarget_bits": decision.expected_bits,
        "retained_20_ascending": retained
    });
    FullHistoryOracle {
        genesis: genesis.hash,
        records,
        manifest: serde_json::to_vec(&manifest).unwrap(),
        coins: serde_json::to_vec(&coins).unwrap(),
        expected_window: window,
        expected_root: root,
        expected_supply: supply,
    }
}

fn trim_batch(oracle: &FullHistoryOracle) -> WriteBatch {
    let mut batch = WriteBatch::default();
    for height in 1..=FULL_HEIGHT - KEEP {
        batch.delete(frame_key(height));
    }
    batch.delete(OLD_MARKER);
    batch.put(MANIFEST_KEY, &oracle.manifest);
    batch.put(ECONOMIC_KEY, &oracle.coins);
    batch
}

fn read_authenticated_trial_after_trim(db: &DB, oracle: &FullHistoryOracle) -> Result<(), String> {
    let manifest = db
        .get(MANIFEST_KEY)
        .map_err(|e| e.to_string())?
        .ok_or("checkpoint manifest missing")?;
    let ledger = db
        .get(ECONOMIC_KEY)
        .map_err(|e| e.to_string())?
        .ok_or("economic snapshot missing")?;
    if manifest.as_ref() != oracle.manifest || ledger.as_ref() != oracle.coins {
        return Err(
            "persisted checkpoint or economic snapshot differs from full-history authority".into(),
        );
    }
    let coins: Vec<Utxo> = serde_json::from_slice(&ledger).map_err(|e| e.to_string())?;
    if coins.len() != FULL_HEIGHT {
        return Err("incomplete economic snapshot".into());
    }
    let (root, supply) = reconstructed_state(&coins)?;
    if root != oracle.expected_root || supply != oracle.expected_supply {
        return Err("economic snapshot root/supply differs from independent full replay".into());
    }

    let mut retained = BTreeMap::new();
    let mut selected = BTreeMap::new();
    let mut previous = None;
    for height in FULL_HEIGHT - KEEP + 1..=FULL_HEIGHT {
        let raw = db
            .get(frame_key(height))
            .map_err(|e| e.to_string())?
            .ok_or("mandatory 20-sample header missing")?;
        let record = NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &raw,
            CHAIN,
            GENESIS_NS,
            &oracle.genesis,
        )
        .map_err(|e| e.to_string())?;
        let env = record.envelope();
        if env.header.height != height as u64
            || env.header.parents.len() != 1
            || env.header.timestamp_ns != GENESIS_NS + height as u64 * 500_000_000
        {
            return Err("retained header height/parent/time mismatch".into());
        }
        if let Some(previous_hash) = &previous {
            if &env.header.parents[0] != previous_hash {
                return Err("retained selected chain is disconnected".into());
            }
        }
        previous = Some(env.hash.clone());
        selected.insert(env.hash.clone(), Some(env.header.parents[0].clone()));
        retained.insert(env.hash.clone(), env.header.clone());
    }
    let tip = previous.ok_or("retained selection missing")?;
    if oracle.records.last().unwrap().envelope().hash != tip {
        return Err("retained tip diverges from independently authenticated DAG".into());
    }
    let window = selected_parent_retarget_window_v3_ns(CHAIN, &tip, &retained, &selected)
        .map_err(|e| e.to_string())?;
    if window != oracle.expected_window {
        return Err("post-prune retarget differs from full-genesis replay".into());
    }
    let full_decision =
        expected_difficulty_for_v3_window_ns(&oracle.expected_window).map_err(|e| e.to_string())?;
    let compact_decision =
        expected_difficulty_for_v3_window_ns(&window).map_err(|e| e.to_string())?;
    if full_decision != compact_decision {
        return Err("post-prune difficulty decision changed".into());
    }
    // The ordinary full-genesis trial MUST continue to reject this truncated
    // DAG. Snapshot test trust does not bypass real consensus fail-closed guards.
    if derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &oracle.genesis,
        std::slice::from_ref(&tip),
        &retained,
        1,
    )
    .is_ok()
    {
        return Err("TRUST BOUNDARY: truncated history became accepted consensus".into());
    }
    Ok(())
}

#[test]
fn atomic_rocksdb_trial_snapshot_and_prune_matches_complete_v3_dag_oracle() {
    let oracle = full_history_oracle();
    let disk = DisposableDb::new();
    {
        let db = disk.open();
        let mut initial = WriteBatch::default();
        for (i, record) in oracle.records.iter().enumerate() {
            initial.put(
                frame_key(i + 1),
                record
                    .encode_verified(CHAIN, GENESIS_NS, &oracle.genesis)
                    .unwrap(),
            );
        }
        initial.put(OLD_MARKER, b"pretrim");
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(initial, &sync).unwrap();

        // Fault injection BEFORE the atomic WriteBatch call: dropping an
        // uncommitted batch cannot publish a manifest or delete history.
        drop(trim_batch(&oracle));
    } // Close and reopen actual disposable RocksDB.
    {
        let db = disk.open();
        assert!(db.get(MANIFEST_KEY).unwrap().is_none());
        assert!(db.get(ECONOMIC_KEY).unwrap().is_none());
        assert!(db.get(OLD_MARKER).unwrap().is_some());
        for height in 1..=FULL_HEIGHT {
            assert!(db.get(frame_key(height)).unwrap().is_some());
        }
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(trim_batch(&oracle), &sync).unwrap();
    } // No in-process DB instance is reused.
    {
        let db = disk.open();
        assert!(db.get(OLD_MARKER).unwrap().is_none());
        for height in 1..=FULL_HEIGHT - KEEP {
            assert!(db.get(frame_key(height)).unwrap().is_none());
        }
        read_authenticated_trial_after_trim(&db, &oracle).unwrap();
    }
    {
        let db = disk.open();
        let mut untrusted: serde_json::Value =
            serde_json::from_slice(&db.get(MANIFEST_KEY).unwrap().unwrap()).unwrap();
        untrusted["blue_work_u512_limbs"][0] = serde_json::json!(999);
        db.put(MANIFEST_KEY, serde_json::to_vec(&untrusted).unwrap())
            .unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_authenticated_trial_after_trim(&db, &oracle).is_err(),
            "self-attested blue-work cannot replace independent full-history oracle"
        );
        db.put(MANIFEST_KEY, &oracle.manifest).unwrap();
        let mut forged_coins: Vec<Utxo> = serde_json::from_slice(&oracle.coins).unwrap();
        forged_coins[0].amount += 1;
        db.put(ECONOMIC_KEY, serde_json::to_vec(&forged_coins).unwrap())
            .unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_authenticated_trial_after_trim(&db, &oracle).is_err(),
            "mutated economic state must fail even if the header PoW is valid"
        );
        db.put(ECONOMIC_KEY, &oracle.coins).unwrap();
        db.delete(frame_key(FULL_HEIGHT - 10)).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_authenticated_trial_after_trim(&db, &oracle).is_err(),
            "an omitted retained PoW retarget sample must fail closed"
        );
    }
}


/// TEST ONLY; supports one diagnostic three-way shape, not arbitrary persisted
/// checkpoints. A later independently verified full-genesis oracle is retained
/// exclusively in the test process and never reconstructed from stored bytes.
fn mine_side_or_rejoin(
    parents: &[BlockEnvelopeV3],
    height: u64,
    score: u64,
    marker: u8,
) -> BlockEnvelopeV3 {
    let mut parent_ids: Vec<_> = parents.iter().map(|p| p.hash.clone()).collect();
    parent_ids.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: parent_ids,
        timestamp_ns: GENESIS_NS + height * 500_000_000 + u64::from(marker) * 10_000,
        difficulty: parents[0].header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: format!("{marker:02x}").repeat(32), // NOT economic authority.
        blue_score: score,
        height,
    };
    for nonce in 0..4096 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("easy PoW test fixture nonce exhausted");
}

fn frontier_key(id: &str) -> Vec<u8> {
    format!("__TEST_ONLY_V3_MULTIPARENT_FRONTIER_{id}").into_bytes()
}

/// Real disposable RocksDB, real KHeavyHash and a real two-parent join across
/// an atomic trim boundary. Comparison data was independently fully verified
/// BEFORE prune. Does NOT claim a universal anticone proof or portable trust.
#[test]
fn atomic_prune_restores_authentic_two_parent_frontier_for_k0_red_and_k1_blue() {
    use std::collections::BTreeSet;

    let full_linear = full_history_oracle();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    assert_eq!(genesis.hash, full_linear.genesis);
    let linear: Vec<BlockEnvelopeV3> = full_linear.records.iter()
        .map(|r| r.envelope().clone()).collect();
    let other_at_boundary = mine_side_or_rejoin(&[linear[24].clone()], 26, 26, 0xa3);
    assert_ne!(other_at_boundary.hash, linear[25].hash);
    let (winner, loser) = if linear[25].hash < other_at_boundary.hash {
        (linear[25].clone(), other_at_boundary)
    } else {
        (other_at_boundary, linear[25].clone())
    };

    for k in [0_usize, 1] {
        let joined = mine_side_or_rejoin(
            &[winner.clone(), loser.clone()], 27, 27 + k as u64, 0xb3 + k as u8
        );
        let child = mine_side_or_rejoin(&[joined.clone()], 28, 28 + k as u64, 0xc3 + k as u8);

        let mut records: BTreeMap<BlockId, BlockEnvelopeV3> =
            linear.iter().map(|b| (b.hash.clone(), b.clone())).collect();
        for block in [&winner, &loser, &joined, &child] {
            records.insert(block.hash.clone(), block.clone());
        }
        assert_eq!(records.len(), FULL_HEIGHT + 3);
        let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header.clone())]);
        headers.extend(records.iter().map(|(id, envelope)| (id.clone(), envelope.header.clone())));
        let verified = derive_dormant_native_v3_ghostdag_trial(
            CHAIN, GENESIS_NS, &genesis.hash, std::slice::from_ref(&child.hash), &headers, k
        ).unwrap();
        assert_eq!(verified.blocks.len(), FULL_HEIGHT + 4);
        let selected_meta = &verified.blocks[&joined.hash];
        assert_eq!(selected_meta.selected_parent.as_ref(), Some(&winner.hash));
        assert_eq!(selected_meta.merge_blues, if k == 1 { vec![loser.hash.clone()] } else { vec![] });
        assert_eq!(selected_meta.merge_reds, if k == 0 { vec![loser.hash.clone()] } else { vec![] });
        assert_eq!(selected_meta.blue_work.0[0], (27 + k as u64) * 2);
        assert_eq!(verified.blocks[&child.hash].blue_work.0[0], (28 + k as u64) * 2);
        assert_eq!(verified.blocks[&child.hash].blue_score, 28 + k as u64);

        let ordered = trial_order_preview::preview_unapproved_native_v3_order(
            CHAIN, GENESIS_NS, &genesis.hash, std::slice::from_ref(&child.hash), &headers, k
        ).unwrap();
        assert_eq!(ordered.ordered_blocks.len(), FULL_HEIGHT + 4);
        let detached_coins: Vec<Utxo> = ordered.ordered_blocks.iter()
            .filter(|id| *id != &genesis.hash)
            .map(|id| {
                let block = &records[id];
                Utxo {
                    outpoint: OutPoint { txid: id.clone(), index: 0 },
                    address: format!("TEST-NOT-SPENDABLE-{id}"),
                    amount: subsidy_atoms_for_score(
                        verified.blocks[id].blue_score, &PRODUCTION_CADENCE_V3
                    ).unwrap(),
                    coinbase: true,
                    height: block.header.height,
                }
            })
            .collect();
        let (expected_root, expected_supply) = reconstructed_state(&detached_coins).unwrap();

        let selected_map: BTreeMap<_, _> = verified.blocks.iter()
            .map(|(id, m)| (id.clone(), m.selected_parent.clone())).collect();
        let full_window = selected_parent_retarget_window_v3_ns(
            CHAIN, &child.hash, &headers, &selected_map
        ).unwrap();
        assert_eq!(full_window.len(), KEEP);
        let full_retarget = expected_difficulty_for_v3_window_ns(&full_window).unwrap();
        let mut tip_first = Vec::new();
        let mut walk = child.hash.clone();
        for _ in 0..KEEP {
            tip_first.push(walk.clone());
            walk = verified.blocks[&walk].selected_parent.clone().unwrap();
        }
        assert_eq!(tip_first[0], child.hash);
        assert!(tip_first.contains(&winner.hash));
        let mut retained: BTreeSet<BlockId> = tip_first.iter().cloned().collect();
        retained.insert(loser.hash.clone());
        assert_eq!(retained.len(), KEEP + 1);
        assert!(!retained.contains(&genesis.hash));
        assert!(!retained.contains(&linear[0].hash));

        // The manifest is an INFORMATIONAL TEST record, not a signature,
        // accepted snapshot or independently trusted checkpoint.
        let manifest = serde_json::json!({
            "schema": "TEST_ONLY_fork_frontier_v1",
            "chain": CHAIN,
            "genesis": genesis.hash,
            "tip": child.hash,
            "trial_k": k,
            "selected_parent": winner.hash,
            "merge_blues": selected_meta.merge_blues,
            "merge_reds": selected_meta.merge_reds,
            "blue_work_limbs": verified.blocks[&child.hash].blue_work.0,
            "ordered_trial_digest": ordered.digest,
            "retarget_bits": full_retarget.expected_bits,
            "detached_preview_root": expected_root,
            "detached_supply": expected_supply.to_string()
        });
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let economic_bytes = serde_json::to_vec(&detached_coins).unwrap();
        let all_encoded: BTreeMap<BlockId, Vec<u8>> = records.iter()
            .map(|(id, envelope)| {
                let r = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                    envelope.clone(), CHAIN, GENESIS_NS, &genesis.hash
                ).unwrap();
                (id.clone(), r.encode_verified(CHAIN, GENESIS_NS, &genesis.hash).unwrap())
            })
            .collect();
        let disk = DisposableDb::new();
        let manifest_key = b"__TEST_ONLY_V3_MULTIPARENT_ATOMIC_MANIFEST";
        let economic_key = b"__TEST_ONLY_V3_MULTIPARENT_ATOMIC_PREVIEW_UTXO";
        {
            let db = disk.open();
            let mut batch = WriteBatch::default();
            for (id, encoded) in &all_encoded {
                batch.put(frontier_key(id), encoded);
            }
            let mut sync = WriteOptions::default();
            sync.set_sync(true);
            db.write_opt(batch, &sync).unwrap();
            let mut interrupted = WriteBatch::default();
            interrupted.put(manifest_key, &manifest_bytes);
            interrupted.delete(frontier_key(&linear[0].hash));
            drop(interrupted);
        }
        {
            let db = disk.open();
            assert!(db.get(manifest_key).unwrap().is_none());
            assert!(db.get(frontier_key(&linear[0].hash)).unwrap().is_some());
            let mut batch = WriteBatch::default();
            for id in records.keys() {
                if !retained.contains(id) {
                    batch.delete(frontier_key(id));
                }
            }
            batch.put(manifest_key, &manifest_bytes);
            batch.put(economic_key, &economic_bytes);
            let mut sync = WriteOptions::default();
            sync.set_sync(true);
            db.write_opt(batch, &sync).unwrap();
        }
        let check_after_reopen = |db: &DB| -> Result<(), String> {
            let persisted_manifest = db.get(manifest_key).map_err(|e| e.to_string())?
                .ok_or("missing independent checkpoint manifest")?;
            let persisted_economic = db.get(economic_key).map_err(|e| e.to_string())?
                .ok_or("missing economic snapshot")?;
            if persisted_manifest != manifest_bytes || persisted_economic != economic_bytes {
                return Err("stored manifest or detached preview differs from preauthenticated oracle".into());
            }
            let decoded_coins: Vec<Utxo> = serde_json::from_slice(&persisted_economic)
                .map_err(|e| e.to_string())?;
            if reconstructed_state(&decoded_coins)? != (expected_root.clone(), expected_supply) {
                return Err("after-prune UTXO snapshot root/supply differs from full replay".into());
            }
            let mut bounded = BTreeMap::new();
            for id in &retained {
                let raw = db.get(frontier_key(id)).map_err(|e| e.to_string())?
                    .ok_or("missing retained authentic side or selected header")?;
                let verified_record = NativeV3EnvelopeRestoreRecordV1::decode_verified(
                    &raw, CHAIN, GENESIS_NS, &genesis.hash
                ).map_err(|e| e.to_string())?;
                if verified_record.envelope().hash != *id {
                    return Err("stored header identity mismatches preauthenticated ID".into());
                }
                bounded.insert(id.clone(), verified_record.envelope().header.clone());
            }
            if bounded.len() != KEEP + 1 || !bounded.contains_key(&loser.hash) {
                return Err("missing necessary crossing-boundary side frontier".into());
            }
            // Exactly one predecessor gap at the oldest selected retarget
            // sample; the side-parent and two-parent join remain fully linked.
            for (id, header) in &bounded {
                if id != tip_first.last().unwrap()
                    && header.parents.iter().any(|parent| !bounded.contains_key(parent))
                {
                    return Err("required retained DAG parent was pruned".into());
                }
            }
            let bounded_selection: BTreeMap<_, _> = selected_map.iter()
                .filter(|(id, _)| bounded.contains_key(*id))
                .map(|(id, chosen)| (id.clone(), chosen.clone()))
                .collect();
            let resumed_window = selected_parent_retarget_window_v3_ns(
                CHAIN, &child.hash, &bounded, &bounded_selection
            ).map_err(|e| e.to_string())?;
            if resumed_window != full_window ||
                expected_difficulty_for_v3_window_ns(&resumed_window)
                    .map_err(|e| e.to_string())? != full_retarget
            {
                return Err("cross-prune native-v3 retarget changed".into());
            }
            if derive_dormant_native_v3_ghostdag_trial(
                CHAIN, GENESIS_NS, &genesis.hash, std::slice::from_ref(&child.hash), &bounded, k
            ).is_ok() {
                return Err("pruned history improperly accepted by full-genesis DAG verifier".into());
            }
            Ok(())
        };
        {
            let db = disk.open();
            assert!(db.get(frontier_key(&linear[0].hash)).unwrap().is_none());
            check_after_reopen(&db).unwrap();
        }
        {
            let db = disk.open();
            db.delete(frontier_key(&loser.hash)).unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = disk.open();
            assert!(check_after_reopen(&db).is_err(), "lost RED/BLUE frontier must fail closed");
            db.put(frontier_key(&loser.hash), &all_encoded[&loser.hash]).unwrap();
            db.delete(frontier_key(tip_first.last().unwrap())).unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = disk.open();
            assert!(check_after_reopen(&db).is_err(), "lost retarget sample must fail closed");
            db.put(
                frontier_key(tip_first.last().unwrap()),
                &all_encoded[tip_first.last().unwrap()]
            ).unwrap();
            let mut forged_manifest: serde_json::Value =
                serde_json::from_slice(&manifest_bytes).unwrap();
            forged_manifest["blue_work_limbs"][0] = serde_json::json!(0);
            db.put(manifest_key, serde_json::to_vec(&forged_manifest).unwrap()).unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = disk.open();
            assert!(check_after_reopen(&db).is_err(), "untrusted work cannot self-certify");
            db.put(manifest_key, &manifest_bytes).unwrap();
            let mut tampered_coins: Vec<Utxo> = serde_json::from_slice(&economic_bytes).unwrap();
            tampered_coins[0].amount += 1;
            db.put(economic_key, serde_json::to_vec(&tampered_coins).unwrap()).unwrap();
            db.flush_wal(true).unwrap();
        }
        {
            let db = disk.open();
            assert!(check_after_reopen(&db).is_err(), "tampered preview UTXO must fail");
        }
    }
}
