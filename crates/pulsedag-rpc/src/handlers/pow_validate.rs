use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Deserialize)]
pub struct PowValidateHeaderRequest {
    pub header: pulsedag_core::BlockHeader,
}

#[derive(Debug, serde::Serialize)]
pub struct PowValidateHeaderData {
    pub algorithm: String,
    pub structurally_valid: bool,
    pub ready_for_hash_validation: bool,
    pub preimage: String,
    pub target_block_interval_secs: u64,
    pub max_future_drift_secs: u64,
    pub reasons: Vec<String>,
}

pub async fn post_pow_validate_header(
    Json(req): Json<PowValidateHeaderRequest>,
) -> Json<ApiResponse<PowValidateHeaderData>> {
    let header = req.header;
    let mut reasons = Vec::new();

    if header.parents.is_empty() && header.height > 0 {
        reasons.push("non-genesis block should declare at least one parent".to_string());
    }
    if header.difficulty == 0 {
        reasons.push("difficulty must be greater than zero".to_string());
    }
    if header.merkle_root.trim().is_empty() {
        reasons.push("merkle_root is required".to_string());
    }
    if header.state_root.trim().is_empty() {
        reasons.push("state_root is required".to_string());
    }
    let target_block_interval_secs = pulsedag_core::dev_target_block_interval_secs();
    let max_future_drift_secs = pulsedag_core::dev_max_future_drift_secs();

    if header.timestamp == 0 {
        reasons.push("timestamp must be greater than zero".to_string());
    } else {
        let now = pulsedag_core::mining::current_ts();
        if header.timestamp > now.saturating_add(max_future_drift_secs) {
            reasons.push(format!(
                "timestamp is too far in the future for current policy (max drift {}s)",
                max_future_drift_secs
            ));
        }
    }

    let structurally_valid = reasons.is_empty();
    Json(ApiResponse::ok(PowValidateHeaderData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        structurally_valid,
        ready_for_hash_validation: structurally_valid,
        preimage: pulsedag_core::pow_preimage_string(&header),
        target_block_interval_secs,
        max_future_drift_secs,
        reasons,
    }))
}
