use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    mining_protocol::derive_activated_v2_mining_parent_context,
    mining_state_v2::{
        finalize_activated_v2_mining_candidate_state, ActivatedV2MiningStateContext,
    },
    monetary_v3::{
        monetary_cadence_fingerprint_v3, monetary_policy_fingerprint_v3, MonetaryCadenceSegment,
    },
    protocol::{ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2},
    retarget::expected_difficulty_for_parent,
    reward_settlement_v3::{
        build_reward_claim_transaction_v3, validate_reward_claim_transaction_v3,
    },
    state::ChainState,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, Block, BlockHeader, Hash, Transaction},
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedMonetaryMiningCandidateV3 {
    pub block: Block,
    pub state: ActivatedV2MiningStateContext,
    pub protocol_fingerprint: String,
    pub monetary_policy_fingerprint: String,
    pub monetary_cadence_fingerprint: String,
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

/// Bind an amountless v3 monetary template to the authoritative state root and
/// block hash before nonce search.
///
/// Issuance is still not embedded in the candidate: the first transaction is a
/// zero-amount reward claim and settlement remains deferred until canonical DAG
/// order is accepted. This function only closes the mining-template/state-root
/// gap and deliberately reuses the activated-v2 header/state replay machinery.
///
/// The supplied fingerprints and cadence are re-derived and checked so a caller
/// cannot finalize a template under a different monetary or protocol identity.
pub fn finalize_monetary_mining_template_v3(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
    template: &MonetaryMiningTemplateV3,
) -> Result<FinalizedMonetaryMiningCandidateV3, PulseError> {
    if template.schema_version != MONETARY_MINING_TEMPLATE_SCHEMA_V3 {
        return Err(invalid_template(format!(
            "unsupported schema version {}, expected {}",
            template.schema_version, MONETARY_MINING_TEMPLATE_SCHEMA_V3
        )));
    }
    if template.ready_for_nonce_search {
        return Err(invalid_template(
            "pre-state template must not already be marked ready for nonce search",
        ));
    }
    if !template.reward_settlement_deferred {
        return Err(invalid_template(
            "reward settlement must remain deferred until canonical ordering",
        ));
    }

    let expected_protocol_fingerprint = identity
        .fingerprint()
        .map_err(|error| invalid_template(format!("protocol identity: {error}")))?;
    if template.protocol_fingerprint != expected_protocol_fingerprint {
        return Err(invalid_template("protocol fingerprint mismatch"));
    }

    let expected_policy_fingerprint = monetary_policy_fingerprint_v3();
    if template.monetary_policy_fingerprint != expected_policy_fingerprint {
        return Err(invalid_template("monetary policy fingerprint mismatch"));
    }

    let expected_cadence_fingerprint = monetary_cadence_fingerprint_v3(cadence_segments)
        .map_err(|error| invalid_template(error.to_string()))?;
    if template.monetary_cadence_fingerprint != expected_cadence_fingerprint {
        return Err(invalid_template("monetary cadence fingerprint mismatch"));
    }

    let claim = template
        .transactions
        .first()
        .ok_or_else(|| invalid_template("template has no reward claim"))?;
    validate_reward_claim_transaction_v3(claim, &identity.chain_id)
        .map_err(|error| invalid_template(error.to_string()))?;
    if claim.txid != template.reward_claim_txid {
        return Err(invalid_template("reward claim txid mismatch"));
    }
    if claim.outputs.len() != 1 || claim.outputs[0].amount != 0 {
        return Err(invalid_template(
            "reward claim must remain amountless before ordered settlement",
        ));
    }
    if template
        .transactions
        .iter()
        .skip(1)
        .any(|tx| tx.inputs.is_empty())
    {
        return Err(invalid_template(
            "template contains an additional inputless transaction",
        ));
    }

    let merkle_root = compute_merkle_root(&template.transactions);
    if merkle_root != template.merkle_root {
        return Err(invalid_template("template merkle root mismatch"));
    }

    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: template.parents.clone(),
            timestamp: template.timestamp,
            difficulty: template.difficulty,
            nonce: 0,
            merkle_root,
            state_root: "00".repeat(32),
            blue_score: template.blue_score,
            height: template.height,
        },
        transactions: template.transactions.clone(),
    };

    let finalized_state =
        finalize_activated_v2_mining_candidate_state(&mut block, state, identity)?;

    Ok(FinalizedMonetaryMiningCandidateV3 {
        block,
        state: finalized_state,
        protocol_fingerprint: expected_protocol_fingerprint,
        monetary_policy_fingerprint: expected_policy_fingerprint,
        monetary_cadence_fingerprint: expected_cadence_fingerprint,
        reward_settlement_deferred: true,
        ready_for_nonce_search: true,
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
    fn finalizer_binds_state_root_without_embedding_issuance() {
        let state = init_chain_state("monetary-mining-v3-finalize".into());
        let identity = identity(&state);
        let parent_ts = state.dag.blocks[&state.dag.genesis_hash].header.timestamp;
        let template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1miner",
            10,
            parent_ts.saturating_add(1),
            vec![],
        )
        .unwrap();

        let finalized =
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &template)
                .unwrap();

        assert!(finalized.ready_for_nonce_search);
        assert!(finalized.reward_settlement_deferred);
        assert_ne!(finalized.block.header.state_root, "00".repeat(32));
        assert_eq!(finalized.block.transactions[0].outputs[0].amount, 0);
        assert_eq!(finalized.state.block_hash, finalized.block.hash);
        assert_eq!(
            finalized.monetary_policy_fingerprint,
            crate::MONETARY_POLICY_FINGERPRINT_V3
        );
    }

    #[test]
    fn finalizer_rejects_identity_or_cadence_substitution() {
        let state = init_chain_state("monetary-mining-v3-bindings".into());
        let identity = identity(&state);
        let parent_ts = state.dag.blocks[&state.dag.genesis_hash].header.timestamp;
        let template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1miner",
            11,
            parent_ts.saturating_add(1),
            vec![],
        )
        .unwrap();

        let alternate = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 2_000_000_000,
        }];
        assert!(
            finalize_monetary_mining_template_v3(&state, &identity, &alternate, &template)
                .unwrap_err()
                .to_string()
                .contains("cadence fingerprint")
        );

        let mut tampered = template.clone();
        tampered.monetary_policy_fingerprint = "00".repeat(32);
        assert!(
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &tampered)
                .unwrap_err()
                .to_string()
                .contains("policy fingerprint")
        );
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

        assert!(build_monetary_mining_template_v3(
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
        .contains("hidden issuance"));
    }
}
