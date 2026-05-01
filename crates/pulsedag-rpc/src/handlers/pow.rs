use crate::api::ApiResponse;
use axum::Json;

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
        status: "pre-private-testnet-hardening".to_string(),
        target_model: "hash <= target".to_string(),
        notes: vec![
            "Intended algorithm remains kHeavyHash, with deterministic PoW validation and canonical preimage foundations in v2.2.8.".to_string(),
            "Implementation status: deterministic-devnet-engine suitable for dev/private-testnet rehearsal, not production readiness.".to_string(),
            "Final algorithm compatibility is not yet declared production/final and remains part of v2.3.0 closure.".to_string(),
        ],
    }))
}
