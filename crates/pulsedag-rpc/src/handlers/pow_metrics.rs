use axum::{extract::State, Json};

use crate::{api::ApiResponse, api::RpcStateLike};

/// Compact compatibility hint embedded in mining template and mining job responses.
#[derive(Debug, serde::Serialize)]
pub struct PowMetricsData {
    pub algorithm: String,
    pub window_size: usize,
    pub best_height: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub notes: Vec<String>,
}

/// Canonical consensus diagnostics returned by the `/pow/metrics` endpoint.
#[derive(Debug, serde::Serialize)]
pub struct ConsensusPowMetricsData {
    pub algorithm: String,
    pub window_size: usize,
    pub best_height: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    /// Compatibility field: canonical compact target bits encoded as u64.
    pub suggested_difficulty: u64,
    pub current_bits: u32,
    pub suggested_bits: u32,
    pub target_u64: u64,
    pub target_hex: String,
    pub current_target_hex: String,
    pub pow_limit_bits: u32,
    pub pow_limit_target_hex: String,
    pub target_block_interval_secs: u64,
    /// Work/difficulty multiplier. Values above 10_000 harden work.
    pub retarget_multiplier_bps: u64,
    /// Target multiplier. Values below 10_000 harden work.
    pub target_multiplier_bps: u64,
    pub retarget_was_clamped: bool,
    pub target_was_clamped_to_pow_limit: bool,
    pub retarget_rationale: String,
    pub retarget_signal_quality: String,
    pub notes: Vec<String>,
}

pub async fn get_pow_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<ConsensusPowMetricsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::consensus_difficulty_snapshot(&chain);

    Json(ApiResponse::ok(ConsensusPowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        window_size: snapshot.policy.window_size,
        best_height: snapshot.best_height,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        suggested_difficulty: u64::from(snapshot.expected_bits),
        current_bits: snapshot.current_bits,
        suggested_bits: snapshot.expected_bits,
        target_u64: snapshot.expected_target_u64,
        target_hex: snapshot.expected_target_hex.clone(),
        current_target_hex: snapshot.current_target_hex.clone(),
        pow_limit_bits: snapshot.pow_limit_bits,
        pow_limit_target_hex: snapshot.pow_limit_target_hex.clone(),
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        target_multiplier_bps: snapshot.target_multiplier_bps,
        retarget_was_clamped: snapshot.retarget_was_clamped,
        target_was_clamped_to_pow_limit: snapshot.target_was_clamped_to_pow_limit,
        retarget_rationale: snapshot.retarget_rationale.clone(),
        retarget_signal_quality: snapshot.retarget_signal_quality.clone(),
        notes: vec![
            "Metrics are sourced from the same fixed consensus snapshot used by templates and block validation".to_string(),
            format!(
                "Canonical compact bits target 1 block every {} seconds",
                snapshot.policy.target_block_interval_secs
            ),
            format!(
                "Work multiplier={} bps target multiplier={} bps",
                snapshot.retarget_multiplier_bps, snapshot.target_multiplier_bps
            ),
            "Environment variables do not alter consensus retarget parameters".to_string(),
            "The observation window follows the selected chain and excludes genesis timestamp zero".to_string(),
        ],
    }))
}
