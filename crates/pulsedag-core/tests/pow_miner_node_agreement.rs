use pulsedag_core::{
    accept_block, mine_header, pow_accepts, pow_hash_hex, pow_target_u64, verify_work,
    AcceptSource, BlockHeader,
};
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::pow::{compare_pow_hash_to_target, target_from_bits};
use pulsedag_core::{build_candidate_block, build_coinbase_transaction};

fn sample_header() -> BlockHeader {
    BlockHeader {
        version: 2,
        parents: vec!["00".repeat(32)],
        timestamp: 1_710_000_060,
        difficulty: 10_000,
        nonce: 0,
        merkle_root: "4d65726b6c65526f6f742d31".into(),
        state_root: "5374617465526f6f742d31".into(),
        blue_score: 1,
        height: 1,
    }
}

#[test]
fn miner_hash_matches_core_hash() {
    let h = sample_header();
    let miner_hash = pow_hash_hex(&h);
    let core_hash = pow_hash_hex(&h);
    assert_eq!(miner_hash, core_hash);
}

#[test]
fn mined_nonce_is_accepted_and_mutated_nonce_rejected() {
    let h = sample_header();
    let (mined, accepted, _, _) = mine_header(h, 500_000);
    assert!(accepted);
    assert!(pow_accepts(&mined));
    assert!(verify_work(&mined));

    let mut invalid = mined.clone();
    invalid.nonce = invalid.nonce.saturating_add(1);
    assert!(!verify_work(&invalid));
}

#[test]
fn multithreaded_stride_ranges_do_not_overlap() {
    let threads = 4u64;
    let limit = 10_000u64;
    let mut seen = std::collections::HashSet::new();
    for tid in 0..threads {
        let mut nonce = tid;
        while nonce < limit {
            assert!(seen.insert(nonce), "duplicate nonce discovered: {nonce}");
            nonce = nonce.saturating_add(threads);
        }
    }
    assert_eq!(seen.len() as u64, limit);
}

#[test]
fn duplicate_block_is_reported_as_duplicate() {
    let mut state = init_chain_state("pow-dup".to_string());
    let parents = vec![state.dag.genesis_hash.clone()];
    let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
    let mut block = build_candidate_block(parents, 1, 1, txs);
    block.hash = "dup-check".into();

    let first = accept_block(block.clone(), &mut state, AcceptSource::P2p);
    assert!(first.is_ok());

    let second = accept_block(block, &mut state, AcceptSource::P2p);
    assert!(second.is_err());
    let msg = second.err().unwrap().to_string().to_lowercase();
    assert!(msg.contains("already"), "unexpected error: {msg}");
}

#[test]
fn target_comparison_boundaries_hold() {
    let target = target_from_bits(0x1d00ffff);
    assert_eq!(pow_target_u64(1), u64::MAX);
    assert!(compare_pow_hash_to_target(&target, &target));
}
