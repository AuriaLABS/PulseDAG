use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Serialize)]
pub struct ReleaseInfoData {
    pub version: String,
    pub stage: String,
    pub capabilities: Vec<String>,
    pub core_endpoints: Vec<String>,
}

pub async fn get_release_info() -> Json<ApiResponse<ReleaseInfoData>> {
    Json(ApiResponse::ok(ReleaseInfoData {
        version: "v1.0.0".to_string(),
        stage: "stable".to_string(),
        capabilities: vec![
            "wallets".into(),
            "mining".into(),
            "mempool".into(),
            "explorer_api".into(),
            "sync_diagnostics".into(),
            "storage_snapshot_inspection".into(),
            "p2p_observability".into(),
            "release_readiness_checks".into(),
        ],
        core_endpoints: vec![
            "/health".into(),
            "/status".into(),
            "/dashboard".into(),
            "/blocks".into(),
            "/txs".into(),
            "/address/:address".into(),
            "/mine".into(),
            "/wallet/new".into(),
            "/wallet/transfer".into(),
            "/sync/status".into(),
            "/sync/verify".into(),
            "/snapshot".into(),
            "/checks".into(),
            "/readiness".into(),
        ],
    }))
}
