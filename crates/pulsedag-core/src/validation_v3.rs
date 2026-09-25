use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    monetary_v3::{max_coinbase_claim_atoms, MonetaryCadenceSegment, MonetaryV3Error},
    ordering_v2::derive_ordered_dag_v2,
    reward_settlement_v3::{validate_reward_claim_transaction_v3, RewardSettlementV3Error},
    state::ChainState,
    types::{Block, Hash},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedMonetaryRewardV3 {
    pub block_hash: Hash,
    pub monetary_score: u64,
    pub claim_txid: Hash,
    pub beneficiary: String,
    pub authorized_subsidy_atoms: u64,
    pub eligible_fees_atoms: u64,
    pub authorized_settlement_atoms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MonetaryValidationV3Error {
    #[error("ordered DAG derivation failed: {0}")]
    Ordering(String),
    #[error("block {0} is not present in the accepted state")]
    UnknownBlock(Hash),
    #[error("block {0} is not present in the canonical ordered DAG")]
    BlockNotOrdered(Hash),
    #[error("genesis does not carry a monetary reward claim")]
    GenesisRewardForbidden,
    #[error("block {block_hash} contains an additional inputless transaction {txid}")]
    HiddenIssuancePath { block_hash: Hash, txid: Hash },
    #[error("eligible fee arithmetic overflow")]
    FeeOverflow,
    #[error(transparent)]
    Reward(#[from] RewardSettlementV3Error),
    #[error(transparent)]
    Monetary(#[from] MonetaryV3Error),
}

pub(crate) fn validate_monetary_reward_at_canonical_score_v3(
    chain_id: &str,
    block: &Block,
    monetary_score: u64,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ValidatedMonetaryRewardV3, MonetaryValidationV3Error> {
    if monetary_score == 0 {
        return Err(MonetaryValidationV3Error::GenesisRewardForbidden);
    }

    let claim =
        block
            .transactions
            .first()
            .ok_or_else(|| RewardSettlementV3Error::MissingRewardClaim {
                block_hash: block.hash.clone(),
            })?;
    validate_reward_claim_transaction_v3(claim, chain_id)?;

    let mut eligible_fees_atoms = 0u64;
    for transaction in block.transactions.iter().skip(1) {
        if transaction.inputs.is_empty() {
            return Err(MonetaryValidationV3Error::HiddenIssuancePath {
                block_hash: block.hash.clone(),
                txid: transaction.txid.clone(),
            });
        }
        eligible_fees_atoms = eligible_fees_atoms
            .checked_add(transaction.fee)
            .ok_or(MonetaryValidationV3Error::FeeOverflow)?;
    }

    let authorized_settlement_atoms =
        max_coinbase_claim_atoms(monetary_score, eligible_fees_atoms, cadence_segments)?;
    let authorized_subsidy_atoms = authorized_settlement_atoms
        .checked_sub(eligible_fees_atoms)
        .ok_or(MonetaryValidationV3Error::FeeOverflow)?;

    Ok(ValidatedMonetaryRewardV3 {
        block_hash: block.hash.clone(),
        monetary_score,
        claim_txid: claim.txid.clone(),
        beneficiary: claim.outputs[0].address.clone(),
        authorized_subsidy_atoms,
        eligible_fees_atoms,
        authorized_settlement_atoms,
    })
}

/// Validate one accepted canonical v3 reward directly against state-derived
/// monetary position.
///
/// The caller supplies only the accepted block hash and frozen cadence table.
/// Monetary score is derived from the authoritative ordered DAG and can never
/// be supplied by a miner, RPC caller or block header. The block's first
/// transaction is an amountless reward claim; subsidy is derived from the
/// frozen policy and fees are transfers, not issuance.
pub fn validate_ordered_monetary_reward_v3(
    state: &ChainState,
    block_hash: &str,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ValidatedMonetaryRewardV3, MonetaryValidationV3Error> {
    let block = state
        .dag
        .blocks
        .get(block_hash)
        .ok_or_else(|| MonetaryValidationV3Error::UnknownBlock(block_hash.to_string()))?;
    let ordered = derive_ordered_dag_v2(state)
        .map_err(|error| MonetaryValidationV3Error::Ordering(format!("{error:?}")))?;
    let position = ordered
        .blocks
        .iter()
        .position(|hash| hash == block_hash)
        .ok_or_else(|| MonetaryValidationV3Error::BlockNotOrdered(block_hash.to_string()))?;
    let monetary_score =
        u64::try_from(position).map_err(|_| MonetaryValidationV3Error::FeeOverflow)?;

    validate_monetary_reward_at_canonical_score_v3(
        &state.chain_id,
        block,
        monetary_score,
        cadence_segments,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput},
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn accepted_state_with_reward(extra: Option<Transaction>) -> ChainState {
        let mut state = init_chain_state("monetary-validation-v3".into());
        let genesis = state.dag.genesis_hash.clone();
        let claim = build_reward_claim_transaction_v3("pulse1miner", 7, &state.chain_id).unwrap();
        let mut transactions = vec![claim];
        if let Some(extra) = extra {
            transactions.push(extra);
        }
        let block = Block {
            hash: "reward-block".into(),
            header: BlockHeader {
                version: 2,
                parents: vec![genesis.clone()],
                timestamp: 1,
                difficulty: 1,
                nonce: 0,
                merkle_root: "m".into(),
                state_root: "s".into(),
                blue_score: 1,
                height: 1,
            },
            transactions,
        };
        state.dag.blocks.insert(block.hash.clone(), block);
        state.dag.blue_work.insert("reward-block".into(), 10);
        state
            .dag
            .selected_parents
            .insert("reward-block".into(), Some(genesis.clone()));
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis.clone(), vec![]);
        state
            .dag
            .merge_set_blues
            .insert("reward-block".into(), vec![]);
        state
            .dag
            .merge_set_reds
            .insert("reward-block".into(), vec![]);
        state.dag.selected_chain = vec![genesis, "reward-block".into()];
        state
    }

    #[test]
    fn monetary_score_is_derived_and_claim_amount_is_not_issuance_authority() {
        let state = accepted_state_with_reward(None);
        let validated =
            validate_ordered_monetary_reward_v3(&state, "reward-block", &ONE_SECOND).unwrap();

        assert_eq!(validated.monetary_score, 1);
        assert_eq!(validated.eligible_fees_atoms, 0);
        assert_eq!(
            validated.authorized_subsidy_atoms,
            crate::subsidy_atoms_for_score(1, &ONE_SECOND).unwrap()
        );
        assert_eq!(
            state.dag.blocks["reward-block"].transactions[0].outputs[0].amount,
            0
        );
    }

    #[test]
    fn fees_are_transfers_and_add_to_settlement_without_changing_subsidy() {
        let fee_tx = Transaction {
            txid: "fee-tx".into(),
            version: 2,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "source".into(),
                    index: 0,
                },
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: 1,
            }],
            fee: 7,
            nonce: 1,
        };
        let state = accepted_state_with_reward(Some(fee_tx));
        let validated =
            validate_ordered_monetary_reward_v3(&state, "reward-block", &ONE_SECOND).unwrap();

        assert_eq!(validated.eligible_fees_atoms, 7);
        assert_eq!(
            validated.authorized_settlement_atoms,
            validated.authorized_subsidy_atoms + 7
        );
    }

    #[test]
    fn a_second_inputless_transaction_is_rejected_as_hidden_issuance() {
        let hidden = Transaction {
            txid: "hidden".into(),
            version: 2,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "pulse1hidden".into(),
                amount: 1,
            }],
            fee: 1,
            nonce: 9,
        };
        let state = accepted_state_with_reward(Some(hidden));
        assert!(matches!(
            validate_ordered_monetary_reward_v3(&state, "reward-block", &ONE_SECOND),
            Err(MonetaryValidationV3Error::HiddenIssuancePath { .. })
        ));
    }
}
