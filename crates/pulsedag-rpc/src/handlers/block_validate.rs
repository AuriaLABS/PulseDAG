use crate::api::{ApiResponse, RpcStateLike, SubmitBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::pow_validation_result;

#[derive(Debug, serde::Serialize)]
pub struct BlockValidateData {
    pub valid: bool,
    pub block_hash: String,
    pub height: u64,
    pub parent_count: usize,
    pub reason: Option<String>,
    pub pow_hash: String,
    pub pow_target_u64: u64,
    pub pow_accepted_dev: bool,
}

pub async fn post_block_validate<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitBlockRequest>,
) -> Json<ApiResponse<BlockValidateData>> {
    let monetary_activation = match state.storage().protocol_monetary_activation_record() {
        Ok(record) => record,
        Err(error) => {
            return Json(ApiResponse::err(
                "MONETARY_POLICY_ERROR",
                format!("cannot verify v3 monetary activation sidecar: {error}"),
            ));
        }
    };

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut simulated = chain.clone();
    drop(chain);

    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;
    let parent_count = req.block.header.parents.len();
    let pow = pow_validation_result(&req.block.header);
    let pow_hash = pow.hash_hex.unwrap_or_default();
    let pow_target_u64 = pow.target_u64;
    let pow_accepted_dev = pow.accepted;

    let validation = if let Some(record) = monetary_activation.as_ref() {
        if record.identity.chain_id != simulated.chain_id
            || record.identity.genesis_hash != simulated.dag.genesis_hash
        {
            Err(pulsedag_core::PulseError::InvalidBlock(
                "v3 monetary activation identity does not match validation state".to_string(),
            ))
        } else if record.reward_finality_policy_version
            != pulsedag_core::GHOSTDAG_V1_FINALITY_POLICY_VERSION
        {
            Err(pulsedag_core::PulseError::InvalidBlock(format!(
                "unsupported v3 reward-finality policy {}; implemented live policy is {}",
                record.reward_finality_policy_version,
                pulsedag_core::GHOSTDAG_V1_FINALITY_POLICY_VERSION
            )))
        } else {
            pulsedag_core::prepare_monetary_v3_p2p_block_state(
                &req.block,
                &simulated,
                &record.identity,
                &record.monetary_cadence_segments,
            )
            .map(|_| ())
        }
    } else {
        pulsedag_core::accept_block(
            req.block.clone(),
            &mut simulated,
            pulsedag_core::AcceptSource::Rpc,
        )
        .map(|_| ())
    };

    match validation {
        Ok(()) => Json(ApiResponse::ok(BlockValidateData {
            valid: true,
            block_hash,
            height,
            parent_count,
            reason: None,
            pow_hash,
            pow_target_u64,
            pow_accepted_dev,
        })),
        Err(e) => Json(ApiResponse::ok(BlockValidateData {
            valid: false,
            block_hash,
            height,
            parent_count,
            reason: Some(e.to_string()),
            pow_hash,
            pow_target_u64,
            pow_accepted_dev,
        })),
    }
}
