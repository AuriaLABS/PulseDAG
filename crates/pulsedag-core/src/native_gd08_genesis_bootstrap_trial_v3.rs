//! DORMANT first-network native-v3 GD-08 bootstrap from ZERO issued UTXOs.
//!
//! Generic signed-prefix trials accept caller-supplied opening UTXOs, not a
//! trusted first-network balance. This wrapper derives an EMPTY UTXO state
//! from the independently pinned native-v3 genesis. Before native reward
//! settlement and maturity exist, no spendable rewards or miner fees exist.
//! No monetary acceptance, frozen K, checkpoint trust, mining or P2P changes.

use std::collections::BTreeMap;

use crate::{
    block_envelope_v3::BlockEnvelopeV3,
    errors::PulseError,
    genesis_v3::{
        dormant_native_genesis_envelope_v3_ns, verify_dormant_native_genesis_freeze_v3_ns,
    },
    native_gd08_signed_utxo_trial_v3::{
        inspect_dormant_native_gd08_signed_prefix_with_tip_root_trial,
        DormantNativeGd08SignedPrefixV3,
    },
    state::UtxoState,
    types::BlockId,
};

fn invalid(why: &str) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED native-v3 genesis bootstrap, NOT monetary authority: {why}"
    ))
}

/// A test-only native-v3 signed-prefix replay from EMPTY frozen genesis UTXO.
/// The trusted_genesis hash must be supplied by independent trusted config,
/// NEVER fetched from or adopted from an inbound P2P header.
/// This does not create/mature the amountless v3 reward claims or write state.
pub fn inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    trial_k: usize,
) -> Result<DormantNativeGd08SignedPrefixV3, PulseError> {
    let canonical = dormant_native_genesis_envelope_v3_ns(chain_id, genesis_ns)?;
    if canonical.hash != trusted_genesis {
        return Err(invalid("genesis hash differs from frozen native identity"));
    }
    let genesis = envelopes
        .get(trusted_genesis)
        .ok_or_else(|| invalid("missing full native-v3 frozen genesis"))?;
    verify_dormant_native_genesis_freeze_v3_ns(genesis, chain_id, genesis_ns, trusted_genesis)?;
    let empty_utxo = UtxoState::default();
    if empty_utxo.compute_state_root()? != canonical.header.state_root {
        return Err(invalid("genesis root must bind exactly zero issuance"));
    }
    let report = inspect_dormant_native_gd08_signed_prefix_with_tip_root_trial(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        envelopes,
        trial_k,
        &empty_utxo,
    )?;
    if !report.applied_txids.is_empty()
        || report
            .eligible_fees_by_block
            .iter()
            .any(|(_, fee)| *fee != 0)
        || report.resulting_state_root != canonical.header.state_root
    {
        return Err(invalid(
            "ordinary spending requires independently settled native rewards",
        ));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        tx::{compute_txid_v2, signing_message_v2, TRANSACTION_VERSION_V2},
        types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput},
    };
    use ed25519_dalek::{Signer, SigningKey};

    const CHAIN: &str = "native-v3-zero-genesis-utxo-bootstrap-test";
    const NS: u64 = 1_800_000_000_500_000_000;

    fn mine(
        transfers: Vec<Transaction>,
        committed_root: String,
    ) -> (String, BlockId, BTreeMap<BlockId, BlockEnvelopeV3>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let mut txs =
            vec![build_reward_claim_transaction_v3("pulse1unspendable-reward", 1, CHAIN).unwrap()];
        txs.extend(transfers);
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![g.clone()],
            timestamp_ns: NS + 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: committed_root,
            blue_score: 1,
            height: 1,
        };
        header.nonce = (0_u64..4096)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .expect("easy native-v3 PoW target");
        let child = build_block_envelope_v3(CHAIN, header, txs).unwrap();
        let tip = child.hash.clone();
        (
            g.clone(),
            tip.clone(),
            BTreeMap::from([(g, genesis), (tip, child)]),
        )
    }

    #[test]
    fn zero_issuance_genesis_cannot_materialize_amountless_reward_claim() {
        let empty_root = UtxoState::default().compute_state_root().unwrap();
        let (genesis, tip, blocks) = mine(vec![], empty_root.clone());
        let report = inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
            CHAIN, NS, &genesis, &tip, &blocks, 0,
        )
        .unwrap();
        assert!(report.applied_txids.is_empty());
        assert!(report.skipped_conflict_txids.is_empty());
        assert_eq!(report.eligible_fees_by_block, vec![(tip.clone(), 0)]);
        assert_eq!(report.resulting_state_root, empty_root);
        assert!(inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
            CHAIN,
            NS + 1,
            &genesis,
            &tip,
            &blocks,
            0,
        )
        .is_err());
        assert!(inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
            CHAIN,
            NS,
            &"f".repeat(64),
            &tip,
            &blocks,
            0,
        )
        .is_err());
    }

    #[test]
    fn real_signature_and_pow_cannot_spend_unissued_genesis_outpoint() {
        let key = SigningKey::from_bytes(&[33u8; 32]);
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "unissued-genesis-coin".into(),
                    index: 0,
                },
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1fake-bootstrap-output".into(),
                amount: 9,
            }],
            fee: 1,
            nonce: 7,
        };
        let signable = signing_message_v2(&tx, CHAIN).unwrap();
        tx.inputs[0].signature = hex::encode(key.sign(&signable).to_bytes());
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        let empty_root = UtxoState::default().compute_state_root().unwrap();
        let (genesis, tip, blocks) = mine(vec![tx], empty_root);
        assert!(inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
            CHAIN, NS, &genesis, &tip, &blocks, 0,
        )
        .is_err());
    }

    #[test]
    fn valid_pow_cannot_replace_frozen_genesis_utxo_root_with_placeholder() {
        let (genesis, tip, blocks) = mine(vec![], "c".repeat(64));
        assert!(inspect_dormant_native_gd08_zero_issuance_bootstrap_trial(
            CHAIN, NS, &genesis, &tip, &blocks, 0,
        )
        .is_err());
    }
}
