use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    mining_protocol::derive_activated_v2_mining_parent_context,
    monetary_v3::{
        monetary_cadence_fingerprint_v3, monetary_policy_fingerprint_v3, MonetaryCadenceSegment,
    },
    protocol::ProtocolActivationIdentity,
    retarget::expected_difficulty_for_parent,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    state::ChainState,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, Hash, Transaction},
};

pub const MONETARY_MINING_TEMPLATE_SCHEMA_V3: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonetaryMiningTemplateV3 {
    pub schema_version: u32,
    pub protocol_fingerprint: String,
    pub monetary_policy_fingerprint: String,
    pub monetary_cadence_fingerprint: String,
    pub parents: Vec<Hash>,
    pub selected_parent: Hash,
    pub timestamp: u64,
    pub height: u64,
    pub blue_score: u64,
    pub difficulty: u32,
    pub merkle_root: String,
    pub transactions: Vec<Transaction>,
    pub reward_claim_txid: Hash,
    pub eligible_fees_atoms: u64,
    pub reward_settlement_deferred: bool,
    pub ready_for_nonce_search: bool,
}

fn invalid_template(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("monetary-v3 mining template: {}", message.into()))
}

/// Build the monetary portion of a v3 mining template without using block
/// height as issuance authority.
///
/// The first transaction is an amountless reward claim. No subsidy amount is
/// embedded in the block because the canonical monetary score can move until
/// DAG order is final. The actual reward is derived during ordered-DAG
/// settlement from the frozen monetary policy plus eligible fees.
///
/// This function deliberately returns a pre-state template with
/// `ready_for_nonce_search=false`. A future/live v3 state finalizer must bind
/// the authoritative state root before nonce search. Calling this helper alone
/// never activates v3 consensus.
pub fn build_monetary_mining_template_v3(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
    beneficiary: &str,
    claim_nonce: u64,
    timestamp: u64,
    ordinary_transactions: Vec<Transaction>,
) -> Result<MonetaryMiningTemplateV3, PulseError> {
    let parent_context = derive_activated_v2_mining_parent_context(state, identity)?;
    let protocol_fingerprint = identity
        .fingerprint()
        .map_err(|error| invalid_template(format!("protocol identity: {error}")))?;
    let monetary_cadence_fingerprint = monetary_cadence_fingerprint_v3(cadence_segments)
        .map_err(|error| invalid_template(error.to_string()))?;

    let height = parent_context
        .parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .map(|block| block.header.height.saturating_add(1))
                .ok_or_else(|| invalid_template(format!("missing parent {parent}")))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or_else(|| invalid_template("candidate parent set is empty"))?;

    let newest_parent_timestamp = parent_context
        .parents
        .iter()
        .filter_map(|parent| state.dag.blocks.get(parent))
        .map(|block| block.header.timestamp)
        .max()
        .unwrap_or(0);
    if timestamp == 0 || timestamp < newest_parent_timestamp {
        return Err(invalid_template(format!(
            "timestamp {timestamp} is older than newest parent {newest_parent_timestamp}"
        )));
    }

    let difficulty = expected_difficulty_for_parent(state, &parent_context.selected_parent)
        .ok_or_else(|| {
            invalid_template(format!(
                "difficulty unavailable for selected parent {}",
                parent_context.selected_parent
            ))
        })?;

    let mut eligible_fees_atoms = 0u64;
    for transaction in &ordinary_transactions {
        if transaction.inputs.is_empty() {
            return Err(invalid_template(format!(
                "ordinary transaction {} is inputless and would create a hidden issuance path",
                transaction.txid
            )));
        }
        if transaction.version != TRANSACTION_VERSION_V2 {
            return Err(invalid_template(format!(
                "ordinary transaction {} uses version {}, expected {}",
                transaction.txid, transaction.version, TRANSACTION_VERSION_V2
            )));
        }
        let expected_txid = compute_txid_v2(transaction, &identity.chain_id)?;
        if expected_txid != transaction.txid {
            return Err(invalid_template(format!(
                "ordinary transaction txid mismatch: supplied {}, computed {}",
                transaction.txid, expected_txid
            )));
        }
        eligible_fees_atoms = eligible_fees_atoms
            .checked_add(transaction.fee)
            .ok_or_else(|| invalid_template("eligible fee arithmetic overflow"))?;
    }

    let reward_claim =
        build_reward_claim_transaction_v3(beneficiary, claim_nonce, &identity.chain_id)
            .map_err(|error| invalid_template(error.to_string()))?;
    debug_assert_eq!(reward_claim.outputs[0].amount, 0);

    let reward_claim_txid = reward_claim.txid.clone();
    let mut transactions = Vec::with_capacity(ordinary_transactions.len().saturating_add(1));
    transactions.push(reward_claim);
    transactions.extend(ordinary_transactions);
    let merkle_root = compute_merkle_root(&transactions);

    Ok(MonetaryMiningTemplateV3 {
        schema_version: MONETARY_MINING_TEMPLATE_SCHEMA_V3,
        protocol_fingerprint,
        monetary_policy_fingerprint: monetary_policy_fingerprint_v3(),
        monetary_cadence_fingerprint,
        parents: parent_context.parents,
        selected_parent: parent_context.selected_parent,
        timestamp,
        height,
        blue_score: parent_context.blue_score,
        difficulty,
        merkle_root,
        transactions,
        reward_claim_txid,
        eligible_fees_atoms,
        reward_settlement_deferred: true,
        ready_for_nonce_search: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        types::{OutPoint, TxInput, TxOutput},
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    #[test]
    fn template_reward_is_amountless_and_never_height_subsidy_authority() {
        let state = init_chain_state("monetary-mining-v3".into());
        let parent_ts = state.dag.blocks[&state.dag.genesis_hash].header.timestamp;
        let template = build_monetary_mining_template_v3(
            &state,
            &identity(&state),
            &ONE_SECOND,
            "pulse1miner",
            7,
            parent_ts.saturating_add(1),
            vec![],
        )
        .unwrap();

        assert_eq!(template.transactions[0].outputs[0].amount, 0);
        assert_eq!(template.eligible_fees_atoms, 0);
        assert!(template.reward_settlement_deferred);
        assert!(!template.ready_for_nonce_search);
        assert_eq!(
            template.monetary_policy_fingerprint,
            crate::MONETARY_POLICY_FINGERPRINT_V3
        );
    }

    #[test]
    fn template_carries_fees_but_does_not_embed_them_into_claim_amount() {
        let state = init_chain_state("monetary-mining-v3-fees".into());
        let parent_ts = state.dag.blocks[&state.dag.genesis_hash].header.timestamp;
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "11".repeat(32),
                    index: 0,
                },
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: 1,
            }],
            fee: 9,
            nonce: 3,
        };
        tx.txid = compute_txid_v2(&tx, &state.chain_id).unwrap();

        let template = build_monetary_mining_template_v3(
            &state,
            &identity(&state),
            &ONE_SECOND,
            "pulse1miner",
            8,
            parent_ts.saturating_add(1),
            vec![tx],
        )
        .unwrap();

        assert_eq!(template.eligible_fees_atoms, 9);
        assert_eq!(template.transactions[0].outputs[0].amount, 0);
    }

    #[test]
    fn inputless_extra_transaction_is_rejected() {
        let state = init_chain_state("monetary-mining-v3-hidden".into());
        let parent_ts = state.dag.blocks[&state.dag.genesis_hash].header.timestamp;
        let hidden = Transaction {
            txid: "hidden".into(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "pulse1hidden".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 1,
        };

        assert!(
            build_monetary_mining_template_v3(
                &state,
                &identity(&state),
                &ONE_SECOND,
                "pulse1miner",
                9,
                parent_ts.saturating_add(1),
                vec![hidden],
            )
            .unwrap_err()
            .to_string()
            .contains("hidden issuance")
        );
    }
}
