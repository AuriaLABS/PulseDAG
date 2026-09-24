//! Planning matcher for `pay_stream_v1`.
//!
//! Cites PulseClock `pulse_height` for bucket maturity. Default evaluation
//! is fail-closed: the template is not admitted into mempool or consensus.

use crate::pulseclock_v1::{PulseClockV1Error, PulseObservationV1};

pub const PAY_STREAM_TEMPLATE_ID_V1: &str = "pay_stream_v1";
pub const PAY_STREAM_DOMAIN_V1: &str = "PulseDAG:covenant:pay-stream:v1";
pub const PAY_STREAM_BUCKET_PULSES_MIN_V1: u32 = 16;
pub const PAY_STREAM_BUCKET_PULSES_MAX_V1: u32 = 65_536;
pub const PAY_STREAM_BUCKET_COUNT_MIN_V1: u32 = 1;
pub const PAY_STREAM_BUCKET_COUNT_MAX_V1: u32 = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayStreamV1Output {
    pub template_id: String,
    pub payer_pk: String,
    pub recipient_pk: String,
    pub start_pulse: u64,
    pub bucket_pulses: u32,
    pub bucket_count: u32,
    pub amount_per_bucket: u64,
    pub withdrawn_buckets: u32,
    pub amount: u64,
    pub created_pulse_height: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayStreamAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl PayStreamAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayStreamV1Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    PayerAndRecipientMustDiffer,
    EmptyKey { field: &'static str },
    BucketPulsesOutOfRange { value: u32 },
    BucketCountOutOfRange { value: u32 },
    WithdrawnExceedsCount,
    AmountMismatch { expected: u64, actual: u64 },
    StartPulseBeforeCreate,
    PulseClockUnavailable,
    NothingMatured { matured: u32, withdrawn: u32 },
}

impl std::fmt::Display for PayStreamV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "pay_stream_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(f, "unknown covenant template {observed}; not pay_stream_v1")
            }
            Self::PayerAndRecipientMustDiffer => {
                write!(f, "payer_pk and recipient_pk must be distinct")
            }
            Self::EmptyKey { field } => write!(f, "{field} must not be empty"),
            Self::BucketPulsesOutOfRange { value } => {
                write!(f, "bucket_pulses {value} is outside planning bounds")
            }
            Self::BucketCountOutOfRange { value } => {
                write!(f, "bucket_count {value} is outside planning bounds")
            }
            Self::WithdrawnExceedsCount => {
                write!(f, "withdrawn_buckets exceeds bucket_count")
            }
            Self::AmountMismatch { expected, actual } => {
                write!(f, "stream amount {actual} != expected {expected}")
            }
            Self::StartPulseBeforeCreate => {
                write!(f, "start_pulse must be >= created_pulse_height")
            }
            Self::PulseClockUnavailable => write!(
                f,
                "PulseClock metadata unavailable; pay_stream spend fails closed"
            ),
            Self::NothingMatured { matured, withdrawn } => {
                write!(f, "no new buckets: matured {matured} withdrawn {withdrawn}")
            }
        }
    }
}

impl std::error::Error for PayStreamV1Error {}

impl From<PulseClockV1Error> for PayStreamV1Error {
    fn from(_: PulseClockV1Error) -> Self {
        Self::PulseClockUnavailable
    }
}

pub fn validate_pay_stream_output_v1(output: &PayStreamV1Output) -> Result<(), PayStreamV1Error> {
    if output.template_id != PAY_STREAM_TEMPLATE_ID_V1 {
        return Err(PayStreamV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    if output.payer_pk.is_empty() {
        return Err(PayStreamV1Error::EmptyKey { field: "payer_pk" });
    }
    if output.recipient_pk.is_empty() {
        return Err(PayStreamV1Error::EmptyKey {
            field: "recipient_pk",
        });
    }
    if output.payer_pk == output.recipient_pk {
        return Err(PayStreamV1Error::PayerAndRecipientMustDiffer);
    }
    if output.bucket_pulses < PAY_STREAM_BUCKET_PULSES_MIN_V1
        || output.bucket_pulses > PAY_STREAM_BUCKET_PULSES_MAX_V1
    {
        return Err(PayStreamV1Error::BucketPulsesOutOfRange {
            value: output.bucket_pulses,
        });
    }
    if output.bucket_count < PAY_STREAM_BUCKET_COUNT_MIN_V1
        || output.bucket_count > PAY_STREAM_BUCKET_COUNT_MAX_V1
    {
        return Err(PayStreamV1Error::BucketCountOutOfRange {
            value: output.bucket_count,
        });
    }
    if output.withdrawn_buckets > output.bucket_count {
        return Err(PayStreamV1Error::WithdrawnExceedsCount);
    }
    if output.start_pulse < output.created_pulse_height {
        return Err(PayStreamV1Error::StartPulseBeforeCreate);
    }
    let remaining = u64::from(output.bucket_count.saturating_sub(output.withdrawn_buckets));
    let expected = output.amount_per_bucket.saturating_mul(remaining);
    if output.amount != expected {
        return Err(PayStreamV1Error::AmountMismatch {
            expected,
            actual: output.amount,
        });
    }
    Ok(())
}

pub fn matured_buckets_v1(output: &PayStreamV1Output, pulse_height: u64) -> u32 {
    if pulse_height < output.start_pulse || output.bucket_pulses == 0 {
        return 0;
    }
    let elapsed = pulse_height.saturating_sub(output.start_pulse);
    let raw = elapsed / u64::from(output.bucket_pulses);
    raw.min(u64::from(output.bucket_count)) as u32
}

pub fn withdrawable_buckets_v1(output: &PayStreamV1Output, pulse_height: u64) -> u32 {
    matured_buckets_v1(output, pulse_height).saturating_sub(output.withdrawn_buckets)
}

pub fn evaluate_pay_stream_withdraw_v1(
    admission: PayStreamAdmissionV1,
    output: &PayStreamV1Output,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<u32, PayStreamV1Error> {
    if !admission.admitted() {
        return Err(PayStreamV1Error::TemplateDisabled);
    }
    validate_pay_stream_output_v1(output)?;
    let pulse = pulse.map_err(PayStreamV1Error::from)?;
    let k = withdrawable_buckets_v1(output, pulse.pulse_height);
    if k == 0 {
        return Err(PayStreamV1Error::NothingMatured {
            matured: matured_buckets_v1(output, pulse.pulse_height),
            withdrawn: output.withdrawn_buckets,
        });
    }
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn pulse_at(height: u64) -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "stream-test".into(),
            selected_tip: "tip".into(),
            pulse_height: height,
            pulse_time: 1_700_000_000,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 1,
            finality_lag: height,
        }
    }

    fn live_stream() -> PayStreamV1Output {
        PayStreamV1Output {
            template_id: PAY_STREAM_TEMPLATE_ID_V1.into(),
            payer_pk: "payer".into(),
            recipient_pk: "recipient".into(),
            start_pulse: 100,
            bucket_pulses: 16,
            bucket_count: 4,
            amount_per_bucket: 10,
            withdrawn_buckets: 0,
            amount: 40,
            created_pulse_height: 90,
        }
    }

    fn admitted() -> PayStreamAdmissionV1 {
        PayStreamAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_even_when_buckets_matured() {
        let output = live_stream();
        let pulse = pulse_at(200);
        let err =
            evaluate_pay_stream_withdraw_v1(PayStreamAdmissionV1::INACTIVE, &output, Ok(&pulse))
                .unwrap_err();
        assert_eq!(err, PayStreamV1Error::TemplateDisabled);
    }

    #[test]
    fn before_start_nothing_has_matured() {
        let output = live_stream();
        assert_eq!(matured_buckets_v1(&output, 99), 0);
        let pulse = pulse_at(99);
        let err = evaluate_pay_stream_withdraw_v1(admitted(), &output, Ok(&pulse)).unwrap_err();
        assert_eq!(
            err,
            PayStreamV1Error::NothingMatured {
                matured: 0,
                withdrawn: 0
            }
        );
    }

    #[test]
    fn first_bucket_matures_at_start_plus_width() {
        let output = live_stream();
        assert_eq!(matured_buckets_v1(&output, 115), 0);
        assert_eq!(matured_buckets_v1(&output, 116), 1);
        let pulse = pulse_at(116);
        assert_eq!(
            evaluate_pay_stream_withdraw_v1(admitted(), &output, Ok(&pulse)).unwrap(),
            1
        );
    }

    #[test]
    fn all_buckets_cap_at_count() {
        let output = live_stream();
        assert_eq!(matured_buckets_v1(&output, 164), 4);
        assert_eq!(matured_buckets_v1(&output, 10_000), 4);
    }

    #[test]
    fn already_withdrawn_buckets_are_not_paid_again() {
        let mut output = live_stream();
        output.withdrawn_buckets = 2;
        output.amount = 20;
        let pulse = pulse_at(148);
        assert_eq!(matured_buckets_v1(&output, 148), 3);
        assert_eq!(
            evaluate_pay_stream_withdraw_v1(admitted(), &output, Ok(&pulse)).unwrap(),
            1
        );
    }

    #[test]
    fn amount_and_key_shape_are_rejected() {
        let mut output = live_stream();
        output.recipient_pk = output.payer_pk.clone();
        assert_eq!(
            validate_pay_stream_output_v1(&output),
            Err(PayStreamV1Error::PayerAndRecipientMustDiffer)
        );
        output = live_stream();
        output.amount = 39;
        assert!(matches!(
            validate_pay_stream_output_v1(&output),
            Err(PayStreamV1Error::AmountMismatch { .. })
        ));
        output = live_stream();
        output.start_pulse = 80;
        assert_eq!(
            validate_pay_stream_output_v1(&output),
            Err(PayStreamV1Error::StartPulseBeforeCreate)
        );
    }

    #[test]
    fn missing_pulseclock_fails_closed() {
        let output = live_stream();
        let err = evaluate_pay_stream_withdraw_v1(
            admitted(),
            &output,
            Err(PulseClockV1Error::SelectedTipUnavailable),
        )
        .unwrap_err();
        assert_eq!(err, PayStreamV1Error::PulseClockUnavailable);
    }
}
