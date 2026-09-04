use std::{error::Error, fmt};

use pulsedag_core::{
    address_from_public_key, compute_txid,
    errors::PulseError,
    signing_message,
    types::{Transaction, Utxo},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::safety::{
    validate_wallet_high_fee_acknowledgement, validate_wallet_safety_acknowledgements,
    WalletFundingSnapshot, WalletSafetyAcknowledgements,
};
use crate::{build_transaction, select_utxos, BuildTxResponse, SelectedUtxo};

pub const WALLET_NONCE_DOMAIN_V1: &str = "PulseDAG:wallet-plan-nonce:v1";

/// Wallet-visible identity of the chain a keystore or transaction plan expects.
///
/// PulseDAG v1 signatures are not cryptographically bound to these strings, so
/// the wallet must verify this identity before preparing a signing request.
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
        let value = Self {
            network_profile: network_profile.into(),
            chain_id: chain_id.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), WalletPlanError> {
        validate_text("network_profile", &self.network_profile)?;
        validate_text("chain_id", &self.chain_id)
    }

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

/// Human-reviewable payment intent. This is the spend request shown to a user
/// before signing, rather than a loose collection of function arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletTransactionIntent {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
}

impl WalletTransactionIntent {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        amount: u64,
        fee: u64,
    ) -> Result<Self, WalletPlanError> {
        let value = Self {
            from: from.into(),
            to: to.into(),
            amount,
            fee,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), WalletPlanError> {
        validate_canonical_legacy_address("intent.from", &self.from)?;
        validate_canonical_legacy_address("intent.to", &self.to)?;
        if self.amount == 0 {
            return Err(invalid_plan("intent.amount", "must be greater than zero"));
        }
        Ok(())
    }
}

/// Explicit authorization limits for a transaction plan. Hard caps remain
/// fail-closed. Separate persisted warning thresholds determine when an otherwise
/// permitted fee requires explicit human acknowledgement before signing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletSpendPolicy {
    pub max_fee: u64,
    pub max_fee_bps_of_amount: u32,
    pub max_inputs: usize,
    pub high_fee_threshold: u64,
    pub high_fee_bps_threshold: u32,
}

impl WalletSpendPolicy {
    /// Backward source-compatible constructor. Warning thresholds equal the
    /// hard caps, so existing callers do not gain a new acknowledgement step.
    pub fn new(
        max_fee: u64,
        max_fee_bps_of_amount: u32,
        max_inputs: usize,
    ) -> Result<Self, WalletPlanError> {
        Self::new_with_high_fee_thresholds(
            max_fee,
            max_fee_bps_of_amount,
            max_inputs,
            max_fee,
            max_fee_bps_of_amount,
        )
    }

    pub fn new_with_high_fee_thresholds(
        max_fee: u64,
        max_fee_bps_of_amount: u32,
        max_inputs: usize,
        high_fee_threshold: u64,
        high_fee_bps_threshold: u32,
    ) -> Result<Self, WalletPlanError> {
        let value = Self {
            max_fee,
            max_fee_bps_of_amount,
            max_inputs,
            high_fee_threshold,
            high_fee_bps_threshold,
        };
        value.validate_configuration()?;
        Ok(value)
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
        if self.high_fee_threshold > self.max_fee {
            return Err(policy_violation(
                "high_fee_threshold",
                "must not exceed the absolute hard fee limit",
            ));
        }
        if self.high_fee_bps_threshold > self.max_fee_bps_of_amount {
            return Err(policy_violation(
                "high_fee_bps_threshold",
                "must not exceed the hard fee-to-amount basis-point limit",
            ));
        }
        Ok(())
    }

    fn validate_intent(&self, intent: &WalletTransactionIntent) -> Result<(), WalletPlanError> {
        self.validate_configuration()?;
        if intent.fee > self.max_fee {
            return Err(policy_violation("fee", "exceeds absolute fee limit"));
        }
        let fee_scaled = u128::from(intent.fee) * 10_000;
        let allowed_scaled = u128::from(intent.amount) * u128::from(self.max_fee_bps_of_amount);
        if fee_scaled > allowed_scaled {
            return Err(policy_violation(
                "fee",
                "exceeds fee-to-amount basis-point limit",
            ));
        }
        Ok(())
    }

    fn is_high_fee(&self, intent: &WalletTransactionIntent) -> Result<bool, WalletPlanError> {
        self.validate_intent(intent)?;
        let exceeds_absolute_warning = intent.fee > self.high_fee_threshold;
        let fee_scaled = u128::from(intent.fee) * 10_000;
        let warning_scaled = u128::from(intent.amount) * u128::from(self.high_fee_bps_threshold);
        Ok(exceeds_absolute_warning || fee_scaled > warning_scaled)
    }

    fn validate_input_count(&self, count: usize) -> Result<(), WalletPlanError> {
        if count > self.max_inputs {
            return Err(policy_violation("inputs", "exceeds input-count limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletNoncePolicy {
    DeterministicPlanV1,
    ExplicitCallerProvidedV1,
}

/// Compact values a CLI/UI can display immediately before authorization.
/// `unsigned_template_txid` is not the final broadcast txid: v1 recomputes the
/// txid after public keys and signatures are attached.
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
    pub self_send: bool,
    pub spend_all: bool,
    pub self_send_acknowledged: bool,
    pub spend_all_acknowledged: bool,
    pub funding_utxo_count: usize,
    pub funding_total_amount: u64,
    pub funding_snapshot_commitment_hex: String,
    pub high_fee: bool,
    pub high_fee_acknowledged: bool,
}

/// Exact public-key-bound signing request. This structure contains no private
/// key and deliberately leaves `transaction.txid` empty until signatures exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletSigningPreparation {
    pub network: WalletNetworkIdentity,
    pub review: WalletReviewSummary,
    pub spend_policy: WalletSpendPolicy,
    pub transaction: Transaction,
    pub signing_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletTransactionPlan {
    pub network: WalletNetworkIdentity,
    pub intent: WalletTransactionIntent,
    pub spend_policy: WalletSpendPolicy,
    pub nonce_policy: WalletNoncePolicy,
    /// Review-only evidence describing the complete funding set observed when
    /// this plan was constructed. It is not part of transaction signing bytes.
    pub funding_snapshot: WalletFundingSnapshot,
    /// Explicit acknowledgement of dangerous transaction shapes. Signing
    /// revalidates these persisted decisions rather than accepting fresh flags.
    pub safety_acknowledgements: WalletSafetyAcknowledgements,
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
}

impl WalletTransactionPlan {
    pub fn validate_structure(&self) -> Result<(), WalletPlanError> {
        self.network.validate()?;
        self.intent.validate()?;
        self.spend_policy.validate_intent(&self.intent)?;
        let high_fee = self.spend_policy.is_high_fee(&self.intent)?;
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
                total
                    .checked_add(selected.amount)
                    .ok_or_else(|| invalid_plan("selected_utxos", "amount total overflows"))
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
        if self.total_input - target != self.change {
            return Err(invalid_plan("change", "does not match reviewed intent"));
        }

        validate_wallet_safety_acknowledgements(
            self.safety_acknowledgements,
            &self.intent.from,
            &self.intent.to,
            &self.funding_snapshot,
            &self.selected_utxos,
            self.total_input,
            self.change,
        )?;
        validate_wallet_high_fee_acknowledgement(self.safety_acknowledgements, high_fee)?;

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
                    "unsigned plan must not contain keys or signatures",
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

        if self.nonce_policy == WalletNoncePolicy::DeterministicPlanV1 {
            let expected_nonce = derive_wallet_plan_nonce_v1(&self.intent, &self.selected_utxos)?;
            if self.transaction.nonce != expected_nonce {
                return Err(invalid_plan(
                    "transaction.nonce",
                    "does not match deterministic wallet nonce policy",
                ));
            }
        }

        let expected_outputs = if self.change == 0 { 1 } else { 2 };
        if self.transaction.outputs.len() != expected_outputs {
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
                "does not match unsigned template",
            ));
        }
        Ok(())
    }

    pub fn review_summary(&self) -> Result<WalletReviewSummary, WalletPlanError> {
        self.validate_structure()?;
        let self_send = self.intent.from == self.intent.to;
        let spend_all = self.funding_snapshot.is_spend_all(
            &self.selected_utxos,
            self.total_input,
            self.change,
        )?;
        let high_fee = self.spend_policy.is_high_fee(&self.intent)?;
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
            self_send,
            spend_all,
            self_send_acknowledged: self.safety_acknowledgements.self_send,
            spend_all_acknowledged: self.safety_acknowledgements.spend_all,
            funding_utxo_count: self.funding_snapshot.utxo_count,
            funding_total_amount: self.funding_snapshot.total_amount,
            funding_snapshot_commitment_hex: self.funding_snapshot.commitment_hex.clone(),
            high_fee,
            high_fee_acknowledged: self.safety_acknowledgements.high_fee,
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

    /// Re-check the network, prove the supplied public key controls the sender
    /// address, attach it to every input, and derive the exact v1 signing bytes.
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
            Self::InvalidPublicKey { reason } => write!(f, "invalid wallet public key: {reason}"),
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

/// Derive the wallet-only v1 plan nonce from the reviewed intent and the exact
/// selected inputs. The nonce is a deterministic plan identifier/salt, not an
/// account sequence, anti-replay primitive, or cryptographic chain binding.
pub fn derive_wallet_plan_nonce_v1(
    intent: &WalletTransactionIntent,
    selected_utxos: &[SelectedUtxo],
) -> Result<u64, WalletPlanError> {
    intent.validate()?;
    if selected_utxos.is_empty() {
        return Err(invalid_plan(
            "selected_utxos",
            "deterministic nonce requires at least one selected input",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(WALLET_NONCE_DOMAIN_V1.as_bytes());
    hasher.update(1_u32.to_be_bytes());
    hash_len_prefixed(&mut hasher, intent.from.as_bytes());
    hash_len_prefixed(&mut hasher, intent.to.as_bytes());
    hasher.update(intent.amount.to_be_bytes());
    hasher.update(intent.fee.to_be_bytes());
    hasher.update((selected_utxos.len() as u64).to_be_bytes());

    for (index, selected) in selected_utxos.iter().enumerate() {
        if selected_utxos[..index]
            .iter()
            .any(|prior| prior.outpoint == selected.outpoint)
        {
            return Err(invalid_plan(
                "selected_utxos",
                "contains a duplicate outpoint",
            ));
        }
        hash_len_prefixed(&mut hasher, selected.outpoint.txid.as_bytes());
        hasher.update(selected.outpoint.index.to_be_bytes());
        hasher.update(selected.amount.to_be_bytes());
    }

    let digest = hasher.finalize();
    Ok(u64::from_be_bytes(digest[..8].try_into().expect(
        "SHA-256 digest always contains eight nonce bytes",
    )))
}

/// Build the supported wallet v1 plan with a deterministic nonce. Identical
/// intent plus identical selected inputs produces the same nonce and unsigned
/// template identifier for safe retry/resubmission. Any recipient, amount, fee,
/// or selected-input change produces a distinct nonce with overwhelming
/// probability. Network identity is deliberately not part of this hash because
/// v1 signatures are not cryptographically chain-bound; identity is verified
/// separately and fail-closed before signing.
pub fn build_deterministic_transaction_plan(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    intent: WalletTransactionIntent,
    available_utxos: &[Utxo],
) -> Result<WalletTransactionPlan, WalletPlanError> {
    build_deterministic_transaction_plan_with_safety(
        network,
        spend_policy,
        intent,
        available_utxos,
        WalletSafetyAcknowledgements::none(),
    )
}

/// Build a deterministic wallet v1 plan while persisting explicit safety
/// acknowledgements. The complete funding snapshot is derived before selection
/// and is review metadata only; it does not alter nonce or transaction bytes.
pub fn build_deterministic_transaction_plan_with_safety(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    intent: WalletTransactionIntent,
    available_utxos: &[Utxo],
    safety_acknowledgements: WalletSafetyAcknowledgements,
) -> Result<WalletTransactionPlan, WalletPlanError> {
    network.validate()?;
    intent.validate()?;
    spend_policy.validate_intent(&intent)?;
    WalletFundingSnapshot::from_utxos(available_utxos)?;
    let target = intent
        .amount
        .checked_add(intent.fee)
        .ok_or_else(|| invalid_plan("intent.amount", "amount plus fee overflows"))?;
    let (selected, _) = select_utxos(available_utxos, target)?;
    spend_policy.validate_input_count(selected.len())?;
    let selected_utxos = selected
        .iter()
        .map(|utxo| SelectedUtxo {
            outpoint: utxo.outpoint.clone(),
            amount: utxo.amount,
        })
        .collect::<Vec<_>>();
    let nonce = derive_wallet_plan_nonce_v1(&intent, &selected_utxos)?;
    let mut plan = build_transaction_plan_with_safety(
        network,
        spend_policy,
        intent,
        available_utxos,
        nonce,
        safety_acknowledgements,
    )?;
    plan.nonce_policy = WalletNoncePolicy::DeterministicPlanV1;
    plan.validate_structure()?;
    Ok(plan)
}

/// Low-level compatibility builder. Supported wallet application flows should
/// prefer `build_deterministic_transaction_plan`; this function retains explicit
/// caller-provided v1 nonce construction for compatibility and protocol tests.
pub fn build_transaction_plan(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    intent: WalletTransactionIntent,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<WalletTransactionPlan, WalletPlanError> {
    build_transaction_plan_with_safety(
        network,
        spend_policy,
        intent,
        available_utxos,
        nonce,
        WalletSafetyAcknowledgements::none(),
    )
}

/// Low-level explicit-nonce builder with persisted safety acknowledgements.
pub fn build_transaction_plan_with_safety(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    intent: WalletTransactionIntent,
    available_utxos: &[Utxo],
    nonce: u64,
    safety_acknowledgements: WalletSafetyAcknowledgements,
) -> Result<WalletTransactionPlan, WalletPlanError> {
    network.validate()?;
    intent.validate()?;
    spend_policy.validate_intent(&intent)?;
    let funding_snapshot = WalletFundingSnapshot::from_utxos(available_utxos)?;
    let built = build_transaction(
        &intent.from,
        &intent.to,
        intent.amount,
        intent.fee,
        available_utxos,
        nonce,
    )?;
    spend_policy.validate_input_count(built.selected_utxos.len())?;
    let plan = plan_from_build(
        network,
        spend_policy,
        intent,
        built,
        funding_snapshot,
        safety_acknowledgements,
    );
    plan.validate_structure()?;
    Ok(plan)
}

fn plan_from_build(
    network: WalletNetworkIdentity,
    spend_policy: WalletSpendPolicy,
    intent: WalletTransactionIntent,
    built: BuildTxResponse,
    funding_snapshot: WalletFundingSnapshot,
    safety_acknowledgements: WalletSafetyAcknowledgements,
) -> WalletTransactionPlan {
    WalletTransactionPlan {
        network,
        intent,
        spend_policy,
        nonce_policy: WalletNoncePolicy::ExplicitCallerProvidedV1,
        funding_snapshot,
        safety_acknowledgements,
        transaction: built.transaction,
        selected_utxos: built.selected_utxos,
        total_input: built.total_input,
        change: built.change,
    }
}

fn hash_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn invalid_plan(field: &'static str, reason: &'static str) -> WalletPlanError {
    WalletPlanError::InvalidPlanField { field, reason }
}

fn policy_violation(rule: &'static str, reason: &'static str) -> WalletPlanError {
    WalletPlanError::PolicyViolation { rule, reason }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), WalletPlanError> {
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

fn validate_canonical_legacy_address(
    field: &'static str,
    value: &str,
) -> Result<(), WalletPlanError> {
    validate_text(field, value)?;
    let payload = value.strip_prefix("pulse1").ok_or_else(|| {
        invalid_plan(
            field,
            "must use the active canonical pulse1 legacy address format",
        )
    })?;
    if payload.len() != 40 {
        return Err(invalid_plan(
            field,
            "must contain exactly 40 lowercase hexadecimal payload characters",
        ));
    }
    if !payload
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(invalid_plan(
            field,
            "must contain only lowercase hexadecimal payload characters",
        ));
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

    fn high_fee_policy(absolute_warning: u64, bps_warning: u32) -> WalletSpendPolicy {
        WalletSpendPolicy::new_with_high_fee_thresholds(
            100,
            1_000,
            8,
            absolute_warning,
            bps_warning,
        )
        .expect("valid high-fee policy")
    }

    fn public_key() -> String {
        "ab".repeat(32)
    }

    fn sender_address() -> String {
        address_from_public_key(&public_key())
    }

    fn recipient_address() -> String {
        address_from_public_key(&"cd".repeat(32))
    }

    fn alternate_recipient_address() -> String {
        address_from_public_key(&"ef".repeat(32))
    }

    fn intent(amount: u64, fee: u64) -> WalletTransactionIntent {
        WalletTransactionIntent::new(sender_address(), recipient_address(), amount, fee)
            .expect("valid intent")
    }

    fn self_send_intent(amount: u64, fee: u64) -> WalletTransactionIntent {
        WalletTransactionIntent::new(sender_address(), sender_address(), amount, fee)
            .expect("valid self-send intent")
    }

    fn utxo(txid_byte: &str, amount: u64, height: u64) -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: txid_byte.repeat(32),
                index: 0,
            },
            address: sender_address(),
            amount,
            coinbase: false,
            height,
        }
    }

    fn sample_plan() -> WalletTransactionPlan {
        build_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &[utxo("11", 1_000, 10)],
            42,
        )
        .expect("build plan")
    }

    fn deterministic_plan() -> WalletTransactionPlan {
        build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &[utxo("11", 1_000, 10)],
        )
        .expect("build deterministic plan")
    }

    #[test]
    fn plan_preserves_reviewed_intent_nonce_and_template_id() {
        let plan = sample_plan();
        let review = plan.review_summary().expect("review summary");
        assert_eq!(review.chain_id, "pulsedag-public-testnet");
        assert_eq!(review.amount, 400);
        assert_eq!(review.fee, 10);
        assert_eq!(review.input_count, 1);
        assert_eq!(review.nonce, 42);
        assert_eq!(review.unsigned_template_txid, plan.transaction.txid);
        assert!(!review.self_send);
        assert!(!review.spend_all);
        assert!(!review.high_fee);
        assert!(!review.high_fee_acknowledged);
        assert_eq!(review.funding_utxo_count, 1);
        assert_eq!(review.funding_total_amount, 1_000);
        assert_eq!(
            plan.nonce_policy,
            WalletNoncePolicy::ExplicitCallerProvidedV1
        );
    }

    #[test]
    fn canonical_legacy_addresses_are_required_for_both_intent_sides() {
        let sender = sender_address();
        let recipient = recipient_address();
        assert!(WalletTransactionIntent::new(&sender, &recipient, 1, 0).is_ok());

        let malformed = [
            format!(" {recipient}"),
            format!("{recipient} "),
            recipient.to_uppercase(),
            format!("pulse1{}", "a".repeat(39)),
            format!("pulse1{}", "a".repeat(41)),
            format!("pulse1{}g", "a".repeat(39)),
            format!("pulseq1{}", "a".repeat(40)),
        ];
        for address in malformed {
            assert!(WalletTransactionIntent::new(&sender, &address, 1, 0).is_err());
            assert!(WalletTransactionIntent::new(&address, &recipient, 1, 0).is_err());
        }
    }

    #[test]
    fn deterministic_nonce_vector_is_frozen() {
        let intent = WalletTransactionIntent::new(sender_address(), recipient_address(), 400, 10)
            .expect("vector intent");
        let selected = vec![SelectedUtxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            amount: 1_000,
        }];
        assert_eq!(
            derive_wallet_plan_nonce_v1(&intent, &selected).expect("nonce vector"),
            9_502_285_333_810_140_450
        );
    }

    #[test]
    fn deterministic_retry_and_plan_change_semantics_are_stable() {
        let available = [utxo("11", 1_000, 10), utxo("22", 1_000, 11)];
        let first = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &available,
        )
        .expect("first deterministic plan");
        let retry = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &available,
        )
        .expect("retry deterministic plan");
        assert_eq!(first.nonce_policy, WalletNoncePolicy::DeterministicPlanV1);
        assert_eq!(first.transaction.nonce, retry.transaction.nonce);
        assert_eq!(first.transaction.txid, retry.transaction.txid);

        let destination_change = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            WalletTransactionIntent::new(sender_address(), alternate_recipient_address(), 400, 10)
                .expect("destination-change intent"),
            &available,
        )
        .expect("destination-change plan");
        let amount_change = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(401, 10),
            &available,
        )
        .expect("amount-change plan");
        let fee_change = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 11),
            &available,
        )
        .expect("fee-change plan");
        let input_change = build_deterministic_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(1_500, 10),
            &available,
        )
        .expect("input-change plan");

        for changed in [destination_change, amount_change, fee_change, input_change] {
            assert_ne!(first.transaction.nonce, changed.transaction.nonce);
        }
    }

    #[test]
    fn deterministic_plan_round_trip_and_nonce_tamper_are_checked() {
        let plan = deterministic_plan();
        let json = serde_json::to_string(&plan).expect("serialize deterministic plan");
        let mut decoded: WalletTransactionPlan =
            serde_json::from_str(&json).expect("deserialize deterministic plan");
        assert_eq!(decoded.nonce_policy, WalletNoncePolicy::DeterministicPlanV1);
        assert_eq!(decoded.funding_snapshot, plan.funding_snapshot);
        assert_eq!(
            decoded.safety_acknowledgements,
            plan.safety_acknowledgements
        );
        decoded.validate_structure().expect("validate round trip");

        decoded.transaction.nonce ^= 1;
        decoded.transaction.txid = compute_txid(&decoded.transaction);
        assert!(decoded.validate_structure().is_err());
    }

    #[test]
    fn old_unsigned_policy_without_warning_thresholds_fails_closed() {
        let plan = deterministic_plan();
        let mut value = serde_json::to_value(plan).expect("serialize plan value");
        let policy = value["spend_policy"]
            .as_object_mut()
            .expect("serialized policy object");
        policy.remove("high_fee_threshold");
        policy.remove("high_fee_bps_threshold");
        assert!(serde_json::from_value::<WalletTransactionPlan>(value).is_err());
    }

    #[test]
    fn malformed_plan_intent_and_destination_fail_before_signing() {
        let plan = deterministic_plan();
        let mut serialized = serde_json::to_value(&plan).expect("serialize plan value");
        serialized["intent"]["to"] = serde_json::json!(format!("pulseq1{}", "a".repeat(40)));
        let decoded: WalletTransactionPlan =
            serde_json::from_value(serialized).expect("schema remains parseable");
        assert!(decoded.validate_structure().is_err());

        let mut tampered = sample_plan();
        tampered.intent.to = format!("pulseq1{}", "b".repeat(40));
        tampered.transaction.outputs[0].address = tampered.intent.to.clone();
        tampered.transaction.txid = compute_txid(&tampered.transaction);
        assert!(tampered
            .prepare_signing(&identity("pulsedag-public-testnet"), &public_key())
            .is_err());
    }

    #[test]
    fn self_send_requires_persisted_acknowledgement() {
        let available = [utxo("11", 1_000, 10)];
        assert!(build_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            self_send_intent(400, 10),
            &available,
            42,
        )
        .is_err());

        let plan = build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            policy(),
            self_send_intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::new(true, false),
        )
        .expect("acknowledged self-send");
        let review = plan.review_summary().expect("review self-send");
        assert!(review.self_send);
        assert!(review.self_send_acknowledged);
        assert!(!review.spend_all);
    }

    #[test]
    fn true_spend_all_requires_persisted_acknowledgement() {
        let available = [utxo("11", 410, 10)];
        assert!(build_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &available,
            42,
        )
        .is_err());

        let plan = build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::new(false, true),
        )
        .expect("acknowledged spend-all");
        let review = plan.review_summary().expect("review spend-all");
        assert!(review.spend_all);
        assert!(review.spend_all_acknowledged);
        assert_eq!(plan.change, 0);
    }

    #[test]
    fn high_fee_requires_persisted_acknowledgement_and_review_recomputes_it() {
        let available = [utxo("11", 1_000, 10)];
        let high_policy = high_fee_policy(9, 1_000);
        assert!(build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            high_policy.clone(),
            intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::none(),
        )
        .is_err());

        let plan = build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            high_policy,
            intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::new_with_high_fee(false, false, true),
        )
        .expect("acknowledged high fee");
        let review = plan.review_summary().expect("high-fee review");
        assert!(review.high_fee);
        assert!(review.high_fee_acknowledged);
    }

    #[test]
    fn high_fee_absolute_and_ratio_thresholds_are_strictly_greater_than() {
        let normal = intent(400, 10);
        assert!(!high_fee_policy(10, 250)
            .is_high_fee(&normal)
            .expect("exact thresholds"));
        assert!(high_fee_policy(9, 1_000)
            .is_high_fee(&normal)
            .expect("absolute warning"));
        assert!(high_fee_policy(100, 200)
            .is_high_fee(&normal)
            .expect("ratio warning"));
    }

    #[test]
    fn high_fee_warning_thresholds_must_not_exceed_hard_caps() {
        assert!(WalletSpendPolicy::new_with_high_fee_thresholds(100, 1_000, 8, 101, 1_000)
            .is_err());
        assert!(WalletSpendPolicy::new_with_high_fee_thresholds(100, 1_000, 8, 100, 1_001)
            .is_err());
    }

    #[test]
    fn high_fee_acknowledgement_never_overrides_hard_caps() {
        let available = [utxo("11", 1_000, 10)];
        let ack = WalletSafetyAcknowledgements::new_with_high_fee(false, false, true);
        let absolute = WalletSpendPolicy::new_with_high_fee_thresholds(9, 1_000, 8, 5, 500)
            .expect("absolute hard-cap policy");
        assert!(build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            absolute,
            intent(400, 10),
            &available,
            42,
            ack,
        )
        .is_err());

        let ratio = WalletSpendPolicy::new_with_high_fee_thresholds(100, 200, 8, 50, 100)
            .expect("ratio hard-cap policy");
        assert!(build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            ratio,
            intent(400, 10),
            &available,
            42,
            ack,
        )
        .is_err());
    }

    #[test]
    fn high_fee_tampering_fails_before_offline_signing() {
        let available = [utxo("11", 1_000, 10)];
        let mut plan = build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            high_fee_policy(9, 1_000),
            intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::new_with_high_fee(false, false, true),
        )
        .expect("acknowledged high-fee plan");
        plan.safety_acknowledgements.high_fee = false;
        assert!(plan.validate_structure().is_err());
        assert!(plan
            .prepare_signing(&identity("pulsedag-public-testnet"), &public_key())
            .is_err());

        let mut thresholds = sample_plan();
        thresholds.spend_policy.high_fee_threshold = thresholds.spend_policy.max_fee + 1;
        assert!(thresholds.validate_structure().is_err());
        assert!(thresholds
            .prepare_signing(&identity("pulsedag-public-testnet"), &public_key())
            .is_err());
    }

    #[test]
    fn zero_change_partial_snapshot_is_not_spend_all() {
        let available = [utxo("11", 410, 10), utxo("22", 1, 11)];
        let plan = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            policy(),
            intent(400, 10),
            &available,
            42,
        )
        .expect("partial zero-change plan");
        assert_eq!(plan.change, 0);
        assert_eq!(plan.selected_utxos.len(), 1);
        assert!(!plan.review_summary().expect("review").spend_all);
    }

    #[test]
    fn safety_and_snapshot_tampering_fail_before_signing() {
        let available = [utxo("11", 1_000, 10)];
        let mut self_send = build_transaction_plan_with_safety(
            identity("pulsedag-public-testnet"),
            policy(),
            self_send_intent(400, 10),
            &available,
            42,
            WalletSafetyAcknowledgements::new(true, false),
        )
        .expect("acknowledged self-send");
        self_send.safety_acknowledgements.self_send = false;
        assert!(self_send.validate_structure().is_err());
        assert!(self_send
            .prepare_signing(&identity("pulsedag-public-testnet"), &public_key())
            .is_err());

        let mut snapshot = sample_plan();
        snapshot.funding_snapshot.total_amount += 1;
        assert!(snapshot.validate_structure().is_err());
        assert!(snapshot
            .prepare_signing(&identity("pulsedag-public-testnet"), &public_key())
            .is_err());
    }

    #[test]
    fn signing_preparation_attaches_public_key_after_network_check() {
        let plan = sample_plan();
        let key = public_key();
        let prepared = plan
            .prepare_signing(&identity("pulsedag-public-testnet"), &key)
            .expect("prepare signing");
        assert!(prepared.transaction.txid.is_empty());
        assert!(prepared
            .transaction
            .inputs
            .iter()
            .all(|input| input.public_key == key && input.signature.is_empty()));
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
    fn wrong_network_or_sender_public_key_fails_closed() {
        let plan = sample_plan();
        assert!(plan
            .prepare_signing(&identity("pulsedag-private-testnet"), &public_key())
            .is_err());
        assert!(matches!(
            plan.prepare_signing(&identity("pulsedag-public-testnet"), &"22".repeat(32)),
            Err(WalletPlanError::PublicKeyAddressMismatch { .. })
        ));
    }

    #[test]
    fn public_key_encoding_must_be_canonical() {
        let plan = sample_plan();
        assert!(matches!(
            plan.prepare_signing(
                &identity("pulsedag-public-testnet"),
                &public_key().to_uppercase()
            ),
            Err(WalletPlanError::InvalidPublicKey { .. })
        ));
        assert!(matches!(
            plan.prepare_signing(&identity("pulsedag-public-testnet"), "11"),
            Err(WalletPlanError::InvalidPublicKey { .. })
        ));
    }

    #[test]
    fn spend_policy_enforces_fee_and_input_limits() {
        let fee_error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(5, 10_000, 8).expect("policy"),
            intent(400, 10),
            &[utxo("11", 1_000, 10)],
            42,
        )
        .expect_err("absolute fee cap");
        assert!(matches!(fee_error, WalletPlanError::PolicyViolation { .. }));

        let input_error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(100, 10_000, 1).expect("policy"),
            intent(1_500, 0),
            &[utxo("11", 1_000, 10), utxo("22", 1_000, 11)],
            42,
        )
        .expect_err("input cap");
        assert!(matches!(
            input_error,
            WalletPlanError::PolicyViolation { .. }
        ));

        let ratio_error = build_transaction_plan(
            identity("pulsedag-public-testnet"),
            WalletSpendPolicy::new(1_000, 100, 8).expect("policy"),
            intent(400, 10),
            &[utxo("11", 1_000, 10)],
            42,
        )
        .expect_err("fee ratio cap");
        assert!(matches!(
            ratio_error,
            WalletPlanError::PolicyViolation { .. }
        ));
    }

    #[test]
    fn tampering_duplicate_inputs_and_signed_templates_are_rejected() {
        let mut plan = sample_plan();
        plan.intent.amount = 399;
        assert!(plan.validate_structure().is_err());

        let mut plan = sample_plan();
        plan.transaction.inputs[0].signature = "unexpected".to_string();
        plan.transaction.txid = compute_txid(&plan.transaction);
        assert!(plan.validate_structure().is_err());

        let mut plan = sample_plan();
        let duplicate = plan.selected_utxos[0].clone();
        plan.selected_utxos.push(duplicate.clone());
        plan.transaction
            .inputs
            .push(plan.transaction.inputs[0].clone());
        plan.total_input += duplicate.amount;
        plan.change += duplicate.amount;
        assert!(plan.validate_structure().is_err());
    }

    #[test]
    fn v1_signing_preimage_is_not_claimed_to_be_chain_bound() {
        let public_plan = deterministic_plan();
        let private_plan = build_deterministic_transaction_plan(
            identity("pulsedag-private-testnet"),
            policy(),
            intent(400, 10),
            &[utxo("11", 1_000, 10)],
        )
        .expect("private plan");
        assert_eq!(
            public_plan.transaction.nonce,
            private_plan.transaction.nonce
        );
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
    fn unknown_schema_fields_and_invalid_identity_are_rejected() {
        assert!(WalletNetworkIdentity::new("", "chain").is_err());
        assert!(WalletNetworkIdentity::new("public-testnet", " chain").is_err());

        let value = serde_json::json!({
            "from": sender_address(),
            "to": recipient_address(),
            "amount": 1,
            "fee": 0,
            "unexpected": true
        });
        assert!(serde_json::from_value::<WalletTransactionIntent>(value).is_err());
    }
}
