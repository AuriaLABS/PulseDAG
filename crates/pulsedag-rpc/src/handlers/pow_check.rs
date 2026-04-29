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
    let hash_hex = pulsedag_core::dev_surrogate_pow_hash(&req.header);
    let score_u64 = pulsedag_core::dev_hash_score_u64(&req.header);
    let target_u64 = pulsedag_core::dev_target_u64(req.header.difficulty as u64);
    let accepted = pulsedag_core::dev_pow_accepts(&req.header);

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
