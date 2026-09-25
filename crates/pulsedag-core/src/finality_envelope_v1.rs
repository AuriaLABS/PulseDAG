//! Planning matcher for the measured finality envelope v1.
//!
//! Public cadence and GHOSTDAG `k` change only after explicit activation
//! with measurements. Missing evidence keeps the envelope unpublished.
//! This is an operational bound, not a second consensus gadget.

use crate::pulseclock_v1::{PulseObservationV1, PULSE_FINALITY_DEPTH_UNPUBLISHED_V1};

pub const FINALITY_ENVELOPE_DOMAIN_V1: &str = "PulseDAG:finality-envelope:v1";
pub const FINALITY_ENVELOPE_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadenceClassV1 {
    Experimental,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityMeasurementsV1 {
    pub measured_at_sha: String,
    pub cadence_sample_millis: Vec<u32>,
    pub stale_rate_bps: u32,
    pub apply_lag_millis: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityEnvelopeV1 {
    pub cadence_target_millis: Option<u32>,
    pub k: u32,
    pub finality_depth_pulses: u64,
    pub assumption_honest_hashrate_bps: u32,
    pub measured_at_sha: String,
    pub cadence_class: CadenceClassV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalityEnvelopeAdmissionV1 {
    pub publish_enabled: bool,
}

impl FinalityEnvelopeAdmissionV1 {
    pub const INACTIVE: Self = Self {
        publish_enabled: false,
    };

    pub fn admitted(self) -> bool {
        self.publish_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalityEnvelopeV1Error {
    Unpublished,
    MeasurementsMissing,
    SilentKChange { current: u32, proposed: u32 },
    EmptyMeasurementSha,
}

impl std::fmt::Display for FinalityEnvelopeV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unpublished => {
                write!(
                    f,
                    "finality envelope is unpublished; cadence stays experimental"
                )
            }
            Self::MeasurementsMissing => {
                write!(
                    f,
                    "cannot publish a public cadence without Task 41 measurements"
                )
            }
            Self::SilentKChange { current, proposed } => {
                write!(f, "silent k change {current} -> {proposed} is forbidden")
            }
            Self::EmptyMeasurementSha => write!(f, "measured_at_sha must identify the candidate"),
        }
    }
}

impl std::error::Error for FinalityEnvelopeV1Error {}

pub fn unpublished_envelope_v1(k: u32) -> FinalityEnvelopeV1 {
    FinalityEnvelopeV1 {
        cadence_target_millis: None,
        k,
        finality_depth_pulses: PULSE_FINALITY_DEPTH_UNPUBLISHED_V1,
        assumption_honest_hashrate_bps: 0,
        measured_at_sha: String::new(),
        cadence_class: CadenceClassV1::Experimental,
    }
}

pub fn operational_finality_lag_v1(
    pulse: &PulseObservationV1,
    envelope: &FinalityEnvelopeV1,
) -> u64 {
    pulse
        .pulse_height
        .saturating_sub(envelope.finality_depth_pulses)
}

pub fn validate_measurements_v1(
    measurements: &FinalityMeasurementsV1,
) -> Result<(), FinalityEnvelopeV1Error> {
    if measurements.measured_at_sha.is_empty() {
        return Err(FinalityEnvelopeV1Error::EmptyMeasurementSha);
    }
    if measurements.cadence_sample_millis.is_empty() {
        return Err(FinalityEnvelopeV1Error::MeasurementsMissing);
    }
    Ok(())
}

pub fn publish_finality_envelope_v1(
    admission: FinalityEnvelopeAdmissionV1,
    current_k: u32,
    proposed_k: u32,
    measurements: &FinalityMeasurementsV1,
    cadence_target_millis: u32,
    finality_depth_pulses: u64,
    assumption_honest_hashrate_bps: u32,
) -> Result<FinalityEnvelopeV1, FinalityEnvelopeV1Error> {
    if !admission.admitted() {
        return Err(FinalityEnvelopeV1Error::Unpublished);
    }
    validate_measurements_v1(measurements)?;
    if proposed_k != current_k {
        return Err(FinalityEnvelopeV1Error::SilentKChange {
            current: current_k,
            proposed: proposed_k,
        });
    }
    Ok(FinalityEnvelopeV1 {
        cadence_target_millis: Some(cadence_target_millis),
        k: proposed_k,
        finality_depth_pulses,
        assumption_honest_hashrate_bps,
        measured_at_sha: measurements.measured_at_sha.clone(),
        cadence_class: CadenceClassV1::Public,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn pulse_at(height: u64) -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "fin-test".into(),
            selected_tip: "tip".into(),
            pulse_height: height,
            pulse_time: 1_700_000_000,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 1,
            finality_lag: height,
        }
    }

    fn measurements() -> FinalityMeasurementsV1 {
        FinalityMeasurementsV1 {
            measured_at_sha: "abc".into(),
            cadence_sample_millis: vec![1000, 500, 250],
            stale_rate_bps: 120,
            apply_lag_millis: 40,
        }
    }

    #[test]
    fn missing_admission_keeps_cadence_experimental() {
        let err = publish_finality_envelope_v1(
            FinalityEnvelopeAdmissionV1::INACTIVE,
            18,
            18,
            &measurements(),
            1000,
            100,
            6_000,
        )
        .unwrap_err();
        assert_eq!(err, FinalityEnvelopeV1Error::Unpublished);
        let unpublished = unpublished_envelope_v1(18);
        assert_eq!(unpublished.cadence_class, CadenceClassV1::Experimental);
        assert_eq!(unpublished.cadence_target_millis, None);
        assert_eq!(
            unpublished.finality_depth_pulses,
            PULSE_FINALITY_DEPTH_UNPUBLISHED_V1
        );
    }

    #[test]
    fn measurements_are_required_to_publish() {
        let mut measurements = measurements();
        measurements.cadence_sample_millis.clear();
        let err = publish_finality_envelope_v1(
            FinalityEnvelopeAdmissionV1 {
                publish_enabled: true,
            },
            18,
            18,
            &measurements,
            1000,
            100,
            6_000,
        )
        .unwrap_err();
        assert_eq!(err, FinalityEnvelopeV1Error::MeasurementsMissing);
    }

    #[test]
    fn silent_k_change_is_forbidden() {
        let err = publish_finality_envelope_v1(
            FinalityEnvelopeAdmissionV1 {
                publish_enabled: true,
            },
            18,
            19,
            &measurements(),
            1000,
            100,
            6_000,
        )
        .unwrap_err();
        assert_eq!(
            err,
            FinalityEnvelopeV1Error::SilentKChange {
                current: 18,
                proposed: 19
            }
        );
    }

    #[test]
    fn published_depth_reduces_operational_lag() {
        let envelope = publish_finality_envelope_v1(
            FinalityEnvelopeAdmissionV1 {
                publish_enabled: true,
            },
            18,
            18,
            &measurements(),
            1000,
            40,
            6_000,
        )
        .unwrap();
        assert_eq!(envelope.cadence_class, CadenceClassV1::Public);
        let pulse = pulse_at(50);
        assert_eq!(operational_finality_lag_v1(&pulse, &envelope), 10);
        let unpublished = unpublished_envelope_v1(18);
        assert_eq!(operational_finality_lag_v1(&pulse, &unpublished), 50);
    }
}
