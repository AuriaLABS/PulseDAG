use std::collections::{hash_map::Entry, BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    apply::apply_transaction,
    errors::PulseError,
    genesis_v3::init_chain_state_v3,
    monetary_v3::{economic_maturity_reached, subsidy_atoms_for_score, MonetaryCadenceSegment},
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2},
    reward_settlement_v3::{settlement_outpoint_v3, validate_reward_claim_transaction_v3},
    state::{ChainState, UtxoState},
    tx::{compute_txid_v2, verify_transaction_signatures_v2},
    types::{Hash, OutPoint, Transaction, Utxo},
    validation_v2::validate_transaction_v2,
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
) -> Result<OutPoint, PulseError> {
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
            outpoint: outpoint.clone(),
            address: beneficiary,
            amount,
            coinbase: true,
            height: block.header.height,
        },
    )?;
    Ok(outpoint)
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

/// Verify the chain-bound signature and exact monetary conservation against
/// an explicitly reconstructed spendable-input view.  A spent *conflict loser*
/// is validated against its authenticated historical UTXOs, not silently
/// accepted solely because its inputs are absent from the current live set.
fn validate_signed_replay_spend(
    tx: &Transaction,
    input_state: &ChainState,
    chain_id: &str,
    ordered_pos: usize,
    block_hash: &Hash,
) -> Result<(), PulseError> {
    validate_transaction_v2(tx, input_state, chain_id).map_err(|error| {
        invalid_replay(format!(
            "ordered_pos={ordered_pos} block={block_hash} invalid signed transfer {}: {error}",
            tx.txid
        ))
    })?;
    let inputs = tx.inputs.iter().try_fold(0_u128, |sum, input| {
        let coin = input_state
            .utxo
            .utxos
            .get(&input.previous_output)
            .ok_or_else(|| invalid_replay("signed input vanished before replay"))?;
        sum.checked_add(u128::from(coin.amount))
            .ok_or_else(|| invalid_replay("signed input amount overflow"))
    })?;
    let outputs = tx.outputs.iter().try_fold(0_u128, |sum, output| {
        sum.checked_add(u128::from(output.amount))
            .ok_or_else(|| invalid_replay("signed output amount overflow"))
    })?;
    let outputs_plus_fee = outputs
        .checked_add(u128::from(tx.fee))
        .ok_or_else(|| invalid_replay("signed transfer fee overflow"))?;
    if inputs != outputs_plus_fee {
        return Err(invalid_replay(format!(
            "ordered_pos={ordered_pos} block={block_hash} signed input/output/fee nonconservation for {}",
            tx.txid
        )));
    }
    Ok(())
}

/// Preindex only the outpoints used more than once in this exact ordered
/// prefix. The temporary all-input set is dropped after this pass; ordinary
/// one-time spends need no long-lived consumed-UTXO archive.
fn repeated_replay_input_counts_v3(
    state: &ChainState,
    ordered_dag: &OrderedDagV2,
    end_exclusive: usize,
) -> Result<BTreeMap<OutPoint, usize>, PulseError> {
    let mut seen_once = BTreeSet::new();
    let mut repeated: BTreeMap<OutPoint, usize> = BTreeMap::new();
    for hash in ordered_dag.blocks.iter().take(end_exclusive) {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        let block = state.dag.blocks.get(hash).ok_or_else(|| {
            invalid_replay(format!(
                "ordered block {hash} missing during input preindex"
            ))
        })?;
        for tx in block.transactions.iter().skip(1) {
            for input in &tx.inputs {
                let outpoint = &input.previous_output;
                if let Some(count) = repeated.get_mut(outpoint) {
                    *count = count
                        .checked_add(1)
                        .ok_or_else(|| invalid_replay("input reference count overflow"))?;
                } else if !seen_once.insert(outpoint.clone()) {
                    repeated.insert(outpoint.clone(), 2_usize);
                }
            }
        }
    }
    Ok(repeated)
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
    // Only archive consumed UTXOs needed for an actual repeated input in the
    // authenticated ordered prefix. An ordinary one-time accepted spend does
    // not require a historical clone for every later replay step.
    let mut repeated_input_references =
        repeated_replay_input_counts_v3(state, ordered_dag, end_exclusive)?;
    let mut spent_by_accepted_transfer: BTreeMap<OutPoint, Utxo> = BTreeMap::new();
    let mut validated_reward_outpoints: BTreeMap<OutPoint, Utxo> = BTreeMap::new();

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
        let reward_outpoint = settlement_outpoint_v3(&state.chain_id, hash, &claim.txid);
        let reward_address = claim
            .outputs
            .first()
            .ok_or_else(|| invalid_replay("validated claim lost its beneficiary"))?
            .address
            .clone();
        validated_reward_outpoints.insert(
            reward_outpoint.clone(),
            Utxo {
                outpoint: reward_outpoint,
                address: reward_address,
                // No actual coins: only authenticated signer provenance.
                // Economic value is unknown until reward maturity.
                amount: 0,
                coinbase: true,
                height: block.header.height,
            },
        );

        for tx in block.transactions.iter().skip(1) {
            // Only an actual AVAILABLE set of signed spend inputs is eligible
            // to alter monetary UTXO state or contribute a miner fee. A Merkle
            // commitment / chain-bound txid alone is never spend authority.
            // Preserve the existing deterministic skip for inputs that lost
            // an earlier canonical conflict; their fees never accrue.
            let all_inputs_available = !tx.inputs.is_empty()
                && tx
                    .inputs
                    .iter()
                    .all(|input| rebuilt.utxo.utxos.contains_key(&input.previous_output));
            if !all_inputs_available {
                // A previously consumed accepted transfer is a concrete
                // conflict. An already validated reward claim may exist yet be
                // economically immature (legacy compatibility behavior).
                // Anything else is an unauthenticated phantom outpoint, NOT
                // evidence of a valid conflict loser.
                for input in &tx.inputs {
                    let outpoint = &input.previous_output;
                    if !rebuilt.utxo.utxos.contains_key(outpoint)
                        && !spent_by_accepted_transfer.contains_key(outpoint)
                        && !validated_reward_outpoints.contains_key(outpoint)
                    {
                        return Err(invalid_replay(format!(
                            "ordered_pos={ordered_pos} block={hash} transfer {} references outpoint {}:{} without authenticated replay provenance",
                            tx.txid, outpoint.txid, outpoint.index
                        )));
                    }
                }
            }
            if all_inputs_available {
                validate_signed_replay_spend(tx, &rebuilt, &state.chain_id, ordered_pos, hash)?;
            } else {
                // Missing inputs must have authenticated provenance from
                // previously accepted spends or already validated reward
                // claims. Never resurrect any of them in the real replay.
                let mut historical = rebuilt.clone();
                let mut has_immature_reward = false;
                for input in &tx.inputs {
                    let outpoint = &input.previous_output;
                    // An available reward is already mature and spendable.
                    // Only MISSING reward outputs may take the immature-claim
                    // signature-only path. Otherwise a mixed spent/live input
                    // conflict could bypass exact monetary conservation.
                    if historical.utxo.utxos.contains_key(outpoint) {
                        continue;
                    }
                    let coin = if let Some(coin) = spent_by_accepted_transfer.get(outpoint) {
                        Some(coin)
                    } else if let Some(coin) = validated_reward_outpoints.get(outpoint) {
                        has_immature_reward = true;
                        Some(coin)
                    } else {
                        None
                    };
                    if let Some(coin) = coin {
                        historical.utxo.utxos.insert(outpoint.clone(), coin.clone());
                        historical
                            .utxo
                            .address_index
                            .entry(coin.address.clone())
                            .or_default()
                            .push(outpoint.clone());
                    }
                }
                if has_immature_reward {
                    // The claim proves the beneficiary address, but not a
                    // spendable balance or fee entitlement yet. Authenticate
                    // all signatures and the exact chain-bound txid before
                    // discarding this immature transaction; DO NOT settle it.
                    if tx.outputs.is_empty()
                        || tx.outputs.iter().any(|output| output.amount == 0)
                        || tx
                            .inputs
                            .iter()
                            .map(|input| &input.previous_output)
                            .collect::<BTreeSet<_>>()
                            .len()
                            != tx.inputs.len()
                    {
                        return Err(invalid_replay(format!(
                            "ordered_pos={ordered_pos} block={hash} malformed immature signed transfer {}",
                            tx.txid
                        )));
                    }
                    let expected_txid = compute_txid_v2(tx, &state.chain_id)
                        .map_err(|error| invalid_replay(error.to_string()))?;
                    if expected_txid != tx.txid {
                        return Err(invalid_replay(format!(
                            "ordered_pos={ordered_pos} block={hash} invalid immature signed transfer txid {}",
                            tx.txid
                        )));
                    }
                    verify_transaction_signatures_v2(tx, &historical, &state.chain_id)
                        .map_err(|error| {
                            invalid_replay(format!(
                                "ordered_pos={ordered_pos} block={hash} invalid immature signed transfer {}: {error}",
                                tx.txid
                            ))
                        })?;
                } else {
                    // A true spent-input conflict must also conserve amounts.
                    // The historical copy is never committed or fee-credited.
                    validate_signed_replay_spend(
                        tx,
                        &historical,
                        &state.chain_id,
                        ordered_pos,
                        hash,
                    )?;
                }
            }
            let mut candidate = rebuilt.clone();
            match apply_transaction(tx, &mut candidate, block.header.height) {
                Ok(()) => {
                    for input in &tx.inputs {
                        if repeated_input_references.contains_key(&input.previous_output) {
                            let coin =
                                rebuilt.utxo.utxos.get(&input.previous_output).ok_or_else(
                                    || invalid_replay("accepted transfer input missing"),
                                )?;
                            spent_by_accepted_transfer
                                .insert(input.previous_output.clone(), coin.clone());
                        }
                    }
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
            // Drop the historical coin as soon as its final reference has
            // been examined. Only still-relevant conflict provenance survives
            // into the next score.
            for input in &tx.inputs {
                let outpoint = &input.previous_output;
                let finished = if let Some(remaining) = repeated_input_references.get_mut(outpoint)
                {
                    *remaining = remaining
                        .checked_sub(1)
                        .ok_or_else(|| invalid_replay("input reference count underflow"))?;
                    *remaining == 0
                } else {
                    false
                };
                if finished {
                    repeated_input_references.remove(outpoint);
                    spent_by_accepted_transfer.remove(outpoint);
                }
            }
        }

        mature_reward_prefix_score =
            mature_reward_prefix_score_v3(current_score, cadence_segments)?;
        while next_reward_score <= mature_reward_prefix_score {
            let matured_outpoint = materialize_reward_at_score(
                &mut rebuilt,
                state,
                ordered_dag,
                next_reward_score,
                eligible_fees_by_score[usize::try_from(next_reward_score)
                    .map_err(|_| invalid_replay("reward score exceeds platform index width"))?],
                cadence_segments,
            )?;
            // Once the claim has materialized, only the live UTXO (or a
            // repeated-input spent provenance record) may authorize a spend.
            // Discard its temporary unspendable signer placeholder.
            validated_reward_outpoints.remove(&matured_outpoint);
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
        address_from_public_key,
        reward_settlement_v3::{build_reward_claim_transaction_v3, settlement_outpoint_v3},
        signing_message_v2,
        state::SelectedParentPolicy,
        tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
        types::{compute_merkle_root, Block, BlockHeader, Transaction, TxInput, TxOutput},
    };
    use ed25519_dalek::{Signer, SigningKey};

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

    fn replay_test_signer() -> SigningKey {
        SigningKey::from_bytes(&[37_u8; 32])
    }

    fn replay_test_public_key() -> String {
        hex::encode(replay_test_signer().verifying_key().to_bytes())
    }

    fn replay_test_address() -> String {
        address_from_public_key(&replay_test_public_key())
    }

    fn sign_replay_transfer(tx: &mut Transaction, chain_id: &str) {
        let signer = replay_test_signer();
        let public_key = replay_test_public_key();
        for input in &mut tx.inputs {
            input.public_key = public_key.clone();
            input.signature.clear();
        }
        let signed_message = signing_message_v2(tx, chain_id).unwrap();
        let signature = hex::encode(signer.sign(&signed_message).to_bytes());
        for input in &mut tx.inputs {
            input.signature = signature.clone();
        }
        tx.txid = compute_txid_v2(tx, chain_id).unwrap();
    }

    #[derive(Clone, Serialize, Deserialize)]
    struct DetachedTestCheckpointV3 {
        chain_id: String,
        ordered_dag_digest: String,
        ordered_dag_tip: Option<Hash>,
        mature_reward_prefix_score: u64,
        coins: Vec<Utxo>,
        fees: Vec<u64>,
        conflicts: Vec<String>,
    }

    // Test-only checkpoint verifier. Expected facts MUST be derived from
    // independently authenticated complete history. UTXO roots alone do not
    // bind DAG order, maturity, fee provenance, or conflict decisions.
    fn verify_detached_checkpoint_against_full_replay(
        encoded: &[u8],
        state: &ChainState,
        expected: &StateReplayV3,
    ) -> Result<UtxoState, String> {
        let checkpoint: DetachedTestCheckpointV3 =
            serde_json::from_slice(encoded).map_err(|error| error.to_string())?;
        if checkpoint.chain_id != state.chain_id
            || checkpoint.ordered_dag_digest != expected.diagnostics.ordered_dag_digest
            || checkpoint.ordered_dag_tip != expected.diagnostics.ordered_dag_tip
            || checkpoint.mature_reward_prefix_score
                != expected.diagnostics.mature_reward_prefix_score
        {
            return Err("detached checkpoint chain/order/maturity differs from full replay".into());
        }
        let mut restored = UtxoState::default();
        for coin in checkpoint.coins {
            let outpoint = coin.outpoint.clone();
            let address = coin.address.clone();
            if restored.utxos.insert(outpoint.clone(), coin).is_some() {
                return Err("duplicate checkpoint outpoint".into());
            }
            restored
                .address_index
                .entry(address)
                .or_default()
                .push(outpoint);
        }
        let root = restored
            .compute_state_root()
            .map_err(|error| error.to_string())?;
        if root != expected.diagnostics.state_root {
            return Err("detached checkpoint UTXO root differs from full replay".into());
        }
        if checkpoint.fees != expected.eligible_fees_by_score {
            return Err("detached checkpoint fees differ from full replay".into());
        }
        if checkpoint.conflicts != expected.diagnostics.conflict_diagnostics {
            return Err("detached checkpoint conflicts differ from full replay".into());
        }
        Ok(restored)
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
        // A different in-memory insertion order cannot alter monetary replay:
        // fees, reward outpoints, amounts, and the authenticated UTXO root
        // must all match, including when sibling spends conflict.
        assert_eq!(again.eligible_fees_by_score, replay.eligible_fees_by_score);
        assert_eq!(again.utxo.address_index, replay.utxo.address_index);
        assert_eq!(
            again.utxo.compute_state_root().unwrap(),
            replay.utxo.compute_state_root().unwrap()
        );
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
            vec![claim(&chain_id, &replay_test_address(), 1)],
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
        let mut spend = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward1_outpoint.clone(),
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: subsidy_atoms_for_score(1, &ONE_HOUR_PER_SCORE).unwrap(),
            }],
            fee: 0,
            nonce: 3,
        };
        sign_replay_transfer(&mut spend, &chain_id);
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
            vec![claim(&chain_id, &replay_test_address(), 11)],
            1,
        );
        let claim1 = state.dag.blocks["b1"].transactions[0].clone();
        let reward1 = settlement_outpoint_v3(&chain_id, "b1", &claim1.txid);
        let mut early_spend = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward1.clone(),
                public_key: String::new(),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 12,
        };
        sign_replay_transfer(&mut early_spend, &chain_id);
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1miner2", 12), early_spend.clone()],
            2,
        );

        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(replay.diagnostics.mature_reward_prefix_score, 1);
        assert_eq!(replay.diagnostics.materialized_rewards, 1);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 1);
        assert!(replay.utxo.utxos.contains_key(&reward1));

        // An immature outpoint is not a free pass for fraudulent signatures.
        // Even rehashing the altered transaction must fail before conflict skip.
        let mut forged = early_spend;
        forged.inputs[0].signature = "00".repeat(64);
        forged.txid = compute_txid_v2(&forged, &chain_id).unwrap();
        state.dag.blocks.get_mut("b2").unwrap().transactions[1] = forged;
        let error = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid immature signed transfer"),
            "{error}"
        );
    }

    fn assert_conflicting_sibling_fees_are_not_settled(
        duplicate_transaction: bool,
        corrupt_loser_signature: bool,
        corrupt_loser_balance: bool,
    ) {
        let mut state = init_chain_state_v3("reward-replay-v3-fees".into(), 1_800_000_015).unwrap();
        let chain_id = state.chain_id.clone();

        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, &replay_test_address(), 31)],
            1,
        );
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, &replay_test_address(), 32)],
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
        sign_replay_transfer(&mut first_spend, &chain_id);
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
        sign_replay_transfer(&mut conflicting_spend, &chain_id);
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

        if corrupt_loser_signature {
            // The missing input was authentically spent by the earlier
            // accepted winner. Its competitor still must have a valid
            // chain-bound signature, rather than hiding behind UtxoNotFound.
            let peer = state.dag.blocks.get_mut("b4-peer").unwrap();
            peer.transactions[1].inputs[0].signature = "00".repeat(64);
            peer.transactions[1].txid = compute_txid_v2(&peer.transactions[1], &chain_id).unwrap();
            let error = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string();
            assert!(error.contains("invalid signed transfer"), "{error}");
            return;
        }

        if corrupt_loser_balance {
            // A conflict loser with a VALID Ed25519 signature must not hide
            // an undeclared monetary burn behind the missing spent input.
            // Re-sign after changing its output so the signature and txid are
            // genuine; only exact input = output + fee conservation can fail.
            let peer = state.dag.blocks.get_mut("b4-peer").unwrap();
            let losing_tx = &mut peer.transactions[1];
            losing_tx.outputs[0].amount -= 1;
            sign_replay_transfer(losing_tx, &chain_id);
            let error = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string();
            assert!(error.contains("nonconservation"), "{error}");
            return;
        }

        let b4_claim = state.dag.blocks["b4"].transactions[0].clone();
        let b4_reward = settlement_outpoint_v3(&chain_id, "b4", &b4_claim.txid);
        let peer_claim = state.dag.blocks["b4-peer"].transactions[0].clone();
        let peer_reward = settlement_outpoint_v3(&chain_id, "b4-peer", &peer_claim.txid);
        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();

        assert_eq!(
            &replay.ordered_dag.blocks[1..],
            &["b1", "b2", "b3", "b4", "b4-peer", "b5"]
        );
        let repeated = repeated_replay_input_counts_v3(
            &state,
            &replay.ordered_dag,
            replay.ordered_dag.blocks.len(),
        )
        .unwrap();
        // Only the genuinely competing reward1 input needs historical UTXO
        // retention; reward2 has a single input reference in this full order.
        assert_eq!(repeated.len(), 1);
        assert_eq!(repeated.get(&reward1), Some(&2));
        assert!(!repeated.contains_key(&reward2));
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
        assert_eq!(
            replay.utxo.address_index[&replay_test_address()],
            vec![reward2.clone()]
        );
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

        // The pre-candidate monetary overlay must contain only already-mature
        // rewards and must not re-credit a conflicting sibling fee. This is
        // the same boundary used by candidate validation and template logic.
        let pre_merge = materialize_authoritative_pre_candidate_state_v3(
            &state,
            &"b5".to_string(),
            &ONE_HOUR_PER_SCORE,
        )
        .unwrap();
        assert_eq!(pre_merge.dag.ordered_dag, replay.ordered_dag.blocks[..6]);
        assert_eq!(
            pre_merge.utxo.utxos[&b4_reward].amount,
            subsidy_atoms_for_score(4, &ONE_HOUR_PER_SCORE).unwrap() + first_spend.fee
        );
        assert!(!pre_merge.utxo.utxos.contains_key(&peer_reward));
        assert_eq!(pre_merge.utxo.utxos[&reward2].amount, subsidy2);

        // An unpruned monetary snapshot must match a fresh full-history
        // replay even when a signed sibling spend is skipped. Re-check both
        // duplicate-txid and different-txid conflict variants.
        let snapshot = materialize_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(
            verify_authoritative_state_snapshot_v3(&snapshot, &ONE_HOUR_PER_SCORE).unwrap(),
            replay.diagnostics
        );

        // A detached serialized checkpoint has to bind economic facts to
        // the independently verified chain, canonical order, and maturity
        // boundary. This is NOT an independently trusted pruned checkpoint.
        let mut coins: Vec<Utxo> = snapshot.utxo.utxos.values().cloned().collect();
        coins.sort_by_key(|coin| (coin.outpoint.txid.clone(), coin.outpoint.index));
        let checkpoint = DetachedTestCheckpointV3 {
            chain_id: state.chain_id.clone(),
            ordered_dag_digest: replay.diagnostics.ordered_dag_digest.clone(),
            ordered_dag_tip: replay.diagnostics.ordered_dag_tip.clone(),
            mature_reward_prefix_score: replay.diagnostics.mature_reward_prefix_score,
            coins,
            fees: replay.eligible_fees_by_score.clone(),
            conflicts: replay.diagnostics.conflict_diagnostics.clone(),
        };
        let encoded = serde_json::to_vec(&checkpoint).unwrap();
        let restored =
            verify_detached_checkpoint_against_full_replay(&encoded, &state, &replay).unwrap();
        assert_eq!(
            restored.compute_state_root().unwrap(),
            replay.diagnostics.state_root
        );
        assert_eq!(replay.eligible_fees_by_score[4], first_spend.fee);
        assert_eq!(replay.eligible_fees_by_score[5], 0);

        // A forged fee or missing conflict cannot pass simply by preserving
        // the exact same UTXO state root.
        let mut forged_fees = checkpoint.clone();
        forged_fees.fees[5] = conflicting_spend.fee;
        assert!(verify_detached_checkpoint_against_full_replay(
            &serde_json::to_vec(&forged_fees).unwrap(),
            &state,
            &replay
        )
        .unwrap_err()
        .contains("fees differ"));

        let mut forged_conflicts = checkpoint.clone();
        forged_conflicts.conflicts.clear();
        assert!(verify_detached_checkpoint_against_full_replay(
            &serde_json::to_vec(&forged_conflicts).unwrap(),
            &state,
            &replay
        )
        .unwrap_err()
        .contains("conflicts differ"));

        // Consensus identity and reward maturity cannot be replaced with
        // values from another fork or chain, even if the UTXO root matches.
        let mut forged_order = checkpoint.clone();
        forged_order.ordered_dag_digest = "forged-ordered-dag".into();
        assert!(verify_detached_checkpoint_against_full_replay(
            &serde_json::to_vec(&forged_order).unwrap(),
            &state,
            &replay
        )
        .unwrap_err()
        .contains("chain/order/maturity differs"));

        let mut forged_maturity = checkpoint.clone();
        forged_maturity.mature_reward_prefix_score += 1;
        assert!(verify_detached_checkpoint_against_full_replay(
            &serde_json::to_vec(&forged_maturity).unwrap(),
            &state,
            &replay
        )
        .unwrap_err()
        .contains("chain/order/maturity differs"));

        let mut forged_chain = checkpoint;
        forged_chain.chain_id = "wrong-checkpoint-chain".into();
        assert!(verify_detached_checkpoint_against_full_replay(
            &serde_json::to_vec(&forged_chain).unwrap(),
            &state,
            &replay
        )
        .unwrap_err()
        .contains("chain/order/maturity differs"));

        // Modifying a miner reward in the restored UTXO set must fail even
        // when the stored claimed root and DAG ordering remain unchanged.
        let mut corrupted_amount = snapshot.clone();
        corrupted_amount
            .utxo
            .utxos
            .get_mut(&b4_reward)
            .unwrap()
            .amount += 1;
        assert!(
            verify_authoritative_state_snapshot_v3(&corrupted_amount, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string()
                .contains("snapshot UTXO root does not match")
        );

        let mut corrupted_commitment = snapshot.clone();
        corrupted_commitment.dag.ordered_dag_state_root = Some("forged-state-root".into());
        assert!(
            verify_authoritative_state_snapshot_v3(&corrupted_commitment, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string()
                .contains("snapshot recorded state root does not match")
        );

        // A snapshot with the advertised UTXO state but missing historical
        // ancestry is NOT a verified compact checkpoint. Reject rather than
        // trusting the saved monetary root as independent history authority.
        let mut missing_ancestor = snapshot.clone();
        missing_ancestor.dag.blocks.remove("b1");
        assert!(
            verify_authoritative_state_snapshot_v3(&missing_ancestor, &ONE_HOUR_PER_SCORE).is_err()
        );

        let mut omitted_conflict = snapshot;
        omitted_conflict
            .dag
            .ordered_dag_conflict_diagnostics
            .clear();
        assert!(
            verify_authoritative_state_snapshot_v3(&omitted_conflict, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string()
                .contains("snapshot conflict diagnostics do not match")
        );
    }

    #[test]
    fn conflict_skipped_transaction_fee_is_not_paid_to_miner() {
        assert_conflicting_sibling_fees_are_not_settled(false, false, false);
    }

    #[test]
    fn duplicate_outpoint_transaction_fee_is_not_paid_twice() {
        assert_conflicting_sibling_fees_are_not_settled(true, false, false);
    }

    #[test]
    fn conflict_loser_with_invalid_signature_is_not_silently_discarded() {
        assert_conflicting_sibling_fees_are_not_settled(false, true, false);
    }

    #[test]
    fn conflict_loser_with_valid_signature_and_implicit_fee_gap_is_rejected() {
        assert_conflicting_sibling_fees_are_not_settled(false, false, true);
    }

    #[test]
    fn authoritative_replay_rejects_bad_signature_and_implicit_fee_gap_before_state_mutation() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-signature-gate".into(), 1_800_000_045).unwrap();
        let chain_id = state.chain_id.clone();
        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, &replay_test_address(), 71)],
            1,
        );
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1other-reward", 72)],
            2,
        );
        let reward_claim = state.dag.blocks["b1"].transactions[0].clone();
        let mature_source = settlement_outpoint_v3(&chain_id, "b1", &reward_claim.txid);
        let subsidy = subsidy_atoms_for_score(1, &ONE_HOUR_PER_SCORE).unwrap();
        assert!(subsidy > 10);

        let mut signed_spend = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: mature_source,
                public_key: String::new(),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1legitimate-recipient".into(),
                amount: subsidy - 5,
            }],
            fee: 5,
            nonce: 73,
        };
        sign_replay_transfer(&mut signed_spend, &chain_id);
        append_linear_block(
            &mut state,
            "b3",
            vec![
                claim(&chain_id, "pulse1third-reward", 73),
                signed_spend.clone(),
            ],
            3,
        );

        let baseline = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        assert_eq!(baseline.diagnostics.applied_transactions, 1);
        assert_eq!(baseline.diagnostics.skipped_conflicting_transactions, 0);
        assert_eq!(baseline.eligible_fees_by_score[3], 5);
        assert!(baseline
            .utxo
            .utxos
            .keys()
            .any(|outpoint| outpoint.txid == signed_spend.txid));

        let mut forged_signature = signed_spend.clone();
        forged_signature.inputs[0].signature = "00".repeat(64);
        forged_signature.txid = compute_txid_v2(&forged_signature, &chain_id).unwrap();
        let mut mutated = state.clone();
        mutated.dag.blocks.get_mut("b3").unwrap().transactions[1] = forged_signature;
        let error = rebuild_authoritative_state_v3(&mutated, &ONE_HOUR_PER_SCORE)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid signed transfer"), "{error}");

        // Rehashing txid and block commitment is NOT a substitute for
        // authenticating the transaction signing message.
        let mut forged_output = signed_spend.clone();
        forged_output.outputs[0].address = "pulse1signature-hijack".into();
        forged_output.txid = compute_txid_v2(&forged_output, &chain_id).unwrap();
        let mut altered = state.clone();
        altered.dag.blocks.get_mut("b3").unwrap().transactions[1] = forged_output;
        assert!(
            rebuild_authoritative_state_v3(&altered, &ONE_HOUR_PER_SCORE)
                .unwrap_err()
                .to_string()
                .contains("invalid signed transfer")
        );

        // Even a fully valid v2 signature/txid may hide an implicit extra
        // burn: v2 admits input >= output + claimed fee. Monetary v3 requires
        // EXACT conservation so the only fee counted is the one claimed.
        let mut unclaimed_gap = signed_spend.clone();
        unclaimed_gap.outputs[0].amount = subsidy - 8;
        sign_replay_transfer(&mut unclaimed_gap, &chain_id);
        let mut gap_state = state;
        gap_state.dag.blocks.get_mut("b3").unwrap().transactions[1] = unclaimed_gap;
        let error = rebuild_authoritative_state_v3(&gap_state, &ONE_HOUR_PER_SCORE)
            .unwrap_err()
            .to_string();
        assert!(error.contains("nonconservation"), "{error}");
    }

    #[test]
    fn repeated_input_preindex_remains_sparse_across_many_independent_spends() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-sparse-provenance".into(), 1_800_000_000)
                .unwrap();
        let chain_id = state.chain_id.clone();
        let mut first_spend = None;
        let mut first_source = None;

        // The first two rewards must mature before any ordinary transfer can
        // execute. Subsequent scores spend distinct mature reward outputs,
        // except for the final intentionally conflicting sibling.
        for score in 1_u64..=18 {
            let mut transactions = vec![claim(&chain_id, &replay_test_address(), 100 + score)];
            if score >= 3 {
                let reward_score = score - 2;
                let block_hash = format!("b{reward_score}");
                let reward_claim = &state.dag.blocks[&block_hash].transactions[0];
                let source = settlement_outpoint_v3(&chain_id, &block_hash, &reward_claim.txid);
                let subsidy = subsidy_atoms_for_score(reward_score, &ONE_HOUR_PER_SCORE).unwrap();
                // From b3 onward each reward also contains exactly one fee
                // earned by the accepted spend in that reward's own block.
                let earned_fee = u64::from(reward_score >= 3);
                let settled_amount = subsidy.checked_add(earned_fee).unwrap();
                let mut spend = Transaction {
                    txid: String::new(),
                    version: TRANSACTION_VERSION_V2,
                    inputs: vec![TxInput {
                        previous_output: source.clone(),
                        public_key: String::new(),
                        signature: String::new(),
                    }],
                    outputs: vec![TxOutput {
                        address: format!("pulse1recipient{score}"),
                        amount: settled_amount - 1,
                    }],
                    fee: 1,
                    nonce: score + 200,
                };
                sign_replay_transfer(&mut spend, &chain_id);
                if score == 3 {
                    first_source = Some(source);
                    first_spend = Some(spend.clone());
                }
                transactions.push(spend);
            }
            append_linear_block(&mut state, &format!("b{score}"), transactions, score);
        }

        let mut loser = first_spend.unwrap();
        loser.nonce = 999;
        loser.outputs[0].address = "pulse1lateconflict".into();
        sign_replay_transfer(&mut loser, &chain_id);
        append_linear_block(
            &mut state,
            "b19",
            vec![claim(&chain_id, "pulse1lastminer", 119), loser],
            19,
        );

        let replay = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();
        let references = repeated_replay_input_counts_v3(
            &state,
            &replay.ordered_dag,
            replay.ordered_dag.blocks.len(),
        )
        .unwrap();
        // Sixteen accepted ordinary transfers, yet only one historically
        // consumed outpoint needs a retained value for the late conflict.
        assert_eq!(references.len(), 1);
        assert_eq!(references.get(&first_source.unwrap()), Some(&2));
        assert_eq!(replay.diagnostics.applied_transactions, 16);
        assert_eq!(replay.diagnostics.skipped_conflicting_transactions, 1);
        assert!(replay.eligible_fees_by_score[3..19]
            .iter()
            .all(|fee| *fee == 1));
        assert_eq!(replay.eligible_fees_by_score[19], 0);
        assert_insertion_order_independent(&state, &replay);
    }

    #[test]
    fn entirely_unknown_input_is_not_an_authenticated_conflict_loser() {
        let mut state =
            init_chain_state_v3("reward-replay-v3-unknown-input".into(), 1_800_000_046).unwrap();
        let chain_id = state.chain_id.clone();
        append_linear_block(
            &mut state,
            "b1",
            vec![claim(&chain_id, "pulse1miner", 81)],
            1,
        );
        let phantom = Transaction {
            txid: "not-an-authenticated-conflict".into(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "never-minted-or-created".into(),
                    index: 0,
                },
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1phantom".into(),
                amount: 1,
            }],
            fee: 99,
            nonce: 82,
        };
        append_linear_block(
            &mut state,
            "b2",
            vec![claim(&chain_id, "pulse1miner2", 82), phantom],
            2,
        );

        let error = rebuild_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("without authenticated replay provenance"),
            "{error}"
        );
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
