//! Planning matcher for `htlc_v1`.
//!
//! Cites PulseClock `pulse_height` for the refund deadline. Default
//! evaluation is fail-closed: the template is not mempool-admitted.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::pulseclock_v1::{PulseClockV1Error, PulseObservationV1};

pub const HTLC_TEMPLATE_ID_V1: &str = "htlc_v1";
pub const HTLC_DOMAIN_V1: &str = "PulseDAG:covenant:htlc:v1";
pub const HTLC_REFUND_DELAY_MIN_PULSES_V1: u32 = 64;
pub const HTLC_REFUND_DELAY_MAX_PULSES_V1: u32 = 1_048_576;
pub const HTLC_PREIMAGE_LEN_V1: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtlcV1Output {
    pub template_id: String,
    pub receiver_pk: String,
    pub sender_pk: String,
    pub payment_hash: [u8; 32],
    pub refund_delay_pulses: u32,
    pub created_pulse_height: u64,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtlcSpendPathV1 {
    Claim,
    Refund,
}

impl HtlcSpendPathV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Refund => "refund",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtlcSpendWitnessV1 {
    pub path: HtlcSpendPathV1,
    pub chain_id: String,
    pub outpoint_txid: String,
    pub outpoint_index: u32,
    pub public_key: String,
    pub signature_hex: String,
    pub preimage: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HtlcAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl HtlcAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtlcV1Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    ReceiverAndSenderMustDiffer,
    EmptyKey { field: &'static str },
    RefundDelayOutOfRange { delay: u32 },
    ReservedPreimageHash,
    PulseClockUnavailable,
    ClaimTooLate { current: u64, refund_at: u64 },
    RefundTooEarly { current: u64, required: u64 },
    SpendKeyMismatch { path: &'static str },
    EmptyChainId,
    InvalidSignature,
    PreimageRequired,
    PreimageMismatch,
}

impl std::fmt::Display for HtlcV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "htlc_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(f, "unknown covenant template {observed}; not htlc_v1")
            }
            Self::ReceiverAndSenderMustDiffer => {
                write!(f, "receiver_pk and sender_pk must be distinct")
            }
            Self::EmptyKey { field } => write!(f, "{field} must not be empty"),
            Self::RefundDelayOutOfRange { delay } => {
                write!(f, "refund_delay_pulses {delay} is outside planning bounds")
            }
            Self::ReservedPreimageHash => {
                write!(f, "payment_hash must not commit to the 32-zero preimage")
            }
            Self::PulseClockUnavailable => {
                write!(
                    f,
                    "PulseClock metadata unavailable; htlc spend fails closed"
                )
            }
            Self::ClaimTooLate { current, refund_at } => {
                write!(
                    f,
                    "claim at pulse {current} is not before refund {refund_at}"
                )
            }
            Self::RefundTooEarly { current, required } => {
                write!(f, "refund at pulse {current} before required {required}")
            }
            Self::SpendKeyMismatch { path } => {
                write!(f, "spend key does not match {path} path")
            }
            Self::EmptyChainId => write!(f, "htlc spend chain_id must not be empty"),
            Self::InvalidSignature => {
                write!(f, "htlc spend signature failed domain verification")
            }
            Self::PreimageRequired => write!(f, "claim path requires a 32-byte preimage"),
            Self::PreimageMismatch => write!(f, "preimage does not match payment_hash"),
        }
    }
}

impl std::error::Error for HtlcV1Error {}

impl From<PulseClockV1Error> for HtlcV1Error {
    fn from(_: PulseClockV1Error) -> Self {
        Self::PulseClockUnavailable
    }
}

pub fn reserved_payment_hash_v1() -> [u8; 32] {
    Sha256::digest([0u8; HTLC_PREIMAGE_LEN_V1]).into()
}

pub fn payment_hash_v1(preimage: &[u8; 32]) -> [u8; 32] {
    Sha256::digest(preimage).into()
}

pub fn refund_height_v1(output: &HtlcV1Output) -> u64 {
    output
        .created_pulse_height
        .saturating_add(u64::from(output.refund_delay_pulses))
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn htlc_spend_signing_message_v1(witness: &HtlcSpendWitnessV1) -> Result<Vec<u8>, HtlcV1Error> {
    if witness.chain_id.is_empty() {
        return Err(HtlcV1Error::EmptyChainId);
    }
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, HTLC_DOMAIN_V1.as_bytes());
    encode_len_prefixed(&mut out, witness.chain_id.as_bytes());
    encode_len_prefixed(&mut out, witness.outpoint_txid.as_bytes());
    out.extend_from_slice(&witness.outpoint_index.to_le_bytes());
    encode_len_prefixed(&mut out, witness.path.as_str().as_bytes());
    if witness.path == HtlcSpendPathV1::Claim {
        let preimage = witness.preimage.ok_or(HtlcV1Error::PreimageRequired)?;
        encode_len_prefixed(&mut out, &preimage);
    }
    Ok(out)
}

pub fn validate_htlc_output_v1(output: &HtlcV1Output) -> Result<(), HtlcV1Error> {
    if output.template_id != HTLC_TEMPLATE_ID_V1 {
        return Err(HtlcV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    if output.receiver_pk.is_empty() {
        return Err(HtlcV1Error::EmptyKey {
            field: "receiver_pk",
        });
    }
    if output.sender_pk.is_empty() {
        return Err(HtlcV1Error::EmptyKey { field: "sender_pk" });
    }
    if output.receiver_pk == output.sender_pk {
        return Err(HtlcV1Error::ReceiverAndSenderMustDiffer);
    }
    if output.refund_delay_pulses < HTLC_REFUND_DELAY_MIN_PULSES_V1
        || output.refund_delay_pulses > HTLC_REFUND_DELAY_MAX_PULSES_V1
    {
        return Err(HtlcV1Error::RefundDelayOutOfRange {
            delay: output.refund_delay_pulses,
        });
    }
    if output.payment_hash == reserved_payment_hash_v1() {
        return Err(HtlcV1Error::ReservedPreimageHash);
    }
    Ok(())
}

fn verify_htlc_signature_v1(
    output: &HtlcV1Output,
    witness: &HtlcSpendWitnessV1,
) -> Result<(), HtlcV1Error> {
    let expected_pk = match witness.path {
        HtlcSpendPathV1::Claim => output.receiver_pk.as_str(),
        HtlcSpendPathV1::Refund => output.sender_pk.as_str(),
    };
    if witness.public_key != expected_pk {
        return Err(HtlcV1Error::SpendKeyMismatch {
            path: witness.path.as_str(),
        });
    }
    let message = htlc_spend_signing_message_v1(witness)?;
    let pk_bytes = hex::decode(&witness.public_key).map_err(|_| HtlcV1Error::InvalidSignature)?;
    let sig_bytes =
        hex::decode(&witness.signature_hex).map_err(|_| HtlcV1Error::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| HtlcV1Error::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| HtlcV1Error::InvalidSignature)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pk_arr).map_err(|_| HtlcV1Error::InvalidSignature)?;
    let signature = Signature::from_bytes(&sig_arr);
    verifying_key
        .verify(&message, &signature)
        .map_err(|_| HtlcV1Error::InvalidSignature)
}

pub fn evaluate_htlc_spend_v1(
    admission: HtlcAdmissionV1,
    output: &HtlcV1Output,
    witness: &HtlcSpendWitnessV1,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<(), HtlcV1Error> {
    if !admission.admitted() {
        return Err(HtlcV1Error::TemplateDisabled);
    }
    validate_htlc_output_v1(output)?;
    verify_htlc_signature_v1(output, witness)?;
    let pulse = pulse.map_err(HtlcV1Error::from)?;
    let refund_at = refund_height_v1(output);
    match witness.path {
        HtlcSpendPathV1::Claim => {
            let preimage = witness.preimage.ok_or(HtlcV1Error::PreimageRequired)?;
            if payment_hash_v1(&preimage) != output.payment_hash {
                return Err(HtlcV1Error::PreimageMismatch);
            }
            if pulse.pulse_height >= refund_at {
                return Err(HtlcV1Error::ClaimTooLate {
                    current: pulse.pulse_height,
                    refund_at,
                });
            }
        }
        HtlcSpendPathV1::Refund => {
            if pulse.pulse_height < refund_at {
                return Err(HtlcV1Error::RefundTooEarly {
                    current: pulse.pulse_height,
                    required: refund_at,
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
            chain_id: "htlc-test".into(),
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

    fn fixture() -> (SigningKey, SigningKey, [u8; 32], HtlcV1Output) {
        let receiver = key(3);
        let sender = key(4);
        let preimage = [9u8; 32];
        let output = HtlcV1Output {
            template_id: HTLC_TEMPLATE_ID_V1.into(),
            receiver_pk: pk_hex(&receiver),
            sender_pk: pk_hex(&sender),
            payment_hash: payment_hash_v1(&preimage),
            refund_delay_pulses: 64,
            created_pulse_height: 100,
            amount: 50,
        };
        (receiver, sender, preimage, output)
    }

    fn sign_claim(receiver: &SigningKey, preimage: [u8; 32]) -> HtlcSpendWitnessV1 {
        let mut witness = HtlcSpendWitnessV1 {
            path: HtlcSpendPathV1::Claim,
            chain_id: "htlc-test".into(),
            outpoint_txid: "bb".repeat(32),
            outpoint_index: 0,
            public_key: pk_hex(receiver),
            signature_hex: String::new(),
            preimage: Some(preimage),
        };
        let message = htlc_spend_signing_message_v1(&witness).unwrap();
        witness.signature_hex = hex::encode(receiver.sign(&message).to_bytes());
        witness
    }

    fn sign_refund(sender: &SigningKey) -> HtlcSpendWitnessV1 {
        let mut witness = HtlcSpendWitnessV1 {
            path: HtlcSpendPathV1::Refund,
            chain_id: "htlc-test".into(),
            outpoint_txid: "bb".repeat(32),
            outpoint_index: 0,
            public_key: pk_hex(sender),
            signature_hex: String::new(),
            preimage: None,
        };
        let message = htlc_spend_signing_message_v1(&witness).unwrap();
        witness.signature_hex = hex::encode(sender.sign(&message).to_bytes());
        witness
    }

    fn admitted() -> HtlcAdmissionV1 {
        HtlcAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_claim_and_refund() {
        let (receiver, sender, preimage, output) = fixture();
        let pulse = pulse_at(120);
        assert_eq!(
            evaluate_htlc_spend_v1(
                HtlcAdmissionV1::INACTIVE,
                &output,
                &sign_claim(&receiver, preimage),
                Ok(&pulse)
            ),
            Err(HtlcV1Error::TemplateDisabled)
        );
        let late = pulse_at(200);
        assert_eq!(
            evaluate_htlc_spend_v1(
                HtlcAdmissionV1::INACTIVE,
                &output,
                &sign_refund(&sender),
                Ok(&late)
            ),
            Err(HtlcV1Error::TemplateDisabled)
        );
    }

    #[test]
    fn claim_succeeds_on_last_legal_pulse() {
        let (receiver, _, preimage, output) = fixture();
        let pulse = pulse_at(163);
        evaluate_htlc_spend_v1(
            admitted(),
            &output,
            &sign_claim(&receiver, preimage),
            Ok(&pulse),
        )
        .unwrap();
    }

    #[test]
    fn claim_at_refund_height_is_rejected() {
        let (receiver, _, preimage, output) = fixture();
        let pulse = pulse_at(164);
        let err = evaluate_htlc_spend_v1(
            admitted(),
            &output,
            &sign_claim(&receiver, preimage),
            Ok(&pulse),
        )
        .unwrap_err();
        assert_eq!(
            err,
            HtlcV1Error::ClaimTooLate {
                current: 164,
                refund_at: 164
            }
        );
    }

    #[test]
    fn refund_at_exact_delay_succeeds() {
        let (_, sender, _, output) = fixture();
        let pulse = pulse_at(164);
        evaluate_htlc_spend_v1(admitted(), &output, &sign_refund(&sender), Ok(&pulse)).unwrap();
    }

    #[test]
    fn refund_one_pulse_early_is_rejected() {
        let (_, sender, _, output) = fixture();
        let pulse = pulse_at(163);
        let err = evaluate_htlc_spend_v1(admitted(), &output, &sign_refund(&sender), Ok(&pulse))
            .unwrap_err();
        assert_eq!(
            err,
            HtlcV1Error::RefundTooEarly {
                current: 163,
                required: 164
            }
        );
    }

    #[test]
    fn wrong_preimage_is_rejected() {
        let (receiver, _, _, output) = fixture();
        let pulse = pulse_at(120);
        let err = evaluate_htlc_spend_v1(
            admitted(),
            &output,
            &sign_claim(&receiver, [1u8; 32]),
            Ok(&pulse),
        )
        .unwrap_err();
        assert_eq!(err, HtlcV1Error::PreimageMismatch);
    }

    #[test]
    fn reserved_zero_preimage_hash_is_rejected() {
        let mut output = fixture().3;
        output.payment_hash = reserved_payment_hash_v1();
        assert_eq!(
            validate_htlc_output_v1(&output),
            Err(HtlcV1Error::ReservedPreimageHash)
        );
    }

    #[test]
    fn missing_pulseclock_fails_closed() {
        let (receiver, _, preimage, output) = fixture();
        let err = evaluate_htlc_spend_v1(
            admitted(),
            &output,
            &sign_claim(&receiver, preimage),
            Err(PulseClockV1Error::SelectedTipUnavailable),
        )
        .unwrap_err();
        assert_eq!(err, HtlcV1Error::PulseClockUnavailable);
    }
}
