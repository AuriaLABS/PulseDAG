//! DORMANT native-v3 complete-envelope GHOSTDAG feasibility guard.
//!
//! Rejects unauthenticated header/body pairings and hidden issuance before
//! computing a wholly UNAPPROVED candidate K-cluster diagnostic.
//! This module is NEVER called by mining, P2P, startup, storage or monetary
//! replay. It rejects ordinary transfers pending signature, UTXO/state-root,
//! maturity/finality, retarget or canonical monetary DAG transaction order.

use std::collections::BTreeMap;

use crate::{
    block_envelope_v3::{validate_block_envelope_v3, BlockEnvelopeV3},
    errors::PulseError,
    genesis_v3::verify_dormant_native_genesis_freeze_v3_ns,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, CandidateGhostdagEvaluation,
    },
    native_parent_dag_context_v3::MAX_DORMANT_V3_DAG_HEADERS,
    reward_settlement_v3::validate_reward_claim_transaction_v3,
    types::BlockId,
};

fn reject(reason: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED dormant native-v3 complete-envelope diagnostic: {}",
        reason.into()
    ))
}

/// Whole-block authentication + independently pinned genesis + test-K header
/// derivation. No activation edge and no production consensus authority.
///
/// Every supplied envelope must belong to the exact authenticated ancestor
/// closure of the tips. The untrusted map key, header hash, Merkle commitment,
/// v2 txids, reward-claim wire shape, hidden inputless mints and KHeavyHash
/// checked blue scores are all verified. Fails closed on missing bodies,
/// unrelated extra blocks, malformed side ancestry or future schema fields.
///
/// Fail-closed: any ordinary signed transfer is rejected until a fully
/// diagnostic never creates a UTXO or decides subsidy/maturity/finality.
pub fn inspect_dormant_native_v3_full_envelope_trial(
    chain_id: &str,
    frozen_genesis_ns: u64,
    frozen_genesis_hash: &str,
    candidate_tips: &[BlockId],
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    trial_k: usize,
) -> Result<CandidateGhostdagEvaluation, PulseError> {
    if envelopes.is_empty() || envelopes.len() > MAX_DORMANT_V3_DAG_HEADERS {
        return Err(reject("complete envelope count exceeds bounded diagnostic"));
    }
    let genesis = envelopes
        .get(frozen_genesis_hash)
        .ok_or_else(|| reject("missing independently frozen full genesis envelope"))?;
    verify_dormant_native_genesis_freeze_v3_ns(
        genesis,
        chain_id,
        frozen_genesis_ns,
        frozen_genesis_hash,
    )?;

    let mut headers = BTreeMap::new();
    for (key, block) in envelopes {
        if key != &block.hash {
            return Err(reject(
                "envelope index key differs from canonical block hash",
            ));
        }
        validate_block_envelope_v3(block, chain_id)?;
        if key != frozen_genesis_hash {
            let reward = block
                .transactions
                .first()
                .ok_or_else(|| reject("native-v3 mined block lacks amountless reward claim"))?;
            validate_reward_claim_transaction_v3(reward, chain_id)
                .map_err(|e| reject(format!("invalid native-v3 amountless claim: {e}")))?;
            if block
                .transactions
                .iter()
                .skip(1)
                .any(|transaction| transaction.inputs.is_empty())
            {
                return Err(reject(
                    "additional inputless transaction creates hidden issuance",
                ));
            }
            // Full Merkle/txid commitment is NOT evidence that any ordinary
            // transfer has valid signatures, spendable UTXOs, deterministic
            // fee treatment, finality or a correct native-DAG replay order.
            // Fail closed until a separate fully signed, state-rooted
            // economic validator is available. This bridge must NEVER
            // silently upgrade a header-only trial into block admission.
            if block.transactions.len() != 1 {
                return Err(reject(
                    "ordinary transfers require authoritative signed-UTXO native-v3 replay",
                ));
            }
        }
        if headers.insert(key.clone(), block.header.clone()).is_some() {
            return Err(reject("duplicate full block index"));
        }
    }
    let trial = derive_dormant_native_v3_ghostdag_trial(
        chain_id,
        frozen_genesis_ns,
        frozen_genesis_hash,
        candidate_tips,
        &headers,
        trial_k,
    )?;
    if trial.blocks.len() != envelopes.len()
        || envelopes.keys().any(|id| !trial.blocks.contains_key(id))
    {
        return Err(reject(
            "unrelated full block or missing complete authenticated ancestry",
        ));
    }
    Ok(trial)
}
