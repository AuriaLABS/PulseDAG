//! Planning matcher for wallet PulseClock display v1.
//!
//! A non-custodial 3.0 wallet may show vault/stream delays only from
//! observational PulseClock. Host unix time is forbidden. Default
//! admission keeps this off the v2.4.0 wallet candidate.

use crate::pulseclock_v1::PulseObservationV1;

pub const WALLET_PULSE_DOMAIN_V1: &str = "PulseDAG:wallet-pulse:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalletPulseAdmissionV1 {
    pub surface_enabled: bool,
}

impl WalletPulseAdmissionV1 {
    pub const INACTIVE: Self = Self {
        surface_enabled: false,
    };

    pub fn admitted(self) -> bool {
        self.surface_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletPulseV1Error {
    SurfaceDisabled,
    PulseClockUnavailable,
    HostTimeForbidden,
}

impl std::fmt::Display for WalletPulseV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceDisabled => {
                write!(
                    f,
                    "wallet PulseClock surface is not admitted on this candidate"
                )
            }
            Self::PulseClockUnavailable => {
                write!(
                    f,
                    "wallet must not invent a pulse when PulseClock is missing"
                )
            }
            Self::HostTimeForbidden => {
                write!(f, "wallet must not render vault delays from host time")
            }
        }
    }
}

impl std::error::Error for WalletPulseV1Error {}

pub fn wallet_pulse_view_v1(
    admission: WalletPulseAdmissionV1,
    pulse: Result<&PulseObservationV1, ()>,
    host_unix_secs: Option<i64>,
) -> Result<PulseObservationV1, WalletPulseV1Error> {
    if !admission.admitted() {
        return Err(WalletPulseV1Error::SurfaceDisabled);
    }
    if host_unix_secs.is_some() {
        return Err(WalletPulseV1Error::HostTimeForbidden);
    }
    pulse
        .cloned()
        .map_err(|_| WalletPulseV1Error::PulseClockUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn pulse() -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "wallet".into(),
            selected_tip: "tip".into(),
            pulse_height: 12,
            pulse_time: 1_700_000_012,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 2,
            finality_lag: 12,
        }
    }

    #[test]
    fn inactive_wallet_does_not_render_pulse() {
        let pulse = pulse();
        assert_eq!(
            wallet_pulse_view_v1(WalletPulseAdmissionV1::INACTIVE, Ok(&pulse), None),
            Err(WalletPulseV1Error::SurfaceDisabled)
        );
    }

    #[test]
    fn host_time_cannot_substitute_pulseclock() {
        let pulse = pulse();
        let admitted = WalletPulseAdmissionV1 {
            surface_enabled: true,
        };
        assert_eq!(
            wallet_pulse_view_v1(admitted, Ok(&pulse), Some(1_700_000_000)),
            Err(WalletPulseV1Error::HostTimeForbidden)
        );
        assert_eq!(
            wallet_pulse_view_v1(admitted, Err(()), None),
            Err(WalletPulseV1Error::PulseClockUnavailable)
        );
        assert_eq!(
            wallet_pulse_view_v1(admitted, Ok(&pulse), None)
                .unwrap()
                .pulse_height,
            12
        );
    }
}
