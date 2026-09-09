use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    accept::{
        accept_transaction_with_result, accept_transaction_with_result_for_protocol, AcceptSource,
        TxAcceptanceResult,
    },
    mempool_v3::{
        fee_rate_v3, MempoolPolicyRejectionV3, MempoolPolicyV3, MEMPOOL_POLICY_V3_VERSION,
    },
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    types::Transaction,
};

const POLICY_REASON_SEPARATOR_V3: &str = ": ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolConflictPackageV3 {
    pub direct_conflict_txids: Vec<String>,
    pub conflict_package_txids: Vec<String>,
}

pub fn mempool_policy_rejection_reason_v3(
    rejection: MempoolPolicyRejectionV3,
    detail: impl AsRef<str>,
) -> String {
    format!(
        "{}{}{}",
        rejection.as_code(),
        POLICY_REASON_SEPARATOR_V3,
        detail.as_ref()
    )
}

pub fn mempool_policy_rejection_code_from_reason_v3(reason: &str) -> Option<&str> {
    let code = reason
        .split_once(POLICY_REASON_SEPARATOR_V3)
        .map(|(code, _)| code)
        .unwrap_or(reason);
    match code {
        "MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE"
        | "MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE"
        | "MEMPOOL_V3_CAPACITY_BACKPRESSURE"
        | "MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED"
        | "MEMPOOL_V3_POLICY_IDENTITY_MISMATCH" => Some(code),
        _ => None,
    }
}

pub fn mempool_policy_rejection_detail_v3(reason: &str) -> &str {
    reason
        .split_once(POLICY_REASON_SEPARATOR_V3)
        .map(|(_, detail)| detail)
        .unwrap_or(reason)
}

fn policy_rejected(
    state: &mut ChainState,
    rejection: MempoolPolicyRejectionV3,
    detail: impl AsRef<str>,
) -> TxAcceptanceResult {
    state.mempool.counters.rejected_total = state.mempool.counters.rejected_total.saturating_add(1);
    TxAcceptanceResult::Rejected(mempool_policy_rejection_reason_v3(rejection, detail))
}

fn preflight_policy_v3(
    tx: &Transaction,
    state: &mut ChainState,
    policy: MempoolPolicyV3,
) -> Result<(), TxAcceptanceResult> {
    if policy.version != MEMPOOL_POLICY_V3_VERSION {
        return Err(policy_rejected(
            state,
            MempoolPolicyRejectionV3::PolicyIdentityMismatch,
            format!(
                "policy version {} does not match supported version {}",
                policy.version, MEMPOOL_POLICY_V3_VERSION
            ),
        ));
    }

    if tx.fee > policy.max_transaction_fee {
        return Err(policy_rejected(
            state,
            MempoolPolicyRejectionV3::AboveMaximumTransactionFee,
            format!(
                "transaction fee {} exceeds policy maximum {}",
                tx.fee, policy.max_transaction_fee
            ),
        ));
    }

    let fee_rate = match fee_rate_v3(tx, &state.chain_id) {
        Ok(rate) => rate,
        Err(error) => {
            state.mempool.counters.rejected_total =
                state.mempool.counters.rejected_total.saturating_add(1);
            return Err(TxAcceptanceResult::Invalid(format!(
                "mempool v3 canonical policy assessment failed: {error:?}"
            )));
        }
    };
    if fee_rate.fee_per_kb < u128::from(policy.min_relay_fee_rate_per_kb) {
        return Err(policy_rejected(
            state,
            MempoolPolicyRejectionV3::BelowMinimumRelayFeeRate,
            format!(
                "fee rate {} is below policy minimum {}",
                fee_rate.fee_per_kb, policy.min_relay_fee_rate_per_kb
            ),
        ));
    }

    // Preserve the existing package-aware eviction path when the policy
    // limit matches or exceeds the live mempool cap. A stricter explicit
    // policy is allowed to fail closed before the legacy cap is reached.
    let live_cap = u64::try_from(state.mempool.max_transactions).unwrap_or(u64::MAX);
    let current = u64::try_from(state.mempool.transactions.len()).unwrap_or(u64::MAX);
    if policy.max_transactions < live_cap && current >= policy.max_transactions {
        state.mempool.counters.pressure_events_total = state
            .mempool
            .counters
            .pressure_events_total
            .saturating_add(1);
        return Err(policy_rejected(
            state,
            MempoolPolicyRejectionV3::CapacityBackpressure,
            format!(
                "policy transaction capacity reached (used={} max={})",
                current, policy.max_transactions
            ),
        ));
    }

    Ok(())
}

/// Canonical read-only conflict graph for v3 admission.
///
/// Direct conflicts are live mempool transactions that spend at least one
/// `OutPoint` also spent by the incoming transaction. The conflict package is
/// the direct set plus every live in-mempool descendant reachable from it.
/// Both vectors are unique and lexicographically sorted so equivalent mempool
/// states produce identical classification independent of HashMap iteration.
pub fn classify_mempool_conflicts_v3(
    tx: &Transaction,
    state: &ChainState,
) -> MempoolConflictPackageV3 {
    let direct_conflicts = state
        .mempool
        .transactions
        .values()
        .filter(|existing| {
            existing.txid != tx.txid
                && existing.inputs.iter().any(|existing_input| {
                    tx.inputs.iter().any(|incoming_input| {
                        incoming_input.previous_output == existing_input.previous_output
                    })
                })
        })
        .map(|existing| existing.txid.clone())
        .collect::<BTreeSet<_>>();

    let mut children = BTreeMap::<String, BTreeSet<String>>::new();
    for existing in state.mempool.transactions.values() {
        for input in &existing.inputs {
            let parent_txid = &input.previous_output.txid;
            if state.mempool.transactions.contains_key(parent_txid) {
                children
                    .entry(parent_txid.clone())
                    .or_default()
                    .insert(existing.txid.clone());
            }
        }
    }

    let mut conflict_package = direct_conflicts.clone();
    let mut pending = direct_conflicts.iter().cloned().collect::<VecDeque<_>>();
    while let Some(txid) = pending.pop_front() {
        if let Some(descendants) = children.get(&txid) {
            for descendant in descendants {
                if conflict_package.insert(descendant.clone()) {
                    pending.push_back(descendant.clone());
                }
            }
        }
    }

    MempoolConflictPackageV3 {
        direct_conflict_txids: direct_conflicts.into_iter().collect(),
        conflict_package_txids: conflict_package.into_iter().collect(),
    }
}

fn reject_conflicting_mempool_package_v3(
    tx: &Transaction,
    state: &mut ChainState,
) -> Option<TxAcceptanceResult> {
    let conflicts = classify_mempool_conflicts_v3(tx, state);
    if conflicts.direct_conflict_txids.is_empty() {
        return None;
    }

    Some(policy_rejected(
        state,
        MempoolPolicyRejectionV3::ReplacementNotAuthorized,
        format!(
            "direct_conflicts=[{}] conflict_package=[{}]; replacement semantics are not authorized",
            conflicts.direct_conflict_txids.join(","),
            conflicts.conflict_package_txids.join(",")
        ),
    ))
}

fn normalize_live_result_v3(result: TxAcceptanceResult) -> TxAcceptanceResult {
    match result {
        TxAcceptanceResult::Rejected(reason)
            if reason.contains("mempool backpressure")
                || reason.contains("spent-outpoint capacity")
                || reason.contains("mempool pressure detected") =>
        {
            TxAcceptanceResult::Rejected(mempool_policy_rejection_reason_v3(
                MempoolPolicyRejectionV3::CapacityBackpressure,
                reason,
            ))
        }
        other => other,
    }
}

pub fn accept_transaction_with_mempool_policy_v3(
    tx: Transaction,
    state: &mut ChainState,
    source: AcceptSource,
    policy: MempoolPolicyV3,
) -> TxAcceptanceResult {
    if let Err(result) = preflight_policy_v3(&tx, state, policy) {
        return result;
    }
    if let Some(result) = reject_conflicting_mempool_package_v3(&tx, state) {
        return result;
    }
    normalize_live_result_v3(accept_transaction_with_result(tx, state, source))
}

pub fn accept_transaction_with_mempool_policy_v3_for_protocol(
    tx: Transaction,
    state: &mut ChainState,
    source: AcceptSource,
    identity: &ProtocolActivationIdentity,
    policy: MempoolPolicyV3,
) -> TxAcceptanceResult {
    if let Err(result) = preflight_policy_v3(&tx, state, policy) {
        return result;
    }
    if let Some(result) = reject_conflicting_mempool_package_v3(&tx, state) {
        return result;
    }
    normalize_live_result_v3(accept_transaction_with_result_for_protocol(
        tx, state, source, identity,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mempool::canonical_mempool_txids,
        types::{OutPoint, Transaction, TxInput, TxOutput},
        TRANSACTION_VERSION_V1,
    };

    fn sample_tx(fee: u64) -> Transaction {
        Transaction {
            txid: "live-policy-vector".to_string(),
            version: TRANSACTION_VERSION_V1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "pulse1livepolicy".to_string(),
                amount: 7,
            }],
            fee,
            nonce: 41,
        }
    }

    fn test_tx(txid: &str, inputs: Vec<OutPoint>) -> Transaction {
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
            fee: 0,
            nonce: 0,
        }
    }

    fn insert_live(
        state: &mut ChainState,
        transaction: Transaction,
        first_seen: u64,
        admission_height: u64,
    ) {
        let txid = transaction.txid.clone();
        for input in &transaction.inputs {
            state
                .mempool
                .spent_outpoints
                .insert(input.previous_output.clone());
        }
        state.mempool.transactions.insert(txid.clone(), transaction);
        state.mempool.first_seen.insert(txid.clone(), first_seen);
        state
            .mempool
            .admission_height
            .insert(txid, admission_height);
        state.mempool.next_first_seen = state.mempool.next_first_seen.max(first_seen + 1);
    }

    fn conflict_fixture(reverse: bool) -> (ChainState, Transaction) {
        let mut state = init_chain_state("mempool-v3-conflict-package".to_string());
        let shared = OutPoint {
            txid: "external-shared".to_string(),
            index: 0,
        };
        let entries = vec![
            (test_tx("direct-a", vec![shared.clone()]), 1_u64),
            (test_tx("direct-b", vec![shared.clone()]), 2_u64),
            (
                test_tx(
                    "child-a",
                    vec![OutPoint {
                        txid: "direct-a".to_string(),
                        index: 0,
                    }],
                ),
                3_u64,
            ),
            (
                test_tx(
                    "shared-child",
                    vec![
                        OutPoint {
                            txid: "direct-a".to_string(),
                            index: 0,
                        },
                        OutPoint {
                            txid: "direct-b".to_string(),
                            index: 0,
                        },
                    ],
                ),
                4_u64,
            ),
            (
                test_tx(
                    "grandchild",
                    vec![OutPoint {
                        txid: "shared-child".to_string(),
                        index: 0,
                    }],
                ),
                5_u64,
            ),
            (
                test_tx(
                    "unrelated-root",
                    vec![OutPoint {
                        txid: "external-unrelated".to_string(),
                        index: 0,
                    }],
                ),
                6_u64,
            ),
            (
                test_tx(
                    "unrelated-child",
                    vec![OutPoint {
                        txid: "unrelated-root".to_string(),
                        index: 0,
                    }],
                ),
                7_u64,
            ),
        ];

        if reverse {
            for (transaction, first_seen) in entries.into_iter().rev() {
                insert_live(&mut state, transaction, first_seen, 10 + first_seen);
            }
        } else {
            for (transaction, first_seen) in entries {
                insert_live(&mut state, transaction, first_seen, 10 + first_seen);
            }
        }

        let incoming = test_tx("incoming-conflict", vec![shared]);
        (state, incoming)
    }

    fn serialized_transaction_map(
        transactions: &std::collections::HashMap<String, Transaction>,
    ) -> std::collections::BTreeMap<String, Vec<u8>> {
        transactions
            .iter()
            .map(|(txid, tx)| {
                (
                    txid.clone(),
                    bincode::serialize(tx).expect("transaction serializes"),
                )
            })
            .collect()
    }

    fn assert_conflict_rejection_preserves_mempool(before: &ChainState, after: &ChainState) {
        assert_eq!(
            serialized_transaction_map(&after.mempool.transactions),
            serialized_transaction_map(&before.mempool.transactions)
        );
        assert_eq!(
            after.mempool.spent_outpoints,
            before.mempool.spent_outpoints
        );
        assert_eq!(after.mempool.first_seen, before.mempool.first_seen);
        assert_eq!(
            after.mempool.admission_height,
            before.mempool.admission_height
        );
        assert_eq!(
            after.mempool.next_first_seen,
            before.mempool.next_first_seen
        );
        assert_eq!(
            serialized_transaction_map(&after.mempool.orphan_transactions),
            serialized_transaction_map(&before.mempool.orphan_transactions)
        );
        assert_eq!(
            after.mempool.orphan_missing_outpoints,
            before.mempool.orphan_missing_outpoints
        );
        assert_eq!(
            after.mempool.orphan_received_order,
            before.mempool.orphan_received_order
        );
        assert_eq!(
            after.mempool.next_orphan_order,
            before.mempool.next_orphan_order
        );
        assert_eq!(
            after.mempool.max_transactions,
            before.mempool.max_transactions
        );
        assert_eq!(
            after.mempool.max_spent_outpoints,
            before.mempool.max_spent_outpoints
        );
        assert_eq!(after.mempool.max_orphans, before.mempool.max_orphans);
        assert_eq!(
            after.mempool.counters.rejected_total,
            before.mempool.counters.rejected_total.saturating_add(1)
        );
    }

    fn rejected_reason(result: TxAcceptanceResult) -> String {
        match result {
            TxAcceptanceResult::Rejected(reason) => reason,
            other => panic!("expected policy rejection, got {other:?}"),
        }
    }

    #[test]
    fn stable_policy_reason_round_trips_machine_code_and_detail() {
        let reason = mempool_policy_rejection_reason_v3(
            MempoolPolicyRejectionV3::CapacityBackpressure,
            "bounded queue full",
        );
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&reason),
            Some("MEMPOOL_V3_CAPACITY_BACKPRESSURE")
        );
        assert_eq!(
            mempool_policy_rejection_detail_v3(&reason),
            "bounded queue full"
        );
        assert_eq!(mempool_policy_rejection_code_from_reason_v3("legacy"), None);
    }

    #[test]
    fn explicit_fee_ceiling_rejects_before_mempool_mutation() {
        let mut state = init_chain_state("live-policy-max-fee".to_string());
        let policy = MempoolPolicyV3 {
            max_transaction_fee: 4,
            ..MempoolPolicyV3::compatibility_default()
        };
        let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
            sample_tx(5),
            &mut state,
            AcceptSource::Rpc,
            policy,
        ));
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&reason),
            Some("MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE")
        );
        assert!(state.mempool.transactions.is_empty());
        assert!(state.mempool.orphan_transactions.is_empty());
    }

    #[test]
    fn explicit_relay_floor_rejects_before_mempool_mutation() {
        let mut state = init_chain_state("live-policy-relay-floor".to_string());
        let policy = MempoolPolicyV3 {
            min_relay_fee_rate_per_kb: u64::MAX,
            ..MempoolPolicyV3::compatibility_default()
        };
        let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
            sample_tx(1),
            &mut state,
            AcceptSource::Rpc,
            policy,
        ));
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&reason),
            Some("MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE")
        );
        assert!(state.mempool.transactions.is_empty());
    }

    #[test]
    fn stricter_explicit_capacity_fails_closed_without_changing_live_cap() {
        let mut state = init_chain_state("live-policy-capacity".to_string());
        let original_cap = state.mempool.max_transactions;
        let policy = MempoolPolicyV3 {
            max_transactions: 0,
            ..MempoolPolicyV3::compatibility_default()
        };
        let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
            sample_tx(0),
            &mut state,
            AcceptSource::Rpc,
            policy,
        ));
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&reason),
            Some("MEMPOOL_V3_CAPACITY_BACKPRESSURE")
        );
        assert_eq!(state.mempool.max_transactions, original_cap);
        assert!(state.mempool.transactions.is_empty());
    }

    #[test]
    fn unsupported_policy_version_fails_closed() {
        let mut state = init_chain_state("live-policy-version".to_string());
        let policy = MempoolPolicyV3 {
            version: MEMPOOL_POLICY_V3_VERSION + 1,
            ..MempoolPolicyV3::compatibility_default()
        };
        let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
            sample_tx(0),
            &mut state,
            AcceptSource::Rpc,
            policy,
        ));
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&reason),
            Some("MEMPOOL_V3_POLICY_IDENTITY_MISMATCH")
        );
        assert!(state.mempool.transactions.is_empty());
    }

    #[test]
    fn conflict_classifier_is_canonical_and_descendant_closed() {
        let (forward, incoming) = conflict_fixture(false);
        let (reverse, _) = conflict_fixture(true);

        let forward_classification = classify_mempool_conflicts_v3(&incoming, &forward);
        let reverse_classification = classify_mempool_conflicts_v3(&incoming, &reverse);
        assert_eq!(forward_classification, reverse_classification);
        assert_eq!(
            forward_classification.direct_conflict_txids,
            vec!["direct-a".to_string(), "direct-b".to_string()]
        );
        assert_eq!(
            forward_classification.conflict_package_txids,
            vec![
                "child-a".to_string(),
                "direct-a".to_string(),
                "direct-b".to_string(),
                "grandchild".to_string(),
                "shared-child".to_string(),
            ]
        );
        assert!(!forward_classification
            .conflict_package_txids
            .contains(&"unrelated-root".to_string()));
        assert!(!forward_classification
            .conflict_package_txids
            .contains(&"unrelated-child".to_string()));
    }

    #[test]
    fn exact_duplicate_preserves_duplicate_precedence_over_conflict_classification() {
        let mut state = init_chain_state("mempool-v3-duplicate-precedence".to_string());
        let shared = OutPoint {
            txid: "external-duplicate".to_string(),
            index: 0,
        };
        let existing = test_tx("same-txid", vec![shared]);
        insert_live(&mut state, existing.clone(), 0, 0);

        let classification = classify_mempool_conflicts_v3(&existing, &state);
        assert!(classification.direct_conflict_txids.is_empty());
        assert!(classification.conflict_package_txids.is_empty());
        assert_eq!(
            accept_transaction_with_mempool_policy_v3(
                existing,
                &mut state,
                AcceptSource::Rpc,
                MempoolPolicyV3::compatibility_default(),
            ),
            TxAcceptanceResult::Duplicate
        );
    }

    #[test]
    fn conflict_rejection_is_pre_mutation_and_protocol_parity_even_when_replacement_enabled() {
        let (initial, incoming) = conflict_fixture(false);
        let policy = MempoolPolicyV3 {
            replacement_enabled: true,
            ..MempoolPolicyV3::compatibility_default()
        };

        let mut standard = initial.clone();
        let standard_before = standard.clone();
        let standard_reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
            incoming.clone(),
            &mut standard,
            AcceptSource::Rpc,
            policy,
        ));
        assert_eq!(
            mempool_policy_rejection_code_from_reason_v3(&standard_reason),
            Some("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED")
        );
        assert_conflict_rejection_preserves_mempool(&standard_before, &standard);

        let mut protocol = initial.clone();
        let protocol_before = protocol.clone();
        let identity = ProtocolActivationIdentity::legacy_from_state(&protocol);
        let protocol_reason =
            rejected_reason(accept_transaction_with_mempool_policy_v3_for_protocol(
                incoming,
                &mut protocol,
                AcceptSource::Rpc,
                &identity,
                policy,
            ));
        assert_eq!(standard_reason, protocol_reason);
        assert_conflict_rejection_preserves_mempool(&protocol_before, &protocol);
        assert_eq!(
            classify_mempool_conflicts_v3(
                &test_tx(
                    "incoming-conflict",
                    vec![OutPoint {
                        txid: "external-shared".to_string(),
                        index: 0,
                    }],
                ),
                &standard_before
            ),
            classify_mempool_conflicts_v3(
                &test_tx(
                    "incoming-conflict",
                    vec![OutPoint {
                        txid: "external-shared".to_string(),
                        index: 0,
                    }],
                ),
                &protocol_before
            )
        );
        assert_eq!(
            canonical_mempool_txids(&standard),
            canonical_mempool_txids(&protocol)
        );
    }
}
