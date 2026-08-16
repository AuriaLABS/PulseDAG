use crate::api::{ApiResponse, RpcStateLike, WalletSignRequest, WalletTransferRequest};
use axum::{extract::State, Json};
use ed25519_dalek::SigningKey;
use pulsedag_core::{
    compute_txid, compute_txid_v2, signing_message, signing_message_v2, types::Transaction,
};
use pulsedag_crypto::{generate_keypair, sign_message};
use pulsedag_wallet::build_transaction;

#[derive(Debug, serde::Serialize)]
pub struct NewWalletData {
    pub address: String,
    pub public_key: String,
    pub private_key: String,
}
#[derive(Debug, serde::Serialize)]
pub struct WalletSignData {
    pub signature: String,
}
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
    pub transaction: pulsedag_core::types::Transaction,
}

pub async fn post_wallet_new<S: RpcStateLike>(
    State(_state): State<S>,
) -> Json<ApiResponse<NewWalletData>> {
    let (private_key, public_key, address) = generate_keypair();
    Json(ApiResponse::ok(NewWalletData {
        address,
        public_key,
        private_key,
    }))
}

pub async fn post_wallet_sign<S: RpcStateLike>(
    State(_state): State<S>,
    Json(req): Json<WalletSignRequest>,
) -> Json<ApiResponse<WalletSignData>> {
    let msg_bytes = hex::decode(&req.message).unwrap_or_else(|_| req.message.as_bytes().to_vec());
    match sign_message(&req.private_key, &msg_bytes) {
        Ok(signature) => Json(ApiResponse::ok(WalletSignData { signature })),
        Err(e) => Json(ApiResponse::err("SIGN_ERROR", e.to_string())),
    }
}

fn attach_input_public_key(tx: &mut Transaction, private_key: &str) -> Result<(), String> {
    let private_key_bytes = hex::decode(private_key).map_err(|e| e.to_string())?;
    let arr: [u8; 32] = private_key_bytes
        .try_into()
        .map_err(|_| "invalid private key length".to_string())?;
    let public_key = hex::encode(SigningKey::from_bytes(&arr).verifying_key().to_bytes());
    for input in &mut tx.inputs {
        input.public_key = public_key.clone();
    }
    Ok(())
}

fn sign_transaction_inputs(mut tx: Transaction, private_key: &str) -> Result<Transaction, String> {
    // The public key is part of the canonical unsigned transaction. Attach it
    // before deriving the signing message so validators reconstruct the same bytes.
    attach_input_public_key(&mut tx, private_key)?;
    let signature = sign_message(private_key, &signing_message(&tx)).map_err(|e| e.to_string())?;
    for input in &mut tx.inputs {
        input.signature = signature.clone();
    }
    tx.txid = compute_txid(&tx);
    Ok(tx)
}

/// Explicit v2 signing helper for the frozen chain-bound transaction domain.
///
/// This is intentionally not wired into `post_wallet_transfer` yet. Runtime
/// selection of the v2 path remains an activation-gated follow-up so current
/// legacy callers cannot switch protocol behavior implicitly.
pub fn sign_transaction_inputs_v2(
    mut tx: Transaction,
    private_key: &str,
    chain_id: &str,
) -> Result<Transaction, String> {
    attach_input_public_key(&mut tx, private_key)?;
    let message = signing_message_v2(&tx, chain_id).map_err(|e| e.to_string())?;
    let signature = sign_message(private_key, &message).map_err(|e| e.to_string())?;
    for input in &mut tx.inputs {
        input.signature = signature.clone();
    }
    tx.txid = compute_txid_v2(&tx, chain_id).map_err(|e| e.to_string())?;
    Ok(tx)
}

pub async fn post_wallet_transfer<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<WalletTransferRequest>,
) -> Json<ApiResponse<WalletTransferData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let available = chain
        .utxo
        .address_index
        .get(&req.from)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|op| chain.utxo.utxos.get(&op).cloned())
        .collect::<Vec<_>>();
    let built = match build_transaction(&req.from, &req.to, req.amount, req.fee, &available, 1) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("BUILD_ERROR", e.to_string())),
    };
    let tx = match sign_transaction_inputs(built.transaction.clone(), &req.private_key) {
        Ok(tx) => tx,
        Err(e) => return Json(ApiResponse::err("SIGN_ERROR", e)),
    };
    match pulsedag_core::accept_transaction(
        tx.clone(),
        &mut chain,
        pulsedag_core::AcceptSource::Rpc,
    ) {
        Ok(_) => {
            let mempool_size = chain.mempool.transactions.len();
            let snapshot = chain.clone();
            drop(chain);
            if let Err(e) = state.storage().persist_chain_state(&snapshot) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Some(p2p) = state.p2p() {
                let _ = p2p.broadcast_transaction(&tx);
            }
            Json(ApiResponse::ok(WalletTransferData {
                accepted: true,
                txid: tx.txid.clone(),
                from: req.from,
                to: req.to,
                amount: req.amount,
                fee: req.fee,
                total_input: built.total_input,
                change: built.change,
                mempool_size,
                transaction: tx,
            }))
        }
        Err(e) => Json(ApiResponse::err("TX_REJECTED", e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use pulsedag_core::types::{OutPoint, TxInput, TxOutput};

    fn unsigned_transaction(version: u32) -> Transaction {
        Transaction {
            txid: String::new(),
            version,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "funding-tx".to_string(),
                    index: 0,
                },
                public_key: String::new(),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".to_string(),
                amount: 1,
            }],
            fee: 1,
            nonce: 1,
        }
    }

    #[test]
    fn wallet_transfer_mempool_signs_after_attaching_public_keys() {
        let (private_key, public_key, _address) = generate_keypair();
        let signed = sign_transaction_inputs(unsigned_transaction(1), &private_key)
            .expect("transaction signs");
        assert_eq!(signed.inputs[0].public_key, public_key);
        assert_eq!(signed.txid, compute_txid(&signed));

        let public_key_bytes: [u8; 32] = hex::decode(&signed.inputs[0].public_key)
            .expect("public key hex")
            .try_into()
            .expect("public key length");
        let signature_bytes: [u8; 64] = hex::decode(&signed.inputs[0].signature)
            .expect("signature hex")
            .try_into()
            .expect("signature length");
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).expect("verifying key");
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(&signing_message(&signed), &signature)
            .expect("signature matches canonical unsigned transaction");
    }

    #[test]
    fn v2_rpc_signing_is_chain_bound_and_verifiable() {
        let (private_key, public_key, _address) = generate_keypair();
        let unsigned = unsigned_transaction(2);
        let testnet =
            sign_transaction_inputs_v2(unsigned.clone(), &private_key, "pulsedag-testnet")
                .expect("testnet v2 transaction signs");
        let private = sign_transaction_inputs_v2(unsigned, &private_key, "pulsedag-private")
            .expect("private v2 transaction signs");

        assert_eq!(testnet.inputs[0].public_key, public_key);
        assert_ne!(testnet.inputs[0].signature, private.inputs[0].signature);
        assert_ne!(testnet.txid, private.txid);
        assert_eq!(
            testnet.txid,
            compute_txid_v2(&testnet, "pulsedag-testnet").unwrap()
        );

        let public_key_bytes: [u8; 32] = hex::decode(&testnet.inputs[0].public_key)
            .expect("public key hex")
            .try_into()
            .expect("public key length");
        let signature_bytes: [u8; 64] = hex::decode(&testnet.inputs[0].signature)
            .expect("signature hex")
            .try_into()
            .expect("signature length");
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).expect("verifying key");
        let signature = Signature::from_bytes(&signature_bytes);
        let message = signing_message_v2(&testnet, "pulsedag-testnet").unwrap();
        verifying_key
            .verify(&message, &signature)
            .expect("signature matches chain-bound v2 unsigned transaction");
    }

    #[test]
    fn v2_rpc_signing_rejects_empty_chain_id() {
        let (private_key, _public_key, _address) = generate_keypair();
        assert!(sign_transaction_inputs_v2(unsigned_transaction(2), &private_key, "").is_err());
    }
}
