use std::collections::BTreeSet;

use crate::{
    mempool_admission_v3::classify_mempool_conflicts_v3,
    mempool_v3::{
        canonical_transaction_size_for_mempool_v3, fee_rate_v3, MempoolPolicyAssessmentErrorV3,
        FEE_RATE_SCALE_BYTES_V3,
    },
    state::ChainState,
    types::Transaction,
};

/// Versioned, read-only assessment of a potential mempool replacement.
///
/// This is intentionally not an authorization result. Live v3 admission remains
/// fail-closed for every conflict until a later activation contract explicitly
/// enables replacement semantics.
pub const MEMPOOL_REPLACEMENT_ASSESSMENT_V3_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolReplacementAssessmentV3 {
    pub version: u32,
    pub direct_conflict_txids: Vec<String>,
    pub replacement_package_txids: Vec<String>,
    pub replacement_package_total_fee: u128,
    pub replacement_package_total_size_bytes: u128,
    pub replacement_package_fee_rate_per_kb: u128,
    pub incoming_fee: u64,
    pub incoming_size_bytes: u64,
    pub incoming_fee_rate_per_kb: u128,
    /// Positive absolute fee delta over the entire replacement package.
    /// `None` means the incoming fee is not strictly greater.
    pub positive_fee_delta_over_package: Option<u128>,
    pub pays_strictly_higher_total_fee: bool,
    pub pays_strictly_higher_fee_rate: bool,
    /// Live mempool parents used by the incoming transaction that are not part
    /// of the replacement package. Sorted and unique for deterministic output.
    pub new_unconfirmed_parent_txids: Vec<String>,
    /// True when the incoming transaction spends an output of a transaction
    /// that its own replacement package would remove.
    pub depends_on_replacement_package: bool,
}

/// Build a deterministic, mutation-free assessment for a potential replacement.
///
/// The assessment deliberately does not inspect `replacement_enabled`, mutate
/// mempool state, or return an "authorized" bit. It supplies stable inputs for a
/// later RBF protocol freeze while current live admission continues to reject
/// conflicts with `MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED`.
pub fn assess_mempool_replacement_v3(
    tx: &Transaction,
    state: &ChainState,
) -> Result<MempoolReplacementAssessmentV3, MempoolPolicyAssessmentErrorV3> {
    let conflicts = classify_mempool_conflicts_v3(tx, state);
    let replacement_package = conflicts
        .conflict_package_txids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut replacement_package_total_fee = 0_u128;
    let mut replacement_package_total_size_bytes = 0_u128;
    for txid in &conflicts.conflict_package_txids {
        let Some(existing) = state.mempool.transactions.get(txid) else {
            // The classifier derives its package from this same immutable state,
            // so a missing member is not expected. Keep the assessment fail-safe
            // and deterministic rather than introducing a mutation-time race path.
            continue;
        };
        replacement_package_total_fee =
            replacement_package_total_fee.saturating_add(u128::from(existing.fee));
        replacement_package_total_size_bytes =
            replacement_package_total_size_bytes.saturating_add(u128::from(
                canonical_transaction_size_for_mempool_v3(existing, &state.chain_id)?,
            ));
    }

    let replacement_package_fee_rate_per_kb = if replacement_package_total_size_bytes == 0 {
        0
    } else {
        replacement_package_total_fee.saturating_mul(u128::from(FEE_RATE_SCALE_BYTES_V3))
            / replacement_package_total_size_bytes
    };

    let incoming_rate = fee_rate_v3(tx, &state.chain_id)?;
    let incoming_fee_u128 = u128::from(tx.fee);
    let positive_fee_delta_over_package = incoming_fee_u128
        .checked_sub(replacement_package_total_fee)
        .filter(|delta| *delta > 0);

    let mut new_unconfirmed_parent_txids = BTreeSet::new();
    let mut depends_on_replacement_package = false;
    for input in &tx.inputs {
        let parent_txid = &input.previous_output.txid;
        if !state.mempool.transactions.contains_key(parent_txid) {
            continue;
        }
        if replacement_package.contains(parent_txid) {
            depends_on_replacement_package = true;
        } else {
            new_unconfirmed_parent_txids.insert(parent_txid.clone());
        }
    }

    Ok(MempoolReplacementAssessmentV3 {
        version: MEMPOOL_REPLACEMENT_ASSESSMENT_V3_VERSION,
        direct_conflict_txids: conflicts.direct_conflict_txids,
        replacement_package_txids: conflicts.conflict_package_txids,
        replacement_package_total_fee,
        replacement_package_total_size_bytes,
        replacement_package_fee_rate_per_kb,
        incoming_fee: tx.fee,
        incoming_size_bytes: incoming_rate.canonical_size_bytes,
        incoming_fee_rate_per_kb: incoming_rate.fee_per_kb,
        positive_fee_delta_over_package,
        pays_strictly_higher_total_fee: incoming_fee_u128 > replacement_package_total_fee,
        pays_strictly_higher_fee_rate: incoming_rate.fee_per_kb
            > replacement_package_fee_rate_per_kb,
        new_unconfirmed_parent_txids: new_unconfirmed_parent_txids.into_iter().collect(),
        depends_on_replacement_package,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mempool_admission_v3::accept_transaction_with_mempool_policy_v3,
        mempool_v3::MempoolPolicyV3,
        types::{OutPoint, Transaction, TxInput, TxOutput},
        AcceptSource, TxAcceptanceResult, TRANSACTION_VERSION_V1,
    };

    fn tx(txid: &str, inputs: Vec<OutPoint>, fee: u64, nonce: u64) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: TRANSACTION_VERSION_V1,
            inputs: inputs
                .into_iter()
                .map(|previous_output| TxInput {
                    previous_output,
                    public_key: String::new(),
                    signature: String::new(),
                })
                .collect(),
            outputs: vec![TxOutput {
                address: format!("pulse1-{txid}"),
                amount: 1,
            }],
            fee,
            nonce,
        }
    }

    fn insert_live(state: &mut ChainState, transaction: Transaction, first_seen: u64) {
        let txid = transaction.txid.clone();
        for input in &transaction.inputs {
            state
                .mempool
                .spent_outpoints
                .insert(input.previous_output.clone());
        }
        state.mempool.transactions.insert(txid.clone(), transaction);
        state.mempool.first_seen.insert(txid.clone(), first_seen);
        state.mempool.admission_height.insert(txid, 100);
        state.mempool.next_first_seen = state.mempool.next_first_seen.max(first_seen + 1);
    }

    fn fixture(reverse: bool) -> (ChainState, Transaction) {
        let mut state = init_chain_state("rbf-assessment-v3".to_string());
        let external_a = OutPoint {
            txid: "external-a".to_string(),
            index: 0,
        };
        let external_b = OutPoint {
            txid: "external-b".to_string(),
            index: 0,
        };
        let anchor_external = OutPoint {
            txid: "external-anchor".to_string(),
            index: 0,
        };

        let direct_a = tx("direct-a", vec![external_a.clone()], 3, 1);
        let direct_b = tx("direct-b", vec![external_b.clone()], 5, 2);
        let child = tx(
            "child-a",
            vec![OutPoint {
                txid: direct_a.txid.clone(),
                index: 0,
            }],
            2,
            3,
        );
        let anchor = tx("anchor", vec![anchor_external], 1, 4);

        let entries = vec![
            (direct_a, 1_u64),
            (direct_b, 2_u64),
            (child, 3_u64),
            (anchor, 4_u64),
        ];
        if reverse {
            for (transaction, first_seen) in entries.into_iter().rev() {
                insert_live(&mut state, transaction, first_seen);
            }
        } else {
            for (transaction, first_seen) in entries {
                insert_live(&mut state, transaction, first_seen);
            }
        }

        let incoming = tx("incoming", vec![external_b, external_a], 1_000_000, 9);
        (state, incoming)
    }

    #[test]
    fn assessment_is_identical_across_equivalent_hashmap_orders() {
        let (state_a, incoming_a) = fixture(false);
        let (state_b, incoming_b) = fixture(true);

        let a = assess_mempool_replacement_v3(&incoming_a, &state_a).unwrap();
        let b = assess_mempool_replacement_v3(&incoming_b, &state_b).unwrap();

        assert_eq!(a, b);
        assert_eq!(
            a.direct_conflict_txids,
            vec!["direct-a".to_string(), "direct-b".to_string()]
        );
        assert_eq!(
            a.replacement_package_txids,
            vec![
                "child-a".to_string(),
                "direct-a".to_string(),
                "direct-b".to_string(),
            ]
        );
        assert_eq!(a.replacement_package_total_fee, 10);
        assert!(a.replacement_package_total_size_bytes > 0);
        assert_eq!(a.incoming_fee, 1_000_000);
        assert!(a.pays_strictly_higher_total_fee);
        assert!(a.pays_strictly_higher_fee_rate);
        assert_eq!(a.positive_fee_delta_over_package, Some(999_990));
        assert!(a.new_unconfirmed_parent_txids.is_empty());
        assert!(!a.depends_on_replacement_package);
    }

    #[test]
    fn assessment_reports_new_unconfirmed_parents_without_authorizing_them() {
        let (state, mut incoming) = fixture(false);
        incoming.inputs.push(TxInput {
            previous_output: OutPoint {
                txid: "anchor".to_string(),
                index: 0,
            },
            public_key: String::new(),
            signature: String::new(),
        });

        let assessment = assess_mempool_replacement_v3(&incoming, &state).unwrap();
        assert_eq!(
            assessment.new_unconfirmed_parent_txids,
            vec!["anchor".to_string()]
        );
        assert!(!assessment.depends_on_replacement_package);
    }

    #[test]
    fn assessment_reports_dependency_on_transaction_that_would_be_evicted() {
        let (state, mut incoming) = fixture(false);
        incoming.inputs.push(TxInput {
            previous_output: OutPoint {
                txid: "direct-a".to_string(),
                index: 0,
            },
            public_key: String::new(),
            signature: String::new(),
        });

        let assessment = assess_mempool_replacement_v3(&incoming, &state).unwrap();
        assert!(assessment.depends_on_replacement_package);
        assert!(assessment.new_unconfirmed_parent_txids.is_empty());
    }

    #[test]
    fn no_conflict_produces_an_empty_replacement_package() {
        let (state, _) = fixture(false);
        let incoming = tx(
            "unrelated-incoming",
            vec![OutPoint {
                txid: "external-unrelated".to_string(),
                index: 0,
            }],
            7,
            10,
        );

        let assessment = assess_mempool_replacement_v3(&incoming, &state).unwrap();
        assert!(assessment.direct_conflict_txids.is_empty());
        assert!(assessment.replacement_package_txids.is_empty());
        assert_eq!(assessment.replacement_package_total_fee, 0);
        assert_eq!(assessment.replacement_package_total_size_bytes, 0);
        assert_eq!(assessment.replacement_package_fee_rate_per_kb, 0);
        assert_eq!(assessment.positive_fee_delta_over_package, Some(7));
    }

    #[test]
    fn pure_assessment_does_not_change_live_fail_closed_admission() {
        let (mut state, incoming) = fixture(false);
        let before = state.mempool.transactions.len();
        let assessment = assess_mempool_replacement_v3(&incoming, &state).unwrap();
        assert!(!assessment.direct_conflict_txids.is_empty());
        assert_eq!(state.mempool.transactions.len(), before);

        let policy = MempoolPolicyV3 {
            replacement_enabled: true,
            ..MempoolPolicyV3::compatibility_default()
        };
        let result = accept_transaction_with_mempool_policy_v3(
            incoming,
            &mut state,
            AcceptSource::Rpc,
            policy,
        );
        match result {
            TxAcceptanceResult::Rejected(reason) => {
                assert!(reason.starts_with("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED"));
            }
            other => panic!("expected fail-closed conflict rejection, got {other:?}"),
        }
        assert_eq!(state.mempool.transactions.len(), before);
    }
}
