use crate::{
    api::p2p_status_for_rpc, api::read_chain_for_rpc, api::read_runtime_for_rpc, api::ApiResponse,
    api::RpcStateLike,
};
use axum::{extract::State, Json};
use pulsedag_core::preferred_tip_hash;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadinessStatus {
    Pass,
    Warn,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessCategory {
    pub status: ReadinessStatus,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessMetrics {
    pub accepted_blocks: u64,
    pub rejected_blocks_by_reason: BTreeMap<String, u64>,
    pub orphan_count: usize,
    pub pending_missing_parents: usize,
    pub pending_block_requests: usize,
    pub inflight_block_requests: usize,
    pub duplicate_block_requests_suppressed: u64,
    pub missing_parent_requests_sent: u64,
    pub orphan_blocks_retried: u64,
    pub orphan_blocks_resolved: u64,
    pub selected_tip: Option<String>,
    pub best_height: u64,
    pub p2p_peer_count: usize,
    pub storage_last_commit_height: Option<u64>,
    pub state_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessData {
    pub effective_rpc_bind: String,
    pub effective_api_profile: String,
    pub admin_enabled: bool,
    pub storage_path_class: String,
    pub peer_health: String,
    pub mining_templates_available: bool,
    pub node_ready: bool,
    pub private_testnet_ready: bool,
    pub public_testnet_ready: bool,
    pub ready_for_release: bool,
    pub overall_status: ReadinessStatus,
    pub categories: BTreeMap<String, ReadinessCategory>,
    pub metrics: ReadinessMetrics,
    pub release_blockers: Vec<String>,
    pub warnings: Vec<String>,
}

fn category(status: ReadinessStatus, reasons: Vec<String>) -> ReadinessCategory {
    ReadinessCategory { status, reasons }
}

fn overall_status(categories: &BTreeMap<String, ReadinessCategory>) -> ReadinessStatus {
    if categories
        .values()
        .any(|category| category.status == ReadinessStatus::Fail)
    {
        ReadinessStatus::Fail
    } else if categories
        .values()
        .any(|category| category.status == ReadinessStatus::Warn)
    {
        ReadinessStatus::Warn
    } else if categories
        .values()
        .any(|category| category.status == ReadinessStatus::Unknown)
    {
        ReadinessStatus::Unknown
    } else {
        ReadinessStatus::Pass
    }
}

fn status_reasons(
    fail: Vec<String>,
    warn: Vec<String>,
    pass_reason: impl Into<String>,
) -> ReadinessCategory {
    if !fail.is_empty() {
        category(ReadinessStatus::Fail, fail)
    } else if !warn.is_empty() {
        category(ReadinessStatus::Warn, warn)
    } else {
        category(ReadinessStatus::Pass, vec![pass_reason.into()])
    }
}

static READINESS_RESPONSE_CACHE: OnceLock<Mutex<Option<ReadinessData>>> = OnceLock::new();

fn cached_readiness_response(reason: String) -> Option<ReadinessData> {
    READINESS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|cache| cache.clone())
        .map(|mut data| {
            data.overall_status = ReadinessStatus::Warn;
            data.node_ready = false;
            data.private_testnet_ready = false;
            data.ready_for_release = false;
            data.warnings.push(format!(
                "rpc_degraded_response: returning stale readiness because {reason}"
            ));
            data.categories.insert(
                "rpc_capture".to_string(),
                category(
                    ReadinessStatus::Warn,
                    vec![format!("stale degraded readiness fallback: {reason}")],
                ),
            );
            data
        })
}

fn cache_readiness_response(data: &ReadinessData) {
    if let Ok(mut cache) = READINESS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *cache = Some(data.clone());
    }
}

pub async fn get_readiness<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<ReadinessData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let snapshot_metadata = match state.storage().snapshot_metadata() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_block_count = match state.storage().block_count() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_block_hashes = if persisted_block_count == 0 {
        Some(BTreeSet::new())
    } else {
        match state.storage().list_blocks() {
            Ok(blocks) if blocks.len() == persisted_block_count => Some(
                blocks
                    .into_iter()
                    .map(|block| block.hash)
                    .collect::<BTreeSet<_>>(),
            ),
            Ok(_) => None,
            Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
        }
    };

    let p2p_status = match p2p_status_for_rpc(state.p2p(), "/readiness").await {
        Ok(status) => status,
        Err(e) => {
            if let Some(data) = cached_readiness_response(e.clone()) {
                return Json(ApiResponse::ok(data));
            }
            return Json(ApiResponse::err("P2P_STATUS_BUSY", e));
        }
    };
    let chain_handle = state.chain();
    let chain = match read_chain_for_rpc(&chain_handle, "/readiness").await {
        Ok(chain) => chain,
        Err(e) => {
            if let Some(data) = cached_readiness_response(e.clone()) {
                return Json(ApiResponse::ok(data));
            }
            return Json(ApiResponse::err("STATE_LOCK_BUSY", e));
        }
    };
    let runtime_handle = state.runtime();
    let runtime = match read_runtime_for_rpc(&runtime_handle, "/readiness").await {
        Ok(runtime) => runtime,
        Err(e) => {
            if let Some(data) = cached_readiness_response(e.clone()) {
                return Json(ApiResponse::ok(data));
            }
            return Json(ApiResponse::err("STATE_LOCK_BUSY", e));
        }
    };
    let p2p_mode = std::env::var("PULSEDAG_P2P_MODE").unwrap_or_else(|_| "unknown".to_string());
    let rpc_bind = std::env::var("PULSEDAG_EFFECTIVE_RPC_BIND")
        .or_else(|_| std::env::var("PULSEDAG_RPC_BIND"))
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let api_profile =
        std::env::var("PULSEDAG_API_PROFILE").unwrap_or_else(|_| "local_dev".to_string());
    let admin_enabled =
        std::env::var("PULSEDAG_ADMIN_ENABLED").unwrap_or_else(|_| "false".to_string()) == "true";
    let storage_path = std::env::var("PULSEDAG_STORAGE_PATH").unwrap_or_default();
    let storage_path_class = if storage_path.is_empty() {
        "default".to_string()
    } else if storage_path.starts_with("/") {
        "absolute_configured".to_string()
    } else {
        "relative_configured".to_string()
    };
    let p2p_peer_count = p2p_status
        .as_ref()
        .map(|snapshot| snapshot.status.connected_peers.len())
        .unwrap_or(0);

    let selected_tip = preferred_tip_hash(&chain);
    let state_root = chain.utxo.compute_state_root().ok();
    let storage_last_commit_height = snapshot_metadata
        .as_ref()
        .map(|metadata| metadata.best_height);

    let metrics = ReadinessMetrics {
        accepted_blocks: runtime.pulsedag_blocks_accepted_total,
        rejected_blocks_by_reason: runtime.rejected_blocks_by_reason.clone(),
        orphan_count: chain.orphan_blocks.len(),
        pending_missing_parents: pulsedag_core::pending_missing_parent_count(&chain),
        pending_block_requests: runtime.pending_block_requests,
        inflight_block_requests: runtime.inflight_block_requests,
        duplicate_block_requests_suppressed: runtime.duplicate_block_requests_suppressed,
        missing_parent_requests_sent: runtime.missing_parent_requests_sent,
        orphan_blocks_retried: runtime.orphan_blocks_retried,
        orphan_blocks_resolved: runtime.orphan_blocks_resolved,
        selected_tip: selected_tip.clone(),
        best_height: chain.dag.best_height,
        p2p_peer_count,
        storage_last_commit_height,
        state_root: state_root.clone(),
    };

    let mut categories = BTreeMap::new();

    categories.insert(
        "consensus".to_string(),
        status_reasons(
            if !chain.dag.blocks.contains_key(&chain.dag.genesis_hash) {
                vec!["genesis block missing from in-memory dag".to_string()]
            } else if selected_tip.is_none() {
                vec!["no deterministic selected tip is available".to_string()]
            } else {
                Vec::new()
            },
            if runtime.pulsedag_blocks_rejected_total > runtime.pulsedag_blocks_accepted_total
                && runtime.pulsedag_blocks_rejected_total > 0
            {
                vec![format!(
                    "rejected blocks ({}) exceed accepted blocks ({}) since startup",
                    runtime.pulsedag_blocks_rejected_total, runtime.pulsedag_blocks_accepted_total
                )]
            } else {
                Vec::new()
            },
            "consensus tip selection and genesis anchor are available",
        ),
    );

    let mut dag_fail = Vec::new();
    let mut dag_warn = Vec::new();
    let memory_block_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    let block_hashes_aligned = persisted_block_hashes
        .as_ref()
        .is_some_and(|persisted_hashes| persisted_hashes == &memory_block_hashes);
    if persisted_block_count != chain.dag.blocks.len() {
        dag_fail.push(format!(
            "memory block count ({}) and persisted block count ({}) are not aligned",
            chain.dag.blocks.len(),
            persisted_block_count
        ));
    } else if !block_hashes_aligned {
        dag_fail.push(
            "memory and persisted block hash sets are not aligned despite matching counts"
                .to_string(),
        );
    }
    if chain.dag.tips.is_empty() {
        dag_fail.push("no active tips in dag".to_string());
    }
    let pending_missing_parents = pulsedag_core::pending_missing_parent_count(&chain);
    if !chain.orphan_missing_parents.is_empty() {
        dag_warn.push(format!(
            "{} orphan block(s) are waiting for {} missing parent(s)",
            chain.orphan_blocks.len(),
            pending_missing_parents
        ));
    }
    if runtime.pending_block_requests > 0 || runtime.inflight_block_requests > 0 {
        dag_warn.push(format!(
            "{} pending / {} inflight block request(s) remain",
            runtime.pending_block_requests, runtime.inflight_block_requests
        ));
    }
    categories.insert(
        "dag".to_string(),
        status_reasons(
            dag_fail,
            dag_warn,
            "dag tips, blocks, and persistence are aligned",
        ),
    );

    categories.insert(
        "pow".to_string(),
        status_reasons(
            Vec::new(),
            if runtime.pulsedag_invalid_pow_total > 0 {
                vec![format!(
                    "{} invalid PoW block(s) observed since startup",
                    runtime.pulsedag_invalid_pow_total
                )]
            } else {
                Vec::new()
            },
            "no invalid PoW blocks observed since startup",
        ),
    );

    categories.insert(
        "p2p".to_string(),
        if state.p2p().is_none() {
            category(
                ReadinessStatus::Unknown,
                vec!["p2p is disabled".to_string()],
            )
        } else if p2p_peer_count == 0 {
            category(
                ReadinessStatus::Warn,
                vec!["p2p is enabled but no peers are connected".to_string()],
            )
        } else {
            category(
                ReadinessStatus::Pass,
                vec![format!("{} p2p peer(s) connected", p2p_peer_count)],
            )
        },
    );

    categories.insert(
        "api_profile_safety".to_string(),
        if (rpc_bind.starts_with("0.0.0.0:") || !rpc_bind.starts_with("127.0.0.1:"))
            && api_profile == "local_dev"
        {
            category(
                ReadinessStatus::Fail,
                vec!["API profile local_dev is unsafe for non-local RPC bind".to_string()],
            )
        } else {
            category(
                ReadinessStatus::Pass,
                vec![format!("api_profile={api_profile} rpc_bind={rpc_bind}")],
            )
        },
    );

    categories.insert(
        "admin_exposure".to_string(),
        if admin_enabled
            && (rpc_bind.starts_with("0.0.0.0:") || !rpc_bind.starts_with("127.0.0.1:"))
        {
            category(
                ReadinessStatus::Fail,
                vec!["admin is exposed on non-local RPC bind".to_string()],
            )
        } else if admin_enabled {
            category(
                ReadinessStatus::Warn,
                vec!["admin endpoints are enabled".to_string()],
            )
        } else {
            category(
                ReadinessStatus::Pass,
                vec!["admin endpoints disabled".to_string()],
            )
        },
    );

    let mut storage_fail = Vec::new();
    let mut storage_warn = Vec::new();
    if persisted_block_count != chain.dag.blocks.len() {
        storage_fail.push(format!(
            "persisted block count ({persisted_block_count}) does not match in-memory DAG block count ({})",
            chain.dag.blocks.len()
        ));
    } else if !block_hashes_aligned {
        storage_fail.push(
            "persisted block hashes do not match in-memory DAG block hashes despite matching counts"
                .to_string(),
        );
    }
    if !snapshot_exists {
        storage_warn.push("snapshot is missing".to_string());
    }
    if let Some(storage_height) = storage_last_commit_height {
        if storage_height < chain.dag.best_height {
            storage_warn.push(format!(
                "storage last commit height {} is behind best height {}",
                storage_height, chain.dag.best_height
            ));
        }
    } else {
        storage_warn.push("storage last commit height is unknown".to_string());
    }
    categories.insert(
        "storage".to_string(),
        status_reasons(
            storage_fail,
            storage_warn,
            "storage snapshot and block index are current",
        ),
    );

    let mempool_pressure = (chain.mempool.transactions.len() * 100)
        .checked_div(chain.mempool.max_transactions)
        .unwrap_or(0);
    categories.insert(
        "mempool".to_string(),
        status_reasons(
            if chain.mempool.transactions.len() >= chain.mempool.max_transactions {
                vec!["mempool is at capacity".to_string()]
            } else {
                Vec::new()
            },
            if mempool_pressure >= 75 {
                vec![format!("mempool pressure is {}%", mempool_pressure)]
            } else {
                Vec::new()
            },
            "mempool pressure is below warning threshold",
        ),
    );

    categories.insert(
        "mining".to_string(),
        if runtime.pulsedag_mining_templates_total == 0
            && runtime.pulsedag_mining_submits_total == 0
        {
            category(
                ReadinessStatus::Unknown,
                vec!["no mining templates or submissions observed since startup".to_string()],
            )
        } else if runtime.rejected_mined_blocks > 0 || runtime.external_mining_submit_rejected > 0 {
            category(
                ReadinessStatus::Warn,
                vec![format!(
                    "mining has {} rejected block(s) and {} external submit rejection(s)",
                    runtime.rejected_mined_blocks, runtime.external_mining_submit_rejected
                )],
            )
        } else {
            category(
                ReadinessStatus::Pass,
                vec!["mining templates/submissions have no observed rejections".to_string()],
            )
        },
    );

    categories.insert(
        "replay".to_string(),
        status_reasons(
            if runtime.startup_consistency_issue_count > 0
                || (runtime.last_self_audit_unix.is_some() && !runtime.last_self_audit_ok)
            {
                vec![format!(
                    "startup/self-audit reported {} issue(s)",
                    runtime
                        .startup_consistency_issue_count
                        .max(runtime.last_self_audit_issue_count)
                )]
            } else {
                Vec::new()
            },
            if runtime.startup_replay_required || runtime.startup_fallback_reason.is_some() {
                vec![format!(
                    "startup replay_required={} fallback_reason={}",
                    runtime.startup_replay_required,
                    runtime
                        .startup_fallback_reason
                        .clone()
                        .unwrap_or_else(|| "none".to_string())
                )]
            } else {
                Vec::new()
            },
            "startup replay and self-audit indicators are clean",
        ),
    );

    categories.insert(
        "sync_status".to_string(),
        if runtime.sync_state == "degraded"
            || runtime.sync_pipeline.last_error.is_some()
            || runtime.sync_pipeline.counters.phase_failures > 5
        {
            category(
                ReadinessStatus::Fail,
                vec![format!(
                    "sync degraded: state={} last_error={} phase_failures={}",
                    runtime.sync_state,
                    runtime
                        .sync_pipeline
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "none".to_string()),
                    runtime.sync_pipeline.counters.phase_failures
                )],
            )
        } else {
            category(
                ReadinessStatus::Pass,
                vec![format!(
                    "sync state healthy: {} (p2p_mode={})",
                    runtime.sync_state, p2p_mode
                )],
            )
        },
    );

    if runtime.external_mining_rejected_chain_id_mismatch > 0 {
        categories.insert(
            "chain_id".to_string(),
            category(
                ReadinessStatus::Fail,
                vec![format!(
                    "chain id mismatch detected {} time(s)",
                    runtime.external_mining_rejected_chain_id_mismatch
                )],
            ),
        );
    } else {
        categories.insert(
            "chain_id".to_string(),
            category(
                ReadinessStatus::Pass,
                vec!["no chain id mismatch observed".to_string()],
            ),
        );
    }

    if !runtime.last_self_audit_ok || runtime.last_self_audit_issue_count > 0 {
        categories.insert(
            "critical_warnings".to_string(),
            category(
                ReadinessStatus::Fail,
                vec![format!(
                    "unresolved critical warnings: self-audit issue_count={}",
                    runtime.last_self_audit_issue_count
                )],
            ),
        );
    }

    categories.insert(
        "public_testnet_evidence".to_string(),
        category(
            ReadinessStatus::Warn,
            vec!["public testnet readiness is gated by explicit evidence and remains disabled for v2.2.19".to_string()],
        ),
    );

    let blockers = categories
        .iter()
        .filter(|(_, category)| category.status == ReadinessStatus::Fail)
        .flat_map(|(name, category)| {
            category
                .reasons
                .iter()
                .map(move |reason| format!("{name}: {reason}"))
        })
        .collect::<Vec<_>>();

    let node_ready = blockers.is_empty();
    let private_testnet_ready = node_ready;
    let public_testnet_ready = false;

    let warnings = categories
        .iter()
        .filter(|(_, category)| {
            matches!(
                category.status,
                ReadinessStatus::Warn | ReadinessStatus::Unknown
            )
        })
        .flat_map(|(name, category)| {
            category
                .reasons
                .iter()
                .map(move |reason| format!("{name}: {reason}"))
        })
        .collect::<Vec<_>>();
    let overall_status = overall_status(&categories);

    let peer_health = if state.p2p().is_none() {
        "p2p_disabled".to_string()
    } else if p2p_peer_count == 0 {
        "no_peers".to_string()
    } else {
        "peers_connected".to_string()
    };
    let data = ReadinessData {
        effective_rpc_bind: rpc_bind,
        effective_api_profile: api_profile,
        admin_enabled,
        storage_path_class,
        peer_health,
        mining_templates_available: runtime.pulsedag_mining_templates_total > 0,
        node_ready,
        private_testnet_ready,
        public_testnet_ready,
        ready_for_release: private_testnet_ready,
        overall_status,
        categories,
        metrics,
        release_blockers: blockers,
        warnings,
    };
    cache_readiness_response(&data);
    Json(ApiResponse::ok(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_status_serializes_lowercase_values() {
        assert_eq!(
            serde_json::to_string(&ReadinessStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&ReadinessStatus::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&ReadinessStatus::Fail).unwrap(),
            "\"fail\""
        );
        assert_eq!(
            serde_json::to_string(&ReadinessStatus::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    #[test]
    fn readiness_data_serializes_categories_and_metrics() {
        let mut categories = BTreeMap::new();
        categories.insert(
            "consensus".to_string(),
            ReadinessCategory {
                status: ReadinessStatus::Pass,
                reasons: vec!["selected tip is stable".to_string()],
            },
        );
        let mut rejected_blocks_by_reason = BTreeMap::new();
        rejected_blocks_by_reason.insert("invalid_pow".to_string(), 2);
        let data = ReadinessData {
            effective_rpc_bind: "127.0.0.1:8080".to_string(),
            effective_api_profile: "local_dev".to_string(),
            admin_enabled: false,
            storage_path_class: "default".to_string(),
            peer_health: "p2p_disabled".to_string(),
            mining_templates_available: false,
            node_ready: true,
            private_testnet_ready: true,
            public_testnet_ready: false,
            ready_for_release: true,
            overall_status: ReadinessStatus::Warn,
            categories,
            metrics: ReadinessMetrics {
                accepted_blocks: 7,
                rejected_blocks_by_reason,
                orphan_count: 1,
                pending_missing_parents: 1,
                pending_block_requests: 2,
                inflight_block_requests: 2,
                duplicate_block_requests_suppressed: 3,
                missing_parent_requests_sent: 4,
                orphan_blocks_retried: 5,
                orphan_blocks_resolved: 6,
                selected_tip: Some("tip".to_string()),
                best_height: 4,
                p2p_peer_count: 3,
                storage_last_commit_height: Some(4),
                state_root: Some("root".to_string()),
            },
            release_blockers: Vec::new(),
            warnings: vec!["p2p: no peers".to_string()],
        };

        let value = serde_json::to_value(data).unwrap();
        assert_eq!(value["overall_status"], "warn");
        assert_eq!(value["categories"]["consensus"]["status"], "pass");
        assert_eq!(value["metrics"]["accepted_blocks"], 7);
        assert_eq!(
            value["metrics"]["rejected_blocks_by_reason"]["invalid_pow"],
            2
        );
        assert_eq!(value["metrics"]["selected_tip"], "tip");
    }

    #[test]
    fn readiness_defaults_keep_public_testnet_disabled() {
        let data = ReadinessData {
            effective_rpc_bind: "127.0.0.1:8080".to_string(),
            effective_api_profile: "local_dev".to_string(),
            admin_enabled: false,
            storage_path_class: "default".to_string(),
            peer_health: "p2p_disabled".to_string(),
            mining_templates_available: false,
            node_ready: false,
            private_testnet_ready: false,
            public_testnet_ready: false,
            ready_for_release: false,
            overall_status: ReadinessStatus::Warn,
            categories: BTreeMap::new(),
            metrics: ReadinessMetrics {
                accepted_blocks: 0,
                rejected_blocks_by_reason: BTreeMap::new(),
                orphan_count: 0,
                pending_missing_parents: 0,
                pending_block_requests: 0,
                inflight_block_requests: 0,
                duplicate_block_requests_suppressed: 0,
                missing_parent_requests_sent: 0,
                orphan_blocks_retried: 0,
                orphan_blocks_resolved: 0,
                selected_tip: None,
                best_height: 0,
                p2p_peer_count: 0,
                storage_last_commit_height: None,
                state_root: None,
            },
            release_blockers: vec!["public_testnet_evidence: missing explicit gate evidence".to_string()],
            warnings: vec!["public_testnet_evidence: public testnet readiness is gated by explicit evidence and remains disabled for v2.2.19".to_string()],
        };
        let value = serde_json::to_value(data).unwrap();
        assert_eq!(value["ready_for_release"], false);
        assert_eq!(value["public_testnet_ready"], false);
    }

    #[test]
    fn readiness_overall_status_reflects_warnings_and_blockers() {
        let mut categories = BTreeMap::new();
        categories.insert(
            "warn_only".to_string(),
            ReadinessCategory {
                status: ReadinessStatus::Warn,
                reasons: vec!["warn".to_string()],
            },
        );
        assert_eq!(super::overall_status(&categories), ReadinessStatus::Warn);

        categories.insert(
            "fail".to_string(),
            ReadinessCategory {
                status: ReadinessStatus::Fail,
                reasons: vec!["fail".to_string()],
            },
        );
        assert_eq!(super::overall_status(&categories), ReadinessStatus::Fail);
    }

    #[test]
    fn readiness_contract_does_not_claim_future_versions() {
        let source = include_str!("readiness.rs");
        let future_flag = ["ready", "for", "v3"].join("_");
        let v230_phrase = ["v2.3.0", "readiness"].join(" ");
        let v300_phrase = ["v3.0", "readiness"].join(" ");
        let public_testnet_live_phrase = ["public", "testnet", "live"].join(" ");
        assert!(!source.contains(&future_flag));
        assert!(!source.contains(&v230_phrase));
        assert!(!source.contains(&v300_phrase));
        assert!(!source.contains(&public_testnet_live_phrase));
    }
}
