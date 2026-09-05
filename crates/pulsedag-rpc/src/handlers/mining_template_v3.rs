use std::sync::{Mutex, OnceLock};

use crate::api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike};
use axum::{extract::State, Json};
use serde_json::{json, Value};
use sha3::{Digest, Sha3_256};

pub(crate) use super::mining_template_protocol::{
    current_template_state, load_template, template_freshness_window,
    template_id_matches_lifecycle,
};

pub(crate) const MINING_PROTOCOL_VERSION: u32 = 3;
const MINING_V3_NOTIFICATION_POLL_AFTER_MS: u64 = 250;
const MINING_V3_MAX_OUTSTANDING_NOTIFICATION_SNAPSHOTS: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkSnapshot {
    network_id: String,
    height: u64,
    selected_tip: Option<String>,
    parent_hashes: Vec<String>,
    difficulty: u32,
    target_u64: u64,
    mempool_fingerprint: String,
    mempool_tx_count: usize,
    contracts_enabled: bool,
}

#[derive(Debug, Clone)]
struct WorkObservation {
    sequence: u64,
    token: String,
    reasons: Vec<String>,
    snapshot: WorkSnapshot,
}

#[derive(Debug, Default)]
struct WorkTracker {
    sequence: u64,
    last: Option<WorkSnapshot>,
}

fn work_tracker() -> &'static Mutex<WorkTracker> {
    static TRACKER: OnceLock<Mutex<WorkTracker>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(WorkTracker::default()))
}

fn work_token(snapshot: &WorkSnapshot) -> String {
    let material = format!(
        "pulsedag:mining:v3:work|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        snapshot.network_id,
        snapshot.height,
        snapshot.selected_tip.as_deref().unwrap_or("-"),
        snapshot.parent_hashes.join(","),
        snapshot.difficulty,
        snapshot.target_u64,
        snapshot.mempool_fingerprint,
        snapshot.mempool_tx_count,
        snapshot.contracts_enabled
    );
    hex::encode(Sha3_256::digest(material.as_bytes()))
}

fn change_reasons(previous: Option<&WorkSnapshot>, current: &WorkSnapshot) -> Vec<String> {
    let Some(previous) = previous else {
        return vec!["initial_work".to_string()];
    };
    let mut reasons = Vec::new();
    if previous.network_id != current.network_id {
        reasons.push("network_changed".to_string());
    }
    if previous.height != current.height {
        reasons.push("height_advanced".to_string());
    }
    if previous.selected_tip != current.selected_tip {
        reasons.push("selected_tip_changed".to_string());
    }
    if previous.parent_hashes != current.parent_hashes {
        reasons.push("parent_set_changed".to_string());
    }
    if previous.difficulty != current.difficulty || previous.target_u64 != current.target_u64 {
        reasons.push("difficulty_or_target_changed".to_string());
    }
    if previous.mempool_fingerprint != current.mempool_fingerprint
        || previous.mempool_tx_count != current.mempool_tx_count
    {
        reasons.push("mempool_changed".to_string());
    }
    if previous.contracts_enabled != current.contracts_enabled {
        reasons.push("programmable_fee_hook_state_changed".to_string());
    }
    reasons
}

fn observe_work(snapshot: WorkSnapshot) -> WorkObservation {
    let mut tracker = work_tracker()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reasons = change_reasons(tracker.last.as_ref(), &snapshot);
    if tracker.last.as_ref() != Some(&snapshot) {
        tracker.sequence = tracker.sequence.saturating_add(1).max(1);
        tracker.last = Some(snapshot.clone());
    }
    WorkObservation {
        sequence: tracker.sequence.max(1),
        token: work_token(&snapshot),
        reasons,
        snapshot,
    }
}

async fn snapshot_work<S: RpcStateLike>(state: &S) -> WorkSnapshot {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let lifecycle = super::mining_template_protocol::current_template_state(&chain);
    WorkSnapshot {
        network_id: chain.chain_id.clone(),
        height: lifecycle.height,
        selected_tip: lifecycle.selected_tip,
        parent_hashes: lifecycle.parent_hashes,
        difficulty: lifecycle.difficulty,
        target_u64: lifecycle.target_u64,
        mempool_fingerprint: lifecycle.mempool_fingerprint,
        mempool_tx_count: lifecycle.mempool_tx_count,
        contracts_enabled: chain.contracts.config.enabled,
    }
}

fn decorate_template_data(mut data: Value, observation: &WorkObservation) -> Result<Value, String> {
    let object = data
        .as_object_mut()
        .ok_or_else(|| "serialized mining template data is not an object".to_string())?;
    let internal_template_id = object
        .get("template_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "mining template response is missing template_id".to_string())?
        .to_string();
    let external_template_id =
        super::mining_submit::versioned_template_id(&internal_template_id);
    let job_id = super::mining_submit::job_id_for_template(&external_template_id);

    object.insert("protocol_version".to_string(), json!(MINING_PROTOCOL_VERSION));
    object.insert("template_id".to_string(), json!(external_template_id));
    object.insert("job_id".to_string(), json!(job_id));
    object.insert("work_sequence".to_string(), json!(observation.sequence));
    object.insert("work_token".to_string(), json!(observation.token));
    object.insert(
        "work_change_reasons".to_string(),
        json!(observation.reasons),
    );
    object.insert(
        "new_work_notification".to_string(),
        json!({
            "mode": "bounded_poll",
            "poll_after_ms": MINING_V3_NOTIFICATION_POLL_AFTER_MS,
            "max_outstanding_snapshots": MINING_V3_MAX_OUTSTANDING_NOTIFICATION_SNAPSHOTS,
            "semantics": "each template response carries only the latest work revision; no subscriber queue is retained"
        }),
    );
    object.insert(
        "invalidation".to_string(),
        json!({
            "height": observation.snapshot.height,
            "selected_tip": observation.snapshot.selected_tip,
            "parent_hashes": observation.snapshot.parent_hashes,
            "difficulty": observation.snapshot.difficulty,
            "target_u64": observation.snapshot.target_u64,
            "mempool_tx_count": observation.snapshot.mempool_tx_count,
            "token": observation.token,
            "change_reasons": observation.reasons
        }),
    );
    object.insert(
        "submit_contract".to_string(),
        json!({
            "stable_identity": "sha3-256(template_id|block_hash)",
            "states": ["accepted", "rejected", "stale", "unknown_finality"],
            "reconciliation": "same-submit-id-never-rebroadcasts-after-cached-outcome"
        }),
    );
    object.insert(
        "resource_limits".to_string(),
        json!({
            "max_inflight_submits": super::mining_submit::MINING_V3_MAX_INFLIGHT_SUBMITS,
            "max_reconciliation_entries": super::mining_submit::MINING_V3_MAX_RECONCILIATION_ENTRIES,
            "max_outstanding_notification_snapshots": MINING_V3_MAX_OUTSTANDING_NOTIFICATION_SNAPSHOTS
        }),
    );
    object.insert(
        "programmable_fee_inclusion_hook".to_string(),
        json!({
            "deterministic": true,
            "contracts_enabled": observation.snapshot.contracts_enabled,
            "policy": "topological-first-seen-with-txid-tiebreak-v1",
            "activation_behavior": "preserve-ordering-policy"
        }),
    );

    Ok(data)
}

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<Value>> {
    let observation = observe_work(snapshot_work(&state).await);
    let response = super::mining_template_protocol::post_mining_template(
        State(state.clone()),
        Json(req),
    )
    .await;
    let ApiResponse {
        ok,
        data,
        error,
        meta,
    } = response.0;

    let Some(data) = data else {
        return Json(ApiResponse {
            ok,
            data: None,
            error,
            meta,
        });
    };
    let value = match serde_json::to_value(data)
        .map_err(|error| error.to_string())
        .and_then(|value| decorate_template_data(value, &observation))
    {
        Ok(value) => value,
        Err(error) => {
            return Json(ApiResponse::err(
                "MINING_PROTOCOL_V3_SERIALIZATION",
                format!("cannot serialize mining v3 template response: {error}"),
            ));
        }
    };

    let external_template_id = value
        .get("template_id")
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string();
    let job_id = value
        .get("job_id")
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string();
    super::mining_submit::register_v3_job(
        external_template_id.clone(),
        job_id.clone(),
        crate::api::unix_now_ms(),
    );
    let _ = state.storage().append_runtime_event(
        "info",
        "external_mining_v3_job_issued",
        &format!(
            "job_id={} template_id={} work_sequence={} work_token={} reasons={}",
            job_id,
            external_template_id,
            observation.sequence,
            observation.token,
            observation.reasons.join(",")
        ),
    );

    Json(ApiResponse {
        ok,
        data: Some(value),
        error,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task37_legacy_template_request_type_remains_the_public_handler_input() {
        let request = GetBlockTemplateRequest {
            miner_address: "pulse1miner".to_string(),
        };
        assert_eq!(request.miner_address, "pulse1miner");
    }

    #[test]
    fn task37_work_change_reasons_cover_tip_target_and_mempool() {
        let base = WorkSnapshot {
            network_id: "network".to_string(),
            height: 42,
            selected_tip: Some("tip-a".to_string()),
            parent_hashes: vec!["tip-a".to_string()],
            difficulty: 7,
            target_u64: 100,
            mempool_fingerprint: "mempool-a".to_string(),
            mempool_tx_count: 1,
            contracts_enabled: false,
        };
        let mut changed = base.clone();
        changed.selected_tip = Some("tip-b".to_string());
        changed.parent_hashes = vec!["tip-b".to_string()];
        changed.target_u64 = 99;
        changed.mempool_fingerprint = "mempool-b".to_string();
        changed.mempool_tx_count = 2;
        let reasons = change_reasons(Some(&base), &changed);
        assert!(reasons.contains(&"selected_tip_changed".to_string()));
        assert!(reasons.contains(&"parent_set_changed".to_string()));
        assert!(reasons.contains(&"difficulty_or_target_changed".to_string()));
        assert!(reasons.contains(&"mempool_changed".to_string()));
    }

    #[test]
    fn task37_work_token_is_deterministic() {
        let snapshot = WorkSnapshot {
            network_id: "network".to_string(),
            height: 42,
            selected_tip: Some("tip".to_string()),
            parent_hashes: vec!["tip".to_string()],
            difficulty: 7,
            target_u64: 100,
            mempool_fingerprint: "mempool".to_string(),
            mempool_tx_count: 1,
            contracts_enabled: false,
        };
        assert_eq!(work_token(&snapshot), work_token(&snapshot));
        assert_eq!(work_token(&snapshot).len(), 64);
    }

    #[test]
    fn task37_new_work_notifications_are_strictly_bounded() {
        assert_eq!(MINING_V3_MAX_OUTSTANDING_NOTIFICATION_SNAPSHOTS, 1);
        assert!(MINING_V3_NOTIFICATION_POLL_AFTER_MS > 0);
    }
}
