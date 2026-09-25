use axum::{
    extract::{Path, State},
    Json,
};

use pulsedag_core::{
    assemble_explorer_block_v1, validate_explorer_block_v1, ExplorerDagAdmissionV1,
    ExplorerDagV1Error, ExplorerMergeColorV1, PulseObservationV1, EXPLORER_DAG_DOMAIN_V1,
    EXPLORER_DAG_VERSION_V1,
};

use crate::api::{ApiResponse, RpcStateLike};

/// Official explorer DAG view stays off the Task31 candidate.
pub const EXPLORER_RPC_ADMISSION_V1: ExplorerDagAdmissionV1 = ExplorerDagAdmissionV1::INACTIVE;

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct ExplorerPulseV1Data {
    pub pulse_height: u64,
    pub pulse_time: i64,
    pub uncertainty_secs: u32,
    pub finality_lag: u64,
}

impl From<&PulseObservationV1> for ExplorerPulseV1Data {
    fn from(pulse: &PulseObservationV1) -> Self {
        Self {
            pulse_height: pulse.pulse_height,
            pulse_time: pulse.pulse_time,
            uncertainty_secs: pulse.uncertainty_secs,
            finality_lag: pulse.finality_lag,
        }
    }
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct ExplorerBlockV1Data {
    pub domain: String,
    pub view_version: u32,
    pub hash: String,
    pub parents: Vec<String>,
    pub selected_parent: Option<String>,
    pub merge_color: String,
    pub blue_score: u64,
    pub pulse: Option<ExplorerPulseV1Data>,
}

fn color_name(color: ExplorerMergeColorV1) -> &'static str {
    match color {
        ExplorerMergeColorV1::Selected => "selected",
        ExplorerMergeColorV1::Blue => "blue",
        ExplorerMergeColorV1::Red => "red",
    }
}

fn explorer_error(err: ExplorerDagV1Error) -> (String, String) {
    let code = match &err {
        ExplorerDagV1Error::SurfaceDisabled => "explorer_surface_disabled",
        ExplorerDagV1Error::EmptyHash => "explorer_empty_hash",
        ExplorerDagV1Error::LinearOnly => "explorer_linear_only",
        ExplorerDagV1Error::MissingSelectedParent => "explorer_missing_selected_parent",
        ExplorerDagV1Error::EmptyParentHash => "explorer_empty_parent",
        ExplorerDagV1Error::MissingBlock { .. } => "explorer_block_missing",
    };
    (code.into(), err.to_string())
}

/// DAG explorer row. Default admission rejects so Task31 does not publish
/// this surface. Existing `/blocks/:hash` is unchanged.
pub async fn get_explorer_block<S: RpcStateLike>(
    State(state): State<S>,
    Path(hash): Path<String>,
) -> Json<ApiResponse<ExplorerBlockV1Data>> {
    if !EXPLORER_RPC_ADMISSION_V1.admitted() {
        let (code, message) = explorer_error(ExplorerDagV1Error::SurfaceDisabled);
        return Json(ApiResponse::err(code, message));
    }
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match assemble_explorer_block_v1(&chain, &hash) {
        Ok(block) => match validate_explorer_block_v1(EXPLORER_RPC_ADMISSION_V1, &block) {
            Ok(()) => Json(ApiResponse::ok(ExplorerBlockV1Data {
                domain: EXPLORER_DAG_DOMAIN_V1.to_string(),
                view_version: EXPLORER_DAG_VERSION_V1,
                hash: block.hash,
                parents: block.parents,
                selected_parent: block.selected_parent,
                merge_color: color_name(block.merge_color).to_string(),
                blue_score: block.blue_score,
                pulse: block.pulse.as_ref().map(ExplorerPulseV1Data::from),
            })),
            Err(err) => {
                let (code, message) = explorer_error(err);
                Json(ApiResponse::err(code, message))
            }
        },
        Err(err) => {
            let (code, message) = explorer_error(err);
            Json(ApiResponse::err(code, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task31_rpc_does_not_admit_explorer_view() {
        assert!(!EXPLORER_RPC_ADMISSION_V1.admitted());
    }
}
