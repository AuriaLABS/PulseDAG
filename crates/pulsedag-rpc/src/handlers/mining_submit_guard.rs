use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{PowValidationPath, BLOCK_HEADER_VERSION_V1};

pub(crate) use super::mining_submit_protocol::bind_template_protocol;
pub use super::mining_submit_protocol::MiningSubmitData;

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

    data.accepted = false;
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

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<MiningSubmitData>> {
    if req.block.header.version != BLOCK_HEADER_VERSION_V1 {
        return super::mining_submit_protocol::post_mining_submit(State(state), Json(req)).await;
    }

    let local_identity = match super::mining_submit_protocol::rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot resolve mining submit protocol identity: {error}"),
            ));
        }
    };

    if let Some(identity) = local_identity.as_ref() {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        match super::mining_submit_protocol::mining_submit_protocol_path(
            &req.block,
            &chain,
            Some(identity),
        ) {
            Ok(PowValidationPath::LegacyV1) => {}
            Ok(_) => {
                return Json(ApiResponse::err(
                    "PROTOCOL_MISMATCH",
                    "legacy mining submit is disabled by the active protocol identity",
                ));
            }
            Err(error) => {
                return Json(ApiResponse::err(
                    "PROTOCOL_MISMATCH",
                    format!(
                        "legacy mining submit does not match the active protocol identity: {error}"
                    ),
                ));
            }
        }
    }

    let state_for_observability = state.clone();
    let mut response =
        super::mining_submit_legacy::post_mining_submit(State(state), Json(req)).await;
    if let Some(data) = response.0.data.as_mut() {
        if normalize_post_enqueue_timeout(data) {
            record_finality_unknown(&state_for_observability, data).await;
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
    };

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
            target_hex: format!("{:064x}", 0_u64),
            pow_hash: None,
            template_id: Some("template-1".to_string()),
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
    fn task29_post_enqueue_timeout_is_explicitly_non_final() {
        let mut data = submit_data("submit_timeout");
        assert!(normalize_post_enqueue_timeout(&mut data));
        assert!(!data.accepted);
        assert_eq!(data.reason_code, FINALITY_UNKNOWN_CODE);
        assert!(data.reason.contains("reconcile the block hash"));
        assert!(data.reason.contains("block-123"));
        assert!(data.reason.contains("42"));
        assert_eq!(
            data.pow_rejection_reason.as_deref(),
            Some(data.reason.as_str())
        );
    }

    #[test]
    fn task29_definitive_pre_acceptance_timeout_is_not_reclassified() {
        let mut data = submit_data("submit_timeout_before_acceptance");
        assert!(!normalize_post_enqueue_timeout(&mut data));
        assert_eq!(data.reason_code, "submit_timeout_before_acceptance");
    }

    #[test]
    fn task29_ordinary_rejection_is_not_reclassified() {
        let mut data = submit_data("invalid_pow");
        assert!(!normalize_post_enqueue_timeout(&mut data));
        assert_eq!(data.reason_code, "invalid_pow");
    }

    #[test]
    fn activated_identity_rejects_legacy_submit_preflight() {
        let state = init_chain_state("task28-rpc-mining-submit-v1-preflight".to_string());
        let block = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        let activated = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(
            super::super::mining_submit_protocol::mining_submit_protocol_path(
                &block,
                &state,
                Some(&activated),
            )
            .is_err()
        );
        assert_eq!(
            super::super::mining_submit_protocol::mining_submit_protocol_path(
                &block, &state, None,
            )
            .unwrap(),
            PowValidationPath::LegacyV1
        );
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert_eq!(
            super::super::mining_submit_protocol::mining_submit_protocol_path(
                &block,
                &state,
                Some(&legacy),
            )
            .unwrap(),
            PowValidationPath::LegacyV1
        );
    }
}
