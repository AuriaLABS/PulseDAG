//! Planning matcher for `vault_v1`.
//!
//! Cites PulseClock `pulse_height` for delay checks. Does not admit vault
//! outputs into mempool or consensus. Default evaluation is fail-closed:
//! `covenants_enabled` and template admission are both off.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::pulseclock_v1::{PulseClockV1Error, PulseObservationV1};

pub const VAULT_TEMPLATE_ID_V1: &str = "vault_v1";
pub const VAULT_DOMAIN_V1: &str = "PulseDAG:covenant:vault:v1";
pub const VAULT_OWNER_DELAY_MIN_PULSES_V1: u32 = 64;
pub const VAULT_OWNER_DELAY_MAX_PULSES_V1: u32 = 1_048_576;
pub const VAULT_EMERGENCY_DELAY_MAX_PULSES_V1: u32 = 2_097_152;
pub const VAULT_EMERGENCY_DELAY_GAP_PULSES_V1: u32 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultV1Output {
    pub template_id: String,
    pub owner_pk: String,
    pub emergency_pk: String,
    pub owner_delay_pulses: u32,
    pub emergency_delay_pulses: u32,
    pub created_pulse_height: u64,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultSpendPathV1 {
    Owner,
    Emergency,
}

impl VaultSpendPathV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Emergency => "emergency",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSpendWitnessV1 {
    pub path: VaultSpendPathV1,
    pub chain_id: String,
    pub outpoint_txid: String,
    pub outpoint_index: u32,
    pub public_key: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl VaultAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultV1Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    OwnerAndEmergencyKeysMustDiffer,
    EmptyKey { field: &'static str },
    OwnerDelayOutOfRange { delay: u32 },
    EmergencyDelayOutOfRange { delay: u32 },
    PulseClockUnavailable,
    OwnerSpendTooEarly { current: u64, required: u64 },
    EmergencySpendTooEarly { current: u64, required: u64 },
    SpendKeyMismatch { path: &'static str },
    EmptyChainId,
    InvalidSignature,
}

impl std::fmt::Display for VaultV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "vault_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(f, "unknown covenant template {observed}; not vault_v1")
            }
            Self::OwnerAndEmergencyKeysMustDiffer => {
                write!(f, "owner_pk and emergency_pk must be distinct")
            }
            Self::EmptyKey { field } => write!(f, "{field} must not be empty"),
            Self::OwnerDelayOutOfRange { delay } => {
                write!(f, "owner_delay_pulses {delay} is outside planning bounds")
            }
            Self::EmergencyDelayOutOfRange { delay } => write!(
                f,
                "emergency_delay_pulses {delay} is outside planning bounds"
            ),
            Self::PulseClockUnavailable => {
                write!(
                    f,
                    "PulseClock metadata unavailable; vault spend fails closed"
                )
            }
            Self::OwnerSpendTooEarly { current, required } => {
                write!(
                    f,
                    "owner spend at pulse {current} before required {required}"
                )
            }
            Self::EmergencySpendTooEarly { current, required } => write!(
                f,
                "emergency spend at pulse {current} before required {required}"
            ),
            Self::SpendKeyMismatch { path } => {
                write!(f, "spend key does not match {path} path")
            }
            Self::EmptyChainId => write!(f, "vault spend chain_id must not be empty"),
            Self::InvalidSignature => {
                write!(f, "vault spend signature failed domain verification")
            }
        }
    }
}

impl std::error::Error for VaultV1Error {}

impl From<PulseClockV1Error> for VaultV1Error {
    fn from(_: PulseClockV1Error) -> Self {
        Self::PulseClockUnavailable
    }
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

/// Domain-bound message: `PulseDAG:covenant:vault:v1` + chain_id + outpoint + path.
pub fn vault_spend_signing_message_v1(
    witness: &VaultSpendWitnessV1,
) -> Result<Vec<u8>, VaultV1Error> {
    if witness.chain_id.is_empty() {
        return Err(VaultV1Error::EmptyChainId);
    }
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, VAULT_DOMAIN_V1.as_bytes());
    encode_len_prefixed(&mut out, witness.chain_id.as_bytes());
    encode_len_prefixed(&mut out, witness.outpoint_txid.as_bytes());
    out.extend_from_slice(&witness.outpoint_index.to_le_bytes());
    encode_len_prefixed(&mut out, witness.path.as_str().as_bytes());
    Ok(out)
}

pub fn verify_vault_spend_signature_v1(
    output: &VaultV1Output,
    witness: &VaultSpendWitnessV1,
) -> Result<(), VaultV1Error> {
    let expected_pk = match witness.path {
        VaultSpendPathV1::Owner => output.owner_pk.as_str(),
        VaultSpendPathV1::Emergency => output.emergency_pk.as_str(),
    };
    if witness.public_key != expected_pk {
        return Err(VaultV1Error::SpendKeyMismatch {
            path: witness.path.as_str(),
        });
    }
    let message = vault_spend_signing_message_v1(witness)?;
    let pk_bytes = hex::decode(&witness.public_key).map_err(|_| VaultV1Error::InvalidSignature)?;
    let sig_bytes =
        hex::decode(&witness.signature_hex).map_err(|_| VaultV1Error::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| VaultV1Error::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| VaultV1Error::InvalidSignature)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pk_arr).map_err(|_| VaultV1Error::InvalidSignature)?;
    let signature = Signature::from_bytes(&sig_arr);
    verifying_key
        .verify(&message, &signature)
        .map_err(|_| VaultV1Error::InvalidSignature)
}

pub fn validate_vault_output_v1(output: &VaultV1Output) -> Result<(), VaultV1Error> {
    if output.template_id != VAULT_TEMPLATE_ID_V1 {
        return Err(VaultV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    if output.owner_pk.is_empty() {
        return Err(VaultV1Error::EmptyKey { field: "owner_pk" });
    }
    if output.emergency_pk.is_empty() {
        return Err(VaultV1Error::EmptyKey {
            field: "emergency_pk",
        });
    }
    if output.owner_pk == output.emergency_pk {
        return Err(VaultV1Error::OwnerAndEmergencyKeysMustDiffer);
    }
    if output.owner_delay_pulses < VAULT_OWNER_DELAY_MIN_PULSES_V1
        || output.owner_delay_pulses > VAULT_OWNER_DELAY_MAX_PULSES_V1
    {
        return Err(VaultV1Error::OwnerDelayOutOfRange {
            delay: output.owner_delay_pulses,
        });
    }
    let min_emergency = output
        .owner_delay_pulses
        .saturating_add(VAULT_EMERGENCY_DELAY_GAP_PULSES_V1);
    if output.emergency_delay_pulses < min_emergency
        || output.emergency_delay_pulses > VAULT_EMERGENCY_DELAY_MAX_PULSES_V1
    {
        return Err(VaultV1Error::EmergencyDelayOutOfRange {
            delay: output.emergency_delay_pulses,
        });
    }
    Ok(())
}

pub fn created_pulse_height_from_tip(pulse: &PulseObservationV1) -> u64 {
    pulse.pulse_height
}

pub fn owner_unlock_height(output: &VaultV1Output) -> u64 {
    output
        .created_pulse_height
        .saturating_add(u64::from(output.owner_delay_pulses))
}

pub fn emergency_unlock_height(output: &VaultV1Output) -> u64 {
    output
        .created_pulse_height
        .saturating_add(u64::from(output.emergency_delay_pulses))
}

pub fn pulses_remaining_owner(output: &VaultV1Output, current_pulse_height: u64) -> u64 {
    owner_unlock_height(output).saturating_sub(current_pulse_height)
}

/// Evaluate a vault spend: admission, shape, domain signature, PulseClock height.
pub fn evaluate_vault_spend_v1(
    admission: VaultAdmissionV1,
    output: &VaultV1Output,
    witness: &VaultSpendWitnessV1,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<(), VaultV1Error> {
    if !admission.admitted() {
        return Err(VaultV1Error::TemplateDisabled);
    }
    validate_vault_output_v1(output)?;
    verify_vault_spend_signature_v1(output, witness)?;
    let pulse = pulse.map_err(VaultV1Error::from)?;
    match witness.path {
        VaultSpendPathV1::Owner => {
            let required = owner_unlock_height(output);
            if pulse.pulse_height < required {
                return Err(VaultV1Error::OwnerSpendTooEarly {
                    current: pulse.pulse_height,
                    required,
                });
            }
        }
        VaultSpendPathV1::Emergency => {
            let required = emergency_unlock_height(output);
            if pulse.pulse_height < required {
                return Err(VaultV1Error::EmergencySpendTooEarly {
                    current: pulse.pulse_height,
                    required,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};
    use ed25519_dalek::{Signer, SigningKey};

    fn pulse_at(height: u64) -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "vault-test".into(),
            selected_tip: "tip".into(),
            pulse_height: height,
            pulse_time: 1_700_000_000,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 1,
            finality_lag: height,
        }
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn pk_hex(key: &SigningKey) -> String {
        hex::encode(key.verifying_key().to_bytes())
    }

    fn fixture() -> (SigningKey, SigningKey, VaultV1Output) {
        let owner = key(1);
        let emergency = key(2);
        let output = VaultV1Output {
            template_id: VAULT_TEMPLATE_ID_V1.into(),
            owner_pk: pk_hex(&owner),
            emergency_pk: pk_hex(&emergency),
            owner_delay_pulses: 64,
            emergency_delay_pulses: 128,
            created_pulse_height: 100,
            amount: 1_000,
        };
        (owner, emergency, output)
    }

    fn sign(
        signing_key: &SigningKey,
        path: VaultSpendPathV1,
        chain_id: &str,
    ) -> VaultSpendWitnessV1 {
        let mut witness = VaultSpendWitnessV1 {
            path,
            chain_id: chain_id.into(),
            outpoint_txid: "aa".repeat(32),
            outpoint_index: 0,
            public_key: hex::encode(signing_key.verifying_key().to_bytes()),
            signature_hex: String::new(),
        };
        let message = vault_spend_signing_message_v1(&witness).expect("chain id");
        witness.signature_hex = hex::encode(signing_key.sign(&message).to_bytes());
        witness
    }

    fn admitted() -> VaultAdmissionV1 {
        VaultAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_even_after_delay() {
        let (owner, _, output) = fixture();
        let pulse = pulse_at(10_000);
        let witness = sign(&owner, VaultSpendPathV1::Owner, "vault-test");
        let err =
            evaluate_vault_spend_v1(VaultAdmissionV1::INACTIVE, &output, &witness, Ok(&pulse))
                .unwrap_err();
        assert_eq!(err, VaultV1Error::TemplateDisabled);
    }

    #[test]
    fn owner_spend_one_pulse_early_is_rejected() {
        let (owner, _, output) = fixture();
        let pulse = pulse_at(163);
        let witness = sign(&owner, VaultSpendPathV1::Owner, "vault-test");
        let err = evaluate_vault_spend_v1(admitted(), &output, &witness, Ok(&pulse)).unwrap_err();
        assert_eq!(
            err,
            VaultV1Error::OwnerSpendTooEarly {
                current: 163,
                required: 164
            }
        );
    }

    #[test]
    fn owner_spend_at_exact_delay_boundary_is_accepted_when_admitted() {
        let (owner, _, output) = fixture();
        let pulse = pulse_at(164);
        let witness = sign(&owner, VaultSpendPathV1::Owner, "vault-test");
        evaluate_vault_spend_v1(admitted(), &output, &witness, Ok(&pulse)).unwrap();
    }

    #[test]
    fn emergency_spend_at_exact_boundary_is_accepted_when_admitted() {
        let (_, emergency, output) = fixture();
        let pulse = pulse_at(228);
        let witness = sign(&emergency, VaultSpendPathV1::Emergency, "vault-test");
        evaluate_vault_spend_v1(admitted(), &output, &witness, Ok(&pulse)).unwrap();
    }

    #[test]
    fn emergency_spend_early_is_rejected() {
        let (_, emergency, output) = fixture();
        let pulse = pulse_at(227);
        let witness = sign(&emergency, VaultSpendPathV1::Emergency, "vault-test");
        let err = evaluate_vault_spend_v1(admitted(), &output, &witness, Ok(&pulse)).unwrap_err();
        assert_eq!(
            err,
            VaultV1Error::EmergencySpendTooEarly {
                current: 227,
                required: 228
            }
        );
    }

    #[test]
    fn equal_keys_and_bad_delays_are_rejected() {
        let (owner, _, mut output) = fixture();
        output.emergency_pk = output.owner_pk.clone();
        assert_eq!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::OwnerAndEmergencyKeysMustDiffer)
        );
        output = fixture().2;
        output.owner_delay_pulses = 8;
        assert!(matches!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::OwnerDelayOutOfRange { .. })
        ));
        output = fixture().2;
        output.emergency_delay_pulses = 64;
        assert!(matches!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::EmergencyDelayOutOfRange { .. })
        ));
        let _ = owner;
    }

    #[test]
    fn missing_pulseclock_fails_closed() {
        let (owner, _, output) = fixture();
        let witness = sign(&owner, VaultSpendPathV1::Owner, "vault-test");
        let err = evaluate_vault_spend_v1(
            admitted(),
            &output,
            &witness,
            Err(PulseClockV1Error::SelectedTipUnavailable),
        )
        .unwrap_err();
        assert_eq!(err, VaultV1Error::PulseClockUnavailable);
    }

    #[test]
    fn default_runtime_does_not_admit_vault() {
        let output = fixture().2;
        assert!(!VaultAdmissionV1::INACTIVE.admitted());
        assert_eq!(pulses_remaining_owner(&output, 100), 64);
        assert_eq!(pulses_remaining_owner(&output, 164), 0);
    }

    #[test]
    fn signature_is_domain_and_chain_bound() {
        let (owner, emergency, output) = fixture();
        let pulse = pulse_at(164);
        let mut witness = sign(&owner, VaultSpendPathV1::Owner, "vault-test");

        let mut other_chain = witness.clone();
        other_chain.chain_id = "other-chain".into();
        assert_eq!(
            evaluate_vault_spend_v1(admitted(), &output, &other_chain, Ok(&pulse)),
            Err(VaultV1Error::InvalidSignature)
        );

        let mut empty = witness.clone();
        empty.chain_id.clear();
        assert_eq!(
            evaluate_vault_spend_v1(admitted(), &output, &empty, Ok(&pulse)),
            Err(VaultV1Error::EmptyChainId)
        );

        let mut wrong_path = witness.clone();
        wrong_path.path = VaultSpendPathV1::Emergency;
        wrong_path.public_key = output.emergency_pk.clone();
        assert_eq!(
            evaluate_vault_spend_v1(admitted(), &output, &wrong_path, Ok(&pulse)),
            Err(VaultV1Error::InvalidSignature)
        );

        witness.public_key = pk_hex(&emergency);
        assert_eq!(
            evaluate_vault_spend_v1(admitted(), &output, &witness, Ok(&pulse)),
            Err(VaultV1Error::SpendKeyMismatch { path: "owner" })
        );
    }

    #[test]
    fn spend_message_golden_vector_is_chain_bound() {
        use sha2::{Digest, Sha256};
        let owner = key(1);
        let witness = VaultSpendWitnessV1 {
            path: VaultSpendPathV1::Owner,
            chain_id: "pulsedag-testnet".into(),
            outpoint_txid: "aa".repeat(32),
            outpoint_index: 0,
            public_key: pk_hex(&owner),
            signature_hex: String::new(),
        };
        let testnet = vault_spend_signing_message_v1(&witness).unwrap();
        let mut other = witness.clone();
        other.chain_id = "pulsedag-private".into();
        let private = vault_spend_signing_message_v1(&other).unwrap();
        assert_ne!(testnet, private);
        assert_eq!(
            hex::encode(Sha256::digest(&testnet)),
            "7e6fb65488d1836043755c8c621481637830b1b0b51e1bb5a48c29534b08f5d7"
        );
        assert_eq!(
            hex::encode(Sha256::digest(&private)),
            "5aa937374d363d70b250919df7d475127ac3a65936efc63470b79566827f302b"
        );
    }
}
