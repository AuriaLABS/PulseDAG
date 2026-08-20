use crate::api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_core::{
    resolve_pow_validation_path, ChainState, PowValidationPath, ProtocolActivationIdentity,
    PulseError,
};

#[cfg(test)]
pub(crate) use super::mining_template_legacy::store_template;
pub(crate) use super::mining_template_legacy::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
pub use super::mining_template_legacy::{MiningTemplateData, StoredMiningTemplate};

fn legacy_template_identity_for_protocol(
    chain: &ChainState,
    local_identity: Option<&ProtocolActivationIdentity>,
) -> Result<ProtocolActivationIdentity, PulseError> {
    if let Some(identity) = local_identity {
        let path = resolve_pow_validation_path(identity, chain)?;
        if path != PowValidationPath::LegacyV1 {
            return Err(PulseError::InvalidBlock(
                "activated-v2 mining template emission is not enabled in this slice".to_string(),
            ));
        }
    }
    Ok(ProtocolActivationIdentity::legacy_from_state(chain))
}

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    // The legacy builder remains the only live template emitter in this slice.
    // If an explicit activated-v2 identity is configured, fail closed rather
    // than handing miners v1 work that the protocol-aware submit path rejects.
    let local_identity = match super::mining_submit_protocol::rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot resolve mining template protocol identity: {error}"),
            ));
        }
    };
    let legacy_identity = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        match legacy_template_identity_for_protocol(&chain, local_identity.as_ref()) {
            Ok(identity) => identity,
            Err(error) => {
                return Json(ApiResponse::err(
                    "PROTOCOL_MISMATCH",
                    format!("cannot emit legacy mining template: {error}"),
                ));
            }
        }
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

    let response =
        super::mining_template_legacy::post_mining_template(State(state), Json(req)).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{genesis::init_chain_state, GHOSTDAG_V1_ORDERING_VERSION};

    #[test]
    fn legacy_template_identity_allows_default_and_explicit_legacy() {
        let state = init_chain_state("task28-rpc-template-legacy".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(
            legacy_template_identity_for_protocol(&state, None).unwrap(),
            expected
        );
        assert_eq!(
            legacy_template_identity_for_protocol(&state, Some(&expected)).unwrap(),
            expected
        );
    }

    #[test]
    fn activated_identity_rejects_legacy_template_emission() {
        let state = init_chain_state("task28-rpc-template-v2".to_string());
        let activated = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(legacy_template_identity_for_protocol(&state, Some(&activated)).is_err());
    }
}
