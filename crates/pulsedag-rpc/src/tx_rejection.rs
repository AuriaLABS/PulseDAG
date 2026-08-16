use pulsedag_core::{
    classify_transaction_version, classify_typed_transaction_error, PulseError,
    TransactionRejectionClass, TransactionValidationPath,
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
