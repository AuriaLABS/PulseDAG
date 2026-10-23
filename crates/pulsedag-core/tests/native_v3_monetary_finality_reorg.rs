//! DORMANT TEST ONLY: authoritative v3 monetary replay/finality reorg guards.
//!
//! The ChainState replay engine here uses a *synthetic legacy-v2 Block
//! carrier/ordering* because native-v3 GHOSTDAG ordering is NOT frozen.
//! This does not convert a native-v3 header to a legacy header, or authorize
//! monetary mining, finality, signatures, P2P, smart contracts or mainnet.
//! Unlike a bookkeeping-only UTXO preview, we explicitly invoke the existing
//! authoritative monetary replay + live root/finality validation routines.

use pulsedag_core::{
    genesis_v3::init_chain_state_v3,
    live_reward_settlement_v3::{
        derive_live_reward_finality_boundary_v3, validate_live_reward_settlement_v3,
    },
    monetary_v3::{subsidy_atoms_for_score, MonetaryCadenceSegment},
    reward_settlement_v3::{
        bind_reward_finality_boundary_v3, materializable_reward_utxos_v3,
        validate_reward_finality_boundary_v3, RewardClaimStatusV3,
    },
    state::{ChainState, UtxoState},
    state_replay_v3::{materialize_authoritative_state_v3, REWARD_FINALITY_POLICY_VERSION_V3},
    types::{Block, BlockHeader},
    build_reward_claim_transaction_v3,
};

const CHAIN: &str = "dormant-v3-reward-finality-reorg-audit";
const GENESIS_SEC: u64 = 1_800_000_012;
const ONE_HOUR_PER_SCORE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
    activation_score: 0,
    target_interval_ns: 3_600_000_000_000,
}];

/// Explicitly a fixture of *legacy-carrier* blocks to exercise the existing
/// v3 monetary replay. It does not impersonate the native-v3 header protocol.
fn linear_two_rewards(first_beneficiary: &str, tag: &str) -> ChainState {
    let mut state = init_chain_state_v3(CHAIN.into(), GENESIS_SEC).unwrap();
    assert!(state.utxo.utxos.is_empty(), "native v3 genesis must be empty");
    assert!(!state.contracts.config.enabled);
    let mut parent = state.dag.genesis_hash.clone();
    for score in 1u64..=2 {
        let hash = format!("{tag}-carrier-{score}");
        let beneficiary = if score == 1 {
            first_beneficiary.into()
        } else {
            "pulse1later-miner".to_string()
        };
        let claim = build_reward_claim_transaction_v3(
            &beneficiary, score, &state.chain_id,
        ).unwrap();
        assert!(claim.inputs.is_empty());
        assert_eq!(claim.outputs[0].amount, 0);
        let block = Block {
            hash: hash.clone(),
            header: BlockHeader {
                version: 2,
                parents: vec![parent.clone()],
                timestamp: GENESIS_SEC + score,
                difficulty: 1,
                nonce: score,
                merkle_root: format!("fixture-merkle-{hash}"),
                state_root: format!("fixture-state-{hash}"),
                blue_score: score,
                height: score,
            },
            transactions: vec![claim],
        };
        state.dag.blocks.insert(hash.clone(), block);
        state.dag.selected_parents.insert(hash.clone(), Some(parent));
        state.dag.merge_set_blues.insert(hash.clone(), vec![]);
        state.dag.merge_set_reds.insert(hash.clone(), vec![]);
        state.dag.blue_work.insert(hash.clone(), u128::from(score));
        state.dag.selected_chain.push(hash.clone());
        state.dag.tips.clear();
        state.dag.tips.insert(hash.clone());
        state.dag.best_height = score;
        parent = hash;
    }
    state
}

#[test]
fn old_finalized_coinbase_must_not_survive_a_replayed_prefix_switch() {
    let first = linear_two_rewards("pulse1old-miner", "old");
    let alternative = linear_two_rewards("pulse1new-miner", "new");
    // Real existing monetary settlement derives a canonical ordered carrier,
    // rewards, mature prefix, and UTXO root from scratch in each state.
    let live_old = materialize_authoritative_state_v3(&first, &ONE_HOUR_PER_SCORE).unwrap();
    let live_new = materialize_authoritative_state_v3(&alternative, &ONE_HOUR_PER_SCORE).unwrap();
    let checked_old = validate_live_reward_settlement_v3(
        &live_old, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap();
    let checked_new = validate_live_reward_settlement_v3(
        &live_new, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap();
    assert_eq!(checked_old.current_monetary_score, 2);
    assert_eq!(checked_new.current_monetary_score, 2);
    assert_eq!(checked_old.claims[0].status, RewardClaimStatusV3::Spendable);
    assert_eq!(checked_new.claims[0].status, RewardClaimStatusV3::Spendable);
    assert_eq!(checked_old.claims[1].status, RewardClaimStatusV3::Provisional);
    assert_eq!(checked_new.claims[1].status, RewardClaimStatusV3::Provisional);
    let old_rewards = materializable_reward_utxos_v3(&checked_old);
    let new_rewards = materializable_reward_utxos_v3(&checked_new);
    assert_eq!(old_rewards.len(), 1);
    assert_eq!(new_rewards.len(), 1);
    let subsidy = subsidy_atoms_for_score(1, &ONE_HOUR_PER_SCORE).unwrap();
    assert_eq!(old_rewards[0].amount, subsidy);
    assert_eq!(new_rewards[0].amount, subsidy);
    assert_eq!(old_rewards[0].address, "pulse1old-miner");
    assert_eq!(new_rewards[0].address, "pulse1new-miner");
    assert_ne!(old_rewards[0].outpoint, new_rewards[0].outpoint);
    assert!(!live_new.utxo.utxos.contains_key(&old_rewards[0].outpoint));
    assert!(!live_old.utxo.utxos.contains_key(&new_rewards[0].outpoint));
    let old_root = live_old.utxo.compute_state_root().unwrap();
    let new_root = live_new.utxo.compute_state_root().unwrap();
    assert_ne!(old_root, new_root);

    let old_boundary = derive_live_reward_finality_boundary_v3(
        &live_old, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap();
    assert_eq!(old_boundary.finalized_through_score, 1);
    assert!(validate_reward_finality_boundary_v3(
        &live_old, &old_boundary, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_ok());
    assert!(validate_reward_finality_boundary_v3(
        &live_new, &old_boundary, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err(), "stale prefix certificate must not validate across a reorg");

    // Reusing an old committed UTXO snapshot with the alternative prefix is
    // NOT equivalent to rebuilding the monetary state. Must fail closed.
    let mut corrupted = live_new.clone();
    corrupted.utxo = live_old.utxo.clone();
    assert!(validate_live_reward_settlement_v3(
        &corrupted, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    assert_eq!(live_new.utxo.compute_state_root().unwrap(), new_root);
    assert_eq!(live_old.utxo.compute_state_root().unwrap(), old_root);
}

#[test]
fn maturity_alone_never_authorizes_unmaterialized_rewards_or_a_fake_certificate() {
    let source = linear_two_rewards("pulse1miner", "solo");
    let original_root = source.utxo.compute_state_root().unwrap();
    assert_eq!(original_root, UtxoState::default().compute_state_root().unwrap());
    // Economic time has elapsed for score 1, but the live state hasn't been
    // replayed/materialized: it must NOT silently gain a reward UTXO.
    let error = validate_live_reward_settlement_v3(
        &source, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap_err();
    assert!(error.to_string().contains("does not match live state"));

    let live = materialize_authoritative_state_v3(&source, &ONE_HOUR_PER_SCORE).unwrap();
    let good = validate_live_reward_settlement_v3(
        &live, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap();
    assert_eq!(materializable_reward_utxos_v3(&good).len(), 1);
    let real_binding = bind_reward_finality_boundary_v3(
        &live, 1, REWARD_FINALITY_POLICY_VERSION_V3,
    ).unwrap();
    let mut forged = real_binding.clone();
    forged.ordered_prefix_digest = "00".repeat(32);
    assert!(validate_reward_finality_boundary_v3(
        &live, &forged, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    let mut forged = real_binding.clone();
    forged.finalized_block_hash = "forged-finality-hash".into();
    assert!(validate_reward_finality_boundary_v3(
        &live, &forged, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    let mut forged = real_binding.clone();
    forged.finalized_through_score = 3;
    assert!(validate_reward_finality_boundary_v3(
        &live, &forged, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    let mut forged = real_binding;
    forged.policy_version = "unapproved-p2p-finality".into();
    assert!(validate_reward_finality_boundary_v3(
        &live, &forged, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    // Genesis/candidate consistency and persisted state-root checks are still
    // required beyond a verified prefix digest.
    let mut tampered = live.clone();
    let reward = tampered.utxo.utxos.values_mut().next().unwrap();
    reward.amount = reward.amount.checked_add(1).unwrap();
    assert!(validate_live_reward_settlement_v3(
        &tampered, &ONE_HOUR_PER_SCORE, REWARD_FINALITY_POLICY_VERSION_V3,
    ).is_err());
    assert_eq!(source.utxo.compute_state_root().unwrap(), original_root);
}
