use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_core::preferred_tip_hash;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    pub selected_tip: Option<String>,
    pub best_height: u64,
    pub p2p_peer_count: usize,
    pub storage_last_commit_height: Option<u64>,
    pub state_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessData {
    pub ready_for_v3: bool,
    pub ready_for_release: bool,
    pub overall_status: ReadinessStatus,
    pub categories: BTreeMap<String, ReadinessCategory>,
    pub metrics: ReadinessMetrics,
    pub blockers: Vec<String>,
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

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_hashes = persisted_blocks
        .iter()
        .map(|b| b.hash.clone())
        .collect::<BTreeSet<_>>();
    let persisted_best_height = persisted_blocks.iter().map(|b| b.header.height).max();

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let p2p_status = state.p2p().and_then(|p| p.status().ok());
    let p2p_peer_count = p2p_status
        .as_ref()
        .map(|status| status.connected_peers.len())
        .unwrap_or(0);

    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    let selected_tip = preferred_tip_hash(&chain);
    let state_root = chain.utxo.compute_state_root().ok();
    let storage_last_commit_height = snapshot_metadata
        .as_ref()
        .map(|metadata| metadata.best_height)
        .or(persisted_best_height);

    let metrics = ReadinessMetrics {
        accepted_blocks: runtime.pulsedag_blocks_accepted_total,
        rejected_blocks_by_reason: runtime.rejected_blocks_by_reason.clone(),
        orphan_count: chain.orphan_blocks.len(),
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
    if memory_hashes != persisted_hashes {
        dag_fail.push("memory and persisted blocks are not aligned".to_string());
    }
    if chain.dag.tips.is_empty() {
        dag_fail.push("no active tips in dag".to_string());
    }
    if !chain.orphan_missing_parents.is_empty() {
        dag_warn.push(format!(
            "{} orphan block(s) are waiting for missing parents",
            chain.orphan_blocks.len()
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

    let mut storage_fail = Vec::new();
    let mut storage_warn = Vec::new();
    if memory_hashes != persisted_hashes {
        storage_fail.push("persisted block set does not match in-memory DAG".to_string());
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

    let mempool_pressure = if chain.mempool.max_transactions == 0 {
        0
    } else {
        chain.mempool.transactions.len() * 100 / chain.mempool.max_transactions
    };
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

    let overall_status = overall_status(&categories);
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

    let ready_for_v3 = overall_status == ReadinessStatus::Pass;
    Json(ApiResponse::ok(ReadinessData {
        ready_for_v3,
        ready_for_release: blockers.is_empty(),
        overall_status,
        categories,
        metrics,
        blockers,
        warnings,
    }))
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
            ready_for_v3: false,
            ready_for_release: true,
            overall_status: ReadinessStatus::Warn,
            categories,
            metrics: ReadinessMetrics {
                accepted_blocks: 7,
                rejected_blocks_by_reason,
                orphan_count: 1,
                selected_tip: Some("tip".to_string()),
                best_height: 4,
                p2p_peer_count: 3,
                storage_last_commit_height: Some(4),
                state_root: Some("root".to_string()),
            },
            blockers: Vec::new(),
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
}
