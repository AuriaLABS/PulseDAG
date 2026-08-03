use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};

// Keep the established submit implementation isolated while the public handler
// normalizes the post-enqueue timeout contract. The compatibility module below
// preserves the original sibling-module path used by the implementation.
mod mining_template {
    pub(super) use crate::handlers::mining_template::*;
}

#[path = "mining_submit_legacy.rs"]
mod legacy;

pub use legacy::MiningSubmitData;

const FINALITY_UNKNOWN_CODE: &str = "submit_finality_unknown";
const LEGACY_POST_ENQUEUE_TIMEOUT_CODE: &str = "submit_timeout";

fn normalize_post_enqueue_timeout(data: &mut MiningSubmitData) -> bool {
    if data.reason_code != LEGACY_POST_ENQUEUE_TIMEOUT_CODE {
        return false;
    }

    let block_hash = data.block_hash.as_deref().unwrap_or("unknown");
    let height = data
        .height
        .map(|height| height.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let detail = format!(
        "submit finality is unknown because the serialized submit actor did not return before the RPC deadline; block_hash={block_hash} height={height}; reconcile the block hash before classifying this submit as accepted or rejected"
    );

    data.reason_code = FINALITY_UNKNOWN_CODE.to_string();
    data.reason = detail.clone();
    data.pow_rejection_reason = Some(detail);
    data.invalid_pow = false;
    data.stale = false;
    data.duplicate = false;
    data.stale_template = false;
    true
}

async fn record_finality_unknown<S: RpcStateLike>(state: &S, data: &MiningSubmitData) {
    let detail = data
        .pow_rejection_reason
        .clone()
        .unwrap_or_else(|| "submit finality unknown after actor response timeout".to_string());
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_last_submit_phase = Some("finality_unknown".to_string());
    runtime.external_mining_last_rejection_kind = Some(FINALITY_UNKNOWN_CODE.to_string());
    runtime.external_mining_last_rejection_reason = Some(detail.clone());

    // The legacy implementation records the response deadline as a rejected
    // block reason. Undo only that taxonomy entry: the actor may still accept
    // the block and the miner must reconcile by hash.
    let mut remove_legacy_reason = false;
    if let Some(count) = runtime
        .rejected_blocks_by_reason
        .get_mut(LEGACY_POST_ENQUEUE_TIMEOUT_CODE)
    {
        *count = count.saturating_sub(1);
        remove_legacy_reason = *count == 0;
    }
    if remove_legacy_reason {
        runtime
            .rejected_blocks_by_reason
            .remove(LEGACY_POST_ENQUEUE_TIMEOUT_CODE);
    }
    drop(runtime);

    let _ = state.storage().append_runtime_event(
        "warn",
        "external_mining_submit_finality_unknown",
        &format!(
            "block_hash={} height={} {}",
            data.block_hash.as_deref().unwrap_or("-"),
            data.height
                .map(|height| height.to_string())
                .unwrap_or_else(|| "-".to_string()),
            detail
        ),
    );
}

async fn record_parent_context_unavailable<S: RpcStateLike>(state: &S) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.parent_state_context_unavailable_total = runtime
        .parent_state_context_unavailable_total
        .saturating_add(1);
}

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<MiningSubmitData>> {
    let state_for_observability = state.clone();
    let mut response = legacy::post_mining_submit(State(state), Json(req)).await;

    if let Some(data) = response.0.data.as_mut() {
        if normalize_post_enqueue_timeout(data) {
            record_finality_unknown(&state_for_observability, data).await;
        } else if data.reason_code == "parent_state_context_unavailable" {
            record_parent_context_unavailable(&state_for_observability).await;
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::{normalize_post_enqueue_timeout, MiningSubmitData, FINALITY_UNKNOWN_CODE};

    fn submit_data(reason_code: &str) -> MiningSubmitData {
        MiningSubmitData {
            accepted: false,
            reason: "legacy reason".to_string(),
            block_hash: Some("block-123".to_string()),
            block_id: None,
            height: Some(42),
            pow_algorithm: "kheavyhash".to_string(),
            pow_accepted: false,
            pow_accepted_dev: false,
            protocol_version: 1,
            target_u64: 0,
            target_hex: format!("{:064x}", 0u64),
            pow_hash: None,
            template_id: None,
            invalid_pow: false,
            stale: false,
            duplicate: false,
            stale_template: false,
            reason_code: reason_code.to_string(),
            selected_tip: None,
            adopted_orphans: 0,
            pow_hash_score_u64: 0,
            pow_rejection_code: None,
            pow_rejection_reason: Some("legacy reason".to_string()),
        }
    }

    #[test]
    fn post_enqueue_timeout_is_explicitly_non_final() {
        let mut data = submit_data("submit_timeout");
        assert!(normalize_post_enqueue_timeout(&mut data));
        assert!(!data.accepted);
        assert_eq!(data.reason_code, FINALITY_UNKNOWN_CODE);
        assert!(data.reason.contains("reconcile the block hash"));
        assert!(data.reason.contains("block-123"));
        assert!(data.reason.contains("42"));
    }

    #[test]
    fn definitive_pre_acceptance_timeout_is_not_reclassified() {
        let mut data = submit_data("submit_timeout_before_acceptance");
        assert!(!normalize_post_enqueue_timeout(&mut data));
        assert_eq!(data.reason_code, "submit_timeout_before_acceptance");
    }

    #[test]
    fn ordinary_rejection_is_not_reclassified() {
        let mut data = submit_data("invalid_pow");
        assert!(!normalize_post_enqueue_timeout(&mut data));
        assert_eq!(data.reason_code, "invalid_pow");
    }
}
