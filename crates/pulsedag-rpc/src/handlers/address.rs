use axum::{extract::{Path, State}, Json};
use pulsedag_core::types::Utxo;
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct AddressOutpointData {
    pub txid: String,
    pub index: u32,
    pub amount: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct AddressData {
    pub address: String,
    pub balance: u64,
    pub utxo_count: usize,
    pub largest_utxo: u64,
    pub outpoints: Vec<AddressOutpointData>,
}
#[derive(Debug, serde::Serialize)]
pub struct AddressUtxosData { pub address: String, pub utxos: Vec<Utxo> }
#[derive(Debug, serde::Serialize)]
pub struct UtxoListData { pub count: usize, pub utxos: Vec<Utxo> }

pub async fn get_address<S: RpcStateLike>(State(state): State<S>, Path(address): Path<String>) -> Json<ApiResponse<AddressData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = chain.utxo.address_index.get(&address).cloned().unwrap_or_default().into_iter().filter_map(|op| chain.utxo.utxos.get(&op).cloned()).collect::<Vec<_>>();
    let balance = utxos.iter().map(|u| u.amount).sum();
    let largest_utxo = utxos.iter().map(|u| u.amount).max().unwrap_or(0);
    let outpoints = utxos.iter().map(|u| AddressOutpointData {
        txid: u.outpoint.txid.clone(),
        index: u.outpoint.index,
        amount: u.amount,
    }).collect::<Vec<_>>();
    Json(ApiResponse::ok(AddressData { address, balance, utxo_count: utxos.len(), largest_utxo, outpoints }))
}

pub async fn get_address_utxos<S: RpcStateLike>(State(state): State<S>, Path(address): Path<String>) -> Json<ApiResponse<AddressUtxosData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = chain.utxo.address_index.get(&address).cloned().unwrap_or_default().into_iter().filter_map(|op| chain.utxo.utxos.get(&op).cloned()).collect::<Vec<_>>();
    Json(ApiResponse::ok(AddressUtxosData { address, utxos }))
}

pub async fn get_utxos<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<UtxoListData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = chain.utxo.utxos.values().cloned().collect::<Vec<_>>();
    Json(ApiResponse::ok(UtxoListData { count: utxos.len(), utxos }))
}
