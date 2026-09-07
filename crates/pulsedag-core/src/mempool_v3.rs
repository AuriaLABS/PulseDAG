use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    tx::{
        canonical_transaction_bytes, canonical_transaction_bytes_v2, TRANSACTION_VERSION_V1,
        TRANSACTION_VERSION_V2,
    },
    tx_v3::{canonical_transaction_bytes_v3, TRANSACTION_VERSION_V3},
    types::Transaction,
};

/// Stable identity for the first deterministic v3 mempool-policy contract.
///
/// This is policy metadata, not a consensus activation version.
pub const MEMPOOL_POLICY_V3_VERSION: u32 = 1;
const MEMPOOL_POLICY_V3_FINGERPRINT_DOMAIN: &[u8] = b"PulseDAG:mempool-policy:v3";
pub const FEE_RATE_SCALE_BYTES_V3: u64 = 1_000;

/// Compatibility-first defaults for the foundation slice.
///
/// These values deliberately preserve current admission behavior: no positive
/// relay-fee floor is introduced and no finite high-fee ceiling is imposed by
/// this module. Final production numeric policy remains a launch-freeze item.
pub const MEMPOOL_POLICY_V3_COMPAT_MIN_RELAY_FEE_RATE: u64 = 0;
pub const MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTION_FEE: u64 = u64::MAX;
pub const MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTIONS: u64 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolPolicyV3 {
    pub version: u32,
    pub min_relay_fee_rate_per_kb: u64,
    pub max_transaction_fee: u64,
    pub max_transactions: u64,
    pub replacement_enabled: bool,
}

impl Default for MempoolPolicyV3 {
    fn default() -> Self {
        Self::compatibility_default()
    }
}

impl MempoolPolicyV3 {
    pub const fn compatibility_default() -> Self {
        Self {
            version: MEMPOOL_POLICY_V3_VERSION,
            min_relay_fee_rate_per_kb: MEMPOOL_POLICY_V3_COMPAT_MIN_RELAY_FEE_RATE,
            max_transaction_fee: MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTION_FEE,
            max_transactions: MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTIONS,
            replacement_enabled: false,
        }
    }

    pub fn fingerprint(self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(MEMPOOL_POLICY_V3_FINGERPRINT_DOMAIN);
        hasher.update(self.version.to_le_bytes());
        hasher.update(self.min_relay_fee_rate_per_kb.to_le_bytes());
        hasher.update(self.max_transaction_fee.to_le_bytes());
        hasher.update(self.max_transactions.to_le_bytes());
        hasher.update([u8::from(self.replacement_enabled)]);
        hex::encode(hasher.finalize())
    }

    pub fn validate_identity(
        self,
        expected_fingerprint: &str,
    ) -> Result<(), MempoolPolicyRejectionV3> {
        if self.version != MEMPOOL_POLICY_V3_VERSION || self.fingerprint() != expected_fingerprint {
            return Err(MempoolPolicyRejectionV3::PolicyIdentityMismatch);
        }
        Ok(())
    }

    pub fn assess_transaction(
        self,
        tx: &Transaction,
        chain_id: &str,
        current_transactions: u64,
        conflicts_with_mempool: bool,
    ) -> Result<FeeRateV3, MempoolPolicyAssessmentErrorV3> {
        if self.version != MEMPOOL_POLICY_V3_VERSION {
            return Err(MempoolPolicyRejectionV3::PolicyIdentityMismatch.into());
        }
        if current_transactions >= self.max_transactions {
            return Err(MempoolPolicyRejectionV3::CapacityBackpressure.into());
        }
        if tx.fee > self.max_transaction_fee {
            return Err(MempoolPolicyRejectionV3::AboveMaximumTransactionFee.into());
        }
        if conflicts_with_mempool {
            // Replacement is deliberately not implemented in this foundation.
            // Even a future policy with replacement_enabled=true must not make
            // conflicts admissible until frozen RBF semantics exist.
            return Err(MempoolPolicyRejectionV3::ReplacementNotAuthorized.into());
        }

        let fee_rate = fee_rate_v3(tx, chain_id)?;
        if fee_rate.fee_per_kb < u128::from(self.min_relay_fee_rate_per_kb) {
            return Err(MempoolPolicyRejectionV3::BelowMinimumRelayFeeRate.into());
        }
        Ok(fee_rate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeRateV3 {
    pub fee: u64,
    pub canonical_size_bytes: u64,
    pub fee_per_kb: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MempoolPolicyRejectionV3 {
    BelowMinimumRelayFeeRate,
    AboveMaximumTransactionFee,
    CapacityBackpressure,
    ReplacementNotAuthorized,
    PolicyIdentityMismatch,
}

impl MempoolPolicyRejectionV3 {
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::BelowMinimumRelayFeeRate => "MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE",
            Self::AboveMaximumTransactionFee => "MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE",
            Self::CapacityBackpressure => "MEMPOOL_V3_CAPACITY_BACKPRESSURE",
            Self::ReplacementNotAuthorized => "MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED",
            Self::PolicyIdentityMismatch => "MEMPOOL_V3_POLICY_IDENTITY_MISMATCH",
        }
    }
}

#[derive(Debug)]
pub enum MempoolPolicyAssessmentErrorV3 {
    Policy(MempoolPolicyRejectionV3),
    CanonicalTransaction(PulseError),
    CanonicalSizeOverflow,
}

impl From<MempoolPolicyRejectionV3> for MempoolPolicyAssessmentErrorV3 {
    fn from(value: MempoolPolicyRejectionV3) -> Self {
        Self::Policy(value)
    }
}

impl From<PulseError> for MempoolPolicyAssessmentErrorV3 {
    fn from(value: PulseError) -> Self {
        Self::CanonicalTransaction(value)
    }
}

pub fn canonical_transaction_size_for_mempool_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<u64, MempoolPolicyAssessmentErrorV3> {
    let bytes = match tx.version {
        TRANSACTION_VERSION_V1 => canonical_transaction_bytes(tx),
        TRANSACTION_VERSION_V2 => canonical_transaction_bytes_v2(tx, chain_id)?,
        TRANSACTION_VERSION_V3 => canonical_transaction_bytes_v3(tx, chain_id)?,
        version => {
            return Err(PulseError::InvalidTransaction(format!(
                "mempool v3 canonical-size policy does not support transaction version {version}"
            ))
            .into())
        }
    };
    u64::try_from(bytes.len()).map_err(|_| MempoolPolicyAssessmentErrorV3::CanonicalSizeOverflow)
}

pub fn fee_rate_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<FeeRateV3, MempoolPolicyAssessmentErrorV3> {
    let canonical_size_bytes = canonical_transaction_size_for_mempool_v3(tx, chain_id)?;
    debug_assert!(canonical_size_bytes > 0);
    let scaled_fee = u128::from(tx.fee) * u128::from(FEE_RATE_SCALE_BYTES_V3);
    let fee_per_kb = scaled_fee / u128::from(canonical_size_bytes);
    Ok(FeeRateV3 {
        fee: tx.fee,
        canonical_size_bytes,
        fee_per_kb,
    })
}

/// Preserve the existing deterministic first-seen admission ordering contract.
pub fn admission_order_key_v3(first_seen: Option<u64>, txid: &str) -> (u64, String) {
    (first_seen.unwrap_or(u64::MAX), txid.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Transaction, TxOutput};

    fn sample_v1_tx(fee: u64) -> Transaction {
        Transaction {
            txid: "tx-policy-vector".into(),
            version: TRANSACTION_VERSION_V1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "pulse1policyvector".into(),
                amount: 7,
            }],
            fee,
            nonce: 11,
        }
    }

    #[test]
    fn compatibility_policy_fingerprint_is_golden() {
        let policy = MempoolPolicyV3::compatibility_default();
        assert_eq!(
            policy.fingerprint(),
            "5bda9d47ff368e28e0f9e258e6a9b41e7cb9642b7798b3f7e86769b975ad4efe"
        );
        assert_eq!(policy, MempoolPolicyV3::default());
    }

    #[test]
    fn fee_rate_uses_canonical_bytes_and_integer_arithmetic() {
        let tx = sample_v1_tx(1_000);
        let a = fee_rate_v3(&tx, "ignored-by-v1").unwrap();
        let b = fee_rate_v3(&tx, "different-v1-domain-is-still-legacy").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a.canonical_size_bytes,
            canonical_transaction_bytes(&tx).len() as u64
        );
        assert_eq!(
            a.fee_per_kb,
            (u128::from(tx.fee) * u128::from(FEE_RATE_SCALE_BYTES_V3))
                / u128::from(a.canonical_size_bytes)
        );
    }

    #[test]
    fn maximum_fee_value_does_not_overflow_rate_math() {
        let tx = sample_v1_tx(u64::MAX);
        let rate = fee_rate_v3(&tx, "legacy").unwrap();
        let expected = (u128::from(u64::MAX) * u128::from(FEE_RATE_SCALE_BYTES_V3))
            / u128::from(rate.canonical_size_bytes);
        assert_eq!(rate.fee_per_kb, expected);
        assert!(rate.fee_per_kb > u128::from(u64::MAX));
    }

    #[test]
    fn policy_rejections_have_stable_machine_codes() {
        assert_eq!(
            MempoolPolicyRejectionV3::BelowMinimumRelayFeeRate.as_code(),
            "MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE"
        );
        assert_eq!(
            MempoolPolicyRejectionV3::AboveMaximumTransactionFee.as_code(),
            "MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE"
        );
        assert_eq!(
            MempoolPolicyRejectionV3::CapacityBackpressure.as_code(),
            "MEMPOOL_V3_CAPACITY_BACKPRESSURE"
        );
        assert_eq!(
            MempoolPolicyRejectionV3::ReplacementNotAuthorized.as_code(),
            "MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED"
        );
        assert_eq!(
            MempoolPolicyRejectionV3::PolicyIdentityMismatch.as_code(),
            "MEMPOOL_V3_POLICY_IDENTITY_MISMATCH"
        );
    }

    #[test]
    fn foundation_is_compatibility_first_and_conflicts_fail_closed() {
        let tx = sample_v1_tx(0);
        let policy = MempoolPolicyV3::compatibility_default();
        assert!(policy.assess_transaction(&tx, "legacy", 0, false).is_ok());
        assert!(matches!(
            policy.assess_transaction(&tx, "legacy", 0, true),
            Err(MempoolPolicyAssessmentErrorV3::Policy(
                MempoolPolicyRejectionV3::ReplacementNotAuthorized
            ))
        ));
    }

    #[test]
    fn policy_bounds_are_deterministic_at_edges() {
        let tx = sample_v1_tx(10);
        let rate = fee_rate_v3(&tx, "legacy").unwrap();
        let policy = MempoolPolicyV3 {
            min_relay_fee_rate_per_kb: u64::try_from(rate.fee_per_kb).unwrap().saturating_add(1),
            max_transaction_fee: tx.fee,
            ..MempoolPolicyV3::compatibility_default()
        };
        assert!(matches!(
            policy.assess_transaction(&tx, "legacy", 0, false),
            Err(MempoolPolicyAssessmentErrorV3::Policy(
                MempoolPolicyRejectionV3::BelowMinimumRelayFeeRate
            ))
        ));

        let high_fee_tx = sample_v1_tx(11);
        assert!(matches!(
            policy.assess_transaction(&high_fee_tx, "legacy", 0, false),
            Err(MempoolPolicyAssessmentErrorV3::Policy(
                MempoolPolicyRejectionV3::AboveMaximumTransactionFee
            ))
        ));

        assert!(matches!(
            policy.assess_transaction(&tx, "legacy", policy.max_transactions, false),
            Err(MempoolPolicyAssessmentErrorV3::Policy(
                MempoolPolicyRejectionV3::CapacityBackpressure
            ))
        ));
    }

    #[test]
    fn ordering_key_preserves_first_seen_then_txid_tie_break() {
        let mut keys = [
            admission_order_key_v3(None, "tx-z"),
            admission_order_key_v3(Some(7), "tx-b"),
            admission_order_key_v3(Some(7), "tx-a"),
            admission_order_key_v3(Some(3), "tx-c"),
        ];
        keys.sort();
        assert_eq!(keys[0], (3, "tx-c".into()));
        assert_eq!(keys[1], (7, "tx-a".into()));
        assert_eq!(keys[2], (7, "tx-b".into()));
        assert_eq!(keys[3], (u64::MAX, "tx-z".into()));
    }

    #[test]
    fn policy_identity_mismatch_fails_closed() {
        let policy = MempoolPolicyV3::compatibility_default();
        assert!(policy.validate_identity(&policy.fingerprint()).is_ok());
        assert_eq!(
            policy.validate_identity("00"),
            Err(MempoolPolicyRejectionV3::PolicyIdentityMismatch)
        );
    }
}
