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

fn conflicts_with_live_mempool(tx: &Transaction, state: &ChainState) -> bool {
    tx.inputs.iter().any(|input| {
        state
            .mempool
            .spent_outpoints
            .contains(&input.previous_output)
    })
}

fn normalize_live_result_v3(
    result: TxAcceptanceResult,
    had_mempool_conflict: bool,
) -> TxAcceptanceResult {
    match result {
        TxAcceptanceResult::Invalid(reason) if had_mempool_conflict && reason == "double spend" => {
            TxAcceptanceResult::Rejected(mempool_policy_rejection_reason_v3(
                MempoolPolicyRejectionV3::ReplacementNotAuthorized,
                "conflicting mempool spend; replacement semantics are not authorized",
            ))
        }
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
    let had_mempool_conflict = conflicts_with_live_mempool(&tx, state);
    normalize_live_result_v3(
        accept_transaction_with_result(tx, state, source),
        had_mempool_conflict,
    )
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
    let had_mempool_conflict = conflicts_with_live_mempool(&tx, state);
    normalize_live_result_v3(
        accept_transaction_with_result_for_protocol(tx, state, source, identity),
        had_mempool_conflict,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{genesis::init_chain_state, types::TxOutput, TRANSACTION_VERSION_V1};

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
}
