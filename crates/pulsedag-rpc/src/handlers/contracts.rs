use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct ContractsStatusData {
    pub prepared: bool,
    pub enabled: bool,
    pub vm_version: String,
    pub execution_mode: String,
    pub contract_count: u64,
    pub storage_slots: u64,
    pub receipt_count: u64,
    pub max_gas_per_tx: u64,
    pub max_contract_size_bytes: u64,
    pub max_storage_key_bytes: u32,
    pub max_storage_value_bytes: u32,
}

pub async fn get_contracts_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<ContractsStatusData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let cfg = &chain.contracts.config;

    Json(ApiResponse::ok(ContractsStatusData {
        prepared: state.storage().contract_namespaces_ready(),
        enabled: cfg.enabled,
        vm_version: cfg.vm_version.clone(),
        execution_mode: if cfg.enabled {
            "enabled".into()
        } else {
            "disabled-prepared".into()
        },
        contract_count: chain.contracts.contract_count,
        storage_slots: chain.contracts.storage_slots,
        receipt_count: chain.contracts.receipt_count,
        max_gas_per_tx: cfg.max_gas_per_tx,
        max_contract_size_bytes: cfg.max_contract_size_bytes,
        max_storage_key_bytes: cfg.max_storage_key_bytes,
        max_storage_value_bytes: cfg.max_storage_value_bytes,
    }))
}
