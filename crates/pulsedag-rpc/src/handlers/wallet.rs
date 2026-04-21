use axum::{extract::State, Json};
use ed25519_dalek::SigningKey;
use pulsedag_crypto::{generate_keypair, sign_message};
use pulsedag_core::compute_txid;
use pulsedag_wallet::build_transaction;
use crate::{api::{ApiResponse, RpcStateLike, WalletSignRequest, WalletTransferRequest}};

#[derive(Debug, serde::Serialize)]
pub struct NewWalletData { pub address: String, pub public_key: String, pub private_key: String }
#[derive(Debug, serde::Serialize)]
pub struct WalletSignData { pub signature: String }
#[derive(Debug, serde::Serialize)]
pub struct WalletTransferData {
    pub accepted: bool,
    pub txid: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub total_input: u64,
    pub change: u64,
    pub mempool_size: usize,
}

pub async fn post_wallet_new<S: RpcStateLike>(State(_state): State<S>) -> Json<ApiResponse<NewWalletData>> {
    let (private_key, public_key, address) = generate_keypair();
    Json(ApiResponse::ok(NewWalletData { address, public_key, private_key }))
}

pub async fn post_wallet_sign<S: RpcStateLike>(State(_state): State<S>, Json(req): Json<WalletSignRequest>) -> Json<ApiResponse<WalletSignData>> {
    let msg_bytes = hex::decode(&req.message).unwrap_or_else(|_| req.message.as_bytes().to_vec());
    match sign_message(&req.private_key, &msg_bytes) {
        Ok(signature) => Json(ApiResponse::ok(WalletSignData { signature })),
        Err(e) => Json(ApiResponse::err("SIGN_ERROR", e.to_string())),
    }
}

pub async fn post_wallet_transfer<S: RpcStateLike>(State(state): State<S>, Json(req): Json<WalletTransferRequest>) -> Json<ApiResponse<WalletTransferData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let available = chain.utxo.address_index.get(&req.from).cloned().unwrap_or_default().into_iter().filter_map(|op| chain.utxo.utxos.get(&op).cloned()).collect::<Vec<_>>();
    let built = match build_transaction(&req.from, &req.to, req.amount, req.fee, &available, 1) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("BUILD_ERROR", e.to_string())),
    };
    let message_bytes = match hex::decode(&built.signing_message) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("BUILD_ERROR", e.to_string())),
    };
    let signature = match sign_message(&req.private_key, &message_bytes) {
        Ok(sig) => sig,
        Err(e) => return Json(ApiResponse::err("SIGN_ERROR", e.to_string())),
    };
    let private_key_bytes = match hex::decode(&req.private_key) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("SIGN_ERROR", e.to_string())),
    };
    let arr: [u8; 32] = match private_key_bytes.try_into() {
        Ok(v) => v,
        Err(_) => return Json(ApiResponse::err("SIGN_ERROR", "invalid private key length")),
    };
    let public_key = hex::encode(SigningKey::from_bytes(&arr).verifying_key().to_bytes());
    let mut tx = built.transaction.clone();
    for input in &mut tx.inputs {
        input.public_key = public_key.clone();
        input.signature = signature.clone();
    }
    tx.txid = compute_txid(&tx);
    match pulsedag_core::accept_transaction(tx.clone(), &mut chain, pulsedag_core::AcceptSource::Rpc) {
        Ok(_) => {
            let mempool_size = chain.mempool.transactions.len();
            let snapshot = chain.clone();
            drop(chain);
            if let Err(e) = state.storage().persist_chain_state(&snapshot) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Some(p2p) = state.p2p() { let _ = p2p.broadcast_transaction(&tx); }
            Json(ApiResponse::ok(WalletTransferData {
                accepted: true,
                txid: tx.txid,
                from: req.from,
                to: req.to,
                amount: req.amount,
                fee: req.fee,
                total_input: built.total_input,
                change: built.change,
                mempool_size,
            }))
        }
        Err(e) => Json(ApiResponse::err("TX_REJECTED", e.to_string())),
    }
}
