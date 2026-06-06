use axum::{extract::State, Json};
use std::sync::{Mutex, OnceLock};

use crate::{api::read_chain_for_rpc, api::ApiResponse, api::RpcStateLike};

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanEntry {
    pub hash: String,
    pub missing_parents: Vec<String>,
    pub received_at_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanStatusData {
    pub rpc_response_degraded: bool,
    pub rpc_response_stale: bool,
    pub rpc_response_degraded_reason: Option<String>,
    pub orphan_count: usize,
    pub orphans: Vec<OrphanEntry>,
}

static ORPHANS_RESPONSE_CACHE: OnceLock<Mutex<Option<OrphanStatusData>>> = OnceLock::new();

fn cached_orphans_response(reason: String) -> Option<OrphanStatusData> {
    ORPHANS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|cache| cache.clone())
        .map(|mut data| {
            data.rpc_response_degraded = true;
            data.rpc_response_stale = true;
            data.rpc_response_degraded_reason = Some(reason);
            data
        })
}

fn cache_orphans_response(data: &OrphanStatusData) {
    if let Ok(mut cache) = ORPHANS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *cache = Some(data.clone());
    }
}

pub async fn get_orphans<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<OrphanStatusData>> {
    let chain_handle = state.chain();
    let chain = match read_chain_for_rpc(&chain_handle, "/orphans").await {
        Ok(chain) => chain,
        Err(e) => {
            if let Some(data) = cached_orphans_response(e.clone()) {
                return Json(ApiResponse::ok(data));
            }
            return Json(ApiResponse::err("STATE_LOCK_BUSY", e));
        }
    };
    let mut orphans = chain.orphan_blocks.keys().cloned().collect::<Vec<_>>();
    orphans.sort();
    let orphans = orphans
        .into_iter()
        .map(|hash| OrphanEntry {
            missing_parents: chain
                .orphan_missing_parents
                .get(&hash)
                .cloned()
                .unwrap_or_default(),
            received_at_ms: chain.orphan_received_at_ms.get(&hash).copied(),
            hash,
        })
        .collect::<Vec<_>>();
    let data = OrphanStatusData {
        rpc_response_degraded: false,
        rpc_response_stale: false,
        rpc_response_degraded_reason: None,
        orphan_count: orphans.len(),
        orphans,
    };
    cache_orphans_response(&data);
    Json(ApiResponse::ok(data))
}
