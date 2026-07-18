use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct NodeCheckItem {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_storage_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_memory_dag_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_only_hashes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_only_hashes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch_acceptance_sources: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_generation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_generation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_hash_set_digest: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct NodeChecksData {
    pub overall_ok: bool,
    pub checks: Vec<NodeCheckItem>,
}

pub async fn get_node_checks<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeChecksData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let invariant_snapshot = match state.storage().verify_accepted_storage_invariants(&chain) {
        Ok(report) => report,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let chain_anchor_valid = match state.storage().chain_anchor_valid(&chain) {
        Ok(valid) => valid,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let p2p_status = state.p2p().and_then(|p| p.status().ok());
    let peer_count = p2p_status
        .as_ref()
        .map(|s| s.connected_peers.len())
        .unwrap_or(0);

    let mut max_block_height = 0u64;
    for block in chain.dag.blocks.values() {
        max_block_height = max_block_height.max(block.header.height);
    }

    let accepted_hash_set_digest = invariant_snapshot.storage_generation.clone();
    let checks = vec![
        NodeCheckItem {
            name: "snapshot_exists".into(),
            ok: snapshot_exists,
            detail: if snapshot_exists {
                "snapshot present".into()
            } else {
                "snapshot missing".into()
            },
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
        NodeCheckItem {
            name: "storage_consistency".into(),
            ok: invariant_snapshot.is_ok(),
            accepted_storage_count: Some(invariant_snapshot.accepted_storage_count),
            in_memory_dag_count: Some(invariant_snapshot.in_memory_dag_count),
            storage_only_hashes: Some(invariant_snapshot.storage_only_hashes.clone()),
            memory_only_hashes: Some(invariant_snapshot.memory_only_hashes.clone()),
            mismatch_acceptance_sources: Some(
                invariant_snapshot.mismatch_acceptance_sources.clone(),
            ),
            memory_generation: Some(invariant_snapshot.memory_generation.clone()),
            storage_generation: Some(invariant_snapshot.storage_generation.clone()),
            accepted_hash_set_digest: Some(accepted_hash_set_digest),
            detail: if invariant_snapshot.is_ok() {
                format!(
                    "memory and storage block sets match ({})",
                    invariant_snapshot.memory_generation
                )
            } else {
                format!(
                    "memory={} storage={} storage_only={:?} memory_only={:?} sources={:?} memory_generation={} storage_generation={}",
                    invariant_snapshot.in_memory_dag_count,
                    invariant_snapshot.accepted_storage_count,
                    invariant_snapshot.storage_only_hashes,
                    invariant_snapshot.memory_only_hashes,
                    invariant_snapshot.mismatch_acceptance_sources,
                    invariant_snapshot.memory_generation,
                    invariant_snapshot.storage_generation
                )
            },
        },
        NodeCheckItem {
            name: "chain_anchor_valid".into(),
            ok: chain_anchor_valid,
            detail: if chain.dag.blocks.contains_key(&chain.dag.genesis_hash) {
                format!("genesis present: {}", chain.dag.genesis_hash)
            } else if chain_anchor_valid {
                format!(
                    "valid compact prune checkpoint for genesis {}",
                    chain.dag.genesis_hash
                )
            } else {
                format!(
                    "missing chain anchor for genesis {}",
                    chain.dag.genesis_hash
                )
            },
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
        NodeCheckItem {
            name: "p2p_state".into(),
            ok: state.p2p().is_none() || p2p_status.is_some(),
            detail: if state.p2p().is_some() {
                format!("p2p enabled, peers={peer_count}")
            } else {
                "p2p disabled".into()
            },
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
        NodeCheckItem {
            name: "tip_presence".into(),
            ok: !chain.dag.tips.is_empty(),
            detail: format!("tips={}", chain.dag.tips.len()),
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
        NodeCheckItem {
            name: "best_height_consistency".into(),
            ok: chain.dag.best_height == max_block_height,
            detail: format!(
                "best_height={}, max_block_height={}",
                chain.dag.best_height, max_block_height
            ),
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
        NodeCheckItem {
            name: "contracts_namespaces_ready".into(),
            ok: state.storage().contract_namespaces_ready(),
            detail: if state.storage().contract_namespaces_ready() {
                "contracts namespaces ready".into()
            } else {
                "contracts namespaces missing".into()
            },
            accepted_storage_count: None,
            in_memory_dag_count: None,
            storage_only_hashes: None,
            memory_only_hashes: None,
            mismatch_acceptance_sources: None,
            memory_generation: None,
            storage_generation: None,
            accepted_hash_set_digest: None,
        },
    ];

    let overall_ok = checks.iter().all(|c| c.ok);
    Json(ApiResponse::ok(NodeChecksData { overall_ok, checks }))
}
