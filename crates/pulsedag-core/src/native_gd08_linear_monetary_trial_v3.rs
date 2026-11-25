//! Strictly bounded, DORMANT native monetary replay for a LINEAR prefix.
//!
//! Actual native envelopes, authenticated GD08 order, zero-funded genesis,
//! signed spends, maturity at end of score, block-bound reward outpoints and
//! every committed UTXO root. No legacy Block or OrderedDagV2. Deliberately
//! reject forks/conflicts until the native merge-set economic policy is frozen.
//! The cadence/K inputs are trial inputs, not authorization or finality proof.
//! No production caller, persistence writer, checkpoint or network activation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    block_envelope_v3::{validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3},
    errors::PulseError,
    genesis_v3::verify_dormant_native_genesis_freeze_v3_ns,
    monetary_v3::{
        economic_maturity_reached, subsidy_atoms_for_score, total_supply_atoms_for_score,
        MonetaryCadenceSegment,
    },
    native_gd08_order_kernel_v3::derive_dormant_native_gd08_order_from_ghostdag_trial,
    native_gd08_signed_utxo_trial_v3::verify_full_signed_spend,
    reward_settlement_v3::{settlement_outpoint_v3, validate_reward_claim_transaction_v3},
    state::UtxoState,
    types::{BlockId, OutPoint, Utxo},
};

const MAX_TRIAL_BLOCKS: usize = 128;
const MAX_TRIAL_ORDINARY_ITEMS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeLinearMonetaryReportV3 {
    pub order_digest: String,
    pub state_root: String,
    pub monetary_score: u64,
    pub total_subsidy_atoms: u64,
    pub pending_reward_atoms: u64,
    pub spendable_atoms: u64,
    pub mature_rewards: usize,
    pub applied_transfers: usize,
}

fn invalid(why: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED native linear monetary trial: {}",
        why.into()
    ))
}

fn insert_coin(state: &mut UtxoState, coin: Utxo) -> Result<(), PulseError> {
    if state.utxos.contains_key(&coin.outpoint) {
        return Err(invalid("duplicate settlement or transfer outpoint"));
    }
    state
        .address_index
        .entry(coin.address.clone())
        .or_default()
        .push(coin.outpoint.clone());
    state.utxos.insert(coin.outpoint.clone(), coin);
    Ok(())
}

/// Rebuild from the full pinned genesis; no caller-supplied balances.
/// Outputs are diagnostics only. Reject any unsupported nonlinear history,
/// missing/immature input, duplicate spend, bad signature or root mismatch.
/// Apply ordinary spends before end-of-score maturity; rewards mature only
/// under the explicit trial economic schedule, never wall time or header height.
pub fn inspect_dormant_native_linear_monetary_trial(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
) -> Result<DormantNativeLinearMonetaryReportV3, PulseError> {
    if envelopes.is_empty() || envelopes.len() > MAX_TRIAL_BLOCKS {
        return Err(invalid("unsupported bounded prefix length"));
    }
    let genesis = envelopes
        .get(trusted_genesis)
        .ok_or_else(|| invalid("missing frozen genesis"))?;
    verify_dormant_native_genesis_freeze_v3_ns(genesis, chain_id, genesis_ns, trusted_genesis)?;
    let mut headers = BTreeMap::new();
    let mut item_count = 0usize;
    for (hash, envelope) in envelopes {
        if hash != &envelope.hash {
            return Err(invalid("incorrect envelope map key"));
        }
        validate_dormant_native_v3_monetary_body_preflight(envelope, chain_id)?;
        if hash != trusted_genesis && envelope.header.parents.len() != 1 {
            return Err(invalid("multiparent monetary policy not frozen"));
        }
        for tx in envelope.transactions.iter().skip(1) {
            item_count = item_count
                .checked_add(1)
                .and_then(|n| n.checked_add(tx.inputs.len()))
                .and_then(|n| n.checked_add(tx.outputs.len()))
                .ok_or_else(|| invalid("transaction budget overflow"))?;
            if item_count > MAX_TRIAL_ORDINARY_ITEMS {
                return Err(invalid("transaction budget exceeded"));
            }
        }
        headers.insert(hash.clone(), envelope.header.clone());
    }
    // K=0 here has no anticone effect: only one parent per reachable block,
    // and the ordered set must cover the entire supplied corpus.
    let order = derive_dormant_native_gd08_order_from_ghostdag_trial(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        &headers,
        0,
    )?;
    if order.ordered.len() != envelopes.len() {
        return Err(invalid("unrelated or competing blocks in linear trial"));
    }
    let mut state = UtxoState::default();
    if state.compute_state_root()? != genesis.header.state_root {
        return Err(invalid("genesis must have zero issuance"));
    }
    let mut created = BTreeSet::new();
    let mut pending: VecDeque<(u64, Utxo)> = VecDeque::new();
    let mut mature_rewards = 0usize;
    let mut applied_transfers = 0usize;
    let mut issued = 0_u64;
    for (index, hash) in order.ordered.iter().enumerate().skip(1) {
        let score = u64::try_from(index).map_err(|_| invalid("score overflow"))?;
        let block = &envelopes[hash];
        if block.header.parents.as_slice() != [order.ordered[index - 1].clone()] {
            return Err(invalid("nonlinear canonical prefix"));
        }
        let claim = block
            .transactions
            .first()
            .ok_or_else(|| invalid("missing reward claim"))?;
        validate_reward_claim_transaction_v3(claim, chain_id)
            .map_err(|e| invalid(e.to_string()))?;
        let mut fees = 0_u64;
        for tx in block.transactions.iter().skip(1) {
            let mut view = BTreeMap::new();
            for input in &tx.inputs {
                let coin = state
                    .utxos
                    .get(&input.previous_output)
                    .ok_or_else(|| invalid("missing, spent or immature input"))?;
                view.insert(input.previous_output.clone(), coin.clone());
            }
            verify_full_signed_spend(tx, chain_id, &view)?;
            for input in &tx.inputs {
                let coin = state
                    .utxos
                    .remove(&input.previous_output)
                    .ok_or_else(|| invalid("duplicate input"))?;
                if let Some(index) = state.address_index.get_mut(&coin.address) {
                    index.retain(|outpoint| outpoint != &coin.outpoint);
                }
            }
            for (i, output) in tx.outputs.iter().enumerate() {
                let outpoint = OutPoint {
                    txid: tx.txid.clone(),
                    index: u32::try_from(i).map_err(|_| invalid("output index overflow"))?,
                };
                if !created.insert(outpoint.clone()) {
                    return Err(invalid("repeated output history"));
                }
                insert_coin(
                    &mut state,
                    Utxo {
                        outpoint,
                        address: output.address.clone(),
                        amount: output.amount,
                        coinbase: false,
                        height: block.header.height,
                    },
                )?;
            }
            fees = fees
                .checked_add(tx.fee)
                .ok_or_else(|| invalid("fee overflow"))?;
            applied_transfers += 1;
        }
        let subsidy =
            subsidy_atoms_for_score(score, cadence).map_err(|e| invalid(e.to_string()))?;
        issued = issued
            .checked_add(subsidy)
            .ok_or_else(|| invalid("issuance overflow"))?;
        let amount = subsidy
            .checked_add(fees)
            .ok_or_else(|| invalid("reward overflow"))?;
        let outpoint = settlement_outpoint_v3(chain_id, hash, &claim.txid);
        if !created.insert(outpoint.clone()) {
            return Err(invalid("repeated reward outpoint"));
        }
        pending.push_back((
            score,
            Utxo {
                outpoint,
                address: claim.outputs[0].address.clone(),
                amount,
                coinbase: true,
                height: block.header.height,
            },
        ));
        while let Some((reward_score, _)) = pending.front() {
            if !economic_maturity_reached(*reward_score, score, cadence)
                .map_err(|e| invalid(e.to_string()))?
            {
                break;
            }
            let (_, coin) = pending.pop_front().expect("checked nonempty queue");
            if coin.amount != 0 {
                insert_coin(&mut state, coin)?;
            }
            mature_rewards += 1;
        }
        let spendable: u128 = state
            .utxos
            .values()
            .map(|coin| u128::from(coin.amount))
            .sum();
        let reserved: u128 = pending
            .iter()
            .map(|(_, coin)| u128::from(coin.amount))
            .sum();
        let scheduled =
            total_supply_atoms_for_score(score, cadence).map_err(|e| invalid(e.to_string()))?;
        if issued != scheduled || spendable + reserved != u128::from(scheduled) {
            return Err(invalid(
                "issued supply differs from UTXO plus pending rewards",
            ));
        }
        if state.compute_state_root()? != block.header.state_root {
            return Err(invalid("native committed state root mismatch"));
        }
    }
    let pending_reward_atoms = pending.iter().map(|(_, coin)| coin.amount).sum();
    let spendable_atoms = state.utxos.values().map(|coin| coin.amount).sum();
    Ok(DormantNativeLinearMonetaryReportV3 {
        order_digest: order.digest,
        state_root: state.compute_state_root()?,
        monetary_score: (order.ordered.len() - 1) as u64,
        total_subsidy_atoms: issued,
        pending_reward_atoms,
        spendable_atoms,
        mature_rewards,
        applied_transfers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        tx::{
            address_from_public_key, compute_txid_v2, signing_message_v2, TRANSACTION_VERSION_V2,
        },
        types::{compute_merkle_root, Transaction, TxInput, TxOutput},
    };
    use ed25519_dalek::{Signer, SigningKey};

    const CHAIN: &str = "native-linear-monetary-test-only";
    const NS: u64 = 1_800_000_000_500_000_000;
    const CADENCE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 3_600_000_000_000,
    }];

    fn mine(
        parent: &str,
        height: u64,
        beneficiary: &str,
        root: String,
        spends: Vec<Transaction>,
    ) -> BlockEnvelopeV3 {
        let mut transactions =
            vec![build_reward_claim_transaction_v3(beneficiary, height, CHAIN).unwrap()];
        transactions.extend(spends);
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![parent.to_owned()],
            timestamp_ns: NS + height * 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: root,
            blue_score: height,
            height,
        };
        header.nonce = (0..4096)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .unwrap();
        build_block_envelope_v3(CHAIN, header, transactions).unwrap()
    }

    fn signed_spend(key: &SigningKey, reward: &OutPoint, amount: u64) -> Transaction {
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward.clone(),
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: amount - 1,
            }],
            fee: 1,
            nonce: 3,
        };
        tx.inputs[0].signature = hex::encode(
            key.sign(&signing_message_v2(&tx, CHAIN).unwrap())
                .to_bytes(),
        );
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        tx
    }

    fn reward(block: &BlockEnvelopeV3, amount: u64) -> Utxo {
        let claim = &block.transactions[0];
        Utxo {
            outpoint: settlement_outpoint_v3(CHAIN, &block.hash, &claim.txid),
            address: claim.outputs[0].address.clone(),
            amount,
            coinbase: true,
            height: block.header.height,
        }
    }

    #[test]
    fn native_reward_matures_then_signed_spend_and_fee_preserve_supply() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let key = SigningKey::from_bytes(&[19u8; 32]);
        let owner = address_from_public_key(&hex::encode(key.verifying_key().to_bytes()));
        let mut expected = UtxoState::default();
        let empty = expected.compute_state_root().unwrap();
        let b1 = mine(&g, 1, &owner, empty.clone(), vec![]);
        let subsidy1 = subsidy_atoms_for_score(1, &CADENCE).unwrap();
        let r1 = reward(&b1, subsidy1);
        insert_coin(&mut expected, r1.clone()).unwrap();
        let b2 = mine(
            &b1.hash,
            2,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let spend = signed_spend(&key, &r1.outpoint, subsidy1);
        // Block 2 is the maturity boundary, so the same signed spend there
        // must fail even if its PoW and transaction commitment are valid.
        let premature = mine(&b1.hash, 2, &owner, empty, vec![spend.clone()]);
        let bad = BTreeMap::from([
            (g.clone(), genesis.clone()),
            (b1.hash.clone(), b1.clone()),
            (premature.hash.clone(), premature.clone()),
        ]);
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &premature.hash,
            &bad,
            &CADENCE,
        )
        .is_err());

        expected = UtxoState::default();
        insert_coin(
            &mut expected,
            Utxo {
                outpoint: OutPoint {
                    txid: spend.txid.clone(),
                    index: 0,
                },
                address: spend.outputs[0].address.clone(),
                amount: spend.outputs[0].amount,
                coinbase: false,
                height: 3,
            },
        )
        .unwrap();
        insert_coin(
            &mut expected,
            reward(&b2, subsidy_atoms_for_score(2, &CADENCE).unwrap()),
        )
        .unwrap();
        let b3 = mine(
            &b2.hash,
            3,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![spend.clone()],
        );
        insert_coin(
            &mut expected,
            reward(&b3, subsidy_atoms_for_score(3, &CADENCE).unwrap() + 1),
        )
        .unwrap();
        let b4 = mine(
            &b3.hash,
            4,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let corpus = BTreeMap::from([
            (g.clone(), genesis),
            (b1.hash.clone(), b1),
            (b2.hash.clone(), b2),
            (b3.hash.clone(), b3.clone()),
            (b4.hash.clone(), b4.clone()),
        ]);
        let observed = inspect_dormant_native_linear_monetary_trial(
            CHAIN, NS, &g, &b4.hash, &corpus, &CADENCE,
        )
        .unwrap();
        assert_eq!(observed.applied_transfers, 1);
        assert_eq!(observed.mature_rewards, 3);
        assert_eq!(observed.state_root, expected.compute_state_root().unwrap());
        assert_eq!(
            observed.pending_reward_atoms,
            subsidy_atoms_for_score(4, &CADENCE).unwrap()
        );
        assert_eq!(
            observed.total_subsidy_atoms,
            observed.spendable_atoms + observed.pending_reward_atoms
        );

        let replay = mine(
            &b3.hash,
            4,
            &owner,
            b4.header.state_root.clone(),
            vec![spend],
        );
        let mut bad = corpus.clone();
        bad.remove(&b4.hash);
        bad.insert(replay.hash.clone(), replay.clone());
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &replay.hash,
            &bad,
            &CADENCE,
        )
        .is_err());
        let forged = mine(&b3.hash, 4, &owner, "f".repeat(64), vec![]);
        bad.remove(&replay.hash);
        bad.insert(forged.hash.clone(), forged.clone());
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &forged.hash,
            &bad,
            &CADENCE,
        )
        .is_err());
    }
}
