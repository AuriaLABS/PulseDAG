use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct NodeCheckItem {
    pub name: String,
    pub ok: bool,
    pub detail: String,
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
    let p2p_status = state.p2p().and_then(|p| p.status().ok());
    let peer_count = p2p_status
        .as_ref()
        .map(|s| s.connected_peers.len())
        .unwrap_or(0);

    let mut max_block_height = 0u64;
    for block in chain.dag.blocks.values() {
        max_block_height = max_block_height.max(block.header.height);
    }

    let checks = vec![
        NodeCheckItem {
            name: "snapshot_exists".into(),
            ok: snapshot_exists,
            detail: if snapshot_exists {
                "snapshot present".into()
            } else {
                "snapshot missing".into()
            },
        },
        NodeCheckItem {
            name: "storage_consistency".into(),
            ok: invariant_snapshot.is_ok(),
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
            name: "genesis_present".into(),
            ok: chain.dag.blocks.contains_key(&chain.dag.genesis_hash),
            detail: chain.dag.genesis_hash.clone(),
        },
        NodeCheckItem {
            name: "p2p_state".into(),
            ok: state.p2p().is_none() || p2p_status.is_some(),
            detail: if state.p2p().is_some() {
                format!("p2p enabled, peers={peer_count}")
            } else {
                "p2p disabled".into()
            },
        },
        NodeCheckItem {
            name: "tip_presence".into(),
            ok: !chain.dag.tips.is_empty(),
            detail: format!("tips={}", chain.dag.tips.len()),
        },
        NodeCheckItem {
            name: "best_height_consistency".into(),
            ok: chain.dag.best_height == max_block_height,
            detail: format!(
                "best_height={}, max_block_height={}",
                chain.dag.best_height, max_block_height
            ),
        },
        NodeCheckItem {
            name: "contracts_namespaces_ready".into(),
            ok: state.storage().contract_namespaces_ready(),
            detail: if state.storage().contract_namespaces_ready() {
                "contracts namespaces ready".into()
            } else {
                "contracts namespaces missing".into()
            },
        },
    ];

    let overall_ok = checks.iter().all(|c| c.ok);
    Json(ApiResponse::ok(NodeChecksData { overall_ok, checks }))
}
