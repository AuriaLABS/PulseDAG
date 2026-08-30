use crate::{api::ApiResponse, redaction::redact_if_sensitive_key_value};
use axum::Json;

const SIGNED_TRANSACTION_RELAY_VERSION: &str = "signed-transaction-relay-v1";

#[derive(Debug, serde::Serialize)]
pub struct ReleaseInfoData {
    pub version: String,
    pub git_commit: Option<String>,
    pub build_profile: Option<String>,
    pub network_profile: String,
    pub chain_id: String,
    pub signed_transaction_relay_version: String,
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

fn normalize_config_profile(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "dev" | "development" => Some("dev"),
        "local" => Some("local"),
        "private" | "private-testnet" => Some("private"),
        "testnet" => Some("testnet"),
        "operator" | "staging" => Some("operator"),
        "rehearsal-a" => Some("rehearsal-a"),
        "rehearsal-b" => Some("rehearsal-b"),
        "rehearsal-c" => Some("rehearsal-c"),
        _ => None,
    }
}

fn default_public_network_identity(profile: &str) -> Option<(&'static str, &'static str)> {
    match normalize_config_profile(profile)? {
        "dev" => Some(("dev", "pulsedag-devnet")),
        "local" => Some(("local", "pulsedag-localnet")),
        "private" => Some(("private", "pulsedag-private")),
        "testnet" => Some(("testnet", "pulsedag-testnet")),
        "operator" => Some(("operator", "pulsedag-testnet")),
        "rehearsal-a" => Some(("rehearsal-a", "pulsedag-rehearsal")),
        "rehearsal-b" => Some(("rehearsal-b", "pulsedag-rehearsal")),
        "rehearsal-c" => Some(("rehearsal-c", "pulsedag-rehearsal")),
        _ => None,
    }
}

fn cli_network_profile() -> Option<String> {
    let args = std::env::args().collect::<Vec<_>>();
    let mut selected = None;
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == "--network" {
            if let Some(value) = args.get(index + 1) {
                selected = Some(value.clone());
                index += 1;
            }
        }
        index += 1;
    }
    selected
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn effective_public_network_identity() -> (String, String) {
    let selected_profile = cli_network_profile()
        .or_else(|| nonempty_env("PULSEDAG_CONFIG_PROFILE"))
        .unwrap_or_else(|| "dev".to_string());
    let (default_network_profile, default_chain_id) =
        default_public_network_identity(&selected_profile).unwrap_or(("unknown", "unknown"));

    let network_profile = nonempty_env("PULSEDAG_NETWORK_PROFILE")
        .unwrap_or_else(|| default_network_profile.to_string());
    let chain_id =
        nonempty_env("PULSEDAG_CHAIN_ID").unwrap_or_else(|| default_chain_id.to_string());
    (network_profile, chain_id)
}

fn release_capabilities() -> Vec<String> {
    vec![
        "keyless_node".into(),
        "signed_transaction_relay".into(),
        "external_miner_protocol".into(),
        "mempool".into(),
        "explorer_api".into(),
        "sync_diagnostics".into(),
        "storage_snapshot_inspection".into(),
        "p2p_observability".into(),
        "release_readiness_checks".into(),
        "contracts_disabled".into(),
    ]
}

fn release_core_endpoints() -> Vec<String> {
    vec![
        "/health".into(),
        "/status".into(),
        "/dashboard".into(),
        "/blocks".into(),
        "/txs".into(),
        "/address/:address".into(),
        "/address/:address/utxos".into(),
        "/api/v1/tx/submit".into(),
        "/mine".into(),
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
    ]
}

pub async fn get_release_info() -> Json<ApiResponse<ReleaseInfoData>> {
    let (network_profile, chain_id) = effective_public_network_identity();
    Json(ApiResponse::ok(ReleaseInfoData {
        version: repo_version(),
        git_commit: std::option_env!("GIT_COMMIT").map(|v| v.to_string()),
        build_profile: std::option_env!("PROFILE").map(|v| v.to_string()),
        network_profile,
        chain_id,
        signed_transaction_relay_version: SIGNED_TRANSACTION_RELAY_VERSION.to_string(),
        capabilities: release_capabilities(),
        core_endpoints: release_core_endpoints(),
        api_profile: redact_if_sensitive_key_value(
            "PULSEDAG_API_PROFILE",
            &std::env::var("PULSEDAG_API_PROFILE").unwrap_or_else(|_| "local_dev".into()),
        ),
        pow_algorithm: "kHeavyHash".into(),
        miner_mode: "external_standalone_miner".into(),
        smart_contracts: "disabled_not_included".into(),
        pow_engine: "canonical_core".into(),
        pool_logic: "disabled_not_in_node".into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::{default_public_network_identity, operator_stage, repo_version};

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
    fn public_identity_defaults_match_supported_node_profiles() {
        assert_eq!(
            default_public_network_identity("dev"),
            Some(("dev", "pulsedag-devnet"))
        );
        assert_eq!(
            default_public_network_identity("local"),
            Some(("local", "pulsedag-localnet"))
        );
        assert_eq!(
            default_public_network_identity("private-testnet"),
            Some(("private", "pulsedag-private"))
        );
        assert_eq!(
            default_public_network_identity("testnet"),
            Some(("testnet", "pulsedag-testnet"))
        );
        assert_eq!(
            default_public_network_identity("staging"),
            Some(("operator", "pulsedag-testnet"))
        );
        assert_eq!(
            default_public_network_identity("rehearsal-c"),
            Some(("rehearsal-c", "pulsedag-rehearsal"))
        );
        assert_eq!(default_public_network_identity("unsupported"), None);
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
    fn release_metadata_reports_keyless_signed_relay_surface() {
        let release = include_str!("release.rs");
        assert!(release.contains("\"keyless_node\""));
        assert!(release.contains("\"signed_transaction_relay\""));
        assert!(release.contains("\"/api/v1/tx/submit\""));
        assert!(!release.contains("\"/tx/build\""));
        assert!(!release.contains("\"/tx/submit\""));
        assert!(!release.contains("\"wallets\""));
        assert!(!release.contains("\"/wallet/new\""));
        assert!(!release.contains("\"/wallet/transfer\""));
    }

    #[test]
    fn release_metadata_reports_kheavyhash_and_not_sha256d() {
        let release = include_str!("release.rs");
        assert!(release.contains("\"kHeavyHash\""));
        assert!(!release.contains("\"sha256d\""));
        assert!(!release.contains("\"SHA256D\""));
        assert!(release.contains("\"canonical_core\""));
        assert!(release.contains("\"external_standalone_miner\""));
        assert!(release.contains("\"disabled_not_included\""));
        assert!(release.contains("\"disabled_not_in_node\""));
        assert!(release.contains("\"signed_transaction_relay\""));
        assert!(release.contains("signed-transaction-relay-v1"));
        assert!(release.contains("pub network_profile: String"));
        assert!(release.contains("pub chain_id: String"));
        let removed_legacy_capability = ["legacy", "wallet", "rpc", "dev", "only"].join("_");
        assert!(!release.contains(&removed_legacy_capability));
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
        assert!(release.contains("\"external_standalone_miner\""));
        assert!(release.contains("\"disabled_not_included\""));
        assert!(release.contains("\"disabled_not_in_node\""));
    }
}
