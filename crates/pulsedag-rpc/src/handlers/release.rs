use crate::{api::ApiResponse, redaction::redact_if_sensitive_key_value};
use axum::Json;

#[derive(Debug, serde::Serialize)]
pub struct ReleaseInfoData {
    pub version: String,
    pub git_commit: Option<String>,
    pub build_profile: Option<String>,
    pub capabilities: Vec<String>,
    pub core_endpoints: Vec<String>,
    pub api_profile: String,
    pub pow_algorithm: String,
    pub miner_mode: String,
    pub smart_contracts: String,
    pub pow_engine: String,
    pub pool_logic: String,
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
        git_commit: std::option_env!("GIT_COMMIT").map(|v| v.to_string()),
        build_profile: std::option_env!("PROFILE").map(|v| v.to_string()),
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
            "/p2p/status".into(),
            "/p2p/peers".into(),
            "/p2p/propagation".into(),
            "/checks".into(),
            "/readiness".into(),
        ],
        api_profile: redact_if_sensitive_key_value(
            "PULSEDAG_API_PROFILE",
            &std::env::var("PULSEDAG_API_PROFILE").unwrap_or_else(|_| "local_dev".into()),
        ),
        pow_algorithm: "kHeavyHash".into(),
        miner_mode: "external".into(),
        smart_contracts: "disabled (v2.2.x)".into(),
        pow_engine: "canonical_core".into(),
        pool_logic: "disabled_not_in_node".into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::{operator_stage, repo_version};

    #[test]
    fn version_and_stage_follow_repo_semver_prefix() {
        let version = repo_version();
        let trimmed = version.trim_start_matches('v');
        let mut parts = trimmed.split('.');
        let major = parts.next().expect("semver major");
        let minor = parts.next().expect("semver minor");
        assert!(
            major.parse::<u64>().is_ok(),
            "major must be numeric: {major}"
        );
        assert!(
            minor.parse::<u64>().is_ok(),
            "minor must be numeric: {minor}"
        );
        assert_eq!(operator_stage(), format!("v{major}.{minor}-readiness"));
    }

    #[test]
    fn runbook_index_covers_operator_topics() {
        let index = include_str!("../../../../docs/runbooks/INDEX.md");
        for required in [
            "SNAPSHOT_RESTORE.md",
            "REBUILD_FROM_SNAPSHOT_AND_DELTA.md",
            "RELEASE_EVIDENCE.md",
            "P2P_RECOVERY.md",
            "STAGING_UPGRADE.md",
            "STAGING_ROLLBACK.md",
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
    fn release_metadata_reports_kheavyhash_and_not_sha256d() {
        let release = include_str!("release.rs");
        assert!(release.contains("\"kHeavyHash\""));
        assert!(!release.contains("\"sha256d\""));
        assert!(!release.contains("\"SHA256D\""));
        assert!(release.contains("\"canonical_core\""));
        assert!(release.contains("\"external\""));
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

        assert!(release.contains("\"kHeavyHash\""));
        assert!(release.contains("\"canonical_core\""));
        assert!(release.contains("\"external\""));
    }
}
