use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{PowValidationPath, BLOCK_HEADER_VERSION_V1};

pub use super::mining_submit_protocol::MiningSubmitData;
pub(crate) use super::mining_submit_protocol::bind_template_protocol;

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
                    format!("legacy mining submit does not match the active protocol identity: {error}"),
                ));
            }
        }
    }

    super::mining_submit_legacy::post_mining_submit(State(state), Json(req)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
    };

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

        assert!(super::super::mining_submit_protocol::mining_submit_protocol_path(
            &block,
            &state,
            Some(&activated),
        )
        .is_err());
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
