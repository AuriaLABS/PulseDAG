use std::{error::Error, fmt};

use pulsedag_core::{
    address_from_public_key, compute_txid,
    errors::PulseError,
    signing_message,
    types::{Transaction, Utxo},
};
use serde::{Deserialize, Serialize};

use crate::{build_transaction, BuildTxResponse, SelectedUtxo};

/// Wallet-visible identity of the chain a keystore or transaction plan expects.
///
/// This is an application safety boundary. PulseDAG v1 transaction signatures
/// are not cryptographically bound to these strings; callers must therefore
/// verify the connected node/relay identity before signing or broadcasting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletNetworkIdentity {
    pub network_profile: String,
    pub chain_id: String,
}

impl WalletNetworkIdentity {
    pub fn new(
        network_profile: impl Into<String>,
        chain_id: impl Into<String>,
    ) -> Result<Self, WalletPlanError> {
        let identity = Self {
            network_profile: network_profile.into(),
            chain_id: chain_id.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), WalletPlanError> {
        validate_identity_field("network_profile", &self.network_profile)?;
        validate_identity_field("chain_id", &self.chain_id)?;
        Ok(())
    }

    /// Fail closed unless both the network profile and chain id match exactly.
    pub fn ensure_matches(&self, observed: &Self) -> Result<(), WalletPlanError> {
        self.validate()?;
        observed.validate()?;
        if self == observed {
            return Ok(());
        }

        Err(WalletPlanError::NetworkMismatch {
            expected_network_profile: self.network_profile.clone(),
            expected_chain_id: self.chain_id.clone(),
            observed_network_profile: observed.network_profile.clone(),
            observed_chain_id: observed.chain_id.clone(),
        })
    }
}

/// Human-reviewable payment intent kept next to the unsigned transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletTransactionIntent {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
}

impl WalletTransactionIntent {
    fn validate(&self) -> Result<(), WalletPlanError> {
        validate_identity_field("intent.from", &self.from)?;
        validate_identity_field("intent.to", &self.to)?;
        if self.amount == 0 {
            return Err(invalid_plan("intent.amount", "must be greater than zero"));
        }
        Ok(())
    }
}

/// Explicit wallet-side authorization limits for one transaction plan.
///
/// There is deliberately no implicit/default policy in this API. A caller must
/// choose fee and input limits before a plan can be built for review/signing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletSpendPolicy {
    pub max_fee: u64,
    pub max_fee_bps_of_amount: u32,
    pub max_inputs: usize,
}

impl WalletSpendPolicy {
    pub fn new(
        max_fee: u64,
        max_fee_bps_of_amount: u32,
        max_inputs: usize,
    ) -> Result<Self, WalletPlanError> {
        let policy = Self {
            max_fee,
            max_fee_bps_of_amount,
            max_inputs,
        };
        policy.validate_configuration()?;
        Ok(policy)
    }

    fn validate_configuration(&self) -> Result<(), WalletPlanError> {
        if self.max_inputs == 0 {
            return Err(policy_violation("max_inputs", "must be greater than zero"));
        }
        if self.max_fee_bps_of_amount > 10_000 {
            return Err(policy_violation(
                "max_fee_bps_of_amount",
                "must not exceed 10000 basis points",
            ));
        }
        Ok(())
    }

    fn validate_intent(&self, intent: &WalletTransactionIntent) -> Result<(), WalletPlanError> {
        self.validate_configuration()?;
        if intent.fee > self.max_fee {
            return Err(policy_violation("fee", "exceeds absolute fee limit"));
        }

        let fee_scaled = u128::from(intent.fee) * 10_000_u128;
        let allowed_scaled = u128::from(intent.amount) * u128::from(self.max_fee_bps_of_amount);
        if fee_scaled > allowed_scaled {
            return Err(policy_violation(
                "fee",
                "exceeds fee-to-amount basis-point limit",
            ));
        }
        Ok(())
    }

    fn validate_input_count(&self, input_count: usize) -> Result<(), WalletPlanError> {
        if input_count > self.max_inputs {
            return Err(policy_violation("inputs", "exceeds input-count limit"));
        }
        Ok(())
    }
}

/// v1 wallet nonce policy.
///
/// The caller must supply the nonce explicitly. There is intentionally no
/// default value and no magic `nonce = 1` fallback in the wallet plan API.
/// The protocol-level meaning of the field remains tracked by #821.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletNoncePolicy {
    ExplicitCallerProvidedV1,
}

/// Compact data a wallet UI/CLI can render immediately before authorization.
///
/// `unsigned_template_txid` identifies the unsigned, keyless transaction
/// template only. It is deliberately not named `txid`: PulseDAG v1 recomputes
/// the broadcast transaction id after public keys and signatures are attached.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletReviewSummary {
    pub network_profile: String,
    pub chain_id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub change: u64,
    pub total_input: u64,
    pub input_count: usize,
    pub nonce: u64,
    pub unsigned_template_txid: String,
}

/// Exact public-key-bound bytes that a local/offline signer is asked to sign.
///
/// This structure contains no private key. `transaction.txid` is empty because
/// the final broadcast txid cannot be known until signatures are attached.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletSigningPreparation {
    pub network: WalletNetworkIdentity,
    pub review: WalletReviewSummary,
    pub spend_policy: WalletSpendPolicy,
    pub transaction: Transaction,
    pub signing_message: String,
}

/// Reviewable, serializable unsigned transaction plan for the wallet UI/CLI.
///
/// `network` is wallet metadata and is deliberately kept outside the current
/// consensus signing preimage. The wallet must call `prepare_signing` with the
/// observed node/relay identity immediately before exposing a sign action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletTransactionPlan {
    pub network: WalletNetworkIdentity,
    pub intent: WalletTransactionIntent,
    pub spend_policy: WalletSpendPolicy,
    pub nonce_policy: WalletNoncePolicy,
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
}

impl WalletTransactionPlan {
    /// Validate that serialized review metadata still matches the unsigned tx.
    pub fn validate_structure(&self) -> Result<(), WalletPlanError> {
        self.network.validate()?;
        self.intent.validate()?;
        self.spend_policy.validate_intent(&self.intent)?;
        self.spend_policy
            .validate_input_count(self.selected_utxos.len())?;

        if self.transaction.version != 1 {
            return Err(invalid_plan(
                "transaction.version",
                "unsupported wallet transaction version",
            ));
        }
        if self.transaction.fee != self.intent.fee {
            return Err(invalid_plan(
                "transaction.fee",
                "does not match reviewed intent",
            ));
        }

        let target = self
            .intent
            .amount
            .checked_add(self.intent.fee)
            .ok_or_else(|| invalid_plan("intent.amount", "amount plus fee overflows"))?;
        let selected_total = self
            .selected_utxos
            .iter()
            .try_fold(0_u64, |total, selected| {
                total.checked_add(selected.amount).ok_or_else(|| {
                    invalid_plan("selected_utxos", "selected amount total overflows")
                })
            })?;
        if selected_total != self.total_input {
            return Err(invalid_plan("total_input", "does not match selected UTXOs"));
        }
        if self.total_input < target {
            return Err(invalid_plan(
                "total_input",
                "does not cover amount plus fee",
            ));
        }
        let expected_change = self.total_input - target;
        if expected_change != self.change {
            return Err(invalid_plan("change", "does not match reviewed intent"));
        }

        if self.transaction.inputs.len() != self.selected_utxos.len() {
            return Err(invalid_plan(
                "transaction.inputs",
                "does not match selected UTXOs",
            ));
        }
        for (index, (input, selected)) in self
            .transaction
            .inputs
            .iter()
            .zip(&self.selected_utxos)
            .enumerate()
        {
            if input.previous_output != selected.outpoint {
                return Err(invalid_plan(
                    "transaction.inputs",
                    "outpoint does not match selected UTXO",
                ));
            }
            if !input.public_key.is_empty() || !input.signature.is_empty() {
                return Err(invalid_plan(
                    "transaction.inputs",
                    "unsigned plan must not contain public keys or signatures",
                ));
            }
            if self.selected_utxos[..index]
                .iter()
                .any(|prior| prior.outpoint == selected.outpoint)
            {
                return Err(invalid_plan(
                    "selected_utxos",
                    "contains a duplicate outpoint",
                ));
            }
        }

        let expected_output_count = if self.change > 0 { 2 } else { 1 };
        if self.transaction.outputs.len() != expected_output_count {
            return Err(invalid_plan(
                "transaction.outputs",
                "unexpected output count",
            ));
        }
        let recipient = &self.transaction.outputs[0];
        if recipient.address != self.intent.to || recipient.amount != self.intent.amount {
            return Err(invalid_plan(
                "transaction.outputs",
                "recipient output does not match intent",
            ));
        }
        if self.change > 0 {
            let change = &self.transaction.outputs[1];
            if change.address != self.intent.from || change.amount != self.change {
                return Err(invalid_plan(
                    "transaction.outputs",
                    "change output does not match intent",
                ));
            }
        }

        if compute_txid(&self.transaction) != self.transaction.txid {
            return Err(invalid_plan(
                "transaction.txid",
                "does not match unsigned transaction template",
            ));
        }

        Ok(())
    }

    pub fn review_summary(&self) -> Result<WalletReviewSummary, WalletPlanError> {
        self.validate_structure()?;
        Ok(WalletReviewSummary {
            network_profile: self.network.network_profile.clone(),
            chain_id: self.network.chain_id.clone(),
            from: self.intent.from.clone(),
            to: self.intent.to.clone(),
            amount: self.intent.amount,
            fee: self.intent.fee,
            change: self.change,
            total_input: self.total_input,
            input_count: self.selected_utxos.len(),
            nonce: self.transaction.nonce,
            unsigned_template_txid: self.transaction.txid.clone(),
        })
    }

    pub fn verify_remote_identity(
        &self,
        observed: &WalletNetworkIdentity,
    ) -> Result<(), WalletPlanError> {
        self.validate_structure()?;
        self.network.ensure_matches(observed)
    }

    pub fn verify_keystore_identity(
        &self,
        keystore: &WalletNetworkIdentity,
    ) -> Result<(), WalletPlanError> {
        self.validate_structure()?;
        self.network.ensure_matches(keystore)
    }

    /// Bind the exact unsigned transaction to the sender public key and return
    /// the bytes that must be signed. Network identity is checked in the same
    /// call so a UI/CLI cannot accidentally prepare signing for a mismatched
    /// node or relay.
    pub fn prepare_signing(
        &self,
        observed: &WalletNetworkIdentity,
        public_key_hex: &str,
    ) -> Result<WalletSigningPreparation, WalletPlanError> {
        self.verify_remote_identity(observed)?;
        validate_canonical_public_key(public_key_hex)?;

        let derived_address = address_from_public_key(public_key_hex);
        if derived_address != self.intent.from {
            return Err(WalletPlanError::PublicKeyAddressMismatch {
                expected_address: self.intent.from.clone(),
                derived_address,
            });
        }

        let review = self.review_summary()?;
        let mut transaction = self.transaction.clone();
        transaction.txid.clear();
        for input in &mut transaction.inputs {
            input.public_key = public_key_hex.to_string();
            input.signature.clear();
        }
        let signing_message = hex::encode(signing_message(&transaction));

        Ok(WalletSigningPreparation {
            network: self.network.clone(),
            review,
            spend_policy: self.spend_policy.clone(),
            transaction,
            signing_message,
        })
    }
}

#[derive(Debug)]
pub enum WalletPlanError {
    InvalidIdentityField {
        field: &'static str,
        reason: &'static str,
    },
    InvalidPlanField {
        field: &'static str,
        reason: &'static str,
    },
    PolicyViolation {
        rule: &'static str,
        reason: &'static str,
    },
    InvalidPublicKey {
        reason: &'static str,
    },
    PublicKeyAddressMismatch {
        expected_address: String,
        derived_address: String,
    },
    NetworkMismatch {
        expected_network_profile: String,
        expected_chain_id: String,
        observed_network_profile: String,
        observed_chain_id: String,
    },
    Build(PulseError),
}

impl fmt::Display for WalletPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentityField { field, reason } => {
                write!(f, "invalid wallet network identity field {field}: {reason}")
            }
            Self::InvalidPlanField { field, reason } => {
                write!(f, "invalid wallet transaction plan field {field}: {reason}")
            }
            Self::PolicyViolation { rule, reason } => {
                write!(f, "wallet spend policy violation {rule}: {reason}")
            }
            Self::InvalidPublicKey { reason } => {
                write!(f, "invalid wallet public key: {reason}")
            }
            Self::PublicKeyAddressMismatch {
                expected_address,
                derived_address,
            } => write!(
                f,
                "wallet public key does not control sender address: expected {expected_address}, derived {derived_address}"
            ),
            Self::NetworkMismatch {
                expected_network_profile,
                expected_chain_id,
                observed_network_profile,
                observed_chain_id,
            } => write!(
                f,
                "wallet network mismatch: expected profile={expected_network_profile} chain_id={expected_chain_id}, observed profile={observed_network_profile} chain_id={observed_chain_id}"
            ),
            Self::Build(error) => write!(f, "wallet transaction build failed: {error}"),
        }
    }
}

impl Error for WalletPlanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PulseError> for WalletPlanError {
    fn from(value: PulseError) -> Self {
        Self::Build(value)
    }
}

/// Build a v1 unsigned wallet plan with explicit network identity, spend policy
/// and nonce.
///
/// This function does not alter PulseDAG consensus serialization or signing
/// semantics. In particular, `network` is not added to the signing preimage.
pub fn build_transaction_plan(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<WalletTransactionPlan, WalletPlanError> {
    network.validate()?;
    let intent = WalletTransactionIntent {
        from: from.to_string(),
        to: to.to_string(),
        amount,
        fee,
    };
    intent.validate()?;
    spend_policy.validate_intent(&intent)?;
    let built = build_transaction(from, to, amount, fee, available_utxos, nonce)?;
    spend_policy.validate_input_count(built.selected_utxos.len())?;
    let plan = plan_from_build(network, intent, spend_policy, built);
    plan.validate_structure()?;
    Ok(plan)
}

fn plan_from_build(
    network: WalletNetworkIdentity,
    intent: WalletTransactionIntent,
    spend_policy: WalletSpendPolicy,
    built: BuildTxResponse,
) -> WalletTransactionPlan {
    WalletTransactionPlan {
        network,
        intent,
        spend_policy,
        nonce_policy: WalletNoncePolicy::ExplicitCallerProvidedV1,
        transaction: built.transaction,
        selected_utxos: built.selected_utxos,
        total_input: built.total_input,
        change: built.change,
    }
}

fn invalid_plan(field: &'static str, reason: &'static str) -> WalletPlanError {
    WalletPlanError::InvalidPlanField { field, reason }
}

fn policy_violation(rule: &'static str, reason: &'static str) -> WalletPlanError {
    WalletPlanError::PolicyViolation { rule, reason }
}

fn validate_identity_field(field: &'static str, value: &str) -> Result<(), WalletPlanError> {
    if value.is_empty() {
        return Err(WalletPlanError::InvalidIdentityField {
            field,
            reason: "must not be empty",
        });
    }
    if value.trim() != value {
        return Err(WalletPlanError::InvalidIdentityField {
            field,
            reason: "must not contain leading or trailing whitespace",
        });
    }
    Ok(())
}

fn validate_canonical_public_key(public_key_hex: &str) -> Result<(), WalletPlanError> {
    let decoded = hex::decode(public_key_hex).map_err(|_| WalletPlanError::InvalidPublicKey {
        reason: "must be hexadecimal",
    })?;
    if decoded.len() != 32 {
        return Err(WalletPlanError::InvalidPublicKey {
            reason: "must encode exactly 32 bytes",
        });
    }
    if hex::encode(decoded) != public_key_hex {
        return Err(WalletPlanError::InvalidPublicKey {
            reason: "must use canonical lowercase hexadecimal encoding",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pulsedag_core::{
        address_from_public_key, signing_message,
        types::{OutPoint, Utxo},
    };

    use super::*;

    fn identity(chain_id: &str) -> WalletNetworkIdentity {
        WalletNetworkIdentity::new("public-testnet", chain_id).expect("valid identity")
    }

    fn policy() -> WalletSpendPolicy {
        WalletSpendPolicy::new(100, 1_000, 8).expect("valid policy")
    }

    fn public_key() -> String {
        "11".repeat(32)
    }

    fn sender_address() -> String {
        address_from_public_key(&public_key())
    }

    fn sample_utxo() -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            address: sender_address(),
            amount: 1_000,
            coinbase: false,
            height: 10,
        }
    }

    fn sample_plan() -> WalletTransactionPlan {
        build_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            &sender_address(),
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("build plan")
    }

    #[test]
    fn transaction_plan_preserves_explicit_nonce_intent_and_review_summary() {
        let plan = sample_plan();
        let review = plan.review_summary().expect("review summary");

        assert_eq!(plan.transaction.nonce, 42);
        assert_eq!(plan.intent.amount, 400);
        assert_eq!(plan.intent.fee, 10);
        assert_eq!(review.chain_id, "pulsedag-public-testnet");
        assert_eq!(review.to, "pulse1recipient");
        assert_eq!(review.fee, 10);
        assert_eq!(review.input_count, 1);
        assert_eq!(review.unsigned_template_txid, plan.transaction.txid);
        assert_eq!(
            plan.nonce_policy,
            WalletNoncePolicy::ExplicitCallerProvidedV1
        );
        plan.validate_structure().expect("valid plan");
    }

    #[test]
    fn signing_preparation_attaches_public_key_after_network_check() {
        let plan = sample_plan();
        let expected_public_key = public_key();
        let prepared = plan
            .prepare_signing(&identity("pulsedag-public-testnet"), &expected_public_key)
            .expect("prepare signing");

        assert!(prepared.transaction.txid.is_empty());
        assert!(prepared.transaction.inputs.iter().all(|input| {
            input.public_key == expected_public_key && input.signature.is_empty()
        }));
        assert_eq!(
            prepared.signing_message,
            hex::encode(signing_message(&prepared.transaction))
        );
        assert_ne!(
            prepared.signing_message,
            hex::encode(signing_message(&plan.transaction))
        );
    }

    #[test]
    fn signing_preparation_rejects_wrong_network_or_public_key() {
        let plan = sample_plan();
        assert!(plan
            .prepare_signing(&identity("pulsedag-private-testnet"), &public_key())
            .is_err());

        let other_public_key = "22".repeat(32);
        assert!(matches!(
            plan.prepare_signing(&identity("pulsedag-public-testnet"), &other_public_key),
            Err(WalletPlanError::PublicKeyAddressMismatch { .. })
        ));
    }

    #[test]
    fn canonical_public_key_encoding_is_required() {
        let plan = sample_plan();
        let uppercase = public_key().to_uppercase();
        assert!(matches!(
            plan.prepare_signing(&identity("pulsedag-public-testnet"), &uppercase),
            Err(WalletPlanError::InvalidPublicKey { .. })
        ));
        assert!(matches!(
            plan.prepare_signing(&identity("pulsedag-public-testnet"), "11"),
            Err(WalletPlanError::InvalidPublicKey { .. })
        ));
    }

    #[test]
    fn spend_policy_rejects_excessive_fee_and_input_count() {
        let fee_error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(5, 10_000, 8).expect("policy"),
            &sender_address(),
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect_err("absolute fee cap must be enforced");
        assert!(matches!(fee_error, WalletPlanError::PolicyViolation { .. }));

        let input_error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(100, 10_000, 1).expect("policy"),
            &sender_address(),
            "pulse1recipient",
            1_500,
            0,
            &[
                sample_utxo(),
                Utxo {
                    outpoint: OutPoint {
                        txid: "22".repeat(32),
                        index: 0,
                    },
                    address: sender_address(),
                    amount: 1_000,
                    coinbase: false,
                    height: 11,
                },
            ],
            42,
        )
        .expect_err("input cap must be enforced");
        assert!(matches!(
            input_error,
            WalletPlanError::PolicyViolation { .. }
        ));
    }

    #[test]
    fn spend_policy_rejects_excessive_fee_ratio() {
        let error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(1_000, 100, 8).expect("policy"),
            &sender_address(),
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect_err("fee ratio cap must be enforced");
        assert!(matches!(error, WalletPlanError::PolicyViolation { .. }));
    }

    #[test]
    fn remote_network_mismatch_fails_closed() {
        let expected = identity("pulsedag-public-testnet");
        let observed = identity("pulsedag-private-testnet");

        let error = expected
            .ensure_matches(&observed)
            .expect_err("mismatched chain must fail");
        assert!(matches!(error, WalletPlanError::NetworkMismatch { .. }));
    }

    #[test]
    fn empty_or_padded_identity_is_rejected() {
        assert!(WalletNetworkIdentity::new("", "chain").is_err());
        assert!(WalletNetworkIdentity::new("public-testnet", " chain").is_err());
    }

    #[test]
    fn tampered_review_metadata_or_signed_input_is_rejected() {
        let mut plan = sample_plan();
        plan.intent.amount = 399;
        assert!(plan.validate_structure().is_err());

        let mut plan = sample_plan();
        plan.transaction.inputs[0].signature = "unexpected-signature".to_string();
        plan.transaction.txid = compute_txid(&plan.transaction);
        assert!(plan.validate_structure().is_err());
    }

    #[test]
    fn duplicate_selected_outpoint_is_rejected() {
        let mut plan = sample_plan();
        let duplicate = plan.selected_utxos[0].clone();
        plan.selected_utxos.push(duplicate.clone());
        plan.transaction
            .inputs
            .push(plan.transaction.inputs[0].clone());
        plan.total_input = plan.total_input.saturating_add(duplicate.amount);
        plan.change = plan.change.saturating_add(duplicate.amount);
        assert!(plan.validate_structure().is_err());
    }

    #[test]
    fn v1_signing_preimage_is_not_claimed_to_be_chain_bound() {
        let public_plan = sample_plan();
        let private_plan = build_transaction_plan(
            identity("pulsedag-private-testnet"),
            policy(),
            &sender_address(),
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("private plan");

        assert_eq!(public_plan.transaction.txid, private_plan.transaction.txid);
        assert_eq!(
            signing_message(&public_plan.transaction),
            signing_message(&private_plan.transaction)
        );
        assert!(public_plan
            .verify_remote_identity(&private_plan.network)
            .is_err());
    }

    #[test]
    fn unknown_identity_and_policy_fields_are_rejected() {
        let identity_value = serde_json::json!({
            "network_profile": "public-testnet",
            "chain_id": "pulsedag-public-testnet",
            "unexpected": true
        });
        assert!(serde_json::from_value::<WalletNetworkIdentity>(identity_value).is_err());

        let policy_value = serde_json::json!({
            "max_fee": 100,
            "max_fee_bps_of_amount": 1000,
            "max_inputs": 8,
            "unexpected": true
        });
        assert!(serde_json::from_value::<WalletSpendPolicy>(policy_value).is_err());
    }
}
