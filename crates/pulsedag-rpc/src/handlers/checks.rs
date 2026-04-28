use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use std::collections::BTreeSet;

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
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_hashes = persisted_blocks
        .iter()
        .map(|b| b.hash.clone())
        .collect::<BTreeSet<_>>();

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
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
            ok: memory_hashes == persisted_hashes,
            detail: if memory_hashes == persisted_hashes {
                "memory and storage block sets match".into()
            } else {
                format!(
                    "memory={}, storage={}",
                    memory_hashes.len(),
                    persisted_hashes.len()
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
