use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    state::ChainState,
    types::{Address, OutPoint, Transaction, TxOutput},
};

fn referenced_output_address(state: &ChainState, outpoint: &OutPoint) -> Option<Address> {
    if let Some(utxo) = state.utxo.utxos.get(outpoint) {
        return Some(utxo.address.clone());
    }
    state
        .mempool
        .transactions
        .get(&outpoint.txid)
        .and_then(|tx| tx.outputs.get(outpoint.index as usize))
        .map(|output| output.address.clone())
}

pub fn address_from_public_key(public_key_hex: &str) -> Address {
    let mut hasher = Sha256::new();
    hasher.update(public_key_hex.as_bytes());
    let digest = hasher.finalize();
    format!("pulse1{}", hex::encode(&digest[..20]))
}

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

pub fn canonical_outpoint_bytes(outpoint: &OutPoint) -> Vec<u8> {
    let mut out = Vec::new();
    encode_len_prefixed_str(&mut out, &outpoint.txid);
    out.extend_from_slice(&outpoint.index.to_le_bytes());
    out
}

pub fn canonical_tx_input_bytes(input: &crate::types::TxInput) -> Vec<u8> {
    let mut out = canonical_outpoint_bytes(&input.previous_output);
    encode_len_prefixed_str(&mut out, &input.public_key);
    encode_len_prefixed_str(&mut out, &input.signature);
    out
}

pub fn canonical_tx_output_bytes(output: &TxOutput) -> Vec<u8> {
    let mut out = Vec::new();
    encode_len_prefixed_str(&mut out, &output.address);
    out.extend_from_slice(&output.amount.to_le_bytes());
    out
}

/// Canonical signing message excludes signatures and the transaction id.
pub fn canonical_unsigned_transaction_bytes(tx: &Transaction) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed_bytes(&mut out, b"PulseDAG:unsigned-tx:v1");
    out.extend_from_slice(&tx.version.to_le_bytes());
    let input_count = u32::try_from(tx.inputs.len()).expect("input count exceeds u32::MAX");
    out.extend_from_slice(&input_count.to_le_bytes());
    for input in &tx.inputs {
        out.extend_from_slice(&canonical_outpoint_bytes(&input.previous_output));
        encode_len_prefixed_str(&mut out, &input.public_key);
    }
    let output_count = u32::try_from(tx.outputs.len()).expect("output count exceeds u32::MAX");
    out.extend_from_slice(&output_count.to_le_bytes());
    for output in &tx.outputs {
        out.extend_from_slice(&canonical_tx_output_bytes(output));
    }
    out.extend_from_slice(&tx.fee.to_le_bytes());
    out.extend_from_slice(&tx.nonce.to_le_bytes());
    out
}

/// Canonical consensus serialization for a transaction excludes `txid`.
pub fn canonical_transaction_bytes(tx: &Transaction) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed_bytes(&mut out, b"PulseDAG:tx:v1");
    out.extend_from_slice(&tx.version.to_le_bytes());
    let input_count = u32::try_from(tx.inputs.len()).expect("input count exceeds u32::MAX");
    out.extend_from_slice(&input_count.to_le_bytes());
    for input in &tx.inputs {
        out.extend_from_slice(&canonical_tx_input_bytes(input));
    }
    let output_count = u32::try_from(tx.outputs.len()).expect("output count exceeds u32::MAX");
    out.extend_from_slice(&output_count.to_le_bytes());
    for output in &tx.outputs {
        out.extend_from_slice(&canonical_tx_output_bytes(output));
    }
    out.extend_from_slice(&tx.fee.to_le_bytes());
    out.extend_from_slice(&tx.nonce.to_le_bytes());
    out
}

pub fn signing_message(tx: &Transaction) -> Vec<u8> {
    canonical_unsigned_transaction_bytes(tx)
}

pub fn compute_txid(tx: &Transaction) -> String {
    let digest = Sha256::digest(canonical_transaction_bytes(tx));
    hex::encode(digest)
}

pub fn verify_transaction_signatures(
    tx: &Transaction,
    state: &ChainState,
) -> Result<(), PulseError> {
    let message = signing_message(tx);

    for input in &tx.inputs {
        let expected_address = referenced_output_address(state, &input.previous_output)
            .ok_or(PulseError::UtxoNotFound)?;
        let derived_address = address_from_public_key(&input.public_key);
        if derived_address != expected_address {
            return Err(PulseError::InvalidSignature);
        }

        let pk_bytes = hex::decode(&input.public_key).map_err(|_| PulseError::InvalidSignature)?;
        let sig_bytes = hex::decode(&input.signature).map_err(|_| PulseError::InvalidSignature)?;
        let pk_arr: [u8; 32] = pk_bytes
            .try_into()
            .map_err(|_| PulseError::InvalidSignature)?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| PulseError::InvalidSignature)?;

        let verifying_key =
            VerifyingKey::from_bytes(&pk_arr).map_err(|_| PulseError::InvalidSignature)?;
        let signature = Signature::from_bytes(&sig_arr);
        verifying_key
            .verify(&message, &signature)
            .map_err(|_| PulseError::InvalidSignature)?;
    }

    Ok(())
}
