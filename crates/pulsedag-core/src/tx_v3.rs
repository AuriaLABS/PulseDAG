use sha2::{Digest, Sha256};

use crate::{
    decode_hybrid_public_key_v1, decode_hybrid_signature_v1,
    errors::PulseError,
    tx::{canonical_outpoint_bytes, canonical_tx_input_bytes, canonical_tx_output_bytes},
    types::Transaction,
};

/// Reserved transaction version for the post-quantum hybrid authorization path.
///
/// This module defines canonical bytes only. It does not activate v3 protocol
/// admission and it does not provide ML-DSA verification.
pub const TRANSACTION_VERSION_V3: u32 = 3;

const UNSIGNED_TX_V3_DOMAIN: &[u8] = b"PulseDAG:unsigned-tx:v3:pqc1";
const TX_V3_DOMAIN: &[u8] = b"PulseDAG:tx:v3:pqc1";

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

fn validate_v3_domain(tx: &Transaction, chain_id: &str) -> Result<(), PulseError> {
    if tx.version != TRANSACTION_VERSION_V3 {
        return Err(PulseError::InvalidTransaction(format!(
            "transaction/signing v3 requires version {TRANSACTION_VERSION_V3}, got {}",
            tx.version
        )));
    }
    if chain_id.is_empty() {
        return Err(PulseError::ChainIdMismatch);
    }
    Ok(())
}

fn validate_v3_public_key_envelopes(tx: &Transaction) -> Result<(), PulseError> {
    for input in &tx.inputs {
        decode_hybrid_public_key_v1(&input.public_key)?;
    }
    Ok(())
}

fn validate_v3_signature_envelopes(tx: &Transaction) -> Result<(), PulseError> {
    for input in &tx.inputs {
        decode_hybrid_signature_v1(&input.signature)?;
    }
    Ok(())
}

/// Canonical chain-bound v3 signing bytes for the reserved hybrid
/// Ed25519 + ML-DSA-65 authorization format.
///
/// Public-key envelopes are parsed fail-closed before serialization. Signature
/// envelopes are intentionally absent from the unsigned message.
pub fn canonical_unsigned_transaction_bytes_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    validate_v3_domain(tx, chain_id)?;
    validate_v3_public_key_envelopes(tx)?;

    let mut out = Vec::with_capacity(512);
    encode_len_prefixed_bytes(&mut out, UNSIGNED_TX_V3_DOMAIN);
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

/// Canonical chain-bound v3 signed transaction bytes used for txid derivation.
///
/// Both hybrid key and signature envelopes must be structurally valid before a
/// v3 txid can be derived. Cryptographic ML-DSA verification is deliberately
/// outside this non-activating foundation.
pub fn canonical_transaction_bytes_v3(
    tx: &Transaction,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    validate_v3_domain(tx, chain_id)?;
    validate_v3_public_key_envelopes(tx)?;
    validate_v3_signature_envelopes(tx)?;

    let mut out = Vec::with_capacity(1024);
    encode_len_prefixed_bytes(&mut out, TX_V3_DOMAIN);
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

pub fn signing_message_v3(tx: &Transaction, chain_id: &str) -> Result<Vec<u8>, PulseError> {
    canonical_unsigned_transaction_bytes_v3(tx, chain_id)
}

pub fn compute_txid_v3(tx: &Transaction, chain_id: &str) -> Result<String, PulseError> {
    let digest = Sha256::digest(canonical_transaction_bytes_v3(tx, chain_id)?);
    Ok(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        encode_hybrid_public_key_v1, encode_hybrid_signature_v1,
        types::{OutPoint, Transaction, TxInput, TxOutput},
        ED25519_PUBLIC_KEY_BYTES, ED25519_SIGNATURE_BYTES, ML_DSA_65_PUBLIC_KEY_BYTES,
        ML_DSA_65_SIGNATURE_BYTES,
    };

    fn sample_v3_transaction() -> Transaction {
        let public_key = encode_hybrid_public_key_v1(
            &[0x11; ED25519_PUBLIC_KEY_BYTES],
            &vec![0x22; ML_DSA_65_PUBLIC_KEY_BYTES],
        )
        .unwrap();
        let signature = encode_hybrid_signature_v1(
            &[0x33; ED25519_SIGNATURE_BYTES],
            &vec![0x44; ML_DSA_65_SIGNATURE_BYTES],
        )
        .unwrap();

        Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V3,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "11".repeat(32),
                    index: 2,
                },
                public_key,
                signature,
            }],
            outputs: vec![TxOutput {
                address: "pulseq1recipient".into(),
                amount: 42,
            }],
            fee: 3,
            nonce: 9,
        }
    }

    #[test]
    fn v3_signing_and_txid_are_chain_bound() {
        let tx = sample_v3_transaction();

        let testnet_message = signing_message_v3(&tx, "pulsedag-testnet-v3").unwrap();
        let mainnet_message = signing_message_v3(&tx, "pulsedag-mainnet-v3").unwrap();
        assert_ne!(testnet_message, mainnet_message);

        let testnet_txid = compute_txid_v3(&tx, "pulsedag-testnet-v3").unwrap();
        let mainnet_txid = compute_txid_v3(&tx, "pulsedag-mainnet-v3").unwrap();
        assert_ne!(testnet_txid, mainnet_txid);
    }

    #[test]
    fn v3_rejects_wrong_version_and_empty_chain_domain() {
        let mut tx = sample_v3_transaction();
        tx.version = 2;
        assert!(matches!(
            signing_message_v3(&tx, "pulsedag-testnet-v3"),
            Err(PulseError::InvalidTransaction(_))
        ));

        tx.version = TRANSACTION_VERSION_V3;
        assert!(matches!(
            signing_message_v3(&tx, ""),
            Err(PulseError::ChainIdMismatch)
        ));
    }

    #[test]
    fn v3_unsigned_bytes_reject_malformed_hybrid_public_key() {
        let mut tx = sample_v3_transaction();
        tx.inputs[0].public_key = "pqc1:00:00".into();

        assert!(matches!(
            signing_message_v3(&tx, "pulsedag-testnet-v3"),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("post-quantum")
        ));
    }

    #[test]
    fn v3_txid_rejects_malformed_hybrid_signature() {
        let mut tx = sample_v3_transaction();
        tx.inputs[0].signature = "pqc1:00:00".into();

        assert!(matches!(
            compute_txid_v3(&tx, "pulsedag-testnet-v3"),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("post-quantum")
        ));
    }
}
