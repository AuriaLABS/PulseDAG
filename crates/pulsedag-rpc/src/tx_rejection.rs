use pulsedag_core::{
    classify_transaction_version, classify_typed_transaction_error,
    tx_protocol::{resolve_transaction_validation_path, validate_transaction_for_protocol},
    validation::validate_transaction,
    ChainState, ProtocolActivationIdentity, PulseError, Transaction, TransactionRejectionClass,
    TransactionValidationPath, TxAcceptanceResult,
};

/// Classify a rejection from the currently-live legacy transaction admission
/// path without parsing human-readable error text.
///
/// Version classification runs first so a known v2 transaction presented to
/// the legacy path is reported as inactive even if a later legacy validator
/// would produce a different error. Unknown versions are unsupported. Active
/// v1 failures then use typed core errors; generic `InvalidTransaction` is
/// classified by variant as malformed, never by inspecting its message.
pub fn classify_legacy_rpc_transaction_rejection(
    transaction_version: u32,
    error: &PulseError,
) -> Option<TransactionRejectionClass> {
    if let Err(class) =
        classify_transaction_version(TransactionValidationPath::LegacyV1, transaction_version)
    {
        return Some(class);
    }

    classify_typed_transaction_error(error).or(match error {
        PulseError::InvalidTransaction(_) => Some(TransactionRejectionClass::MalformedTransaction),
        _ => None,
    })
}

fn classify_transaction_validation_error(error: &PulseError) -> Option<TransactionRejectionClass> {
    classify_typed_transaction_error(error).or(match error {
        PulseError::InvalidTransaction(_) => Some(TransactionRejectionClass::MalformedTransaction),
        _ => None,
    })
}

/// Classify a concrete RPC admission result without parsing human-readable
/// rejection text. Structural mempool outcomes win first; version and
/// validator classification then recover typed v1/v2 validation failures.
/// A `Rejected` result after successful validation is the bounded mempool
/// capacity/backpressure outcome produced by the core admission path.
pub fn classify_rpc_transaction_acceptance(
    transaction: &Transaction,
    chain: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
    result: &TxAcceptanceResult,
) -> Option<TransactionRejectionClass> {
    match result {
        TxAcceptanceResult::Accepted => return None,
        TxAcceptanceResult::Duplicate => return Some(TransactionRejectionClass::Duplicate),
        TxAcceptanceResult::Orphan => return Some(TransactionRejectionClass::Orphan),
        TxAcceptanceResult::Invalid(_) | TxAcceptanceResult::Rejected(_) => {}
    }

    let path = match identity {
        Some(identity) => match resolve_transaction_validation_path(identity, chain) {
            Ok(path) => path,
            Err(error) => return classify_typed_transaction_error(&error),
        },
        None => TransactionValidationPath::LegacyV1,
    };

    if let Err(classification) = classify_transaction_version(path, transaction.version) {
        return Some(classification);
    }

    let validation = match (path, identity) {
        (TransactionValidationPath::LegacyV1, _) => validate_transaction(transaction, chain),
        (TransactionValidationPath::ActivatedV2, Some(identity)) => {
            validate_transaction_for_protocol(transaction, chain, identity)
        }
        (TransactionValidationPath::ActivatedV2, None) => return None,
    };

    match validation {
        Err(error) => classify_transaction_validation_error(&error),
        Ok(()) if matches!(result, TxAcceptanceResult::Rejected(_)) => {
            Some(TransactionRejectionClass::MempoolFull)
        }
        Ok(()) if matches!(result, TxAcceptanceResult::Invalid(_)) => {
            Some(TransactionRejectionClass::MalformedTransaction)
        }
        Ok(()) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_v2_is_inactive_before_legacy_error_classification() {
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(2, &PulseError::InvalidSignature),
            Some(TransactionRejectionClass::InactiveTransactionVersion)
        );
    }

    #[test]
    fn unknown_versions_are_unsupported_before_legacy_error_classification() {
        for version in [0, 3, u32::MAX] {
            assert_eq!(
                classify_legacy_rpc_transaction_rejection(version, &PulseError::InvalidSignature),
                Some(TransactionRejectionClass::UnsupportedTransactionVersion)
            );
        }
    }

    #[test]
    fn active_v1_uses_typed_core_rejection_classes() {
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(1, &PulseError::InvalidSignature),
            Some(TransactionRejectionClass::InvalidSignature)
        );
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(1, &PulseError::TxAlreadyExists),
            Some(TransactionRejectionClass::Duplicate)
        );
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(1, &PulseError::DoubleSpend),
            Some(TransactionRejectionClass::Conflict)
        );
    }

    #[test]
    fn generic_invalid_transaction_is_malformed_by_variant_not_message() {
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(
                1,
                &PulseError::InvalidTransaction("mempool full maybe".to_string()),
            ),
            Some(TransactionRejectionClass::MalformedTransaction)
        );
    }

    #[test]
    fn unrelated_internal_failures_remain_unclassified() {
        assert_eq!(
            classify_legacy_rpc_transaction_rejection(
                1,
                &PulseError::Internal("storage unavailable".to_string()),
            ),
            None
        );
    }
}
