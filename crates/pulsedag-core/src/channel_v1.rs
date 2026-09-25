//! Planning matcher for `channel_v1`.
//!
//! Cooperative close or unilateral close + pulse challenge window.
//! L1 does not store the off-chain transcript. Default evaluation is
//! fail-closed: the template is not mempool-admitted.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::pulseclock_v1::{PulseClockV1Error, PulseObservationV1};

pub const CHANNEL_TEMPLATE_ID_V1: &str = "channel_v1";
pub const CHANNEL_DOMAIN_V1: &str = "PulseDAG:covenant:channel:v1";
pub const CHANNEL_CHALLENGE_MIN_PULSES_V1: u32 = 64;
pub const CHANNEL_CHALLENGE_MAX_PULSES_V1: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStageV1 {
    Funded,
    Closing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelV1Output {
    pub template_id: String,
    pub party_a_pk: String,
    pub party_b_pk: String,
    pub challenge_pulses: u32,
    pub amount: u64,
    pub stage: ChannelStageV1,
    pub state_seq: u64,
    pub amt_a: u64,
    pub amt_b: u64,
    pub posted_pulse_height: u64,
    pub closing_pk: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelSpendPathV1 {
    Coop,
    Uni,
    Challenge,
    Timeout,
}

impl ChannelSpendPathV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coop => "coop",
            Self::Uni => "uni",
            Self::Challenge => "challenge",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpendWitnessV1 {
    pub path: ChannelSpendPathV1,
    pub chain_id: String,
    pub outpoint_txid: String,
    pub outpoint_index: u32,
    pub amt_a: u64,
    pub amt_b: u64,
    pub state_seq: u64,
    pub signer_pk: String,
    pub signature_hex: String,
    pub counterparty_signature_hex: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl ChannelAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelV1Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    PartiesMustDiffer,
    EmptyKey { field: &'static str },
    ChallengeWindowOutOfRange { value: u32 },
    AmountSplitMismatch { left: u64, right: u64, total: u64 },
    PulseClockUnavailable,
    EmptyChainId,
    InvalidSignature { party: &'static str },
    SpendKeyMismatch,
    WrongStage { expected: &'static str },
    ChallengeTooLate { current: u64, deadline: u64 },
    TimeoutTooEarly { current: u64, required: u64 },
    StateSeqNotHigher { posted: u64, offered: u64 },
}

impl std::fmt::Display for ChannelV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "channel_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(f, "unknown covenant template {observed}; not channel_v1")
            }
            Self::PartiesMustDiffer => write!(f, "party_a_pk and party_b_pk must be distinct"),
            Self::EmptyKey { field } => write!(f, "{field} must not be empty"),
            Self::ChallengeWindowOutOfRange { value } => {
                write!(f, "challenge_pulses {value} is outside planning bounds")
            }
            Self::AmountSplitMismatch { left, right, total } => {
                write!(f, "split {left}+{right} does not equal capacity {total}")
            }
            Self::PulseClockUnavailable => {
                write!(
                    f,
                    "PulseClock metadata unavailable; channel spend fails closed"
                )
            }
            Self::EmptyChainId => write!(f, "channel spend chain_id must not be empty"),
            Self::InvalidSignature { party } => {
                write!(f, "{party} signature failed domain verification")
            }
            Self::SpendKeyMismatch => write!(f, "signer is not a funding party"),
            Self::WrongStage { expected } => {
                write!(
                    f,
                    "channel stage does not allow this path (need {expected})"
                )
            }
            Self::ChallengeTooLate { current, deadline } => {
                write!(f, "challenge at pulse {current} after deadline {deadline}")
            }
            Self::TimeoutTooEarly { current, required } => {
                write!(f, "timeout at pulse {current} before required {required}")
            }
            Self::StateSeqNotHigher { posted, offered } => {
                write!(
                    f,
                    "challenge seq {offered} is not greater than posted {posted}"
                )
            }
        }
    }
}

impl std::error::Error for ChannelV1Error {}

impl From<PulseClockV1Error> for ChannelV1Error {
    fn from(_: PulseClockV1Error) -> Self {
        Self::PulseClockUnavailable
    }
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn challenge_deadline_v1(output: &ChannelV1Output) -> u64 {
    output
        .posted_pulse_height
        .saturating_add(u64::from(output.challenge_pulses))
}

pub fn validate_channel_output_v1(output: &ChannelV1Output) -> Result<(), ChannelV1Error> {
    if output.template_id != CHANNEL_TEMPLATE_ID_V1 {
        return Err(ChannelV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    if output.party_a_pk.is_empty() {
        return Err(ChannelV1Error::EmptyKey {
            field: "party_a_pk",
        });
    }
    if output.party_b_pk.is_empty() {
        return Err(ChannelV1Error::EmptyKey {
            field: "party_b_pk",
        });
    }
    if output.party_a_pk == output.party_b_pk {
        return Err(ChannelV1Error::PartiesMustDiffer);
    }
    if output.challenge_pulses < CHANNEL_CHALLENGE_MIN_PULSES_V1
        || output.challenge_pulses > CHANNEL_CHALLENGE_MAX_PULSES_V1
    {
        return Err(ChannelV1Error::ChallengeWindowOutOfRange {
            value: output.challenge_pulses,
        });
    }
    if output.stage == ChannelStageV1::Closing
        && output.amt_a.saturating_add(output.amt_b) != output.amount
    {
        return Err(ChannelV1Error::AmountSplitMismatch {
            left: output.amt_a,
            right: output.amt_b,
            total: output.amount,
        });
    }
    Ok(())
}

pub fn channel_spend_signing_message_v1(
    witness: &ChannelSpendWitnessV1,
) -> Result<Vec<u8>, ChannelV1Error> {
    if witness.chain_id.is_empty() {
        return Err(ChannelV1Error::EmptyChainId);
    }
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, CHANNEL_DOMAIN_V1.as_bytes());
    encode_len_prefixed(&mut out, witness.chain_id.as_bytes());
    encode_len_prefixed(&mut out, witness.outpoint_txid.as_bytes());
    out.extend_from_slice(&witness.outpoint_index.to_le_bytes());
    encode_len_prefixed(&mut out, witness.path.as_str().as_bytes());
    out.extend_from_slice(&witness.amt_a.to_le_bytes());
    out.extend_from_slice(&witness.amt_b.to_le_bytes());
    out.extend_from_slice(&witness.state_seq.to_le_bytes());
    Ok(out)
}

fn verify_pk_signature(
    public_key_hex: &str,
    signature_hex: &str,
    message: &[u8],
    party: &'static str,
) -> Result<(), ChannelV1Error> {
    let pk_bytes =
        hex::decode(public_key_hex).map_err(|_| ChannelV1Error::InvalidSignature { party })?;
    let sig_bytes =
        hex::decode(signature_hex).map_err(|_| ChannelV1Error::InvalidSignature { party })?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| ChannelV1Error::InvalidSignature { party })?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| ChannelV1Error::InvalidSignature { party })?;
    let verifying_key = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|_| ChannelV1Error::InvalidSignature { party })?;
    let signature = Signature::from_bytes(&sig_arr);
    verifying_key
        .verify(message, &signature)
        .map_err(|_| ChannelV1Error::InvalidSignature { party })
}

fn signer_is_party(output: &ChannelV1Output, pk: &str) -> bool {
    pk == output.party_a_pk || pk == output.party_b_pk
}

fn other_party<'a>(output: &'a ChannelV1Output, pk: &str) -> Option<&'a str> {
    if pk == output.party_a_pk {
        Some(output.party_b_pk.as_str())
    } else if pk == output.party_b_pk {
        Some(output.party_a_pk.as_str())
    } else {
        None
    }
}

pub fn evaluate_channel_spend_v1(
    admission: ChannelAdmissionV1,
    output: &ChannelV1Output,
    witness: &ChannelSpendWitnessV1,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<(), ChannelV1Error> {
    if !admission.admitted() {
        return Err(ChannelV1Error::TemplateDisabled);
    }
    validate_channel_output_v1(output)?;
    if !signer_is_party(output, &witness.signer_pk) {
        return Err(ChannelV1Error::SpendKeyMismatch);
    }
    if witness.amt_a.saturating_add(witness.amt_b) != output.amount {
        return Err(ChannelV1Error::AmountSplitMismatch {
            left: witness.amt_a,
            right: witness.amt_b,
            total: output.amount,
        });
    }
    let message = channel_spend_signing_message_v1(witness)?;
    verify_pk_signature(
        &witness.signer_pk,
        &witness.signature_hex,
        &message,
        "signer",
    )?;
    match witness.path {
        ChannelSpendPathV1::Coop => {
            if output.stage != ChannelStageV1::Funded {
                return Err(ChannelV1Error::WrongStage { expected: "funded" });
            }
            let other = other_party(output, &witness.signer_pk).expect("checked party");
            let other_sig = witness.counterparty_signature_hex.as_deref().ok_or(
                ChannelV1Error::InvalidSignature {
                    party: "counterparty",
                },
            )?;
            verify_pk_signature(other, other_sig, &message, "counterparty")?;
        }
        ChannelSpendPathV1::Uni => {
            if output.stage != ChannelStageV1::Funded {
                return Err(ChannelV1Error::WrongStage { expected: "funded" });
            }
        }
        ChannelSpendPathV1::Challenge => {
            if output.stage != ChannelStageV1::Closing {
                return Err(ChannelV1Error::WrongStage {
                    expected: "closing",
                });
            }
            let pulse = pulse.map_err(ChannelV1Error::from)?;
            let deadline = challenge_deadline_v1(output);
            if pulse.pulse_height >= deadline {
                return Err(ChannelV1Error::ChallengeTooLate {
                    current: pulse.pulse_height,
                    deadline,
                });
            }
            if witness.state_seq <= output.state_seq {
                return Err(ChannelV1Error::StateSeqNotHigher {
                    posted: output.state_seq,
                    offered: witness.state_seq,
                });
            }
            if Some(witness.signer_pk.as_str()) != other_party(output, &output.closing_pk) {
                return Err(ChannelV1Error::SpendKeyMismatch);
            }
        }
        ChannelSpendPathV1::Timeout => {
            if output.stage != ChannelStageV1::Closing {
                return Err(ChannelV1Error::WrongStage {
                    expected: "closing",
                });
            }
            let pulse = pulse.map_err(ChannelV1Error::from)?;
            let required = challenge_deadline_v1(output);
            if pulse.pulse_height < required {
                return Err(ChannelV1Error::TimeoutTooEarly {
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
            chain_id: "chan-test".into(),
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

    fn funded() -> (SigningKey, SigningKey, ChannelV1Output) {
        let a = key(5);
        let b = key(6);
        let output = ChannelV1Output {
            template_id: CHANNEL_TEMPLATE_ID_V1.into(),
            party_a_pk: pk_hex(&a),
            party_b_pk: pk_hex(&b),
            challenge_pulses: 64,
            amount: 100,
            stage: ChannelStageV1::Funded,
            state_seq: 0,
            amt_a: 0,
            amt_b: 0,
            posted_pulse_height: 0,
            closing_pk: String::new(),
        };
        (a, b, output)
    }

    fn closing_from(a: &SigningKey, b: &SigningKey) -> ChannelV1Output {
        ChannelV1Output {
            template_id: CHANNEL_TEMPLATE_ID_V1.into(),
            party_a_pk: pk_hex(a),
            party_b_pk: pk_hex(b),
            challenge_pulses: 64,
            amount: 100,
            stage: ChannelStageV1::Closing,
            state_seq: 1,
            amt_a: 40,
            amt_b: 60,
            posted_pulse_height: 200,
            closing_pk: pk_hex(a),
        }
    }

    fn sign(
        signer: &SigningKey,
        path: ChannelSpendPathV1,
        amt_a: u64,
        amt_b: u64,
        state_seq: u64,
        counterparty: Option<&SigningKey>,
    ) -> ChannelSpendWitnessV1 {
        let mut witness = ChannelSpendWitnessV1 {
            path,
            chain_id: "chan-test".into(),
            outpoint_txid: "ee".repeat(32),
            outpoint_index: 0,
            amt_a,
            amt_b,
            state_seq,
            signer_pk: pk_hex(signer),
            signature_hex: String::new(),
            counterparty_signature_hex: None,
        };
        let message = channel_spend_signing_message_v1(&witness).unwrap();
        witness.signature_hex = hex::encode(signer.sign(&message).to_bytes());
        if let Some(other) = counterparty {
            witness.counterparty_signature_hex = Some(hex::encode(other.sign(&message).to_bytes()));
        }
        witness
    }

    fn admitted() -> ChannelAdmissionV1 {
        ChannelAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_coop() {
        let (a, b, output) = funded();
        let witness = sign(&a, ChannelSpendPathV1::Coop, 50, 50, 0, Some(&b));
        assert_eq!(
            evaluate_channel_spend_v1(
                ChannelAdmissionV1::INACTIVE,
                &output,
                &witness,
                Ok(&pulse_at(10))
            ),
            Err(ChannelV1Error::TemplateDisabled)
        );
    }

    #[test]
    fn coop_close_requires_both_signatures_and_split() {
        let (a, b, output) = funded();
        let witness = sign(&a, ChannelSpendPathV1::Coop, 70, 30, 0, Some(&b));
        evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&pulse_at(10))).unwrap();
        let bad = sign(&a, ChannelSpendPathV1::Coop, 70, 31, 0, Some(&b));
        assert!(matches!(
            evaluate_channel_spend_v1(admitted(), &output, &bad, Ok(&pulse_at(10))),
            Err(ChannelV1Error::AmountSplitMismatch { .. })
        ));
    }

    #[test]
    fn unilateral_close_from_funded_is_accepted_when_admitted() {
        let (a, _, output) = funded();
        let witness = sign(&a, ChannelSpendPathV1::Uni, 40, 60, 1, None);
        evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&pulse_at(10))).unwrap();
    }

    #[test]
    fn challenge_needs_higher_seq_before_deadline() {
        let a = key(5);
        let b = key(6);
        let output = closing_from(&a, &b);
        let pulse = pulse_at(263);
        let witness = sign(&b, ChannelSpendPathV1::Challenge, 80, 20, 2, None);
        evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&pulse)).unwrap();

        let late = pulse_at(264);
        assert_eq!(
            evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&late)),
            Err(ChannelV1Error::ChallengeTooLate {
                current: 264,
                deadline: 264
            })
        );
        let stale = sign(&b, ChannelSpendPathV1::Challenge, 80, 20, 1, None);
        assert!(matches!(
            evaluate_channel_spend_v1(admitted(), &output, &stale, Ok(&pulse)),
            Err(ChannelV1Error::StateSeqNotHigher { .. })
        ));
    }

    #[test]
    fn timeout_settle_at_deadline() {
        let a = key(5);
        let b = key(6);
        let output = closing_from(&a, &b);
        let witness = sign(&a, ChannelSpendPathV1::Timeout, 40, 60, 1, None);
        evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&pulse_at(264))).unwrap();
        assert_eq!(
            evaluate_channel_spend_v1(admitted(), &output, &witness, Ok(&pulse_at(263))),
            Err(ChannelV1Error::TimeoutTooEarly {
                current: 263,
                required: 264
            })
        );
    }

    #[test]
    fn missing_pulseclock_fails_closed_on_timed_paths() {
        let a = key(5);
        let b = key(6);
        let output = closing_from(&a, &b);
        let witness = sign(&a, ChannelSpendPathV1::Timeout, 40, 60, 1, None);
        assert_eq!(
            evaluate_channel_spend_v1(
                admitted(),
                &output,
                &witness,
                Err(PulseClockV1Error::SelectedTipUnavailable)
            ),
            Err(ChannelV1Error::PulseClockUnavailable)
        );
    }
}
