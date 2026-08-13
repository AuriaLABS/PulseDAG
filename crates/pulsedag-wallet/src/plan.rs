use std::{error::Error, fmt};

use pulsedag_core::{errors::PulseError, types::Utxo};
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
    pub nonce_policy: WalletNoncePolicy,
    pub transaction: pulsedag_core::types::Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
    pub signing_message: String,
}

impl WalletTransactionPlan {
    pub fn verify_remote_identity(
        &self,
        observed: &WalletNetworkIdentity,
    ) -> Result<(), WalletPlanError> {
        self.network.ensure_matches(observed)
    }

    pub fn verify_keystore_identity(
        &self,
        keystore: &WalletNetworkIdentity,
    ) -> Result<(), WalletPlanError> {
        self.network.ensure_matches(keystore)
    }
}

#[derive(Debug)]
pub enum WalletPlanError {
    InvalidIdentityField {
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
    let built = build_transaction(from, to, amount, fee, available_utxos, nonce)?;
    Ok(plan_from_build(network, built))
}

fn plan_from_build(
    network: WalletNetworkIdentity,
    built: BuildTxResponse,
) -> WalletTransactionPlan {
    WalletTransactionPlan {
        network,
        nonce_policy: WalletNoncePolicy::ExplicitCallerProvidedV1,
        transaction: built.transaction,
        selected_utxos: built.selected_utxos,
        total_input: built.total_input,
        change: built.change,
        signing_message: built.signing_message,
    }
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

    #[test]
    fn transaction_plan_preserves_explicit_nonce() {
        let plan = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            "pulse1sender",
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("build plan");

        assert_eq!(plan.transaction.nonce, 42);
        assert_eq!(
            plan.nonce_policy,
            WalletNoncePolicy::ExplicitCallerProvidedV1
        );
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
    fn v1_signing_message_is_not_claimed_to_be_chain_bound() {
        let public_plan = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            "pulse1sender",
            "pulse1recipient",
            400,
            10,
            &[sample_utxo()],
            42,
        )
        .expect("public plan");
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
