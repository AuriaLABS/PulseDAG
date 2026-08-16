use crate::{errors::PulseError, tx_protocol::TransactionValidationPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionRejectionClass {
    UnsupportedTransactionVersion,
    InactiveTransactionVersion,
    WrongChainDomain,
    InvalidTxid,
    InvalidSignature,
    Duplicate,
    Conflict,
    Orphan,
    MalformedTransaction,
    InsufficientFunds,
    MempoolFull,
}

impl TransactionRejectionClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedTransactionVersion => "unsupported_transaction_version",
            Self::InactiveTransactionVersion => "inactive_transaction_version",
            Self::WrongChainDomain => "wrong_chain_domain",
            Self::InvalidTxid => "invalid_txid",
            Self::InvalidSignature => "invalid_signature",
            Self::Duplicate => "duplicate",
            Self::Conflict => "conflict",
            Self::Orphan => "orphan",
            Self::MalformedTransaction => "malformed_transaction",
            Self::InsufficientFunds => "insufficient_funds",
            Self::MempoolFull => "mempool_full",
        }
    }
}

pub fn classify_transaction_version(
    path: TransactionValidationPath,
    version: u32,
) -> Result<(), TransactionRejectionClass> {
    match (path, version) {
        (TransactionValidationPath::LegacyV1, 1) | (TransactionValidationPath::ActivatedV2, 2) => {
            Ok(())
        }
        (TransactionValidationPath::LegacyV1, 2) | (TransactionValidationPath::ActivatedV2, 1) => {
            Err(TransactionRejectionClass::InactiveTransactionVersion)
        }
        _ => Err(TransactionRejectionClass::UnsupportedTransactionVersion),
    }
}

pub fn classify_typed_transaction_error(error: &PulseError) -> Option<TransactionRejectionClass> {
    match error {
        PulseError::ChainIdMismatch => Some(TransactionRejectionClass::WrongChainDomain),
        PulseError::InvalidTxid => Some(TransactionRejectionClass::InvalidTxid),
        PulseError::InvalidSignature => Some(TransactionRejectionClass::InvalidSignature),
        PulseError::TxAlreadyExists => Some(TransactionRejectionClass::Duplicate),
        PulseError::DoubleSpend => Some(TransactionRejectionClass::Conflict),
        PulseError::UtxoNotFound => Some(TransactionRejectionClass::Orphan),
        PulseError::InsufficientFunds => Some(TransactionRejectionClass::InsufficientFunds),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_codes_match_the_frozen_transaction_v2_contract() {
        let cases = [
            (
                TransactionRejectionClass::UnsupportedTransactionVersion,
                "unsupported_transaction_version",
            ),
            (
                TransactionRejectionClass::InactiveTransactionVersion,
                "inactive_transaction_version",
            ),
            (
                TransactionRejectionClass::WrongChainDomain,
                "wrong_chain_domain",
            ),
            (TransactionRejectionClass::InvalidTxid, "invalid_txid"),
            (
                TransactionRejectionClass::InvalidSignature,
                "invalid_signature",
            ),
            (TransactionRejectionClass::Duplicate, "duplicate"),
            (TransactionRejectionClass::Conflict, "conflict"),
            (TransactionRejectionClass::Orphan, "orphan"),
            (
                TransactionRejectionClass::MalformedTransaction,
                "malformed_transaction",
            ),
            (
                TransactionRejectionClass::InsufficientFunds,
                "insufficient_funds",
            ),
            (TransactionRejectionClass::MempoolFull, "mempool_full"),
        ];

        for (class, expected) in cases {
            assert_eq!(class.as_str(), expected);
        }
    }

    #[test]
    fn known_other_protocol_version_is_inactive_not_unsupported() {
        assert_eq!(
            classify_transaction_version(TransactionValidationPath::LegacyV1, 2),
            Err(TransactionRejectionClass::InactiveTransactionVersion)
        );
        assert_eq!(
            classify_transaction_version(TransactionValidationPath::ActivatedV2, 1),
            Err(TransactionRejectionClass::InactiveTransactionVersion)
        );
    }

    #[test]
    fn unknown_transaction_versions_are_unsupported() {
        for version in [0, 3, u32::MAX] {
            assert_eq!(
                classify_transaction_version(TransactionValidationPath::LegacyV1, version),
                Err(TransactionRejectionClass::UnsupportedTransactionVersion)
            );
            assert_eq!(
                classify_transaction_version(TransactionValidationPath::ActivatedV2, version),
                Err(TransactionRejectionClass::UnsupportedTransactionVersion)
            );
        }
    }

    #[test]
    fn active_transaction_versions_pass_classification() {
        assert_eq!(
            classify_transaction_version(TransactionValidationPath::LegacyV1, 1),
            Ok(())
        );
        assert_eq!(
            classify_transaction_version(TransactionValidationPath::ActivatedV2, 2),
            Ok(())
        );
    }

    #[test]
    fn typed_core_errors_map_without_string_parsing() {
        let cases = [
            (
                PulseError::ChainIdMismatch,
                TransactionRejectionClass::WrongChainDomain,
            ),
            (
                PulseError::InvalidTxid,
                TransactionRejectionClass::InvalidTxid,
            ),
            (
                PulseError::InvalidSignature,
                TransactionRejectionClass::InvalidSignature,
            ),
            (
                PulseError::TxAlreadyExists,
                TransactionRejectionClass::Duplicate,
            ),
            (PulseError::DoubleSpend, TransactionRejectionClass::Conflict),
            (PulseError::UtxoNotFound, TransactionRejectionClass::Orphan),
            (
                PulseError::InsufficientFunds,
                TransactionRejectionClass::InsufficientFunds,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(classify_typed_transaction_error(&error), Some(expected));
        }
    }

    #[test]
    fn generic_error_text_is_never_parsed_for_semantic_classification() {
        let malformed = PulseError::InvalidTransaction("mempool full maybe".into());
        assert_eq!(classify_typed_transaction_error(&malformed), None);
    }
}
