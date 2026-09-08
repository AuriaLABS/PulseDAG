use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    state::ChainState,
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
pub const MEMPOOL_FEE_ESTIMATE_V3_VERSION: u32 = 1;
const MEMPOOL_POLICY_V3_FINGERPRINT_DOMAIN: &[u8] = b"PulseDAG:mempool-policy:v3";
pub const FEE_RATE_SCALE_BYTES_V3: u64 = 1_000;
const FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3: u64 = 10_000;

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

/// Deterministic, observational fee-rate estimate for the current mempool.
///
/// This is not an admission guarantee. Package/conflict policy, capacity
/// limits, and future replacement semantics remain independent admission
/// conditions. The estimator intentionally reports canonical fee-rate
/// quantiles without changing the live eviction path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolFeeEstimateV3 {
    pub version: u32,
    pub policy_fingerprint: String,
    pub min_relay_fee_rate_per_kb: u128,
    pub economy_fee_rate_per_kb: u128,
    pub standard_fee_rate_per_kb: u128,
    pub priority_fee_rate_per_kb: u128,
    pub observed_min_fee_rate_per_kb: Option<u128>,
    pub observed_max_fee_rate_per_kb: Option<u128>,
    pub mempool_transactions: u64,
    pub effective_max_transactions: u64,
    pub pressure_bps: u64,
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

fn nearest_rank_fee_rate_v3(sorted_fee_rates: &[u128], numerator: u128, denominator: u128) -> u128 {
    debug_assert!(numerator > 0);
    debug_assert!(denominator > 0);
    debug_assert!(numerator <= denominator);
    if sorted_fee_rates.is_empty() {
        return 0;
    }

    let count = sorted_fee_rates.len() as u128;
    let rank = count
        .saturating_mul(numerator)
        .saturating_add(denominator.saturating_sub(1))
        / denominator;
    let index = rank
        .saturating_sub(1)
        .min(count.saturating_sub(1)) as usize;
    sorted_fee_rates[index]
}

fn fee_estimate_pressure_bps_v3(used: u64, capacity: u64) -> u64 {
    if capacity == 0 {
        return if used == 0 {
            0
        } else {
            FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3
        };
    }

    let scaled = u128::from(used)
        .saturating_mul(u128::from(FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3))
        / u128::from(capacity);
    u64::try_from(scaled.min(u128::from(FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3)))
        .unwrap_or(FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3)
}

pub fn estimate_mempool_fee_rates_v3(
    state: &ChainState,
    policy: MempoolPolicyV3,
) -> Result<MempoolFeeEstimateV3, MempoolPolicyAssessmentErrorV3> {
    if policy.version != MEMPOOL_POLICY_V3_VERSION {
        return Err(MempoolPolicyRejectionV3::PolicyIdentityMismatch.into());
    }

    let mut fee_rates = state
        .mempool
        .transactions
        .values()
        .map(|tx| fee_rate_v3(tx, &state.chain_id).map(|rate| rate.fee_per_kb))
        .collect::<Result<Vec<_>, _>>()?;
    fee_rates.sort_unstable();

    let relay_floor = u128::from(policy.min_relay_fee_rate_per_kb);
    let quantile_or_floor = |numerator, denominator| {
        if fee_rates.is_empty() {
            relay_floor
        } else {
            nearest_rank_fee_rate_v3(&fee_rates, numerator, denominator).max(relay_floor)
        }
    };

    let mempool_transactions =
        u64::try_from(state.mempool.transactions.len()).unwrap_or(u64::MAX);
    let live_max_transactions = u64::try_from(state.mempool.max_transactions).unwrap_or(u64::MAX);
    let effective_max_transactions = policy.max_transactions.min(live_max_transactions);

    Ok(MempoolFeeEstimateV3 {
        version: MEMPOOL_FEE_ESTIMATE_V3_VERSION,
        policy_fingerprint: policy.fingerprint(),
        min_relay_fee_rate_per_kb: relay_floor,
        economy_fee_rate_per_kb: quantile_or_floor(1, 4),
        standard_fee_rate_per_kb: quantile_or_floor(1, 2),
        priority_fee_rate_per_kb: quantile_or_floor(9, 10),
        observed_min_fee_rate_per_kb: fee_rates.first().copied(),
        observed_max_fee_rate_per_kb: fee_rates.last().copied(),
        mempool_transactions,
        effective_max_transactions,
        pressure_bps: fee_estimate_pressure_bps_v3(
            mempool_transactions,
            effective_max_transactions,
        ),
    })
}

/// Preserve the existing deterministic first-seen admission ordering contract.
pub fn admission_order_key_v3(first_seen: Option<u64>, txid: &str) -> (u64, String) {
    (first_seen.unwrap_or(u64::MAX), txid.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{genesis::init_chain_state, types::{Transaction, TxOutput}};

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

    fn estimator_v1_tx(txid: &str, fee: u64, nonce: u64) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: TRANSACTION_VERSION_V1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "pulse1estimatevector".into(),
                amount: 7,
            }],
            fee,
            nonce,
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

    #[test]
    fn empty_mempool_fee_estimate_collapses_to_relay_floor() {
        let state = init_chain_state("estimate-empty".into());
        let policy = MempoolPolicyV3 {
            min_relay_fee_rate_per_kb: 123,
            ..MempoolPolicyV3::compatibility_default()
        };
        let estimate = estimate_mempool_fee_rates_v3(&state, policy).unwrap();
        assert_eq!(estimate.version, MEMPOOL_FEE_ESTIMATE_V3_VERSION);
        assert_eq!(estimate.policy_fingerprint, policy.fingerprint());
        assert_eq!(estimate.min_relay_fee_rate_per_kb, 123);
        assert_eq!(estimate.economy_fee_rate_per_kb, 123);
        assert_eq!(estimate.standard_fee_rate_per_kb, 123);
        assert_eq!(estimate.priority_fee_rate_per_kb, 123);
        assert_eq!(estimate.observed_min_fee_rate_per_kb, None);
        assert_eq!(estimate.observed_max_fee_rate_per_kb, None);
        assert_eq!(estimate.mempool_transactions, 0);
        assert_eq!(estimate.pressure_bps, 0);
    }

    #[test]
    fn fee_estimate_quantiles_are_nearest_rank_and_deterministic() {
        let mut state = init_chain_state("estimate-quantiles".into());
        let txs = [
            estimator_v1_tx("tx-a1", 10, 1),
            estimator_v1_tx("tx-b2", 20, 2),
            estimator_v1_tx("tx-c3", 30, 3),
            estimator_v1_tx("tx-d4", 40, 4),
            estimator_v1_tx("tx-e5", 50, 5),
        ];
        for tx in &txs {
            state.mempool.transactions.insert(tx.txid.clone(), tx.clone());
        }

        let mut rates = txs
            .iter()
            .map(|tx| fee_rate_v3(tx, &state.chain_id).unwrap().fee_per_kb)
            .collect::<Vec<_>>();
        rates.sort_unstable();

        let estimate =
            estimate_mempool_fee_rates_v3(&state, MempoolPolicyV3::compatibility_default())
                .unwrap();
        assert_eq!(estimate.economy_fee_rate_per_kb, rates[1]);
        assert_eq!(estimate.standard_fee_rate_per_kb, rates[2]);
        assert_eq!(estimate.priority_fee_rate_per_kb, rates[4]);
        assert_eq!(estimate.observed_min_fee_rate_per_kb, Some(rates[0]));
        assert_eq!(estimate.observed_max_fee_rate_per_kb, Some(rates[4]));
    }

    #[test]
    fn equivalent_mempool_insertion_orders_have_identical_fee_estimates() {
        let txs = [
            estimator_v1_tx("tx-a1", 10, 1),
            estimator_v1_tx("tx-b2", 20, 2),
            estimator_v1_tx("tx-c3", 30, 3),
            estimator_v1_tx("tx-d4", 40, 4),
        ];
        let mut forward = init_chain_state("estimate-order".into());
        let mut reverse = init_chain_state("estimate-order".into());
        for tx in &txs {
            forward
                .mempool
                .transactions
                .insert(tx.txid.clone(), tx.clone());
        }
        for tx in txs.iter().rev() {
            reverse
                .mempool
                .transactions
                .insert(tx.txid.clone(), tx.clone());
        }

        let policy = MempoolPolicyV3::compatibility_default();
        assert_eq!(
            estimate_mempool_fee_rates_v3(&forward, policy).unwrap(),
            estimate_mempool_fee_rates_v3(&reverse, policy).unwrap()
        );
    }

    #[test]
    fn relay_floor_clamps_all_estimate_tiers_without_hiding_observed_rates() {
        let mut state = init_chain_state("estimate-floor".into());
        let tx = estimator_v1_tx("tx-a1", 1, 1);
        let observed = fee_rate_v3(&tx, &state.chain_id).unwrap().fee_per_kb;
        state.mempool.transactions.insert(tx.txid.clone(), tx);
        let floor = u64::try_from(observed).unwrap().saturating_add(1_000);
        let policy = MempoolPolicyV3 {
            min_relay_fee_rate_per_kb: floor,
            ..MempoolPolicyV3::compatibility_default()
        };

        let estimate = estimate_mempool_fee_rates_v3(&state, policy).unwrap();
        assert_eq!(estimate.economy_fee_rate_per_kb, u128::from(floor));
        assert_eq!(estimate.standard_fee_rate_per_kb, u128::from(floor));
        assert_eq!(estimate.priority_fee_rate_per_kb, u128::from(floor));
        assert_eq!(estimate.observed_min_fee_rate_per_kb, Some(observed));
        assert_eq!(estimate.observed_max_fee_rate_per_kb, Some(observed));
    }

    #[test]
    fn fee_estimate_pressure_uses_stricter_effective_capacity_and_saturates() {
        let mut state = init_chain_state("estimate-pressure".into());
        for (txid, fee, nonce) in [("tx-a1", 1, 1), ("tx-b2", 2, 2), ("tx-c3", 3, 3)] {
            let tx = estimator_v1_tx(txid, fee, nonce);
            state.mempool.transactions.insert(tx.txid.clone(), tx);
        }
        let policy = MempoolPolicyV3 {
            max_transactions: 2,
            ..MempoolPolicyV3::compatibility_default()
        };
        let estimate = estimate_mempool_fee_rates_v3(&state, policy).unwrap();
        assert_eq!(estimate.mempool_transactions, 3);
        assert_eq!(estimate.effective_max_transactions, 2);
        assert_eq!(estimate.pressure_bps, FEE_ESTIMATE_PRESSURE_SCALE_BPS_V3);
    }

    #[test]
    fn fee_estimate_rejects_unsupported_policy_version() {
        let state = init_chain_state("estimate-version".into());
        let policy = MempoolPolicyV3 {
            version: MEMPOOL_POLICY_V3_VERSION + 1,
            ..MempoolPolicyV3::compatibility_default()
        };
        assert!(matches!(
            estimate_mempool_fee_rates_v3(&state, policy),
            Err(MempoolPolicyAssessmentErrorV3::Policy(
                MempoolPolicyRejectionV3::PolicyIdentityMismatch
            ))
        ));
    }
}
