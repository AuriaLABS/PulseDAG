use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Deserialize)]
pub struct PowMineHeaderRequest {
    pub header: pulsedag_core::BlockHeader,
    pub max_tries: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct PowMineHeaderData {
    pub declared_algorithm: String,
    pub effective_mode: String,
    pub accepted: bool,
    pub tries: u64,
    pub final_nonce: u64,
    pub final_hash_hex: String,
    pub target_u64: u64,
}

pub async fn post_pow_mine_header(Json(req): Json<PowMineHeaderRequest>) -> Json<ApiResponse<PowMineHeaderData>> {
    let max_tries = req.max_tries.unwrap_or(10_000).min(1_000_000);
    let (header, accepted, tries, final_hash_hex) = pulsedag_core::dev_mine_header(req.header, max_tries);
    let target_u64 = pulsedag_core::dev_target_u64(header.difficulty as u64);

    Json(ApiResponse::ok(PowMineHeaderData {
        declared_algorithm: pulsedag_core::selected_pow_name().to_string(),
        effective_mode: "dev-surrogate-mining-loop".to_string(),
        accepted,
        tries,
        final_nonce: header.nonce,
        final_hash_hex,
        target_u64,
    }))
}
