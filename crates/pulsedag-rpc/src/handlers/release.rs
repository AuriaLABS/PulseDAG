use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Serialize)]
pub struct ReleaseInfoData {
    pub version: String,
    pub stage: String,
    pub capabilities: Vec<String>,
    pub core_endpoints: Vec<String>,
}

fn repo_version() -> String {
    include_str!("../../../../VERSION").trim().to_string()
}

pub async fn get_release_info() -> Json<ApiResponse<ReleaseInfoData>> {
    Json(ApiResponse::ok(ReleaseInfoData {
        version: repo_version(),
        stage: "rc-final".to_string(),
        capabilities: vec![
            "wallets".into(),
            "external_miner_protocol".into(),
            "mempool".into(),
            "explorer_api".into(),
            "sync_diagnostics".into(),
            "storage_snapshot_inspection".into(),
            "p2p_observability".into(),
            "release_readiness_checks".into(),
            "contracts_disabled".into(),
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
            "/mining/template".into(),
            "/mining/submit".into(),
            "/snapshot".into(),
            "/sync/status".into(),
            "/sync/verify".into(),
            "/checks".into(),
            "/readiness".into(),
        ],
    }))
}
