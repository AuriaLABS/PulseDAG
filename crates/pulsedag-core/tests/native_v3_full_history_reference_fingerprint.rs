//! DORMANT full-history native-v3 reference fingerprint: NOT a checkpoint.
//!
//! The independently pinned genesis and complete authenticated header closure
//! must be present before a byte-for-byte Python/Rust reference comparison.
//! No pruned state is authorized, no selection/K policy and no monetary replay.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    types::BlockId,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const CHAIN: &str = "pulsedag-dormant-ghostdag-rust-policy-probe";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
const DOMAIN: &[u8] = b"PulseDAG:dormant-v3-full-history-reference:v1";
const FROZEN: &str = include_str!("../../../docs/fixtures/native_v3_ghostdag_fork_crosslang.json");
const GOLDEN: &str =
    include_str!("../../../docs/fixtures/native_v3_full_history_reference_trial.json");

fn frame(payload: &mut Vec<u8>, bytes: &[u8]) {
    payload.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    payload.extend_from_slice(bytes);
}

fn reference_digest(
    genesis_hash: &str,
    tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> String {
    // This verifier requires ALL reachable ancestors (including side/red).
    // It rejects pruned history instead of treating 20 PoW samples as proof.
    let topo = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        genesis_hash,
        std::slice::from_ref(tip),
        headers,
    )
    .expect("full-genesis KHeavyHash DAG, not pruned history");
    assert_eq!(topo.verified_dag.authenticated_non_genesis_headers, 3);
    let mut buf = Vec::new();
    frame(&mut buf, DOMAIN);
    frame(&mut buf, CHAIN.as_bytes());
    buf.extend_from_slice(&GENESIS_NS.to_le_bytes());
    buf.extend_from_slice(&hex::decode(genesis_hash).unwrap());
    buf.extend_from_slice(&(topo.ordered_reachable_headers.len() as u32).to_le_bytes());
    for hash in topo.ordered_reachable_headers {
        let header = &headers[&hash];
        buf.extend_from_slice(&hex::decode(&hash).unwrap());
        buf.extend_from_slice(&header.height.to_le_bytes());
        buf.extend_from_slice(&header.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(&(header.parents.len() as u32).to_le_bytes());
        for parent in &header.parents {
            buf.extend_from_slice(&hex::decode(parent).unwrap());
        }
    }
    hex::encode(Sha256::digest(&buf))
}

fn corpus() -> (String, BTreeMap<BlockId, BlockHeaderV3>, Value, Value) {
    let frozen: Value = serde_json::from_str(FROZEN).unwrap();
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(frozen["chain_id"], CHAIN);
    assert_eq!(frozen["genesis_ns"].as_u64(), Some(GENESIS_NS));
    assert_eq!(golden["chain_id"], CHAIN);
    assert_eq!(golden["genesis_ns"].as_u64(), Some(GENESIS_NS));
    assert_eq!(golden["status"], "TEST_ONLY_NOT_CHECKPOINT_AUTHORITY");
    assert_eq!(golden["domain"], std::str::from_utf8(DOMAIN).unwrap());
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    assert_eq!(golden["genesis_hash"], genesis.hash);
    let mut headers = BTreeMap::new();
    for block in frozen["blocks"].as_object().unwrap().values() {
        let header: BlockHeaderV3 = serde_json::from_value(block["header"].clone()).unwrap();
        let block_hash = block["id"].as_str().unwrap().to_owned();
        assert_eq!(compute_block_hash_v3(&header, CHAIN).unwrap(), block_hash);
        assert_eq!(
            hex::encode(
                pulsedag_core::header_v3::canonical_block_header_bytes_v3(&header, CHAIN).unwrap()
            ),
            block["header_bytes_hex"].as_str().unwrap()
        );
        assert!(headers.insert(block_hash, header).is_none());
    }
    (genesis.hash, headers, frozen, golden)
}

#[test]
fn independent_python_and_rust_full_genesis_header_closures_match() {
    let (genesis, headers, frozen, golden) = corpus();
    let x0 = frozen["blocks"]["X0"]["id"].as_str().unwrap().to_owned();
    let x1 = frozen["blocks"]["X1"]["id"].as_str().unwrap().to_owned();
    let actual_0 = reference_digest(&genesis, &x0, &headers);
    let actual_1 = reference_digest(&genesis, &x1, &headers);
    assert_eq!(actual_0, golden["expected"]["X0"].as_str().unwrap());
    assert_eq!(actual_1, golden["expected"]["X1"].as_str().unwrap());
    assert_ne!(actual_0, actual_1);

    // An unrelated PoW-valid alternative tip does not silently enter the
    // selected candidate's full authenticated ancestor closure.
    let mut without_other_tip = headers.clone();
    without_other_tip.remove(&x1);
    assert_eq!(
        reference_digest(&genesis, &x0, &without_other_tip),
        actual_0
    );

    // A missing SIDE parent invalidates full history, even when all
    // selected-branch retarget samples could still be present.
    let side = frozen["blocks"]["A"]["id"].as_str().unwrap();
    without_other_tip.remove(side);
    assert!(derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis,
        std::slice::from_ref(&x0),
        &without_other_tip,
    )
    .is_err());

    for forged_genesis in ["00".repeat(32), "ff".repeat(32)] {
        assert!(derive_dormant_native_v3_topological_evaluation(
            CHAIN,
            GENESIS_NS,
            &forged_genesis,
            std::slice::from_ref(&x0),
            &headers,
        )
        .is_err());
    }
    assert!(derive_dormant_native_v3_topological_evaluation(
        "foreign-network",
        GENESIS_NS,
        &genesis,
        std::slice::from_ref(&x0),
        &headers,
    )
    .is_err());
}
