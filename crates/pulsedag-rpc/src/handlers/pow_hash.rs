use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Deserialize)]
pub struct PowHashHeaderRequest {
    pub header: pulsedag_core::BlockHeader,
}

#[derive(Debug, serde::Serialize)]
pub struct PowHashHeaderData {
    pub declared_algorithm: String,
    pub effective_hash_mode: String,
    pub preimage: String,
    pub hash_hex: String,
    pub notes: Vec<String>,
}

pub async fn post_pow_hash_header(
    Json(req): Json<PowHashHeaderRequest>,
) -> Json<ApiResponse<PowHashHeaderData>> {
    let preimage = pulsedag_core::pow_preimage_string(&req.header);
    let pow = pulsedag_core::pow_validation_result(&req.header);
    let hash_hex = pow.hash_hex.unwrap_or_default();
    Json(ApiResponse::ok(PowHashHeaderData {
        declared_algorithm: pulsedag_core::selected_pow_name().to_string(),
        effective_hash_mode: "dev-surrogate-blake3".to_string(),
        preimage,
        hash_hex,
        notes: vec![
            "This is an effective development hash for the PoW pipeline".to_string(),
            "It is not a final kHeavyHash-compatible consensus hash yet".to_string(),
        ],
    }))
}
