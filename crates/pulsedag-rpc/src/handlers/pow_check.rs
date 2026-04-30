use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Deserialize)]
pub struct PowCheckHeaderRequest {
    pub header: pulsedag_core::BlockHeader,
}

#[derive(Debug, serde::Serialize)]
pub struct PowCheckHeaderData {
    pub declared_algorithm: String,
    pub effective_mode: String,
    pub hash_hex: String,
    pub score_u64: u64,
    pub target_u64: u64,
    pub accepted: bool,
    pub notes: Vec<String>,
}

pub async fn post_pow_check_header(
    Json(req): Json<PowCheckHeaderRequest>,
) -> Json<ApiResponse<PowCheckHeaderData>> {
    let pow = pulsedag_core::pow_validation_result(&req.header);
    let hash_hex = pow.hash_hex.unwrap_or_default();
    let score_u64 = pow.score_u64.unwrap_or(0);
    let target_u64 = pow.target_u64;
    let accepted = pow.accepted;

    Json(ApiResponse::ok(PowCheckHeaderData {
        declared_algorithm: pulsedag_core::selected_pow_name().to_string(),
        effective_mode: "dev-surrogate-target-check".to_string(),
        hash_hex,
        score_u64,
        target_u64,
        accepted,
        notes: vec![
            "This is a development acceptance check for the PoW pipeline".to_string(),
            "It is useful for flow testing but is not final kHeavyHash consensus logic".to_string(),
        ],
    }))
}
