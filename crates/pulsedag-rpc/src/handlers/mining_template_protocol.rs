use crate::api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_core::ProtocolActivationIdentity;

pub use super::mining_template_legacy::{MiningTemplateData, StoredMiningTemplate};
pub(crate) use super::mining_template_legacy::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    // The legacy template builder still emits v1 work in this slice. Bind that
    // work to the exact legacy chain identity so submit cannot later reinterpret
    // the same template under an activated-v2 identity.
    let legacy_identity = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        ProtocolActivationIdentity::legacy_from_state(&chain)
    };
    let fingerprint = match legacy_identity.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot bind mining template to protocol identity: {error}"),
            ));
        }
    };

    let response = super::mining_template_legacy::post_mining_template(State(state), Json(req)).await;
    if let Some(data) = response.0.data.as_ref() {
        if let Err(error) = super::mining_submit::bind_template_protocol(
            data.template_id.clone(),
            legacy_identity,
            fingerprint,
        ) {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot store mining template protocol binding: {error}"),
            ));
        }
    }
    response
}
