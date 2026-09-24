use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    monetary_v3::{
        monetary_cadence_fingerprint_v3, monetary_policy_fingerprint_v3,
        total_supply_atoms_for_score, MonetaryCadenceSegment, MonetaryV3Error,
    },
    ordering_v2::{derive_ordered_dag_v2, OrderingV2Error},
    state::ChainState,
    validation_v3::{
        validate_monetary_reward_at_canonical_score_v3, MonetaryValidationV3Error,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonetaryStateAuditV3 {
    pub monetary_policy_fingerprint: String,
    pub monetary_cadence_fingerprint: String,
    pub ordered_dag_digest: String,
    pub current_monetary_score: u64,
    pub reward_claim_blocks: u64,
    pub scheduled_supply_atoms: u64,
    pub authorized_subsidy_atoms: u64,
    pub eligible_fee_transfers_atoms: u64,
    pub genesis_issuance_atoms: u64,
    pub hidden_issuance_paths: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MonetaryStateAuditV3Error {
    #[error("ordered DAG derivation failed: {0:?}")]
    Ordering(OrderingV2Error),
    #[error("ordered DAG is empty")]
    EmptyOrderedDag,
    #[error("ordered DAG does not start at the configured genesis")]
    GenesisNotFirst,
    #[error("v3 genesis contains spendable transaction allocations")]
    GenesisAllocationPresent,
    #[error("accepted block {0} is missing from state")]
    MissingBlock(String),
    #[error("monetary score exceeds u64")]
    ScoreOverflow,
    #[error("audit arithmetic overflow")]
    ArithmeticOverflow,
    #[error(
        "authorized subsidy {observed} does not match scheduled supply {expected} at score {score}"
    )]
    SupplyMismatch {
        score: u64,
        expected: u64,
        observed: u64,
    },
    #[error(transparent)]
    Validation(#[from] MonetaryValidationV3Error),
    #[error(transparent)]
    Monetary(#[from] MonetaryV3Error),
}

impl From<OrderingV2Error> for MonetaryStateAuditV3Error {
    fn from(error: OrderingV2Error) -> Self {
        Self::Ordering(error)
    }
}

/// Audit the complete accepted v3 monetary surface in one ordered-DAG pass.
///
/// This is intentionally linear after the canonical ordering is derived once.
/// Every non-genesis canonical position must contain exactly one amountless
/// reward claim, no later transaction may be inputless, and the sum of all
/// state-derived subsidies must equal the exact cumulative issuance curve.
pub fn audit_monetary_state_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<MonetaryStateAuditV3, MonetaryStateAuditV3Error> {
    let ordered = derive_ordered_dag_v2(state)?;
    let Some(first) = ordered.blocks.first() else {
        return Err(MonetaryStateAuditV3Error::EmptyOrderedDag);
    };
    if first != &state.dag.genesis_hash {
        return Err(MonetaryStateAuditV3Error::GenesisNotFirst);
    }
    let genesis = state
        .dag
        .blocks
        .get(first)
        .ok_or_else(|| MonetaryStateAuditV3Error::MissingBlock(first.clone()))?;
    if !genesis.transactions.is_empty() {
        return Err(MonetaryStateAuditV3Error::GenesisAllocationPresent);
    }

    let mut authorized_subsidy_atoms = 0u64;
    let mut eligible_fee_transfers_atoms = 0u64;
    for (position, block_hash) in ordered.blocks.iter().enumerate().skip(1) {
        let score =
            u64::try_from(position).map_err(|_| MonetaryStateAuditV3Error::ScoreOverflow)?;
        let block = state
            .dag
            .blocks
            .get(block_hash)
            .ok_or_else(|| MonetaryStateAuditV3Error::MissingBlock(block_hash.clone()))?;
        let validated = validate_monetary_reward_at_canonical_score_v3(
            &state.chain_id,
            block,
            score,
            cadence_segments,
        )?;
        authorized_subsidy_atoms = authorized_subsidy_atoms
            .checked_add(validated.authorized_subsidy_atoms)
            .ok_or(MonetaryStateAuditV3Error::ArithmeticOverflow)?;
        eligible_fee_transfers_atoms = eligible_fee_transfers_atoms
            .checked_add(validated.eligible_fees_atoms)
            .ok_or(MonetaryStateAuditV3Error::ArithmeticOverflow)?;
    }

    let current_monetary_score = u64::try_from(ordered.blocks.len().saturating_sub(1))
        .map_err(|_| MonetaryStateAuditV3Error::ScoreOverflow)?;
    let scheduled_supply_atoms =
        total_supply_atoms_for_score(current_monetary_score, cadence_segments)?;
    if authorized_subsidy_atoms != scheduled_supply_atoms {
        return Err(MonetaryStateAuditV3Error::SupplyMismatch {
            score: current_monetary_score,
            expected: scheduled_supply_atoms,
            observed: authorized_subsidy_atoms,
        });
    }

    Ok(MonetaryStateAuditV3 {
        monetary_policy_fingerprint: monetary_policy_fingerprint_v3(),
        monetary_cadence_fingerprint: monetary_cadence_fingerprint_v3(cadence_segments)?,
        ordered_dag_digest: ordered.digest,
        current_monetary_score,
        reward_claim_blocks: current_monetary_score,
        scheduled_supply_atoms,
        authorized_subsidy_atoms,
        eligible_fee_transfers_atoms,
        genesis_issuance_atoms: 0,
        hidden_issuance_paths: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::init_chain_state_v3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::{Block, BlockHeader},
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn linear_reward_state() -> ChainState {
        let mut state =
            init_chain_state_v3("monetary-audit-v3".into(), 1_800_000_000).unwrap();
        let genesis = state.dag.genesis_hash.clone();
        let mut previous = genesis.clone();

        for score in 1..=3u64 {
            let hash = format!("reward-{score}");
            let claim = build_reward_claim_transaction_v3(
                &format!("pulse1miner{score}"),
                score,
                &state.chain_id,
            )
            .unwrap();
            let block = Block {
                hash: hash.clone(),
                header: BlockHeader {
                    version: 2,
                    parents: vec![previous.clone()],
                    timestamp: 1_800_000_000 + score,
                    difficulty: 1,
                    nonce: 0,
                    merkle_root: format!("m-{score}"),
                    state_root: format!("s-{score}"),
                    blue_score: score,
                    height: score,
                },
                transactions: vec![claim],
            };
            state.dag.blocks.insert(hash.clone(), block);
            state.dag.blue_work.insert(hash.clone(), u128::from(score));
            state
                .dag
                .selected_parents
                .insert(hash.clone(), Some(previous.clone()));
            state.dag.merge_set_blues.insert(hash.clone(), vec![]);
            state.dag.merge_set_reds.insert(hash.clone(), vec![]);
            previous = hash;
        }
        state.dag.selected_chain = vec![
            genesis,
            "reward-1".into(),
            "reward-2".into(),
            "reward-3".into(),
        ];
        state
    }

    #[test]
    fn arbitrary_accepted_state_has_exact_scheduled_supply() {
        let state = linear_reward_state();
        let audit = audit_monetary_state_v3(&state, &ONE_SECOND).unwrap();
        assert_eq!(audit.current_monetary_score, 3);
        assert_eq!(audit.reward_claim_blocks, 3);
        assert_eq!(audit.hidden_issuance_paths, 0);
        assert_eq!(audit.authorized_subsidy_atoms, audit.scheduled_supply_atoms);
        assert_eq!(
            audit.scheduled_supply_atoms,
            total_supply_atoms_for_score(3, &ONE_SECOND).unwrap()
        );
    }

    #[test]
    fn genesis_allocation_is_rejected() {
        let mut state = linear_reward_state();
        let genesis = state.dag.genesis_hash.clone();
        state.dag.blocks.get_mut(&genesis).unwrap().transactions.push(
            build_reward_claim_transaction_v3("pulse1forbidden", 99, &state.chain_id).unwrap(),
        );
        assert!(matches!(
            audit_monetary_state_v3(&state, &ONE_SECOND),
            Err(MonetaryStateAuditV3Error::GenesisAllocationPresent)
        ));
    }
}
