use sha2::{Digest, Sha256};

use crate::{errors::PulseError, tx::canonical_transaction_bytes_v2, types::Transaction};

const TX_SUBMISSION_V1_DOMAIN: &[u8] = b"PulseDAG:tx-submission:v1";

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("submission field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

/// Stable v2 submission identity used to correlate idempotent retries without
/// conflating wallet-plan identity with the final canonical transaction id.
///
/// The identity is chain-bound and domain-separated from the txid. It is not a
/// replacement key and does not authorize replace-by-fee semantics.
pub fn compute_submission_id_v2(tx: &Transaction, chain_id: &str) -> Result<String, PulseError> {
    let canonical_tx = canonical_transaction_bytes_v2(tx, chain_id)?;
    let mut bytes = Vec::with_capacity(canonical_tx.len().saturating_add(64));
    encode_len_prefixed_bytes(&mut bytes, TX_SUBMISSION_V1_DOMAIN);
    encode_len_prefixed_bytes(&mut bytes, chain_id.as_bytes());
    encode_len_prefixed_bytes(&mut bytes, &canonical_tx);
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compute_txid_v2,
        types::{OutPoint, Transaction, TxInput, TxOutput},
        TRANSACTION_VERSION_V2,
    };

    fn sample_transaction() -> Transaction {
        Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
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

    #[test]
    fn submission_identity_is_stable_chain_bound_and_not_txid() {
        let tx = sample_transaction();
        let testnet = compute_submission_id_v2(&tx, "pulsedag-testnet").unwrap();
        let private = compute_submission_id_v2(&tx, "pulsedag-private").unwrap();

        assert_eq!(
            testnet,
            "35f8159c20b910a5cd86bdf1088385f93307e0519b9e0213ea92eef737e2f9e0"
        );
        assert_eq!(
            private,
            "456ff7c6c1a5628432b2fa0df19fb60deb5cf757881ab7add7d30bdea2f416b1"
        );
        assert_ne!(testnet, private);
        assert_ne!(testnet, compute_txid_v2(&tx, "pulsedag-testnet").unwrap());
        assert_eq!(
            testnet,
            compute_submission_id_v2(&tx, "pulsedag-testnet").unwrap()
        );
    }

    #[test]
    fn submission_identity_fails_closed_with_invalid_v2_domain() {
        let tx = sample_transaction();
        assert!(compute_submission_id_v2(&tx, "").is_err());

        let mut v1 = tx;
        v1.version = crate::TRANSACTION_VERSION_V1;
        assert!(compute_submission_id_v2(&v1, "pulsedag-testnet").is_err());
    }
}
