use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Serialize)]
pub struct ReleaseInfoData {
    pub version: String,
    pub stage: String,
    pub capabilities: Vec<String>,
    pub core_endpoints: Vec<String>,
}

pub fn repo_version() -> String {
    include_str!("../../../../VERSION").trim().to_string()
}

pub fn operator_stage() -> String {
    let version = repo_version();
    let mut parts = version.trim_start_matches('v').split('.');
    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    format!("v{major}.{minor}-readiness")
}

pub async fn get_release_info() -> Json<ApiResponse<ReleaseInfoData>> {
    Json(ApiResponse::ok(ReleaseInfoData {
        version: repo_version(),
        stage: operator_stage(),
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

#[cfg(test)]
mod tests {
    use super::{operator_stage, repo_version};

    #[test]
    fn version_and_stage_match_v2_2_readiness() {
        assert!(repo_version().starts_with("v2.2."));
        assert_eq!(operator_stage(), "v2.2-readiness");
    }

    #[test]
    fn runbook_index_covers_v2_2_operator_topics() {
        let index = include_str!("../../../../docs/runbooks/INDEX.md");
        for required in [
            "Snapshot Restore",
            "Rebuild from Snapshot + Delta",
            "Burn-in Evidence",
            "P2P Recovery / Partition Rejoin",
            "Staging Upgrade",
            "Staging Rollback",
        ] {
            assert!(
                index.contains(required),
                "runbook index missing: {required}"
            );
        }
    }

    #[test]
    fn policy_and_diagnostics_expose_aligned_release_metadata() {
        let policy = include_str!("policy.rs");
        let diagnostics = include_str!("diagnostics.rs");

        assert!(policy.contains("pub version: String"));
        assert!(policy.contains("pub stage: String"));
        assert!(policy.contains("version: repo_version()"));
        assert!(policy.contains("stage: operator_stage()"));

        assert!(diagnostics.contains("pub version: String"));
        assert!(diagnostics.contains("pub stage: String"));
        assert!(diagnostics.contains("version: repo_version()"));
        assert!(diagnostics.contains("stage: operator_stage()"));
    }

    #[test]
    fn dashboard_package_is_published_and_referenced() {
        let index = include_str!("../../../../docs/runbooks/INDEX.md");
        let dashboard_readme = include_str!("../../../../docs/dashboard/README.md");
        let dashboard_json =
            include_str!("../../../../docs/dashboard/assets/pulsedag-operator-overview.json");
        let datasource =
            include_str!("../../../../docs/dashboard/config/datasource-prometheus.yml");

        assert!(index.contains("docs/dashboard/README.md"));
        assert!(dashboard_readme.contains("Operator Dashboard Package (v2.2)"));
        assert!(dashboard_json.contains("PulseDAG Operator Overview (v2.2)"));
        assert!(datasource.contains("PulseDAG-Prometheus"));
    }
    #[test]
    fn legacy_versions_are_not_used_in_operator_handlers() {
        let release = include_str!("release.rs");
        let policy = include_str!("policy.rs");
        let diagnostics = include_str!("diagnostics.rs");
        let stale_versions = [
            format!("v{}.{}.{}", 1, 1, 0),
            format!("v{}.{}.{}", 1, 1, 1),
            ["rc", "final"].join("-"),
        ];
        for stale in stale_versions {
            assert!(
                !release.contains(&stale),
                "release.rs still contains {stale}"
            );
            assert!(!policy.contains(&stale), "policy.rs still contains {stale}");
            assert!(
                !diagnostics.contains(&stale),
                "diagnostics.rs still contains {stale}"
            );
        }
    }
}
