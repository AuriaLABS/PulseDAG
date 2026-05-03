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
        status: "active-devnet".to_string(),
        target_model: "pow_hash <= target".to_string(),
        notes: vec![
            "PulseDAG uses a Kaspa-based kHeavyHash-style PoW hashing adapter in the canonical PoW engine.".to_string(),
            "PulseDAG consensus and header rules are PulseDAG-specific and are not Kaspa consensus rules.".to_string(),
            "v2.2.9 is private-testnet rehearsal only; miner remains external.".to_string(),
        ],
    }))
}
