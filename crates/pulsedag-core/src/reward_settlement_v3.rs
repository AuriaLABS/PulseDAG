use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    monetary_v3::{
        economic_maturity_reached, subsidy_atoms_for_score, MonetaryCadenceSegment,
        MonetaryV3Error,
    },
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2, OrderingV2Error},
    state::ChainState,
    types::{Hash, OutPoint, Transaction, TxOutput, Utxo},
};

pub const REWARD_CLAIM_TRANSACTION_VERSION_V3: u32 = 3;
pub const REWARD_SETTLEMENT_SCHEMA_VERSION_V3: u32 = 1;
pub const REWARD_FINALITY_BINDING_SCHEMA_VERSION_V3: u32 = 1;

const REWARD_CLAIM_TX_DOMAIN_V3: &[u8] = b"PulseDAG:reward-claim-tx:v3";
const REWARD_SETTLEMENT_OUTPOINT_DOMAIN_V3: &[u8] = b"PulseDAG:reward-settlement-outpoint:v3";
const REWARD_FINALITY_PREFIX_DOMAIN_V3: &[u8] = b"PulseDAG:reward-finality-prefix:v3";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RewardSettlementV3Error {
    #[error("ordered DAG derivation failed: {0}")]
    Ordering(String),
    #[error("ordered DAG is empty")]
    EmptyOrderedDag,
    #[error("ordered DAG must start at genesis {expected}, observed {observed:?}")]
    GenesisNotFirst {
        expected: Hash,
        observed: Option<Hash>,
    },
    #[error("finality boundary score {score} exceeds current monetary score {current_score}")]
    FinalityBeyondTip { score: u64, current_score: u64 },
    #[error("finality boundary block mismatch at score {score}: expected {expected}, observed {observed}")]
    FinalityBlockMismatch {
        score: u64,
        expected: Hash,
        observed: Hash,
    },
    #[error("finality boundary ordered-prefix digest mismatch")]
    FinalityPrefixDigestMismatch,
    #[error("finality policy version must not be empty")]
    EmptyFinalityPolicyVersion,
    #[error("chain id must not be empty")]
    EmptyChainId,
    #[error("block {block_hash} is missing its v3 reward claim")]
    MissingRewardClaim { block_hash: Hash },
    #[error("block {block_hash} contains more than one reward claim")]
    MultipleRewardClaims { block_hash: Hash },
    #[error("invalid v3 reward claim: {0}")]
    InvalidRewardClaim(String),
    #[error("reward claim txid mismatch: expected {expected}, observed {observed}")]
    RewardClaimTxidMismatch { expected: Hash, observed: Hash },
    #[error("reward arithmetic overflow")]
    RewardOverflow,
    #[error(transparent)]
    Monetary(#[from] MonetaryV3Error),
}

impl From<OrderingV2Error> for RewardSettlementV3Error {
    fn from(error: OrderingV2Error) -> Self {
        Self::Ordering(format!("{error:?}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardClaimStatusV3 {
    /// The ordered position may still move. No spendable reward UTXO exists.
    Provisional,
    /// The ordered position is final but the 3,600-economic-second maturity has
    /// not elapsed yet. No spendable reward UTXO exists.
    FinalizedImmature,
    /// Both finality and economic maturity are satisfied. The deterministic
    /// settlement UTXO may be materialized in the authoritative state.
    Spendable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardFinalityBoundaryV3 {
    pub schema_version: u32,
    pub policy_version: String,
    pub finalized_through_score: u64,
    pub finalized_block_hash: Hash,
    pub ordered_prefix_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardClaimSettlementV3 {
    pub block_hash: Hash,
    pub block_height: u64,
    pub monetary_score: u64,
    pub claim_txid: Hash,
    pub beneficiary: String,
    pub settlement_outpoint: OutPoint,
    pub subsidy_atoms: u64,
    pub fees_atoms: u64,
    pub settlement_amount_atoms: u64,
    pub finality_protected: bool,
    pub economic_maturity_reached: bool,
    pub status: RewardClaimStatusV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardSettlementSnapshotV3 {
    pub schema_version: u32,
    pub ordering_version: String,
    pub ordered_dag_digest: String,
    pub ordered_dag_tip: Option<Hash>,
    pub current_monetary_score: u64,
    pub finality_boundary: Option<RewardFinalityBoundaryV3>,
    pub claims: Vec<RewardClaimSettlementV3>,
    pub total_authorized_subsidy_atoms: u64,
    pub total_pending_or_spendable_fees_atoms: u64,
    pub total_spendable_reward_atoms: u64,
}

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

fn validate_reward_claim_shape_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<(), RewardSettlementV3Error> {
    if chain_id.is_empty() {
        return Err(RewardSettlementV3Error::EmptyChainId);
    }
    if tx.version != REWARD_CLAIM_TRANSACTION_VERSION_V3 {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(format!(
            "transaction version must be {REWARD_CLAIM_TRANSACTION_VERSION_V3}, got {}",
            tx.version
        )));
    }
    if !tx.inputs.is_empty() {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(
            "reward claim must have zero inputs".to_string(),
        ));
    }
    if tx.outputs.len() != 1 {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(
            "reward claim must have exactly one beneficiary output".to_string(),
        ));
    }
    if tx.outputs[0].address.trim().is_empty() {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(
            "reward beneficiary must not be empty".to_string(),
        ));
    }
    if tx.outputs[0].amount != 0 {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(
            "reward claim output amount must be zero; consensus derives the settled amount after finality"
                .to_string(),
        ));
    }
    if tx.fee != 0 {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(
            "reward claim fee must be zero".to_string(),
        ));
    }
    Ok(())
}

/// Canonical, chain-bound txid for the v3 reward-claim transaction.
///
/// A reward claim commits only the beneficiary and nonce. The monetary amount
/// is deliberately absent: before finality the block's canonical monetary
/// position may move, so embedding a subsidy amount into the block Merkle root
/// would make deterministic re-settlement impossible.
pub fn compute_reward_claim_txid_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<Hash, RewardSettlementV3Error> {
    validate_reward_claim_shape_v3(tx, chain_id)?;

    let mut bytes = Vec::with_capacity(192);
    encode_len_prefixed_bytes(&mut bytes, REWARD_CLAIM_TX_DOMAIN_V3);
    encode_len_prefixed_str(&mut bytes, chain_id);
    bytes.extend_from_slice(&tx.version.to_le_bytes());
    encode_len_prefixed_str(&mut bytes, &tx.outputs[0].address);
    bytes.extend_from_slice(&tx.outputs[0].amount.to_le_bytes());
    bytes.extend_from_slice(&tx.fee.to_le_bytes());
    bytes.extend_from_slice(&tx.nonce.to_le_bytes());
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn build_reward_claim_transaction_v3(
    beneficiary: &str,
    nonce: u64,
    chain_id: &str,
) -> Result<Transaction, RewardSettlementV3Error> {
    let mut tx = Transaction {
        txid: String::new(),
        version: REWARD_CLAIM_TRANSACTION_VERSION_V3,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: beneficiary.to_string(),
            amount: 0,
        }],
        fee: 0,
        nonce,
    };
    tx.txid = compute_reward_claim_txid_v3(&tx, chain_id)?;
    Ok(tx)
}

pub fn validate_reward_claim_transaction_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<(), RewardSettlementV3Error> {
    let expected = compute_reward_claim_txid_v3(tx, chain_id)?;
    if tx.txid != expected {
        return Err(RewardSettlementV3Error::RewardClaimTxidMismatch {
            expected,
            observed: tx.txid.clone(),
        });
    }
    Ok(())
}

/// Deterministic synthetic outpoint for a settled reward.
///
/// It is intentionally distinct from the amountless claim transaction's own
/// output. The synthetic id binds chain, block and claim identity and is only
/// materialized once finality + maturity authorize the reward.
pub fn settlement_outpoint_v3(chain_id: &str, block_hash: &str, claim_txid: &str) -> OutPoint {
    let mut bytes = Vec::with_capacity(224);
    encode_len_prefixed_bytes(&mut bytes, REWARD_SETTLEMENT_OUTPOINT_DOMAIN_V3);
    encode_len_prefixed_str(&mut bytes, chain_id);
    encode_len_prefixed_str(&mut bytes, block_hash);
    encode_len_prefixed_str(&mut bytes, claim_txid);
    OutPoint {
        txid: hex::encode(Sha256::digest(bytes)),
        index: 0,
    }
}

fn validate_ordered_genesis(
    state: &ChainState,
    ordered: &OrderedDagV2,
) -> Result<(), RewardSettlementV3Error> {
    let observed = ordered.blocks.first().cloned();
    if observed.as_ref() != Some(&state.dag.genesis_hash) {
        return Err(RewardSettlementV3Error::GenesisNotFirst {
            expected: state.dag.genesis_hash.clone(),
            observed,
        });
    }
    Ok(())
}

fn ordered_prefix_digest(blocks: &[Hash], through_score: u64) -> Result<String, RewardSettlementV3Error> {
    let end = usize::try_from(through_score)
        .map_err(|_| RewardSettlementV3Error::RewardOverflow)?
        .checked_add(1)
        .ok_or(RewardSettlementV3Error::RewardOverflow)?;
    if end > blocks.len() {
        return Err(RewardSettlementV3Error::FinalityBeyondTip {
            score: through_score,
            current_score: blocks.len().saturating_sub(1) as u64,
        });
    }

    let mut bytes = Vec::new();
    encode_len_prefixed_bytes(&mut bytes, REWARD_FINALITY_PREFIX_DOMAIN_V3);
    bytes.extend_from_slice(&through_score.to_le_bytes());
    bytes.extend_from_slice(&(end as u64).to_le_bytes());
    for hash in &blocks[..end] {
        encode_len_prefixed_str(&mut bytes, hash);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Bind an externally selected finality score to the exact authoritative DAG
/// prefix. This function does *not* decide finality; the production finality
/// engine must choose the score and policy version. The binding prevents a
/// stale/reordered prefix from being reused for monetary settlement.
pub fn bind_reward_finality_boundary_v3(
    state: &ChainState,
    finalized_through_score: u64,
    policy_version: &str,
) -> Result<RewardFinalityBoundaryV3, RewardSettlementV3Error> {
    if policy_version.trim().is_empty() {
        return Err(RewardSettlementV3Error::EmptyFinalityPolicyVersion);
    }
    let ordered = derive_ordered_dag_v2(state)?;
    validate_ordered_genesis(state, &ordered)?;
    let current_score = ordered.blocks.len().saturating_sub(1) as u64;
    if finalized_through_score > current_score {
        return Err(RewardSettlementV3Error::FinalityBeyondTip {
            score: finalized_through_score,
            current_score,
        });
    }
    let index = usize::try_from(finalized_through_score)
        .map_err(|_| RewardSettlementV3Error::RewardOverflow)?;
    let finalized_block_hash = ordered.blocks[index].clone();
    let ordered_prefix_digest = ordered_prefix_digest(&ordered.blocks, finalized_through_score)?;
    Ok(RewardFinalityBoundaryV3 {
        schema_version: REWARD_FINALITY_BINDING_SCHEMA_VERSION_V3,
        policy_version: policy_version.to_string(),
        finalized_through_score,
        finalized_block_hash,
        ordered_prefix_digest,
    })
}

pub fn validate_reward_finality_boundary_v3(
    state: &ChainState,
    boundary: &RewardFinalityBoundaryV3,
) -> Result<(), RewardSettlementV3Error> {
    if boundary.schema_version != REWARD_FINALITY_BINDING_SCHEMA_VERSION_V3 {
        return Err(RewardSettlementV3Error::InvalidRewardClaim(format!(
            "unsupported reward finality binding schema {}",
            boundary.schema_version
        )));
    }
    if boundary.policy_version.trim().is_empty() {
        return Err(RewardSettlementV3Error::EmptyFinalityPolicyVersion);
    }

    let ordered = derive_ordered_dag_v2(state)?;
    validate_ordered_genesis(state, &ordered)?;
    let current_score = ordered.blocks.len().saturating_sub(1) as u64;
    if boundary.finalized_through_score > current_score {
        return Err(RewardSettlementV3Error::FinalityBeyondTip {
            score: boundary.finalized_through_score,
            current_score,
        });
    }
    let index = usize::try_from(boundary.finalized_through_score)
        .map_err(|_| RewardSettlementV3Error::RewardOverflow)?;
    let observed = ordered.blocks[index].clone();
    if observed != boundary.finalized_block_hash {
        return Err(RewardSettlementV3Error::FinalityBlockMismatch {
            score: boundary.finalized_through_score,
            expected: boundary.finalized_block_hash.clone(),
            observed,
        });
    }
    let digest = ordered_prefix_digest(&ordered.blocks, boundary.finalized_through_score)?;
    if digest != boundary.ordered_prefix_digest {
        return Err(RewardSettlementV3Error::FinalityPrefixDigestMismatch);
    }
    Ok(())
}

fn block_fees_atoms(block: &crate::types::Block) -> Result<u64, RewardSettlementV3Error> {
    block.transactions.iter().skip(1).try_fold(0_u64, |acc, tx| {
        acc.checked_add(tx.fee)
            .ok_or(RewardSettlementV3Error::RewardOverflow)
    })
}

pub fn derive_reward_settlement_snapshot_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    finality_boundary: Option<&RewardFinalityBoundaryV3>,
) -> Result<RewardSettlementSnapshotV3, RewardSettlementV3Error> {
    if state.chain_id.is_empty() {
        return Err(RewardSettlementV3Error::EmptyChainId);
    }

    let ordered = derive_ordered_dag_v2(state)?;
    if ordered.blocks.is_empty() {
        return Err(RewardSettlementV3Error::EmptyOrderedDag);
    }
    validate_ordered_genesis(state, &ordered)?;

    if let Some(boundary) = finality_boundary {
        validate_reward_finality_boundary_v3(state, boundary)?;
    }
    let finalized_through = finality_boundary.map(|boundary| boundary.finalized_through_score);
    let current_monetary_score = ordered.blocks.len().saturating_sub(1) as u64;

    let mut claims = Vec::with_capacity(ordered.blocks.len().saturating_sub(1));
    let mut total_authorized_subsidy_atoms = 0_u64;
    let mut total_pending_or_spendable_fees_atoms = 0_u64;
    let mut total_spendable_reward_atoms = 0_u64;

    for (index, block_hash) in ordered.blocks.iter().enumerate().skip(1) {
        let monetary_score = u64::try_from(index).map_err(|_| RewardSettlementV3Error::RewardOverflow)?;
        let block = state
            .dag
            .blocks
            .get(block_hash)
            .ok_or_else(|| RewardSettlementV3Error::Ordering(format!("ordered block {block_hash} is missing")))?;
        let claim = block
            .transactions
            .first()
            .ok_or_else(|| RewardSettlementV3Error::MissingRewardClaim {
                block_hash: block_hash.clone(),
            })?;
        validate_reward_claim_transaction_v3(claim, &state.chain_id).map_err(|error| {
            RewardSettlementV3Error::InvalidRewardClaim(format!("block {block_hash}: {error}"))
        })?;
        if block
            .transactions
            .iter()
            .skip(1)
            .any(|tx| tx.version == REWARD_CLAIM_TRANSACTION_VERSION_V3 && tx.inputs.is_empty())
        {
            return Err(RewardSettlementV3Error::MultipleRewardClaims {
                block_hash: block_hash.clone(),
            });
        }

        let subsidy_atoms = subsidy_atoms_for_score(monetary_score, cadence_segments)?;
        let fees_atoms = block_fees_atoms(block)?;
        let settlement_amount_atoms = subsidy_atoms
            .checked_add(fees_atoms)
            .ok_or(RewardSettlementV3Error::RewardOverflow)?;
        let finality_protected = finalized_through
            .is_some_and(|finalized_score| monetary_score <= finalized_score);
        let maturity = economic_maturity_reached(
            monetary_score,
            current_monetary_score,
            cadence_segments,
        )?;
        let status = match (finality_protected, maturity) {
            (false, _) => RewardClaimStatusV3::Provisional,
            (true, false) => RewardClaimStatusV3::FinalizedImmature,
            (true, true) => RewardClaimStatusV3::Spendable,
        };

        total_authorized_subsidy_atoms = total_authorized_subsidy_atoms
            .checked_add(subsidy_atoms)
            .ok_or(RewardSettlementV3Error::RewardOverflow)?;
        total_pending_or_spendable_fees_atoms = total_pending_or_spendable_fees_atoms
            .checked_add(fees_atoms)
            .ok_or(RewardSettlementV3Error::RewardOverflow)?;
        if status == RewardClaimStatusV3::Spendable {
            total_spendable_reward_atoms = total_spendable_reward_atoms
                .checked_add(settlement_amount_atoms)
                .ok_or(RewardSettlementV3Error::RewardOverflow)?;
        }

        claims.push(RewardClaimSettlementV3 {
            block_hash: block_hash.clone(),
            block_height: block.header.height,
            monetary_score,
            claim_txid: claim.txid.clone(),
            beneficiary: claim.outputs[0].address.clone(),
            settlement_outpoint: settlement_outpoint_v3(&state.chain_id, block_hash, &claim.txid),
            subsidy_atoms,
            fees_atoms,
            settlement_amount_atoms,
            finality_protected,
            economic_maturity_reached: maturity,
            status,
        });
    }

    Ok(RewardSettlementSnapshotV3 {
        schema_version: REWARD_SETTLEMENT_SCHEMA_VERSION_V3,
        ordering_version: ordered.ordering_version,
        ordered_dag_digest: ordered.digest,
        ordered_dag_tip: ordered.blocks.last().cloned(),
        current_monetary_score,
        finality_boundary: finality_boundary.cloned(),
        claims,
        total_authorized_subsidy_atoms,
        total_pending_or_spendable_fees_atoms,
        total_spendable_reward_atoms,
    })
}

/// Produce the exact UTXOs that are eligible to enter authoritative state at
/// this snapshot. Provisional and finalized-but-immature claims are absent.
/// Callers must still perform duplicate-outpoint checks when committing these
/// UTXOs to a state transition.
pub fn materializable_reward_utxos_v3(snapshot: &RewardSettlementSnapshotV3) -> Vec<Utxo> {
    snapshot
        .claims
        .iter()
        .filter(|claim| claim.status == RewardClaimStatusV3::Spendable)
        .map(|claim| Utxo {
            outpoint: claim.settlement_outpoint.clone(),
            address: claim.beneficiary.clone(),
            amount: claim.settlement_amount_atoms,
            coinbase: true,
            height: claim.block_height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput},
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];
    const ONE_HOUR_PER_SCORE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 3_600_000_000_000,
    }];

    fn reward_block(
        chain_id: &str,
        hash: &str,
        parents: Vec<&str>,
        height: u64,
        blue_score: u64,
        beneficiary: &str,
        nonce: u64,
    ) -> Block {
        let claim = build_reward_claim_transaction_v3(beneficiary, nonce, chain_id).unwrap();
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 2,
                parents: parents.into_iter().map(str::to_string).collect(),
                timestamp: height.saturating_add(10),
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("m-{hash}"),
                state_root: format!("s-{hash}"),
                blue_score,
                height,
            },
            transactions: vec![claim],
        }
    }

    fn diamond_state(chain_id: &str, selected_a: bool) -> ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        let genesis = state.dag.genesis_hash.clone();
        let a = reward_block(chain_id, "a", vec![&genesis], 1, 10, "pulse1a", 1);
        let b = reward_block(chain_id, "b", vec![&genesis], 1, 9, "pulse1b", 2);
        let c = reward_block(chain_id, "c", vec!["a", "b"], 2, 20, "pulse1c", 3);
        for block in [a.clone(), b.clone(), c.clone()] {
            state
                .dag
                .blue_work
                .insert(block.hash.clone(), block.header.blue_score as u128 * 10);
            state.dag.blocks.insert(block.hash.clone(), block);
        }
        state
            .dag
            .selected_parents
            .insert("a".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("b".into(), Some(genesis.clone()));
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis.clone(), vec![]);

        if selected_a {
            state
                .dag
                .selected_parents
                .insert("c".into(), Some("a".into()));
            state.dag.selected_chain = vec![genesis, "a".into(), "c".into()];
            state.dag.merge_set_blues.insert("a".into(), vec![]);
            state.dag.merge_set_reds.insert("a".into(), vec![]);
            state.dag.merge_set_blues.insert("c".into(), vec!["b".into()]);
            state.dag.merge_set_reds.insert("c".into(), vec![]);
        } else {
            state
                .dag
                .selected_parents
                .insert("c".into(), Some("b".into()));
            state.dag.selected_chain = vec![genesis, "b".into(), "c".into()];
            state.dag.merge_set_blues.insert("b".into(), vec![]);
            state.dag.merge_set_reds.insert("b".into(), vec![]);
            state.dag.merge_set_blues.insert("c".into(), vec!["a".into()]);
            state.dag.merge_set_reds.insert("c".into(), vec![]);
        }
        state
    }

    #[test]
    fn reward_claim_commits_beneficiary_but_not_amount() {
        let mut claim = build_reward_claim_transaction_v3("pulse1miner", 7, "chain-a").unwrap();
        assert_eq!(claim.outputs[0].amount, 0);
        validate_reward_claim_transaction_v3(&claim, "chain-a").unwrap();

        claim.outputs[0].amount = 1;
        assert!(matches!(
            validate_reward_claim_transaction_v3(&claim, "chain-a"),
            Err(RewardSettlementV3Error::InvalidRewardClaim(_))
        ));
    }

    #[test]
    fn reward_claim_txid_and_settlement_outpoint_are_chain_bound() {
        let a = build_reward_claim_transaction_v3("pulse1miner", 7, "chain-a").unwrap();
        let b = build_reward_claim_transaction_v3("pulse1miner", 7, "chain-b").unwrap();
        assert_ne!(a.txid, b.txid);
        assert_ne!(
            settlement_outpoint_v3("chain-a", "block", &a.txid),
            settlement_outpoint_v3("chain-b", "block", &a.txid)
        );
    }

    #[test]
    fn old_finality_binding_cannot_survive_a_reordered_prefix() {
        let first = diamond_state("reward-finality", true);
        let reordered = diamond_state("reward-finality", false);
        let boundary = bind_reward_finality_boundary_v3(&first, 1, "finality-test-v1").unwrap();
        assert_eq!(boundary.finalized_block_hash, "a");
        assert!(matches!(
            validate_reward_finality_boundary_v3(&reordered, &boundary),
            Err(RewardSettlementV3Error::FinalityBlockMismatch { .. })
                | Err(RewardSettlementV3Error::FinalityPrefixDigestMismatch)
        ));
    }

    #[test]
    fn settlement_requires_both_finality_and_economic_maturity() {
        let state = diamond_state("reward-settlement", true);
        let boundary = bind_reward_finality_boundary_v3(&state, 1, "finality-test-v1").unwrap();

        let immature =
            derive_reward_settlement_snapshot_v3(&state, &ONE_SECOND, Some(&boundary)).unwrap();
        assert_eq!(immature.claims[0].monetary_score, 1);
        assert_eq!(immature.claims[0].status, RewardClaimStatusV3::FinalizedImmature);
        assert!(materializable_reward_utxos_v3(&immature).is_empty());

        let mature = derive_reward_settlement_snapshot_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            Some(&boundary),
        )
        .unwrap();
        assert_eq!(mature.claims[0].status, RewardClaimStatusV3::Spendable);
        assert!(!materializable_reward_utxos_v3(&mature).is_empty());

        let no_finality =
            derive_reward_settlement_snapshot_v3(&state, &ONE_HOUR_PER_SCORE, None).unwrap();
        assert_eq!(no_finality.claims[0].status, RewardClaimStatusV3::Provisional);
        assert!(materializable_reward_utxos_v3(&no_finality).is_empty());
    }

    #[test]
    fn fees_are_carried_into_the_delayed_settlement_amount() {
        let mut state = diamond_state("reward-fees", true);
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
        state
            .dag
            .blocks
            .get_mut("a")
            .unwrap()
            .transactions
            .push(fee_tx);
        let boundary = bind_reward_finality_boundary_v3(&state, 1, "finality-test-v1").unwrap();
        let snapshot = derive_reward_settlement_snapshot_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            Some(&boundary),
        )
        .unwrap();
        assert_eq!(snapshot.claims[0].fees_atoms, 7);
        assert_eq!(
            snapshot.claims[0].settlement_amount_atoms,
            snapshot.claims[0].subsidy_atoms + 7
        );
    }
}
