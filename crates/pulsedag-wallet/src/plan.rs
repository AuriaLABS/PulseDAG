use std::{error::Error, fmt};

use pulsedag_core::{
    compute_txid,
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

/// Reviewable, serializable unsigned transaction plan for the wallet UI/CLI.
///
/// `network` is wallet metadata and is deliberately kept outside the current
/// consensus signing preimage. The wallet must call `verify_remote_identity`
/// against the node/relay it will use before exposing a sign/broadcast action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletTransactionPlan {
    pub network: WalletNetworkIdentity,
    pub intent: WalletTransactionIntent,
    pub nonce_policy: WalletNoncePolicy,
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
    pub signing_message: String,
}

impl WalletTransactionPlan {
    /// Validate that serialized review metadata still matches the unsigned tx.
    pub fn validate_structure(&self) -> Result<(), WalletPlanError> {
        self.network.validate()?;
        self.intent.validate()?;

        if self.transaction.version != 1 {
            return Err(invalid_plan("transaction.version", "unsupported wallet transaction version"));
        }
        if self.transaction.fee != self.intent.fee {
            return Err(invalid_plan("transaction.fee", "does not match reviewed intent"));
        }

        let target = self
            .intent
            .amount
            .checked_add(self.intent.fee)
            .ok_or_else(|| invalid_plan("intent.amount", "amount plus fee overflows"))?;
        let selected_total = self.selected_utxos.iter().try_fold(0_u64, |total, selected| {
            total
                .checked_add(selected.amount)
                .ok_or_else(|| invalid_plan("selected_utxos", "selected amount total overflows"))
        })?;
        if selected_total != self.total_input {
            return Err(invalid_plan("total_input", "does not match selected UTXOs"));
        }
        if self.total_input < target {
            return Err(invalid_plan("total_input", "does not cover amount plus fee"));
        }
        let expected_change = self.total_input - target;
        if expected_change != self.change {
            return Err(invalid_plan("change", "does not match reviewed intent"));
        }

        if self.transaction.inputs.len() != self.selected_utxos.len() {
            return Err(invalid_plan("transaction.inputs", "does not match selected UTXOs"));
        }
        for (input, selected) in self.transaction.inputs.iter().zip(&self.selected_utxos) {
            if input.previous_output != selected.outpoint {
                return Err(invalid_plan("transaction.inputs", "outpoint does not match selected UTXO"));
            }
        }

        let expected_output_count = if self.change > 0 { 2 } else { 1 };
        if self.transaction.outputs.len() != expected_output_count {
            return Err(invalid_plan("transaction.outputs", "unexpected output count"));
        }
        let recipient = &self.transaction.outputs[0];
        if recipient.address != self.intent.to || recipient.amount != self.intent.amount {
            return Err(invalid_plan("transaction.outputs", "recipient output does not match intent"));
        }
        if self.change > 0 {
            let change = &self.transaction.outputs[1];
            if change.address != self.intent.from || change.amount != self.change {
                return Err(invalid_plan("transaction.outputs", "change output does not match intent"));
            }
        }

        let expected_message = hex::encode(signing_message(&self.transaction));
        if expected_message != self.signing_message {
            return Err(invalid_plan("signing_message", "does not match unsigned transaction"));
        }
        if compute_txid(&self.transaction) != self.transaction.txid {
            return Err(invalid_plan("transaction.txid", "does not match transaction bytes"));
        }

        Ok(())
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

/// Build a v1 unsigned wallet plan with explicit network identity and nonce.
///
/// This function does not alter PulseDAG consensus serialization or signing
/// semantics. In particular, `network` is not added to `signing_message`.
pub fn build_transaction_plan(
    network: WalletNetworkIdentity,
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
    let built = build_transaction(from, to, amount, fee, available_utxos, nonce)?;
    let plan = plan_from_build(network, intent, built);
    plan.validate_structure()?;
    Ok(plan)
}

fn plan_from_build(
    network: WalletNetworkIdentity,
    intent: WalletTransactionIntent,
    built: BuildTxResponse,
) -> WalletTransactionPlan {
    WalletTransactionPlan {
        network,
        intent,
        nonce_policy: WalletNoncePolicy::ExplicitCallerProvidedV1,
        transaction: built.transaction,
        selected_utxos: built.selected_utxos,
        total_input: built.total_input,
        change: built.change,
        signing_message: built.signing_message,
    }
}

fn invalid_plan(field: &'static str, reason: &'static str) -> WalletPlanError {
    WalletPlanError::InvalidPlanField { field, reason }
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

#[cfg(test)]
mod tests {
    use pulsedag_core::types::{OutPoint, Utxo};

    use super::*;

    fn identity(chain_id: &str) -> WalletNetworkIdentity {
        WalletNetworkIdentity::new("public-testnet", chain_id).expect("valid identity")
    }

    fn sample_utxo() -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            address: "pulse1sender".to_string(),
            amount: 1_000,
            coinbase: false,
            height: 10,
        }
    }

    fn sample_plan() -> WalletTransactionPlan {
        build_transaction_plan(
            identity("pulsedag-public-testnet"),
            "pulse1sender",
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("build plan")
    }

    #[test]
    fn transaction_plan_preserves_explicit_nonce_and_intent() {
        let plan = sample_plan();

        assert_eq!(plan.transaction.nonce, 42);
        assert_eq!(plan.intent.amount, 400);
        assert_eq!(plan.intent.fee, 10);
        assert_eq!(
            plan.nonce_policy,
            WalletNoncePolicy::ExplicitCallerProvidedV1
        );
        plan.validate_structure().expect("valid plan");
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
    fn tampered_review_metadata_is_rejected() {
        let mut plan = sample_plan();
        plan.intent.amount = 399;
        assert!(plan.validate_structure().is_err());

        let mut plan = sample_plan();
        plan.signing_message = "00".repeat(32);
        assert!(plan.validate_structure().is_err());
    }

    #[test]
    fn v1_signing_message_is_not_claimed_to_be_chain_bound() {
        let public_plan = sample_plan();
        let private_plan = build_transaction_plan(
            identity("pulsedag-private-testnet"),
            "pulse1sender",
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("private plan");

        assert_eq!(public_plan.transaction.txid, private_plan.transaction.txid);
        assert_eq!(public_plan.signing_message, private_plan.signing_message);
        assert!(public_plan
            .verify_remote_identity(&private_plan.network)
            .is_err());
    }

    #[test]
    fn unknown_identity_fields_are_rejected() {
        let value = serde_json::json!({
            "network_profile": "public-testnet",
            "chain_id": "pulsedag-public-testnet",
            "unexpected": true
        });
        assert!(serde_json::from_value::<WalletNetworkIdentity>(value).is_err());
    }
}
