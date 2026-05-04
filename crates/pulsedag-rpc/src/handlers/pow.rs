use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Serialize)]
pub struct PowInfoData {
    pub algorithm: String,
    pub engine: String,
    pub status: String,
    pub target_model: String,
    pub comparison_width: String,
    pub header_adapter: String,
    pub production_ready: bool,
    pub kaspa_consensus_compatible: bool,
    pub notes: Vec<String>,
}

pub async fn get_pow_info() -> Json<ApiResponse<PowInfoData>> {
    Json(ApiResponse::ok(PowInfoData {
        algorithm: "kHeavyHash".to_string(),
        engine: "kaspa-kheavyhash".to_string(),
        status: "active-devnet".to_string(),
        target_model: "pow_hash <= target".to_string(),
        comparison_width: "256-bit".to_string(),
        header_adapter: "pulsedag-canonical-v2.2.10".to_string(),
        production_ready: false,
        kaspa_consensus_compatible: false,
        notes: vec![
            "PulseDAG uses a Kaspa-based kHeavyHash PoW engine implementation.".to_string(),
            "PulseDAG header and consensus rules are PulseDAG-specific and do not implement full Kaspa consensus compatibility.".to_string(),
            "PoW exposure is devnet-oriented and not production-ready.".to_string(),
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::get_pow_info;

    #[tokio::test]
    async fn pow_reports_final_metadata() {
        let response = get_pow_info().await.0;
        let data = response.data.expect("pow data expected");
        assert_ne!(data.status, "scaffold");
        assert_eq!(data.algorithm, "kHeavyHash");
        assert_eq!(data.engine, "kaspa-kheavyhash");
        assert_eq!(data.comparison_width, "256-bit");
        assert!(!data.production_ready);
        assert!(!data.kaspa_consensus_compatible);
    }
}
