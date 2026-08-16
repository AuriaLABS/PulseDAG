use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    state::ChainState,
    types::{Address, OutPoint, Transaction, TxOutput},
};

pub const TRANSACTION_VERSION_V1: u32 = 1;
pub const TRANSACTION_VERSION_V2: u32 = 2;

const UNSIGNED_TX_V1_DOMAIN: &[u8] = b"PulseDAG:unsigned-tx:v1";
const TX_V1_DOMAIN: &[u8] = b"PulseDAG:tx:v1";
const UNSIGNED_TX_V2_DOMAIN: &[u8] = b"PulseDAG:unsigned-tx:v2";
const TX_V2_DOMAIN: &[u8] = b"PulseDAG:tx:v2";

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

/// Canonical v1 signing message. This function is intentionally frozen for
/// historical transaction/signature replay and must not gain chain binding.
pub fn canonical_unsigned_transaction_bytes(tx: &Transaction) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed_bytes(&mut out, UNSIGNED_TX_V1_DOMAIN);
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

/// Canonical v1 consensus serialization for a transaction excludes `txid`.
/// This path is intentionally frozen for historical replay.
pub fn canonical_transaction_bytes(tx: &Transaction) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed_bytes(&mut out, TX_V1_DOMAIN);
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

/// Explicit admission guard for the frozen legacy transaction path.
///
/// Historical v1 canonical serialization remains callable for replay and
/// deterministic fixtures, but ordinary v1 validation must fail closed rather
/// than interpreting a newer transaction version through v1 signature rules.
pub fn validate_transaction_version_v1(tx: &Transaction) -> Result<(), PulseError> {
    if tx.version != TRANSACTION_VERSION_V1 {
        return Err(PulseError::InvalidTransaction(format!(
            "legacy transaction validation requires version {TRANSACTION_VERSION_V1}, got {}",
            tx.version
        )));
    }
    Ok(())
}

fn validate_v2_domain(tx: &Transaction, chain_id: &str) -> Result<(), PulseError> {
    if tx.version != TRANSACTION_VERSION_V2 {
        return Err(PulseError::InvalidTransaction(format!(
            "transaction/signing v2 requires version {TRANSACTION_VERSION_V2}, got {}",
            tx.version
        )));
    }
    if chain_id.is_empty() {
        return Err(PulseError::ChainIdMismatch);
    }
    Ok(())
}

/// Canonical v2 signing serialization. Unlike v1, the exact `chain_id` bytes
/// are part of the signed domain and are encoded before the transaction body.
pub fn canonical_unsigned_transaction_bytes_v2(
    tx: &Transaction,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    validate_v2_domain(tx, chain_id)?;

    let mut out = Vec::with_capacity(288);
    encode_len_prefixed_bytes(&mut out, UNSIGNED_TX_V2_DOMAIN);
    encode_len_prefixed_str(&mut out, chain_id);
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
    Ok(out)
}

/// Canonical v2 transaction serialization used for txid derivation. The exact
/// `chain_id` bytes are included explicitly, so otherwise identical signed
/// transactions on distinct chains cannot share a canonical v2 txid.
pub fn canonical_transaction_bytes_v2(
    tx: &Transaction,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    validate_v2_domain(tx, chain_id)?;

    let mut out = Vec::with_capacity(288);
    encode_len_prefixed_bytes(&mut out, TX_V2_DOMAIN);
    encode_len_prefixed_str(&mut out, chain_id);
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
    Ok(out)
}

/// Frozen v1 signing entry point.
pub fn signing_message(tx: &Transaction) -> Vec<u8> {
    canonical_unsigned_transaction_bytes(tx)
}

/// Chain-bound v2 signing entry point.
pub fn signing_message_v2(tx: &Transaction, chain_id: &str) -> Result<Vec<u8>, PulseError> {
    canonical_unsigned_transaction_bytes_v2(tx, chain_id)
}

/// Frozen v1 txid entry point.
pub fn compute_txid(tx: &Transaction) -> String {
    let digest = Sha256::digest(canonical_transaction_bytes(tx));
    hex::encode(digest)
}

/// Chain-bound v2 txid entry point.
pub fn compute_txid_v2(tx: &Transaction, chain_id: &str) -> Result<String, PulseError> {
    let digest = Sha256::digest(canonical_transaction_bytes_v2(tx, chain_id)?);
    Ok(hex::encode(digest))
}

fn verify_transaction_signatures_with_message(
    tx: &Transaction,
    state: &ChainState,
    message: &[u8],
) -> Result<(), PulseError> {
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
            .verify(message, &signature)
            .map_err(|_| PulseError::InvalidSignature)?;
    }

    Ok(())
}

/// Frozen v1 verification entry point.
pub fn verify_transaction_signatures(
    tx: &Transaction,
    state: &ChainState,
) -> Result<(), PulseError> {
    validate_transaction_version_v1(tx)?;
    let message = signing_message(tx);
    verify_transaction_signatures_with_message(tx, state, &message)
}

/// Chain-bound v2 verification entry point. Wrong transaction versions or an
/// empty chain domain fail before signature verification.
pub fn verify_transaction_signatures_v2(
    tx: &Transaction,
    state: &ChainState,
    chain_id: &str,
) -> Result<(), PulseError> {
    let message = signing_message_v2(tx, chain_id)?;
    verify_transaction_signatures_with_message(tx, state, &message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TxInput, TxOutput};

    fn sample_transaction(version: u32) -> Transaction {
        Transaction {
            txid: String::new(),
            version,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "11".repeat(32),
                    index: 2,
                },
                public_key: "22".repeat(32),
                signature: "33".repeat(64),
            }],
            outputs: vec![TxOutput {
                address: "pulse1abc".into(),
                amount: 42,
            }],
            fee: 3,
            nonce: 9,
        }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    #[test]
    fn v1_golden_vectors_remain_frozen() {
        let tx = sample_transaction(TRANSACTION_VERSION_V1);
        assert_eq!(
            sha256_hex(&signing_message(&tx)),
            "97fabb99f62a1ec99b21f8b96594a576fd224ad3ecd9fc859a00b90e3e574111"
        );
        assert_eq!(
            compute_txid(&tx),
            "1751a527818e473d7b1f0bc88ba363aca1261a8d3a14016ae73e22084b120ed5"
        );
    }

    #[test]
    fn v2_golden_vectors_are_chain_bound() {
        let tx = sample_transaction(TRANSACTION_VERSION_V2);

        let testnet_message = signing_message_v2(&tx, "pulsedag-testnet").unwrap();
        let private_message = signing_message_v2(&tx, "pulsedag-private").unwrap();
        assert_eq!(
            sha256_hex(&testnet_message),
            "33f71a4ae38a5002f429bf29bd36db71502efd33c039f0b61c1e13d1c0a58f06"
        );
        assert_eq!(
            sha256_hex(&private_message),
            "bfe57875392de21b396be9a0aef435b967fd45361f2d733f92cff9592f632236"
        );
        assert_ne!(testnet_message, private_message);

        let testnet_txid = compute_txid_v2(&tx, "pulsedag-testnet").unwrap();
        let private_txid = compute_txid_v2(&tx, "pulsedag-private").unwrap();
        assert_eq!(
            testnet_txid,
            "20eda1a571b4388aee2932df78b054bc2c3d784f0900033bf298d2d043b6708c"
        );
        assert_eq!(
            private_txid,
            "9c01d16ca80682042fa61595df77b5357b77e28aef101d6398b17dcf0c8bd1ca"
        );
        assert_ne!(testnet_txid, private_txid);
    }

    #[test]
    fn v2_rejects_wrong_version_and_empty_chain_domain() {
        let v1 = sample_transaction(TRANSACTION_VERSION_V1);
        assert!(matches!(
            signing_message_v2(&v1, "pulsedag-testnet"),
            Err(PulseError::InvalidTransaction(_))
        ));

        let v2 = sample_transaction(TRANSACTION_VERSION_V2);
        assert!(matches!(
            signing_message_v2(&v2, ""),
            Err(PulseError::ChainIdMismatch)
        ));
    }

    #[test]
    fn legacy_signature_verification_rejects_non_v1_before_utxo_lookup() {
        let state = crate::genesis::init_chain_state("pulsedag-testnet".to_string());
        let v2 = sample_transaction(TRANSACTION_VERSION_V2);

        assert!(matches!(
            verify_transaction_signatures(&v2, &state),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("legacy transaction validation requires version 1")
        ));
    }
}
