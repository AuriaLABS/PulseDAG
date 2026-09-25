use axum::{extract::State, Json};

use pulsedag_core::{observe_pulse_v1, PulseClockV1Error, PulseObservationV1};

use crate::api::{ApiResponse, RpcStateLike};

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct PulseV1Data {
    pub pulse_version: u32,
    pub domain: String,
    pub chain_id: String,
    pub selected_tip: String,
    pub pulse_height: u64,
    pub pulse_time: i64,
    pub window_k: u32,
    pub sample_count: u32,
    pub uncertainty_secs: u32,
    pub finality_lag: u64,
}

impl From<PulseObservationV1> for PulseV1Data {
    fn from(pulse: PulseObservationV1) -> Self {
        Self {
            pulse_version: pulse.pulse_version,
            domain: pulse.domain.to_string(),
            chain_id: pulse.chain_id,
            selected_tip: pulse.selected_tip,
            pulse_height: pulse.pulse_height,
            pulse_time: pulse.pulse_time,
            window_k: pulse.window_k,
            sample_count: pulse.sample_count,
            uncertainty_secs: pulse.uncertainty_secs,
            finality_lag: pulse.finality_lag,
        }
    }
}

fn pulse_error(err: PulseClockV1Error) -> (String, String) {
    match err {
        PulseClockV1Error::SelectedTipUnavailable => {
            ("pulse_selected_tip_unavailable".into(), err.to_string())
        }
        PulseClockV1Error::SelectedTipBlockMissing { .. } => {
            ("pulse_selected_tip_missing".into(), err.to_string())
        }
    }
}

/// Observational PulseClock v1. Does not activate covenants or change consensus.
pub async fn get_pulse<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PulseV1Data>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match observe_pulse_v1(&chain) {
        Ok(pulse) => Json(ApiResponse::ok(PulseV1Data::from(pulse))),
        Err(err) => {
            let (code, message) = pulse_error(err);
            Json(ApiResponse::err(code, message))
        }
    }
}
