use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
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

pub fn signing_message(tx: &Transaction) -> Vec<u8> {
    #[derive(Serialize)]
    struct UnsignedTx<'a> {
        version: u32,
        inputs: Vec<&'a OutPoint>,
        outputs: &'a [TxOutput],
        fee: u64,
        nonce: u64,
    }

    let inputs = tx
        .inputs
        .iter()
        .map(|i| &i.previous_output)
        .collect::<Vec<_>>();
    let unsigned = UnsignedTx {
        version: tx.version,
        inputs,
        outputs: &tx.outputs,
        fee: tx.fee,
        nonce: tx.nonce,
    };

    serde_json::to_vec(&unsigned).unwrap_or_default()
}

pub fn compute_txid(tx: &Transaction) -> String {
    #[derive(Serialize)]
    struct CanonicalInput<'a> {
        previous_output: &'a OutPoint,
        public_key: &'a str,
        signature: &'a str,
    }

    #[derive(Serialize)]
    struct CanonicalTx<'a> {
        version: u32,
        inputs: Vec<CanonicalInput<'a>>,
        outputs: &'a [TxOutput],
        fee: u64,
        nonce: u64,
    }

    let inputs = tx
        .inputs
        .iter()
        .map(|i| CanonicalInput {
            previous_output: &i.previous_output,
            public_key: &i.public_key,
            signature: &i.signature,
        })
        .collect::<Vec<_>>();

    let canonical = CanonicalTx {
        version: tx.version,
        inputs,
        outputs: &tx.outputs,
        fee: tx.fee,
        nonce: tx.nonce,
    };

    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    let digest = Sha256::digest(bytes);
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
