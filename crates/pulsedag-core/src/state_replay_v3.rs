use std::collections::hash_map::Entry;

use serde::{Deserialize, Serialize};

use crate::{
    apply::apply_transaction,
    errors::PulseError,
    genesis_v3::init_chain_state_v3,
    monetary_v3::{economic_maturity_reached, subsidy_atoms_for_score, MonetaryCadenceSegment},
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2},
    reward_settlement_v3::{settlement_outpoint_v3, validate_reward_claim_transaction_v3},
    state::{ChainState, UtxoState},
    types::{Hash, Utxo},
};

/// Candidate v3.0.0 reward-settlement rule.
///
/// This is deliberately a settlement-finality rule, not a claim that PoW
/// history becomes mathematically irreversible. A reward enters authoritative
/// UTXO state only after its canonical ordered-DAG position has accumulated
/// 3,600 economic seconds of maturity. A later selected-DAG reorganization
/// rebuilds this state from the new canonical order.
///
/// Live activation is intentionally separate from this replay foundation.
pub const REWARD_FINALITY_POLICY_VERSION_V3: &str =
    "pulsedag-reward-finality-v3.0.0-mature-prefix-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateReplayV3Diagnostics {
    pub validated_reward_claims: usize,
    pub applied_transactions: usize,
    pub skipped_conflicting_transactions: usize,
    pub materialized_rewards: usize,
    pub mature_reward_prefix_score: u64,
    pub conflict_diagnostics: Vec<String>,
    pub state_root: String,
    pub ordered_dag_tip: Option<Hash>,
    pub ordered_dag_digest: String,
}

#[derive(Debug, Clone)]
pub struct StateReplayV3 {
    pub utxo: UtxoState,
    pub ordered_dag: OrderedDagV2,
    /// Exact per-score fees from ordinary transactions that actually applied
    /// during authoritative replay. Conflict losers never fund settlement.
    pub eligible_fees_by_score: Vec<u64>,
    pub diagnostics: StateReplayV3Diagnostics,
}

fn invalid_replay(message: impl Into<String>) -> PulseError {
    PulseError::NonDeterministicState(format!("v3 reward replay: {}", message.into()))
}

/// Greatest non-genesis monetary score whose reward has completed the frozen
/// economic maturity delay at the supplied current score.
///
/// Economic time is monotonic in score for a valid cadence schedule, so binary
/// search keeps this O(log(score) * cadence_segments) instead of scanning the
/// full chain on every accepted block. Score 0 is returned when no mining reward
/// is mature yet.
pub fn mature_reward_prefix_score_v3(
    current_score: u64,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<u64, PulseError> {
    if current_score == 0
        || !economic_maturity_reached(1, current_score, cadence_segments)
            .map_err(|error| invalid_replay(error.to_string()))?
    {
        return Ok(0);
    }

    let mut low = 1_u64;
    let mut high = current_score;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if economic_maturity_reached(mid, current_score, cadence_segments)
            .map_err(|error| invalid_replay(error.to_string()))?
        {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Ok(low)
}

fn insert_reward_utxo(utxo: &mut UtxoState, reward: Utxo) -> Result<(), PulseError> {
    let outpoint = reward.outpoint.clone();
    match utxo.utxos.entry(outpoint.clone()) {
        Entry::Vacant(entry) => {
            entry.insert(reward.clone());
        }
        Entry::Occupied(_) => {
            return Err(PulseError::DuplicateUtxoOutpoint(format!(
                "{}:{}",
                outpoint.txid, outpoint.index
            )));
        }
    }
    utxo.address_index
        .entry(reward.address.clone())
        .or_default()
        .push(outpoint);
    Ok(())
}

fn materialize_reward_at_score(
    rebuilt: &mut ChainState,
    source: &ChainState,
    ordered: &OrderedDagV2,
    reward_score: u64,
    eligible_fees_atoms: u64,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<(), PulseError> {
    let index = usize::try_from(reward_score)
        .map_err(|_| invalid_replay("reward score exceeds platform index width"))?;
    let block_hash = ordered
        .blocks
        .get(index)
        .ok_or_else(|| invalid_replay(format!("missing ordered reward score {reward_score}")))?;
    if block_hash == &source.dag.genesis_hash {
        return Err(invalid_replay("genesis cannot materialize a mining reward"));
    }
    let block = source
        .dag
        .blocks
        .get(block_hash)
        .ok_or_else(|| invalid_replay(format!("ordered reward block {block_hash} missing")))?;
    let claim = block
        .transactions
        .first()
        .ok_or_else(|| invalid_replay(format!("reward block {block_hash} has no claim")))?;
    validate_reward_claim_transaction_v3(claim, &source.chain_id)
        .map_err(|error| invalid_replay(format!("reward block {block_hash}: {error}")))?;
    if block
        .transactions
        .iter()
        .skip(1)
        .any(|tx| tx.inputs.is_empty())
    {
        return Err(invalid_replay(format!(
            "reward block {block_hash} contains an additional inputless transaction"
        )));
    }

    let beneficiary = claim
        .outputs
        .first()
        .ok_or_else(|| invalid_replay(format!("reward block {block_hash} has no beneficiary")))?
        .address
        .clone();
    let subsidy_atoms = subsidy_atoms_for_score(reward_score, cadence_segments)
        .map_err(|error| invalid_replay(error.to_string()))?;
    let amount = subsidy_atoms
        .checked_add(eligible_fees_atoms)
        .ok_or_else(|| invalid_replay("reward settlement amount overflow"))?;
    let outpoint = settlement_outpoint_v3(&source.chain_id, block_hash, &claim.txid);

    insert_reward_utxo(
        &mut rebuilt.utxo,
        Utxo {
            outpoint,
            address: beneficiary,
            amount,
            coinbase: true,
            height: block.header.height,
        },
    )
}

/// Rebuild authoritative v3 UTXO state from the exact ordered DAG while
/// materializing mining rewards at their deterministic economic-maturity
/// boundary.
///
/// Ordering of one canonical score is:
/// 1. validate the amountless reward claim and apply ordinary txs atomically;
/// 2. materialize all prior rewards mature at the end of that score;
/// 3. expose those synthetic reward UTXOs to the next score and later.
///
/// Same-score spending is therefore impossible and mining-template construction
/// gets a stable pre-state. Reorganizations replay the new canonical order; no
/// local wall clock or raw block height is authority.
///
/// This foundation intentionally supports full v3 replay only. Compact-pruned
/// checkpoint verification and live activation remain fail-closed follow-ups.
fn validate_v3_replay_source(
    state: &ChainState,
) -> Result<(&crate::types::Block, OrderedDagV2), PulseError> {
    let genesis = state
        .dag
        .blocks
        .get(&state.dag.genesis_hash)
        .ok_or_else(|| invalid_replay("full replay requires the v3 genesis block"))?;
    if !genesis.transactions.is_empty() {
        return Err(invalid_replay(
            "v3 reward replay requires zero-allocation transactionless genesis",
        ));
    }

    let ordered_dag = derive_ordered_dag_v2(state)
        .map_err(|error| invalid_replay(format!("ordered DAG unavailable: {error:?}")))?;
    if ordered_dag.blocks.first() != Some(&state.dag.genesis_hash) {
        return Err(invalid_replay("ordered DAG does not start at genesis"));
    }
    Ok((genesis, ordered_dag))
}

struct ReplayPrefixV3 {
    rebuilt: ChainState,
    validated_reward_claims: usize,
    applied_transactions: usize,
    skipped_conflicting_transactions: usize,
    materialized_rewards: usize,
    mature_reward_prefix_score: u64,
    conflict_diagnostics: Vec<String>,
    eligible_fees_by_score: Vec<u64>,
}

fn replay_authoritative_prefix_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    genesis_timestamp: u64,
    ordered_dag: &OrderedDagV2,
    end_exclusive: usize,
) -> Result<ReplayPrefixV3, PulseError> {
    if end_exclusive > ordered_dag.blocks.len() {
        return Err(invalid_replay("replay prefix exceeds ordered DAG length"));
    }

    let mut rebuilt = init_chain_state_v3(state.chain_id.clone(), genesis_timestamp)?;
    if rebuilt.dag.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_replay(format!(
            "v3 genesis identity mismatch: expected {}, rebuilt {}",
            state.dag.genesis_hash, rebuilt.dag.genesis_hash
        )));
    }
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
    rebuilt.dag.selected_parent_policy = state.dag.selected_parent_policy;

    let mut validated_reward_claims = 0usize;
    let mut applied_transactions = 0usize;
    let mut skipped_conflicting_transactions = 0usize;
    let mut materialized_rewards = 0usize;
    let mut conflict_diagnostics = Vec::new();
    let mut next_reward_score = 1_u64;
    let mut mature_reward_prefix_score = 0_u64;
    let mut eligible_fees_by_score = vec![0_u64; ordered_dag.blocks.len()];

    for (ordered_pos, hash) in ordered_dag.blocks.iter().enumerate().take(end_exclusive) {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        let current_score =
            u64::try_from(ordered_pos).map_err(|_| invalid_replay("ordered score exceeds u64"))?;
        let block = state
            .dag
            .blocks
            .get(hash)
            .ok_or_else(|| invalid_replay(format!("ordered block {hash} is missing")))?;
        let claim = block
            .transactions
            .first()
            .ok_or_else(|| invalid_replay(format!("block {hash} has no reward claim")))?;
        validate_reward_claim_transaction_v3(claim, &state.chain_id)
            .map_err(|error| invalid_replay(format!("block {hash}: {error}")))?;
        if block
            .transactions
            .iter()
            .skip(1)
            .any(|tx| tx.inputs.is_empty())
        {
            return Err(invalid_replay(format!(
                "block {hash} contains an additional inputless transaction"
            )));
        }

        validated_reward_claims = validated_reward_claims.saturating_add(1);

        for tx in block.transactions.iter().skip(1) {
            let mut candidate = rebuilt.clone();
            match apply_transaction(tx, &mut candidate, block.header.height) {
                Ok(()) => {
                    rebuilt = candidate;
                    eligible_fees_by_score[ordered_pos] = eligible_fees_by_score[ordered_pos]
                        .checked_add(tx.fee)
                        .ok_or_else(|| invalid_replay("eligible fee arithmetic overflow"))?;
                    applied_transactions = applied_transactions.saturating_add(1);
                }
                Err(PulseError::UtxoNotFound | PulseError::DuplicateUtxoOutpoint(_)) => {
                    skipped_conflicting_transactions =
                        skipped_conflicting_transactions.saturating_add(1);
                    conflict_diagnostics.push(format!(
                        "ordered_pos={ordered_pos} block={} tx={} skipped_conflict_atomic",
                        block.hash, tx.txid
                    ));
                }
                Err(error) => return Err(error),
            }
        }

        mature_reward_prefix_score =
            mature_reward_prefix_score_v3(current_score, cadence_segments)?;
        while next_reward_score <= mature_reward_prefix_score {
            materialize_reward_at_score(
                &mut rebuilt,
                state,
                ordered_dag,
                next_reward_score,
                eligible_fees_by_score[usize::try_from(next_reward_score)
                    .map_err(|_| invalid_replay("reward score exceeds platform index width"))?],
                cadence_segments,
            )?;
            materialized_rewards = materialized_rewards.saturating_add(1);
            next_reward_score = next_reward_score.saturating_add(1);
        }
    }

    Ok(ReplayPrefixV3 {
        rebuilt,
        validated_reward_claims,
        applied_transactions,
        skipped_conflicting_transactions,
        materialized_rewards,
        mature_reward_prefix_score,
        conflict_diagnostics,
        eligible_fees_by_score,
    })
}

/// Rebuild authoritative v3 UTXO state from the exact ordered DAG while
/// materializing mining rewards at their deterministic economic-maturity
/// boundary.
///
/// Ordering of one canonical score is:
/// 1. validate the amountless reward claim and apply ordinary txs atomically;
/// 2. materialize all prior rewards mature at the end of that score;
/// 3. expose those synthetic reward UTXOs to the next score and later.
///
/// Same-score spending is therefore impossible and mining-template construction
/// gets a stable pre-state. Reorganizations replay the new canonical order; no
/// local wall clock or raw block height is authority.
///
/// This foundation intentionally supports full v3 replay only. Compact-pruned
/// checkpoint verification and live activation remain fail-closed follow-ups.
pub fn rebuild_authoritative_state_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<StateReplayV3, PulseError> {
    let (genesis, ordered_dag) = validate_v3_replay_source(state)?;
    let replay = replay_authoritative_prefix_v3(
        state,
        cadence_segments,
        genesis.header.timestamp,
        &ordered_dag,
        ordered_dag.blocks.len(),
    )?;
    let state_root = replay.rebuilt.utxo.compute_state_root()?;

    Ok(StateReplayV3 {
        utxo: replay.rebuilt.utxo,
        eligible_fees_by_score: replay.eligible_fees_by_score,
        diagnostics: StateReplayV3Diagnostics {
            validated_reward_claims: replay.validated_reward_claims,
            applied_transactions: replay.applied_transactions,
            skipped_conflicting_transactions: replay.skipped_conflicting_transactions,
            materialized_rewards: replay.materialized_rewards,
            mature_reward_prefix_score: replay.mature_reward_prefix_score,
            conflict_diagnostics: replay.conflict_diagnostics,
            state_root,
            ordered_dag_tip: ordered_dag.blocks.last().cloned(),
            ordered_dag_digest: ordered_dag.digest.clone(),
        },
        ordered_dag,
    })
}

/// Materialize the exact authoritative state immediately before the candidate
/// transaction set executes, while still using the candidate itself to close
/// and classify its complete parent DAG. This preserves the frozen rule that
/// rewards maturing at the candidate score become spendable only at the next
/// score.
pub fn materialize_authoritative_pre_candidate_state_v3(
    context: &ChainState,
    candidate_hash: &Hash,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ChainState, PulseError> {
    let (genesis, ordered_dag) = validate_v3_replay_source(context)?;
    if ordered_dag.blocks.last() != Some(candidate_hash) {
        return Err(invalid_replay(format!(
            "candidate {candidate_hash} is not the tip of its own authoritative context {:?}",
            ordered_dag.blocks.last()
        )));
    }
    let candidate_pos = ordered_dag
        .blocks
        .iter()
        .position(|hash| hash == candidate_hash)
        .ok_or_else(|| invalid_replay("candidate is missing from authoritative order"))?;
    let replay = replay_authoritative_prefix_v3(
        context,
        cadence_segments,
        genesis.header.timestamp,
        &ordered_dag,
        candidate_pos,
    )?;

    let mut materialized = context.clone();
    materialized.utxo = replay.rebuilt.utxo;
    materialized.dag.ordered_dag = ordered_dag.blocks[..candidate_pos].to_vec();
    materialized.dag.ordering_version = ordered_dag.ordering_version.clone();
    materialized.dag.ordered_dag_tip = candidate_pos
        .checked_sub(1)
        .and_then(|index| ordered_dag.blocks.get(index).cloned());
    materialized.dag.ordered_dag_state_root = Some(materialized.utxo.compute_state_root()?);
    materialized.dag.ordered_dag_conflict_diagnostics = replay.conflict_diagnostics;
    materialized.mempool.transactions.clear();
    materialized.mempool.spent_outpoints.clear();
    materialized.mempool.first_seen.clear();
    materialized.mempool.admission_height.clear();
    Ok(materialized)
}

/// Materialize the exact authoritative v3 monetary state without mutating the
/// caller. Unlike the v2 compatibility replay, reward claims remain amountless
/// until the mature-prefix rule materializes their synthetic settlement UTXOs.
pub fn materialize_authoritative_state_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ChainState, PulseError> {
    let replay = rebuild_authoritative_state_v3(state, cadence_segments)?;
    let mut materialized = state.clone();
    materialized.utxo = replay.utxo.clone();
    materialized.dag.ordered_dag = replay.ordered_dag.blocks.clone();
    materialized.dag.ordering_version = replay.ordered_dag.ordering_version.clone();
    materialized.dag.ordered_dag_tip = replay.diagnostics.ordered_dag_tip.clone();
    materialized.dag.ordered_dag_state_root = Some(replay.diagnostics.state_root.clone());
    materialized.dag.ordered_dag_conflict_diagnostics =
        replay.diagnostics.conflict_diagnostics.clone();
    Ok(materialized)
}

/// Prove that a live/restored full v3 state is already materialized from the
/// same ordered-DAG monetary replay it claims. Compact-pruned v3 verification
/// remains a separate launch gate and therefore fails closed here if full replay
/// cannot be performed.
pub fn verify_authoritative_state_snapshot_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<StateReplayV3Diagnostics, PulseError> {
    let replay = rebuild_authoritative_state_v3(state, cadence_segments)?;
    let observed_state_root = state.utxo.compute_state_root()?;

    if state.dag.ordering_version != replay.ordered_dag.ordering_version {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot ordering version does not match authoritative monetary replay".to_string(),
        ));
    }
    if state.dag.ordered_dag != replay.ordered_dag.blocks {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot ordered DAG does not match authoritative monetary replay".to_string(),
        ));
    }
    if state.dag.ordered_dag_tip != replay.diagnostics.ordered_dag_tip {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot ordered DAG tip does not match authoritative monetary replay".to_string(),
        ));
    }
    if state.dag.ordered_dag_state_root.as_deref() != Some(replay.diagnostics.state_root.as_str()) {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot recorded state root does not match authoritative monetary replay"
                .to_string(),
        ));
    }
    if observed_state_root != replay.diagnostics.state_root {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot UTXO root does not match authoritative monetary replay".to_string(),
        ));
    }
    if state.dag.ordered_dag_conflict_diagnostics != replay.diagnostics.conflict_diagnostics {
        return Err(PulseError::NonDeterministicState(
            "v3 snapshot conflict diagnostics do not match authoritative monetary replay"
                .to_string(),
        ));
    }

    Ok(replay.diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        reward_settlement_v3::{build_reward_claim_transaction_v3, settlement_outpoint_v3},
        state::SelectedParentPolicy,
        tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
        types::{compute_merkle_root, Block, BlockHeader, Transaction, TxInput, TxOutput},
    };

    const ONE_HOUR_PER_SCORE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 3_600_000_000_000,
    }];
    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn append_linear_block(
        state: &mut ChainState,
        label: &str,
        transactions: Vec<Transaction>,
        score: u64,
    ) {
        let parent = state.dag.selected_chain.last().unwrap().clone();
        let hash = label.to_string();
        let block = Block {
            hash: hash.clone(),
            header: BlockHeader {
                version: 2,
                parents: vec![parent.clone()],
                timestamp: 1_800_000_000 + score,
                difficulty: 1,
                nonce: score,
                merkle_root: format!("merkle-{label}"),
                state_root: format!("state-{label}"),
                blue_score: score,
                height: score,
            },
            transactions,
        };
        state.dag.blocks.insert(hash.clone(), block);
        state
            .dag
            .selected_parents
            .insert(hash.clone(), Some(parent));
        state.dag.selected_chain.push(hash.clone());
        state.dag.merge_set_blues.insert(hash.clone(), vec![]);
        state.dag.merge_set_reds.insert(hash.clone(), vec![]);
        state.dag.blue_work.insert(hash.clone(), u128::from(score));
        state.dag.tips.clear();
        state.dag.tips.insert(hash);
        state.dag.best_height = score;
        state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    }

    fn claim(chain_id: &str, beneficiary: &str, nonce: u64) -> Transaction {
        build_reward_claim_transaction_v3(beneficiary, nonce, chain_id).unwrap()
    }

    fn add_sibling_block(
        state: &mut ChainState,
        label: &str,
        sibling_of: &str,
        merge_anchor: &str,
        transactions: Vec<Transaction>,
    ) {
        let mut block = state.dag.blocks[sibling_of].clone();
        block.hash = label.into();
        block.header.nonce += 1;
        block.header.merkle_root = compute_merkle_root(&transactions);
        block.transactions = transactions;
        state.dag.blocks.insert(label.into(), block);
        state
            .dag
            .selected_parents
            .insert(label.into(), state.dag.selected_parents[sibling_of].clone());
        state
            .dag
            .blue_work
            .insert(label.into(), state.dag.blue_work[sibling_of]);
        state
            .dag
            .merge_set_blues
            .get_mut(merge_anchor)
            .unwrap()
            .push(label.into());
        state
            .dag
            .blocks
            .get_mut(merge_anchor)
            .unwrap()
            .header
            .parents
            .push(label.into());
    }

    fn assert_insertion_order_independent(state: &ChainState, replay: &StateReplayV3) {
        let mut reordered = state.clone();
        reordered.dag.blocks = replay
            .ordered_dag
            .blocks
            .iter()
            .rev()
            .map(|hash| (hash.clone(), state.dag.blocks[hash].clone()))
            .collect();
        let again = rebuild_authoritative_state_v3(&reordered, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(again.ordered_dag, replay.ordered_dag);
        assert_eq!(again.diagnostics, replay.diagnostics);
        assert_eq!(again.utxo.address_index, replay.utxo.address_index);
    }

    #[test]
    fn repeated_claim_txid_in_sibling_blocks_settles_once_per_block() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-claims".into(), 1_800_000_030).unwrap();
        let chain_id = state.chain_id.clone();
        let repeated_claim = claim(&chain_id, "pulse1miner", 41);
        append_linear_block(&mut state, "b1", vec![repeated_claim.clone()], 1);
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1merge", 42)],
            2,
        );
        append_linear_block(&mut state, "b3", vec![claim(&chain_id, "pulse1tip", 43)], 3);
        add_sibling_block(
            &mut state,
            "b1-peer",
            "b1",
            "b2",
            vec![repeated_claim.clone()],
        );

        let immature = rebuild_authoritative_state_v3(&state, &ONE_SECOND).unwrap();
        assert_eq!(immature.diagnostics.validated_reward_claims, 4);
        assert_eq!(immature.diagnostics.materialized_rewards, 0);
        assert!(immature.utxo.utxos.is_empty());
        assert!(immature.utxo.address_index.is_empty());

        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(
            &replay.ordered_dag.blocks[1..],
            &["b1", "b1-peer", "b2", "b3"]
        );
        assert_eq!(replay.diagnostics.validated_reward_claims, 4);
        assert_eq!(replay.diagnostics.applied_transactions, 0);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 0);
        assert_eq!(replay.diagnostics.materialized_rewards, 3);
        assert_eq!(replay.utxo.utxos.len(), 3);

        let first = settlement_outpoint_v3(&chain_id, "b1", &repeated_claim.txid);
        let peer = settlement_outpoint_v3(&chain_id, "b1-peer", &repeated_claim.txid);
        assert_ne!(first, peer);
        for (outpoint, score) in [(&first, 1), (&peer, 2)] {
            let reward = &replay.utxo.utxos[outpoint];
            assert_eq!(
                reward.amount,
                subsidy_atoms_for_score(score, &ONE_HOUR_PER_SCORE).unwrap()
            );
            assert_eq!(reward.address, "pulse1miner");
            assert!(reward.coinbase);
        }
        assert_eq!(replay.utxo.address_index["pulse1miner"], vec![first, peer]);
        assert!(!replay
            .utxo
            .utxos
            .keys()
            .any(|outpoint| outpoint.txid == repeated_claim.txid));
        assert_insertion_order_independent(&state, &replay);
    }

    #[test]
    fn maturity_prefix_is_cadence_normalized() {
        assert_eq!(
            mature_reward_prefix_score_v3(3_601, &ONE_SECOND).unwrap(),
            1
        );
        assert_eq!(
            mature_reward_prefix_score_v3(1, &ONE_HOUR_PER_SCORE).unwrap(),
            0
        );
        assert_eq!(
            mature_reward_prefix_score_v3(2, &ONE_HOUR_PER_SCORE).unwrap(),
            1
        );
    }

    #[test]
    fn reward_materializes_at_end_of_maturity_score_and_spends_next_score() {
        let mut state = init_chain_state_v3("reward-replay-v3".into(), 1_800_000_000).unwrap();
        let chain_id = state.chain_id.clone();

        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, "pulse1miner", 1)],
            1,
        );
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1miner2", 2)],
            2,
        );

        let reward1_claim = state.dag.blocks["b1"].transactions[0].clone();
        let reward1_outpoint = settlement_outpoint_v3(&chain_id, "b1", &reward1_claim.txid);
        let spend = Transaction {
            txid: "spend-reward-1".into(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward1_outpoint.clone(),
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 3,
        };
        append_linear_block(
            &mut state,
            "b3",
            vec![claim(&chain_id, "pulse1miner3", 3), spend.clone()],
            3,
        );

        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(replay.diagnostics.mature_reward_prefix_score, 2);
        assert_eq!(replay.diagnostics.materialized_rewards, 2);
        assert_eq!(replay.diagnostics.validated_reward_claims, 3);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 0);
        assert!(!replay.utxo.utxos.keys().any(|outpoint| {
            state
                .dag
                .blocks
                .values()
                .filter_map(|block| block.transactions.first())
                .any(|claim| outpoint.txid == claim.txid)
        }));
        assert!(!replay.utxo.utxos.contains_key(&reward1_outpoint));
        assert!(replay
            .utxo
            .utxos
            .keys()
            .any(|outpoint| outpoint.txid == spend.txid));
    }

    #[test]
    fn same_score_spend_is_not_visible_before_end_of_score_materialization() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-same-score".into(), 1_800_000_010).unwrap();
        let chain_id = state.chain_id.clone();
        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, "pulse1miner", 11)],
            1,
        );
        let claim1 = state.dag.blocks["b1"].transactions[0].clone();
        let reward1 = settlement_outpoint_v3(&chain_id, "b1", &claim1.txid);
        let early_spend = Transaction {
            txid: "same-score-spend".into(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward1.clone(),
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 12,
        };
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1miner2", 12), early_spend],
            2,
        );

        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(replay.diagnostics.mature_reward_prefix_score, 1);
        assert_eq!(replay.diagnostics.materialized_rewards, 1);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 1);
        assert!(replay.utxo.utxos.contains_key(&reward1));
    }

    fn assert_conflicting_sibling_fees_are_not_settled(duplicate_transaction: bool) {
        let mut state = init_chain_state_v3("reward-replay-v3-fees".into(), 1_800_000_015).unwrap();
        let chain_id = state.chain_id.clone();

        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, "pulse1miner1", 31)],
            1,
        );
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1miner2", 32)],
            2,
        );
        append_linear_block(
            &mut state,
            "b3",
            vec![claim(&chain_id, "pulse1miner3", 33)],
            3,
        );

        let reward1_claim = state.dag.blocks["b1"].transactions[0].clone();
        let reward1 = settlement_outpoint_v3(&chain_id, "b1", &reward1_claim.txid);
        let reward2_claim = state.dag.blocks["b2"].transactions[0].clone();
        let reward2 = settlement_outpoint_v3(&chain_id, "b2", &reward2_claim.txid);
        let subsidy1 = subsidy_atoms_for_score(1, &ONE_HOUR_PER_SCORE).unwrap();
        let subsidy2 = subsidy_atoms_for_score(2, &ONE_HOUR_PER_SCORE).unwrap();
        let mut first_spend = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward1.clone(),
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1first".into(),
                amount: subsidy1 - 5,
            }],
            fee: 5,
            nonce: 34,
        };
        first_spend.txid = compute_txid_v2(&first_spend, &chain_id).unwrap();
        append_linear_block(
            &mut state,
            "b4",
            vec![claim(&chain_id, "pulse1winner", 34), first_spend.clone()],
            4,
        );

        let mut conflicting_spend = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            // Both inputs were mature before the sibling fork. The unspent
            // input comes first to check rollback of a partially applied loser.
            inputs: vec![
                TxInput {
                    previous_output: reward2.clone(),
                    public_key: "pk".into(),
                    signature: "sig".into(),
                },
                TxInput {
                    previous_output: reward1.clone(),
                    public_key: "pk".into(),
                    signature: "sig".into(),
                },
            ],
            outputs: vec![TxOutput {
                address: "pulse1conflict".into(),
                amount: subsidy1 + subsidy2 - 99,
            }],
            fee: 99,
            nonce: 35,
        };
        conflicting_spend.txid = compute_txid_v2(&conflicting_spend, &chain_id).unwrap();
        if duplicate_transaction {
            conflicting_spend = first_spend.clone();
        }
        append_linear_block(
            &mut state,
            "b5",
            vec![claim(&chain_id, "pulse1merge", 36)],
            5,
        );
        add_sibling_block(
            &mut state,
            "b4-peer",
            "b4",
            "b5",
            vec![
                claim(&chain_id, "pulse1loser", 35),
                conflicting_spend.clone(),
            ],
        );

        let b4_claim = state.dag.blocks["b4"].transactions[0].clone();
        let b4_reward = settlement_outpoint_v3(&chain_id, "b4", &b4_claim.txid);
        let peer_claim = state.dag.blocks["b4-peer"].transactions[0].clone();
        let peer_reward = settlement_outpoint_v3(&chain_id, "b4-peer", &peer_claim.txid);
        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();

        assert_eq!(
            &replay.ordered_dag.blocks[1..],
            &["b1", "b2", "b3", "b4", "b4-peer", "b5"]
        );
        assert_eq!(replay.diagnostics.validated_reward_claims, 6);
        assert_eq!(replay.diagnostics.applied_transactions, 1);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 1);
        assert_eq!(replay.diagnostics.materialized_rewards, 5);
        assert_eq!(replay.diagnostics.conflict_diagnostics.len(), 1);
        assert_eq!(
            replay.diagnostics.conflict_diagnostics[0],
            format!(
                "ordered_pos=5 block=b4-peer tx={} skipped_conflict_atomic",
                conflicting_spend.txid
            )
        );
        // Reward scores, not the equal sibling heights, determine settlement.
        assert_eq!(
            replay.utxo.utxos[&b4_reward].amount,
            subsidy_atoms_for_score(4, &ONE_HOUR_PER_SCORE).unwrap() + first_spend.fee
        );
        assert_eq!(
            replay.utxo.utxos[&peer_reward].amount,
            subsidy_atoms_for_score(5, &ONE_HOUR_PER_SCORE).unwrap()
        );
        assert!(!replay.utxo.utxos.contains_key(&reward1));
        assert_eq!(replay.utxo.utxos[&reward2].amount, subsidy2);
        assert_eq!(replay.utxo.address_index["pulse1miner2"], vec![reward2]);
        assert_eq!(replay.utxo.utxos.len(), 5);
        if !duplicate_transaction {
            assert!(!replay
                .utxo
                .utxos
                .keys()
                .any(|outpoint| outpoint.txid == conflicting_spend.txid));
            assert!(!replay.utxo.address_index.contains_key("pulse1conflict"));
        }
        // The winner's fee is transferred once; no skipped fee creates value.
        let scheduled: u64 = (1..=5)
            .map(|score| subsidy_atoms_for_score(score, &ONE_HOUR_PER_SCORE).unwrap())
            .sum();
        assert_eq!(
            replay
                .utxo
                .utxos
                .values()
                .map(|utxo| utxo.amount)
                .sum::<u64>(),
            scheduled
        );
        assert_insertion_order_independent(&state, &replay);
    }

    #[test]
    fn conflict_skipped_transaction_fee_is_not_paid_to_miner() {
        assert_conflicting_sibling_fees_are_not_settled(false);
    }

    #[test]
    fn duplicate_outpoint_transaction_fee_is_not_paid_twice() {
        assert_conflicting_sibling_fees_are_not_settled(true);
    }

    #[test]
    fn additional_inputless_transaction_fails_closed() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-hidden".into(), 1_800_000_020).unwrap();
        let chain_id = state.chain_id.clone();
        let hidden = Transaction {
            txid: "hidden".into(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "pulse1hidden".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 99,
        };
        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, "pulse1miner", 21), hidden],
            1,
        );

        assert!(rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE)
            .unwrap_err()
            .to_string()
            .contains("additional inputless"));
    }
}
