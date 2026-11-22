//! Thirty-two genuinely PoW-valid *native-v3* headers, five isolated local
//! replica snapshots, and deterministic catch-up after reconnect simulation.
//!
//! TEST ONLY. These are in-process snapshots, NOT network-connected nodes,
//! native monetary block admission, a frozen GHOSTDAG K/blue-work policy,
//! reward/fee/UTXO replay, or evidence of a launched PulseDAG network.
//! The future public v3 testnet is the FIRST network; no v1/v2 compatibility
//! or migration is required, and no production activation is performed here.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    types::{compute_merkle_root, BlockId},
};

const CHAIN: &str = "pulsedag-first-network-native-v3-local-recovery-fixture";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
const EASY_BITS: u32 = 0x207f_ffff;
const NATIVE_POW_HEADERS: usize = 32;
const REPLICAS: usize = 5;

fn mine_native_header(
    mut parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    salt: u64,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    parents.dedup();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: format!("{:064x}", salt + 1),
        blue_score: height,
        height,
    };
    for nonce in 0..4_096_u64 {
        header.nonce = nonce;
        let evaluation = evaluate_v3_header_pow(&header, CHAIN).expect("valid native-v3 preimage");
        if evaluation.pow.accepted {
            let hash = compute_block_hash_v3(&header, CHAIN).expect("native-v3 canonical hash");
            assert_eq!(evaluation.block_hash, hash);
            verify_v3_header_pow(&header, CHAIN).expect("real KHeavyHash proof of work");
            return (hash, header);
        }
    }
    panic!("easy native-v3 target did not produce a valid nonce");
}

#[test]
fn native_v3_32_real_pow_headers_recover_identically_on_five_local_replicas() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS)
        .expect("independently frozen, chain-bound genesis");
    let mut generated = Vec::with_capacity(NATIVE_POW_HEADERS);
    let mut parent = genesis.hash.clone();
    let mut side_parent = None;

    // Exactly 31 selected-spine headers plus one side header at height 12.
    // The height-13 header names BOTH native-v3 parent hashes; no v1/v2
    // BlockHeader or legacy serialized block participates in this fixture.
    for height in 1..=31_u64 {
        let mut parents = vec![parent.clone()];
        if height == 13 {
            parents.push(side_parent.clone().expect("height-12 side block exists"));
        }
        let timestamp_ns = GENESIS_NS + height * 1_000_000_000;
        let (hash, header) = mine_native_header(parents, height, timestamp_ns, height);
        assert_eq!(header.height, height);
        generated.push((hash.clone(), header));

        if height == 12 {
            let (side_hash, side_header) = mine_native_header(
                vec![parent.clone()],
                height,
                timestamp_ns + 100_000_000,
                100 + height,
            );
            assert_ne!(hash, side_hash, "independent competing native-v3 blocks");
            side_parent = Some(side_hash.clone());
            generated.push((side_hash, side_header));
        }
        parent = hash;
    }
    let tip = parent;
    let side = side_parent.expect("one authentic side ancestor");
    assert_eq!(generated.len(), NATIVE_POW_HEADERS);
    let mut canonical = BTreeMap::new();
    assert!(canonical
        .insert(genesis.hash.clone(), genesis.header.clone())
        .is_none());
    for (hash, header) in &generated {
        assert!(
            canonical.insert(hash.clone(), header.clone()).is_none(),
            "every native header identity must be unique"
        );
    }
    assert_eq!(canonical.len(), NATIVE_POW_HEADERS + 1);

    // Replica 5 is intentionally withheld from this local data transfer
    // while replicas 1..4 receive all 32 native-v3 headers. There is NO
    // claim of an authenticated Noise connection or actual network isolation.
    let mut replicas = Vec::with_capacity(REPLICAS);
    for index in 0..REPLICAS {
        let mut map = BTreeMap::new();
        map.insert(genesis.hash.clone(), genesis.header.clone());
        if index != REPLICAS - 1 {
            for (hash, header) in &generated {
                map.insert(hash.clone(), header.clone());
            }
        }
        replicas.push(map);
    }
    assert_eq!(replicas[REPLICAS - 1].len(), 1);
    assert!(
        derive_dormant_native_v3_topological_evaluation(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&tip),
            &replicas[REPLICAS - 1],
        )
        .is_err(),
        "an isolated replica must refuse absent native-v3 ancestry"
    );

    // Deliberately deliver all headers CHILD-FIRST on replica 5: no
    // arrival-order-dependent consensus or implicit predecessor migration.
    for (hash, header) in generated.iter().rev() {
        assert!(replicas[REPLICAS - 1]
            .insert(hash.clone(), header.clone())
            .is_none());
    }
    let baseline = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&tip),
        &replicas[0],
    )
    .expect("complete real native-v3 PoW ancestry");
    assert_eq!(baseline.ordered_reachable_headers.len(), NATIVE_POW_HEADERS + 1);
    assert_eq!(baseline.ordered_reachable_headers.first(), Some(&genesis.hash));
    assert_eq!(baseline.ordered_reachable_headers.last(), Some(&tip));
    assert_eq!(
        baseline.verified_dag.authenticated_non_genesis_headers,
        NATIVE_POW_HEADERS
    );
    assert_eq!(
        baseline.verified_dag.authenticated_parent_edges,
        NATIVE_POW_HEADERS + 1,
        "one extra edge for the real native multiparent merge"
    );
    assert!(baseline.verified_dag.frozen_genesis_reached);
    for replica in &replicas {
        let result = derive_dormant_native_v3_topological_evaluation(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&tip),
            replica,
        )
        .expect("all five replicas reproduce exact native-v3 header closure");
        assert_eq!(result, baseline);
    }

    // Side ancestry is required even when the selected spine remains
    // present. A foreign chain domain and altered hash commitment fail closed.
    let mut missing_side = canonical.clone();
    assert!(missing_side.remove(&side).is_some());
    assert!(
        derive_dormant_native_v3_topological_evaluation(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&tip),
            &missing_side,
        )
        .is_err(),
        "missing authenticated side-branch ancestry is invalid"
    );
    let mut forged = canonical.clone();
    forged.get_mut(&side).expect("real side header").state_root = "ff".repeat(32);
    assert!(
        derive_dormant_native_v3_topological_evaluation(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&tip),
            &forged,
        )
        .is_err(),
        "changing an authenticated header under its original ID is invalid"
    );
    assert!(
        derive_dormant_native_v3_topological_evaluation(
            "foreign-pulsedag-chain",
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&tip),
            &canonical,
        )
        .is_err(),
        "first-network chain identity cannot be replaced"
    );
}
