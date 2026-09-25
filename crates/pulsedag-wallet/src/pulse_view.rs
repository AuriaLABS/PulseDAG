//! Wallet-facing PulseClock delay view.
//!
//! Session unlock still uses wall clock (`session_clock`). Covenant delays
//! must not. Default admission is inactive on the Task31 wallet.

pub use pulsedag_core::{
    wallet_pulse_view_v1, PulseObservationV1, WalletPulseAdmissionV1, WalletPulseV1Error,
    WALLET_PULSE_DOMAIN_V1,
};

pub fn wallet_covenant_delay_pulse(
    admission: WalletPulseAdmissionV1,
    pulse: Result<&PulseObservationV1, ()>,
    host_unix_secs: Option<i64>,
) -> Result<PulseObservationV1, WalletPulseV1Error> {
    wallet_pulse_view_v1(admission, pulse, host_unix_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn pulse() -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "wallet".into(),
            selected_tip: "tip".into(),
            pulse_height: 4,
            pulse_time: 1_700_000_004,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 2,
            uncertainty_secs: 1,
            finality_lag: 4,
        }
    }

    #[test]
    fn task31_wallet_does_not_show_covenant_delays() {
        let pulse = pulse();
        assert_eq!(
            wallet_covenant_delay_pulse(WalletPulseAdmissionV1::INACTIVE, Ok(&pulse), None),
            Err(WalletPulseV1Error::SurfaceDisabled)
        );
    }

    #[test]
    fn session_wall_clock_is_not_a_pulse_source() {
        let pulse = pulse();
        let admitted = WalletPulseAdmissionV1 {
            surface_enabled: true,
        };
        assert_eq!(
            wallet_covenant_delay_pulse(admitted, Ok(&pulse), Some(1_700_000_000)),
            Err(WalletPulseV1Error::HostTimeForbidden)
        );
    }
}
