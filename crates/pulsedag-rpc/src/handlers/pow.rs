use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Serialize)]
pub struct PowInfoData {
    pub algorithm: String,
    pub status: String,
    pub target_model: String,
    pub notes: Vec<String>,
}

pub async fn get_pow_info() -> Json<ApiResponse<PowInfoData>> {
    Json(ApiResponse::ok(PowInfoData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        status: "scaffold".to_string(),
        target_model: "hash <= target".to_string(),
        notes: vec![
            "PulseDAG now declares kHeavyHash as the intended PoW algorithm".to_string(),
            "This release adds the architectural scaffold, not a fully compatible Kaspa consensus implementation".to_string(),
            "Header preimage preparation is separated to ease future mining/validation integration".to_string(),
        ],
    }))
}
