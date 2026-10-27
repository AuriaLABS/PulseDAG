//! All blocks in this probe are real easy-target KHeavyHash-valid native-v3
//! envelopes with chain-bound txids and body Merkle commitments, but EMPTY
//! state-root placeholders. No monetary state, signatures, finality or P2P.

use std::collections::BTreeMap;

use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_v3_envelope_trial::inspect_dormant_native_v3_full_envelope_trial,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    state::UtxoState,
    types::{compute_merkle_root, BlockId, Transaction},
};

const CHAIN: &str = "dormant-v3-native-envelopes-full-claim-audit";
const GENESIS_NS: u64 = 1_800_000_014_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn mine(
    mut parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    blue_score: u64,
    claim_names: &[(&str, u64)],
) -> BlockEnvelopeV3 {
    parents.sort();
    let transactions: Vec<Transaction> = claim_names
        .iter()
        .map(|(beneficiary, marker)| {
            build_reward_claim_transaction_v3(beneficiary, *marker, CHAIN).unwrap()
        })
        .collect();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: UtxoState::default().compute_state_root().unwrap(),
        blue_score,
        height,
    };
    for nonce in 0..1_024u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        }
    }
    panic!("native header has not mined easy-target PoW");
}

fn fork() -> (BlockEnvelopeV3, BlockEnvelopeV3, BlockEnvelopeV3) {
    let g = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine(
        vec![g.hash.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        &[("pulse1native-miner-a", 1)],
    );
    let b = mine(
        vec![g.hash.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        &[("pulse1native-miner-b", 2)],
    );
    assert_ne!(a.hash, b.hash);
    assert_eq!(a.transactions[0].outputs[0].amount, 0);
    assert_eq!(b.transactions[0].outputs[0].amount, 0);
    (g, a, b)
}

fn indexed(
    blocks: impl IntoIterator<Item = BlockEnvelopeV3>,
) -> BTreeMap<BlockId, BlockEnvelopeV3> {
    blocks
        .into_iter()
        .map(|block| (block.hash.clone(), block))
        .collect()
}

#[test]
fn complete_full_native_envelopes_and_amountless_claims_match_trial_k0_k1() {
    let (g, a, b) = fork();
    let mut parents = vec![a.hash.clone(), b.hash.clone()];
    parents.sort();
    for k in [0_usize, 1] {
        let committed_score = 2 + k as u64;
        let join = mine(
            parents.clone(),
            2,
            GENESIS_NS + 1_000_000_000,
            committed_score,
            &[("pulse1native-miner-join", 10 + k as u64)],
        );
        let full = indexed([g.clone(), a.clone(), b.clone(), join.clone()]);
        let trial = inspect_dormant_native_v3_full_envelope_trial(
            CHAIN,
            GENESIS_NS,
            &g.hash,
            std::slice::from_ref(&join.hash),
            &full,
            k,
        )
        .expect("full native PoW and valid body/amountless claims");
        assert_eq!(trial.blocks.len(), 4);
        assert_eq!(
            trial.blocks[&join.hash].selected_parent.as_deref(),
            Some(parents[0].as_str())
        );
        assert_eq!(trial.blocks[&join.hash].blue_score, committed_score);
        assert_eq!(
            trial.blocks[&join.hash].blue_work.decimal(),
            (4 + k * 2).to_string()
        );
        assert_eq!(trial.blocks[&join.hash].merge_blues.len(), k);
        assert_eq!(trial.blocks[&join.hash].merge_reds.len(), 1 - k);
        assert_eq!(trial.blocks[&a.hash].blue_work.decimal(), "2");
        assert_eq!(trial.blocks[&b.hash].blue_work.decimal(), "2");
        // Insertion order and peer arrival cannot be consumed as authority.
        let reverse: BTreeMap<BlockId, BlockEnvelopeV3> = full
            .iter()
            .rev()
            .map(|(id, envelope)| (id.clone(), envelope.clone()))
            .collect();
        assert_eq!(
            trial,
            inspect_dormant_native_v3_full_envelope_trial(
                CHAIN,
                GENESIS_NS,
                &g.hash,
                std::slice::from_ref(&join.hash),
                &reverse,
                k,
            )
            .unwrap()
        );
    }
}

#[test]
fn full_native_envelope_guard_rejects_hidden_issuance_missing_body_and_bad_score() {
    let (g, a, b) = fork();
    let mut parents = vec![a.hash.clone(), b.hash.clone()];
    parents.sort();
    let join = mine(
        parents.clone(),
        2,
        GENESIS_NS + 1_000_000_000,
        2,
        &[("pulse1native-miner-join", 10)],
    );
    let full = indexed([g.clone(), a.clone(), b.clone(), join.clone()]);
    let check = |corpus: &BTreeMap<BlockId, BlockEnvelopeV3>, tip: &str| {
        inspect_dormant_native_v3_full_envelope_trial(
            CHAIN,
            GENESIS_NS,
            &g.hash,
            &[tip.to_owned()],
            corpus,
            0,
        )
    };
    check(&full, &join.hash).expect("baseline valid full body and PoW");

    let missing_side = indexed([g.clone(), a.clone(), join.clone()]);
    assert!(check(&missing_side, &join.hash).is_err());
    let without_reward = mine(parents.clone(), 2, GENESIS_NS + 1_000_000_000, 2, &[]);
    let missing_claim = indexed([g.clone(), a.clone(), b.clone(), without_reward.clone()]);
    assert!(check(&missing_claim, &without_reward.hash).is_err());

    // A perfectly canonical, real PoW-valid envelope can contain two
    // individually valid zero-valued reward claims. It MUST still be rejected:
    // the second inputless transaction is a hidden issuance path.
    let extra_claim = mine(
        parents.clone(),
        2,
        GENESIS_NS + 1_000_000_000,
        2,
        &[("pulse1native-miner-join", 10), ("pulse1hidden-minter", 11)],
    );
    let hidden = indexed([g.clone(), a.clone(), b.clone(), extra_claim.clone()]);
    let error = check(&hidden, &extra_claim.hash).unwrap_err().to_string();
    assert!(
        error.contains("additional inputless transaction"),
        "{error}"
    );

    let bad_score = mine(
        parents,
        2,
        GENESIS_NS + 1_000_000_000,
        7,
        &[("pulse1native-miner-join", 10)],
    );
    let invalid_score = indexed([g.clone(), a.clone(), b.clone(), bad_score.clone()]);
    assert!(check(&invalid_score, &bad_score.hash).is_err());

    let detached = mine(
        vec![g.hash.clone()],
        1,
        GENESIS_NS + 700_000_000,
        1,
        &[("pulse1detached-native-miner", 12)],
    );
    let mut unrelated = full.clone();
    unrelated.insert(detached.hash.clone(), detached);
    let error = check(&unrelated, &join.hash).unwrap_err().to_string();
    assert!(error.contains("unrelated full block"), "{error}");

    let mut wrong_key = full.clone();
    let removed_envelope = wrong_key.remove(&a.hash).unwrap();
    wrong_key.insert("ff".repeat(32), removed_envelope);
    assert!(check(&wrong_key, &join.hash).is_err());

    let mut altered_body = full.clone();
    altered_body.get_mut(&a.hash).unwrap().transactions[0].outputs[0].amount = 1;
    assert!(check(&altered_body, &join.hash).is_err());

    assert!(inspect_dormant_native_v3_full_envelope_trial(
        CHAIN,
        GENESIS_NS + 1,
        &g.hash,
        std::slice::from_ref(&join.hash),
        &full,
        0,
    )
    .is_err());
    assert!(inspect_dormant_native_v3_full_envelope_trial(
        "different-native-network",
        GENESIS_NS,
        &g.hash,
        std::slice::from_ref(&join.hash),
        &full,
        0,
    )
    .is_err());
}
