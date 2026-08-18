use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    mining::is_coinbase,
    mining_protocol::{derive_activated_v2_mining_parent_context, ActivatedV2MiningParentContext},
    mining_state_v2::{
        finalize_activated_v2_mining_candidate_state, ActivatedV2MiningStateContext,
    },
    mining_v2::{build_candidate_block_v2, build_coinbase_transaction_v2, CandidateBlockV2Spec},
    pow_v2::canonical_pow_v2_adapter,
    protocol::ProtocolActivationIdentity,
    retarget::expected_difficulty_for_parent,
    state::ChainState,
    types::{Block, Transaction},
    validation::block_subsidy,
};

pub const ACTIVATED_V2_MINING_TEMPLATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct ActivatedV2MiningTemplateSpec {
    pub miner_address: String,
    pub timestamp: u64,
    pub coinbase_nonce: u64,
    /// Protocol-bound v2 mempool transactions. The coinbase is constructed by
    /// this builder and callers must not include one here.
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivatedV2MiningTemplate {
    pub schema_version: u32,
    pub protocol_identity: ProtocolActivationIdentity,
    pub protocol_identity_fingerprint: String,
    pub parent_context: ActivatedV2MiningParentContext,
    pub state_context: ActivatedV2MiningStateContext,
    pub block: Block,
    pub pow_algorithm: String,
    pub pow_engine: String,
    pub pre_pow_bytes_hex: String,
    pub target_bits: u32,
    pub target_hex: String,
    pub target_u64: u64,
}

fn invalid_template(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("activated-v2 mining template: {}", message.into()))
}

fn candidate_height(
    state: &ChainState,
    parent_context: &ActivatedV2MiningParentContext,
) -> Result<u64, PulseError> {
    parent_context
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
        .ok_or_else(|| invalid_template("candidate parent set is empty"))
}

fn newest_parent_timestamp(
    state: &ChainState,
    parent_context: &ActivatedV2MiningParentContext,
) -> Result<u64, PulseError> {
    parent_context
        .parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .map(|block| block.header.timestamp)
                .ok_or_else(|| invalid_template(format!("missing parent {parent}")))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or_else(|| invalid_template("candidate parent set is empty"))
}

fn candidate_reward(height: u64, transactions: &[Transaction]) -> Result<u64, PulseError> {
    let fees = transactions.iter().try_fold(0_u64, |acc, transaction| {
        if is_coinbase(transaction) {
            return Err(invalid_template(
                "caller transactions must not contain a coinbase",
            ));
        }
        acc.checked_add(transaction.fee)
            .ok_or(PulseError::RewardOverflow)
    })?;
    block_subsidy(height)
        .checked_add(fees)
        .ok_or(PulseError::RewardOverflow)
}

/// Construct the complete, chain-bound work envelope required by a future
/// activated-v2 mining RPC and standalone miner.
///
/// This helper is deliberately non-live. It derives the frozen selected-tip /
/// parallel-parent context, constructs the v2 coinbase and candidate, finalizes
/// the authoritative ordered-DAG state root, and exposes the exact canonical
/// nonce-independent PoW bytes plus target. It does not search PoW, persist or
/// broadcast the block, mutate chain state, switch `/mining/template`, or alter
/// the historical v1 standalone miner path.
pub fn build_activated_v2_mining_template(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    spec: ActivatedV2MiningTemplateSpec,
) -> Result<ActivatedV2MiningTemplate, PulseError> {
    if spec.miner_address.trim().is_empty() {
        return Err(invalid_template("miner address must not be empty"));
    }

    let parent_context = derive_activated_v2_mining_parent_context(state, identity)?;
    let height = candidate_height(state, &parent_context)?;
    let newest_parent_timestamp = newest_parent_timestamp(state, &parent_context)?;
    if spec.timestamp == 0 || spec.timestamp < newest_parent_timestamp {
        return Err(invalid_template(format!(
            "candidate timestamp {} is older than newest parent {}",
            spec.timestamp, newest_parent_timestamp
        )));
    }

    let difficulty = expected_difficulty_for_parent(state, &parent_context.selected_parent)
        .ok_or_else(|| {
            invalid_template(format!(
                "difficulty context unavailable for selected parent {}",
                parent_context.selected_parent
            ))
        })?;
    let reward = candidate_reward(height, &spec.transactions)?;
    let coinbase = build_coinbase_transaction_v2(
        &spec.miner_address,
        reward,
        spec.coinbase_nonce,
        &identity.chain_id,
    )?;
    let mut transactions = Vec::with_capacity(spec.transactions.len().saturating_add(1));
    transactions.push(coinbase);
    transactions.extend(spec.transactions);

    let mut block = build_candidate_block_v2(
        CandidateBlockV2Spec {
            parents: parent_context.parents.clone(),
            timestamp: spec.timestamp,
            height,
            blue_score: parent_context.blue_score,
            difficulty,
            state_root: "00".repeat(32),
        },
        transactions,
        &identity.chain_id,
    )?;
    let state_context = finalize_activated_v2_mining_candidate_state(&mut block, state, identity)?;

    let adapter = canonical_pow_v2_adapter();
    let material = adapter.pre_pow_material(&block.header, &identity.chain_id)?;
    if material.target.bits != block.header.difficulty {
        return Err(invalid_template(format!(
            "PoW target bits {} do not match candidate difficulty {}",
            material.target.bits, block.header.difficulty
        )));
    }
    let protocol_identity_fingerprint = identity
        .fingerprint()
        .map_err(|error| invalid_template(format!("protocol identity fingerprint: {error}")))?;

    Ok(ActivatedV2MiningTemplate {
        schema_version: ACTIVATED_V2_MINING_TEMPLATE_SCHEMA_VERSION,
        protocol_identity: identity.clone(),
        protocol_identity_fingerprint,
        parent_context,
        state_context,
        block,
        pow_algorithm: adapter.algorithm_name().to_string(),
        pow_engine: adapter.engine_name().to_string(),
        pre_pow_bytes_hex: hex::encode(material.pre_pow_bytes),
        target_bits: material.target.bits,
        target_hex: material.target.target_hex,
        target_u64: material.target.target_u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining_v2::build_coinbase_transaction_v2,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        pow_v2::canonical_pow_v2_adapter,
        protocol::BLOCK_HEADER_VERSION_V2,
        tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    };

    const CHAIN_ID: &str = "task28-mining-template-envelope";

    fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn spec() -> ActivatedV2MiningTemplateSpec {
        ActivatedV2MiningTemplateSpec {
            miner_address: "pulse1task28miner".to_string(),
            timestamp: 1,
            coinbase_nonce: 7,
            transactions: Vec::new(),
        }
    }

    #[test]
    fn envelope_binds_protocol_parent_state_and_pow_material() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let template = build_activated_v2_mining_template(&state, &identity, spec()).unwrap();

        assert_eq!(
            template.schema_version,
            ACTIVATED_V2_MINING_TEMPLATE_SCHEMA_VERSION
        );
        assert_eq!(template.protocol_identity, identity);
        assert_eq!(
            template.protocol_identity_fingerprint,
            template.protocol_identity.fingerprint().unwrap()
        );
        assert_eq!(template.block.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(
            template.block.header.parents,
            template.parent_context.parents
        );
        assert_eq!(
            template.block.header.blue_score,
            template.parent_context.blue_score
        );
        assert_eq!(
            template.block.header.state_root,
            template.state_context.state_root
        );
        assert_eq!(template.block.hash, template.state_context.block_hash);
        assert_eq!(
            template.block.transactions[0].version,
            TRANSACTION_VERSION_V2
        );
        assert_eq!(
            template.block.transactions[0].txid,
            compute_txid_v2(&template.block.transactions[0], CHAIN_ID).unwrap()
        );

        let material = canonical_pow_v2_adapter()
            .pre_pow_material(&template.block.header, CHAIN_ID)
            .unwrap();
        assert_eq!(
            template.pre_pow_bytes_hex,
            hex::encode(material.pre_pow_bytes)
        );
        assert_eq!(template.target_bits, material.target.bits);
        assert_eq!(template.target_hex, material.target.target_hex);
        assert_eq!(template.target_u64, material.target.target_u64);
    }

    #[test]
    fn same_state_and_spec_produce_identical_work_envelope() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let first = build_activated_v2_mining_template(&state, &identity, spec()).unwrap();
        let second = build_activated_v2_mining_template(&state, &identity, spec()).unwrap();

        assert_eq!(first.block.hash, second.block.hash);
        assert_eq!(
            first.block.header.state_root,
            second.block.header.state_root
        );
        assert_eq!(first.parent_context, second.parent_context);
        assert_eq!(first.state_context, second.state_context);
        assert_eq!(first.pre_pow_bytes_hex, second.pre_pow_bytes_hex);
        assert_eq!(first.target_hex, second.target_hex);
    }

    #[test]
    fn legacy_identity_cannot_build_v2_work_envelope() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert!(build_activated_v2_mining_template(&state, &legacy, spec()).is_err());
    }

    #[test]
    fn caller_coinbase_is_rejected_before_candidate_construction() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let mut candidate_spec = spec();
        candidate_spec
            .transactions
            .push(build_coinbase_transaction_v2("pulse1other", 1, 9, CHAIN_ID).unwrap());

        assert!(
            build_activated_v2_mining_template(&state, &identity, candidate_spec)
                .unwrap_err()
                .to_string()
                .contains("must not contain a coinbase")
        );
    }

    #[test]
    fn empty_miner_and_stale_timestamp_fail_closed() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);

        let mut empty_miner = spec();
        empty_miner.miner_address.clear();
        assert!(build_activated_v2_mining_template(&state, &identity, empty_miner).is_err());

        let mut stale = spec();
        stale.timestamp = 0;
        assert!(build_activated_v2_mining_template(&state, &identity, stale).is_err());
    }
}
