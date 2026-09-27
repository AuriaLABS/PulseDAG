use std::collections::hash_map::Entry;

use serde::{Deserialize, Serialize};

use crate::{
    apply::apply_transaction,
    errors::PulseError,
    genesis_v3::init_chain_state_v3,
    monetary_v3::{
        economic_maturity_reached, subsidy_atoms_for_score, MonetaryCadenceSegment,
    },
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2},
    reward_settlement_v3::{
        settlement_outpoint_v3, validate_reward_claim_transaction_v3,
    },
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
    pub diagnostics: StateReplayV3Diagnostics,
}

fn invalid_replay(message: impl Into<String>) -> PulseError {
    PulseError::NonDeterministicState(format!(
        "v3 reward replay: {}",
        message.into()
    ))
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
        let mid = low + (high - low + 1) / 2;
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

fn block_fees_atoms(block: &crate::types::Block) -> Result<u64, PulseError> {
    block
        .transactions
        .iter()
        .skip(1)
        .try_fold(0_u64, |acc, tx| {
            acc.checked_add(tx.fee)
                .ok_or_else(|| invalid_replay("reward fee arithmetic overflow"))
        })
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
    let fees_atoms = block_fees_atoms(block)?;
    let amount = subsidy_atoms
        .checked_add(fees_atoms)
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
/// 1. validate/apply that score's amountless reward claim and ordinary txs;
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

    let mut rebuilt = init_chain_state_v3(state.chain_id.clone(), genesis.header.timestamp)?;
    if rebuilt.dag.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_replay(format!(
            "v3 genesis identity mismatch: expected {}, rebuilt {}",
            state.dag.genesis_hash, rebuilt.dag.genesis_hash
        )));
    }
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
    rebuilt.dag.selected_parent_policy = state.dag.selected_parent_policy;

    let mut applied_transactions = 0usize;
    let mut skipped_conflicting_transactions = 0usize;
    let mut materialized_rewards = 0usize;
    let mut conflict_diagnostics = Vec::new();
    let mut next_reward_score = 1_u64;
    let mut mature_reward_prefix_score = 0_u64;

    for (ordered_pos, hash) in ordered_dag.blocks.iter().enumerate() {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        let current_score = u64::try_from(ordered_pos)
            .map_err(|_| invalid_replay("ordered score exceeds u64"))?;
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

        apply_transaction(claim, &mut rebuilt, block.header.height)?;
        applied_transactions = applied_transactions.saturating_add(1);

        for tx in block.transactions.iter().skip(1) {
            let mut candidate = rebuilt.clone();
            match apply_transaction(tx, &mut candidate, block.header.height) {
                Ok(()) => {
                    rebuilt = candidate;
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
                &ordered_dag,
                next_reward_score,
                cadence_segments,
            )?;
            materialized_rewards = materialized_rewards.saturating_add(1);
            next_reward_score = next_reward_score.saturating_add(1);
        }
    }

    let state_root = rebuilt.utxo.compute_state_root()?;
    Ok(StateReplayV3 {
        utxo: rebuilt.utxo,
        diagnostics: StateReplayV3Diagnostics {
            applied_transactions,
            skipped_conflicting_transactions,
            materialized_rewards,
            mature_reward_prefix_score,
            conflict_diagnostics,
            state_root,
            ordered_dag_tip: ordered_dag.blocks.last().cloned(),
            ordered_dag_digest: ordered_dag.digest.clone(),
        },
        ordered_dag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        reward_settlement_v3::{build_reward_claim_transaction_v3, settlement_outpoint_v3},
        state::SelectedParentPolicy,
        tx::TRANSACTION_VERSION_V2,
        types::{Block, BlockHeader, Transaction, TxInput, TxOutput},
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
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 0);
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
