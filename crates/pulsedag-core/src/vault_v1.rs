//! Planning matcher for `vault_v1`.
//!
//! Cites PulseClock `pulse_height` for delay checks. Does not admit vault
//! outputs into mempool or consensus. Default evaluation is fail-closed:
//! `covenants_enabled` and template admission are both off.

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
}

impl std::fmt::Display for VaultV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => write!(
                f,
                "vault_v1 is not admitted; spend fails closed"
            ),
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
                write!(f, "PulseClock metadata unavailable; vault spend fails closed")
            }
            Self::OwnerSpendTooEarly { current, required } => {
                write!(f, "owner spend at pulse {current} before required {required}")
            }
            Self::EmergencySpendTooEarly { current, required } => write!(
                f,
                "emergency spend at pulse {current} before required {required}"
            ),
            Self::SpendKeyMismatch { path } => {
                write!(f, "spend key does not match {path} path")
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

/// Evaluate a vault spend. Signature verification is a later slice; this
/// matcher checks admission, template shape, claimed key, and PulseClock height.
pub fn evaluate_vault_spend_v1(
    admission: VaultAdmissionV1,
    output: &VaultV1Output,
    path: VaultSpendPathV1,
    claimed_pk: &str,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<(), VaultV1Error> {
    if !admission.admitted() {
        return Err(VaultV1Error::TemplateDisabled);
    }
    validate_vault_output_v1(output)?;
    let pulse = pulse.map_err(VaultV1Error::from)?;
    match path {
        VaultSpendPathV1::Owner => {
            if claimed_pk != output.owner_pk {
                return Err(VaultV1Error::SpendKeyMismatch {
                    path: path.as_str(),
                });
            }
            let required = owner_unlock_height(output);
            if pulse.pulse_height < required {
                return Err(VaultV1Error::OwnerSpendTooEarly {
                    current: pulse.pulse_height,
                    required,
                });
            }
        }
        VaultSpendPathV1::Emergency => {
            if claimed_pk != output.emergency_pk {
                return Err(VaultV1Error::SpendKeyMismatch {
                    path: path.as_str(),
                });
            }
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

    fn valid_output() -> VaultV1Output {
        VaultV1Output {
            template_id: VAULT_TEMPLATE_ID_V1.into(),
            owner_pk: "owner-pk".into(),
            emergency_pk: "emergency-pk".into(),
            owner_delay_pulses: 64,
            emergency_delay_pulses: 128,
            created_pulse_height: 100,
            amount: 1_000,
        }
    }

    fn admitted() -> VaultAdmissionV1 {
        VaultAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_even_after_delay() {
        let output = valid_output();
        let pulse = pulse_at(10_000);
        let err = evaluate_vault_spend_v1(
            VaultAdmissionV1::INACTIVE,
            &output,
            VaultSpendPathV1::Owner,
            "owner-pk",
            Ok(&pulse),
        )
        .unwrap_err();
        assert_eq!(err, VaultV1Error::TemplateDisabled);
    }

    #[test]
    fn owner_spend_one_pulse_early_is_rejected() {
        let output = valid_output();
        let pulse = pulse_at(163);
        let err = evaluate_vault_spend_v1(
            admitted(),
            &output,
            VaultSpendPathV1::Owner,
            "owner-pk",
            Ok(&pulse),
        )
        .unwrap_err();
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
        let output = valid_output();
        let pulse = pulse_at(164);
        evaluate_vault_spend_v1(
            admitted(),
            &output,
            VaultSpendPathV1::Owner,
            "owner-pk",
            Ok(&pulse),
        )
        .unwrap();
    }

    #[test]
    fn emergency_spend_at_exact_boundary_is_accepted_when_admitted() {
        let output = valid_output();
        let pulse = pulse_at(228);
        evaluate_vault_spend_v1(
            admitted(),
            &output,
            VaultSpendPathV1::Emergency,
            "emergency-pk",
            Ok(&pulse),
        )
        .unwrap();
    }

    #[test]
    fn emergency_spend_early_is_rejected() {
        let output = valid_output();
        let pulse = pulse_at(227);
        let err = evaluate_vault_spend_v1(
            admitted(),
            &output,
            VaultSpendPathV1::Emergency,
            "emergency-pk",
            Ok(&pulse),
        )
        .unwrap_err();
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
        let mut output = valid_output();
        output.emergency_pk = output.owner_pk.clone();
        assert_eq!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::OwnerAndEmergencyKeysMustDiffer)
        );
        output = valid_output();
        output.owner_delay_pulses = 8;
        assert!(matches!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::OwnerDelayOutOfRange { .. })
        ));
        output = valid_output();
        output.emergency_delay_pulses = 64;
        assert!(matches!(
            validate_vault_output_v1(&output),
            Err(VaultV1Error::EmergencyDelayOutOfRange { .. })
        ));
    }

    #[test]
    fn missing_pulseclock_fails_closed() {
        let output = valid_output();
        let err = evaluate_vault_spend_v1(
            admitted(),
            &output,
            VaultSpendPathV1::Owner,
            "owner-pk",
            Err(PulseClockV1Error::SelectedTipUnavailable),
        )
        .unwrap_err();
        assert_eq!(err, VaultV1Error::PulseClockUnavailable);
    }

    #[test]
    fn default_runtime_does_not_admit_vault() {
        assert!(!VaultAdmissionV1::INACTIVE.admitted());
        assert_eq!(pulses_remaining_owner(&valid_output(), 100), 64);
        assert_eq!(pulses_remaining_owner(&valid_output(), 164), 0);
    }
}
