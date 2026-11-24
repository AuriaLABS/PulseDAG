//! DORMANT native-v3 GD-08 reward/fee accounting preview only (#1410/#1466).
//!
//! This recomputes signed-spend winner fees from authentic KHeavyHash block
//! envelopes, checks every amountless reward claim, and audits hypothetical
//! subsidy/supply against the v3 economic-time schedule. Unlike the legacy
//! reward engine, it does NOT call OrderedDagV2 or ChainState<types::Block>.
//!
//! NO economic acceptance: the opening UTXO is caller supplied, selected
//! GHOSTDAG K/blue-red monetary eligibility is NOT frozen, there is no
//! finality, mature coinbase settlement, authenticated checkpoint or accepted
//! RocksDB writer. None of the results may be used to mint/spend rewards.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    block_envelope_v3::BlockEnvelopeV3,
    errors::PulseError,
    monetary_v3::{
        economic_maturity_reached, subsidy_atoms_for_score, total_supply_atoms_for_score,
        MonetaryCadenceSegment,
    },
    native_gd08_signed_utxo_trial_v3::inspect_dormant_native_gd08_signed_prefix,
    reward_settlement_v3::validate_reward_claim_transaction_v3,
    state::UtxoState,
    types::BlockId,
};

const MAX_REWARD_PREVIEW_BLOCKS: usize = 128;

/// Explicit experimental K and economic-time cadence; NEVER a frozen native
/// GD-08 policy or an authorization for mining/reward settlement.
#[derive(Debug, Clone, Copy)]
pub struct DormantNativeRewardTrialPolicyV3<'a> {
    pub trial_k: usize,
    pub cadence_segments: &'a [MonetaryCadenceSegment],
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeGd08RewardEntryV3 {
    pub block_hash: BlockId,
    /// Trial monetary position in the UNAPPROVED canonical order.
    pub trial_score: u64,
    pub claim_txid: String,
    pub beneficiary: String,
    pub hypothetical_subsidy_atoms: u64,
    /// Only fees from transactions actually applied to signed UTXOs.
    pub hypothetical_eligible_fees_atoms: u64,
    pub hypothetical_claim_atoms: u64,
    /// Economic-time ONLY: never includes finality or actual spendability.
    pub economic_maturity_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeGd08RewardPreviewV3 {
    pub trial_order_digest: String,
    pub terminal_signed_utxo_root: String,
    pub trial_score: u64,
    pub hypothetical_rewards: Vec<DormantNativeGd08RewardEntryV3>,
    pub total_hypothetical_subsidy_atoms: u64,
    pub total_hypothetical_eligible_fees_atoms: u64,
    pub scheduled_supply_atoms: u64,
}

fn invalid(reason: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED native-v3 GD08 reward/fee preview, NOT monetary authority: {}",
        reason.into()
    ))
}

/// Re-derive signed conflict winners, validate each native amountless reward
/// claim and prove exact subsidy telescoping for a hypothetical canonical
/// order. Red/blue inclusion, K and maturity-finality are UNAPPROVED; no UTXO
/// rewards or settlement outpoints are created or paid.
///
/// The only monetary cadence input is the explicit schedule; it must not
/// enable production mining or constitute a network policy freeze.
pub fn inspect_dormant_native_gd08_reward_preview(
    chain_id: &str,
    frozen_genesis_ns: u64,
    frozen_genesis_hash: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    opening_utxo: &UtxoState,
    policy: DormantNativeRewardTrialPolicyV3<'_>,
) -> Result<DormantNativeGd08RewardPreviewV3, PulseError> {
    let cadence_segments = policy.cadence_segments;
    let signed = inspect_dormant_native_gd08_signed_prefix(
        chain_id,
        frozen_genesis_ns,
        frozen_genesis_hash,
        tip,
        envelopes,
        policy.trial_k,
        opening_utxo,
    )?;
    if signed.ordered_blocks.len() < 2
        || signed.ordered_blocks.len() > MAX_REWARD_PREVIEW_BLOCKS
        || signed.ordered_blocks.first().map(String::as_str) != Some(frozen_genesis_hash)
    {
        return Err(invalid(
            "invalid bounded genesis-anchored native reward preview",
        ));
    }
    let current_score = u64::try_from(signed.ordered_blocks.len() - 1)
        .map_err(|_| invalid("trial monetary score exceeds u64"))?;
    let mut expected_fees = BTreeMap::new();
    for (hash, fee) in &signed.eligible_fees_by_block {
        if expected_fees.insert(hash.clone(), *fee).is_some() {
            return Err(invalid("signed replay repeated fee owner block"));
        }
    }
    if expected_fees.len() != signed.ordered_blocks.len() - 1 {
        return Err(invalid("signed replay fee coverage is incomplete"));
    }
    let all_trial_hashes: BTreeSet<_> = signed.ordered_blocks.iter().cloned().collect();
    if expected_fees
        .keys()
        .any(|hash| !all_trial_hashes.contains(hash))
    {
        return Err(invalid(
            "fees refer to a block absent from authenticated order",
        ));
    }

    let mut rewards = Vec::with_capacity(signed.ordered_blocks.len() - 1);
    let mut total_subsidy = 0_u64;
    let mut total_fees = 0_u64;
    for (index, hash) in signed.ordered_blocks.iter().enumerate().skip(1) {
        let block = envelopes
            .get(hash)
            .ok_or_else(|| invalid("missing authenticated native block body"))?;
        if block.chain_id != chain_id || block.hash != *hash {
            return Err(invalid("foreign or incorrectly keyed native block"));
        }
        let claim = block
            .transactions
            .first()
            .ok_or_else(|| invalid("missing amountless native-v3 reward claim"))?;
        validate_reward_claim_transaction_v3(claim, chain_id)
            .map_err(|e| invalid(format!("invalid claim: {e}")))?;
        if claim.outputs.len() != 1 || claim.outputs[0].amount != 0 {
            return Err(invalid("native reward claim must be amountless"));
        }
        let score = u64::try_from(index).map_err(|_| invalid("score overflow"))?;
        let subsidy = subsidy_atoms_for_score(score, cadence_segments)
            .map_err(|e| invalid(format!("hypothetical subsidy error: {e}")))?;
        let fees = *expected_fees
            .get(hash)
            .ok_or_else(|| invalid("signed replay omitted a block's fees"))?;
        let total = subsidy
            .checked_add(fees)
            .ok_or_else(|| invalid("hypothetical reward plus fees overflow"))?;
        total_subsidy = total_subsidy
            .checked_add(subsidy)
            .ok_or_else(|| invalid("subsidy accumulation overflow"))?;
        total_fees = total_fees
            .checked_add(fees)
            .ok_or_else(|| invalid("accepted-fee accumulation overflow"))?;
        let mature_economic_time =
            economic_maturity_reached(score, current_score, cadence_segments)
                .map_err(|e| invalid(format!("economic maturity schedule invalid: {e}")))?;
        rewards.push(DormantNativeGd08RewardEntryV3 {
            block_hash: hash.clone(),
            trial_score: score,
            claim_txid: claim.txid.clone(),
            beneficiary: claim.outputs[0].address.clone(),
            hypothetical_subsidy_atoms: subsidy,
            hypothetical_eligible_fees_atoms: fees,
            hypothetical_claim_atoms: total,
            economic_maturity_only: mature_economic_time,
        });
    }
    let scheduled = total_supply_atoms_for_score(current_score, cadence_segments)
        .map_err(|e| invalid(format!("scheduled monetary supply error: {e}")))?;
    if total_subsidy != scheduled {
        return Err(invalid(
            "hypothetical subsidy total diverges from scheduled supply",
        ));
    }
    Ok(DormantNativeGd08RewardPreviewV3 {
        trial_order_digest: signed.order_digest,
        terminal_signed_utxo_root: signed.resulting_state_root,
        trial_score: current_score,
        hypothetical_rewards: rewards,
        total_hypothetical_subsidy_atoms: total_subsidy,
        total_hypothetical_eligible_fees_atoms: total_fees,
        scheduled_supply_atoms: scheduled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        monetary_v3::PRODUCTION_CADENCE_V3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        tx::{
            address_from_public_key, compute_txid_v2, signing_message_v2, TRANSACTION_VERSION_V2,
        },
        types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    };
    use ed25519_dalek::{Signer, SigningKey};

    const CHAIN: &str = "native-v3-gd08-hypothetical-reward-preview-fixture";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;

    fn trial_policy(cadence: &[MonetaryCadenceSegment]) -> DormantNativeRewardTrialPolicyV3<'_> {
        DormantNativeRewardTrialPolicyV3 {
            trial_k: 0,
            cadence_segments: cadence,
        }
    }

    fn signed_spend(
        key: &SigningKey,
        source: &OutPoint,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: source.clone(),
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1native-reward-preview-recipient".into(),
                amount,
            }],
            fee,
            nonce,
        };
        let message = signing_message_v2(&tx, CHAIN).unwrap();
        tx.inputs[0].signature = hex::encode(key.sign(&message).to_bytes());
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        tx
    }

    fn fixture() -> (
        String,
        String,
        BTreeMap<BlockId, BlockEnvelopeV3>,
        UtxoState,
    ) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let key = SigningKey::from_bytes(&[9u8; 32]);
        let source = OutPoint {
            txid: "test-only-independent-opening-utxo".into(),
            index: 0,
        };
        let coin = Utxo {
            outpoint: source.clone(),
            address: address_from_public_key(&hex::encode(key.verifying_key().to_bytes())),
            amount: 10,
            coinbase: false,
            height: 0,
        };
        let mut opening = UtxoState::default();
        opening
            .address_index
            .insert(coin.address.clone(), vec![source.clone()]);
        opening.utxos.insert(source.clone(), coin);
        // Two conflicting genuine signed spends, both valid in isolation.
        // Only the first spend gets its fee; the loser NEVER gets paid.
        let transactions = vec![
            build_reward_claim_transaction_v3("pulse1candidate-beneficiary", 1, CHAIN).unwrap(),
            signed_spend(&key, &source, 9, 1, 10),
            signed_spend(&key, &source, 8, 2, 11),
        ];
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: GENESIS_NS + 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "a".repeat(64),
            blue_score: 1,
            height: 1,
        };
        let nonce = (0..4096u64)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .unwrap();
        header.nonce = nonce;
        let body = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        let tip = body.hash.clone();
        let g = genesis.hash.clone();
        (
            g.clone(),
            tip.clone(),
            BTreeMap::from([(g, genesis), (tip, body)]),
            opening,
        )
    }

    #[test]
    fn hypothetically_scheduled_subsidy_matches_supply_and_excludes_losing_fees() {
        let (g, tip, blocks, opening) = fixture();
        let result = inspect_dormant_native_gd08_reward_preview(
            CHAIN,
            GENESIS_NS,
            &g,
            &tip,
            &blocks,
            &opening,
            trial_policy(&PRODUCTION_CADENCE_V3),
        )
        .unwrap();
        assert_eq!(result.trial_score, 1);
        assert_eq!(result.hypothetical_rewards.len(), 1);
        let block = &result.hypothetical_rewards[0];
        assert_eq!(block.block_hash, tip);
        assert_eq!(block.beneficiary, "pulse1candidate-beneficiary");
        assert_eq!(block.hypothetical_eligible_fees_atoms, 1);
        assert!(!block.economic_maturity_only);
        assert_eq!(result.total_hypothetical_eligible_fees_atoms, 1);
        assert_eq!(
            result.total_hypothetical_subsidy_atoms,
            result.scheduled_supply_atoms
        );
        assert_eq!(
            block.hypothetical_claim_atoms,
            block.hypothetical_subsidy_atoms + 1
        );
    }

    #[test]
    fn manipulated_claim_and_invalid_cadence_cannot_produce_reward_preview() {
        let (g, tip, mut blocks, opening) = fixture();
        let edited = blocks.get_mut(&tip).unwrap();
        edited.transactions[0].outputs[0].amount = 99;
        assert!(inspect_dormant_native_gd08_reward_preview(
            CHAIN,
            GENESIS_NS,
            &g,
            &tip,
            &blocks,
            &opening,
            trial_policy(&PRODUCTION_CADENCE_V3),
        )
        .is_err());
        let (g, tip, blocks, opening) = fixture();
        let invalid = [MonetaryCadenceSegment {
            activation_score: 2,
            target_interval_ns: 0,
        }];
        assert!(inspect_dormant_native_gd08_reward_preview(
            CHAIN, GENESIS_NS, &g, &tip, &blocks, &opening, trial_policy(&invalid),
        )
        .is_err());
    }
}
