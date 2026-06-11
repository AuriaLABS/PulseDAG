use axum::{extract::State, Json};
use std::{
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::fresh_or_cached_node_rpc_snapshot, api::read_chain_for_rpc, api::ApiResponse,
    api::NodeRpcSnapshot, api::RpcStateLike,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanEntry {
    pub hash: String,
    pub missing_parents: Vec<String>,
    pub received_at_ms: Option<u64>,
    pub age_secs: Option<u64>,
    pub status: String,
    pub actionable: bool,
    pub stale: bool,
    pub waiting: bool,
    pub evicted: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanStatusData {
    pub rpc_response_degraded: bool,
    pub rpc_response_stale: bool,
    pub rpc_response_degraded_reason: Option<String>,
    pub orphan_count: usize,
    pub saturated: bool,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn orphan_entry_status(
    missing_parent_count: usize,
    received_at_ms: Option<u64>,
    now_ms: u64,
) -> (Option<u64>, String, bool, bool, bool, bool) {
    let age_secs = received_at_ms.map(|received| now_ms.saturating_sub(received) / 1_000);
    let stale = received_at_ms
        .map(|received| now_ms.saturating_sub(received) >= pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS)
        .unwrap_or(false);
    let waiting = missing_parent_count > 0;
    let actionable = !waiting && !stale;
    let status = if stale {
        "stale"
    } else if actionable {
        "actionable"
    } else {
        "waiting"
    }
    .to_string();
    (age_secs, status, actionable, stale, waiting, false)
}

fn orphans_from_rpc_snapshot(snapshot: NodeRpcSnapshot) -> OrphanStatusData {
    OrphanStatusData {
        rpc_response_degraded: true,
        rpc_response_stale: true,
        rpc_response_degraded_reason: snapshot.degraded_reason,
        orphan_count: snapshot.orphan_count,
        saturated: snapshot.orphan_count >= pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
        orphans: Vec::new(),
    }
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
    let liveness_snapshot = fresh_or_cached_node_rpc_snapshot(&state, "/orphans").await;
    if liveness_snapshot.degraded || liveness_snapshot.stale {
        return Json(ApiResponse::ok(orphans_from_rpc_snapshot(
            liveness_snapshot,
        )));
    }
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
    let now = now_ms();
    let orphans = orphans
        .into_iter()
        .map(|hash| {
            let missing_parents = chain
                .orphan_missing_parents
                .get(&hash)
                .cloned()
                .unwrap_or_default();
            let received_at_ms = chain.orphan_received_at_ms.get(&hash).copied();
            let (age_secs, status, actionable, stale, waiting, evicted) =
                orphan_entry_status(missing_parents.len(), received_at_ms, now);
            OrphanEntry {
                missing_parents,
                received_at_ms,
                age_secs,
                status,
                actionable,
                stale,
                waiting,
                evicted,
                hash,
            }
        })
        .collect::<Vec<_>>();
    let data = OrphanStatusData {
        rpc_response_degraded: false,
        rpc_response_stale: false,
        rpc_response_degraded_reason: None,
        orphan_count: orphans.len(),
        saturated: orphans.len() >= pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
        orphans,
    };
    cache_orphans_response(&data);
    Json(ApiResponse::ok(data))
}
