//! Planning receipt for `pulsedag verify` v1.
//!
//! Binds reconstructed DAG/UTXO/state digests to binary identity and
//! `chain_id`. Host time is not a receipt field. Default admitted
//! template set is empty (inactive covenants). Not a CLI activation.

use sha2::{Digest, Sha256};

pub const VERIFY_RECEIPT_DOMAIN_V1: &str = "PulseDAG:verify-receipt:v1";
pub const VERIFY_RECEIPT_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResultV1 {
    Match,
    Mismatch,
}

impl VerifyResultV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Mismatch => "mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReceiptV1 {
    pub receipt_version: u32,
    pub chain_id: String,
    pub binary_digest: [u8; 32],
    pub from: String,
    pub to_selected_tip: String,
    pub to_pulse_height: Option<u64>,
    pub state_digest: [u8; 32],
    pub utxo_digest: [u8; 32],
    pub dag_digest: [u8; 32],
    pub admitted_templates: Vec<String>,
    pub result: VerifyResultV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyReceiptV1Error {
    EmptyChainId,
    EmptyFrom,
    EmptySelectedTip,
    HostTimeForbidden,
}

impl std::fmt::Display for VerifyReceiptV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyChainId => write!(f, "verify receipt chain_id must not be empty"),
            Self::EmptyFrom => write!(f, "verify receipt from must not be empty"),
            Self::EmptySelectedTip => write!(f, "verify receipt selected tip must not be empty"),
            Self::HostTimeForbidden => {
                write!(f, "host time must not enter a verify receipt")
            }
        }
    }
}

impl std::error::Error for VerifyReceiptV1Error {}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn reject_host_time_v1(host_unix_secs: Option<i64>) -> Result<(), VerifyReceiptV1Error> {
    match host_unix_secs {
        None => Ok(()),
        Some(_) => Err(VerifyReceiptV1Error::HostTimeForbidden),
    }
}

pub fn validate_verify_receipt_v1(receipt: &VerifyReceiptV1) -> Result<(), VerifyReceiptV1Error> {
    if receipt.chain_id.is_empty() {
        return Err(VerifyReceiptV1Error::EmptyChainId);
    }
    if receipt.from.is_empty() {
        return Err(VerifyReceiptV1Error::EmptyFrom);
    }
    if receipt.to_selected_tip.is_empty() {
        return Err(VerifyReceiptV1Error::EmptySelectedTip);
    }
    Ok(())
}

/// Canonical bytes. Template ids are encoded in sorted order so the digest
/// does not depend on caller permutation.
pub fn verify_receipt_canonical_bytes_v1(
    receipt: &VerifyReceiptV1,
) -> Result<Vec<u8>, VerifyReceiptV1Error> {
    validate_verify_receipt_v1(receipt)?;
    let mut templates = receipt.admitted_templates.clone();
    templates.sort();
    templates.dedup();
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, VERIFY_RECEIPT_DOMAIN_V1.as_bytes());
    out.extend_from_slice(&receipt.receipt_version.to_le_bytes());
    encode_len_prefixed(&mut out, receipt.chain_id.as_bytes());
    out.extend_from_slice(&receipt.binary_digest);
    encode_len_prefixed(&mut out, receipt.from.as_bytes());
    encode_len_prefixed(&mut out, receipt.to_selected_tip.as_bytes());
    match receipt.to_pulse_height {
        Some(height) => {
            out.push(1);
            out.extend_from_slice(&height.to_le_bytes());
        }
        None => out.push(0),
    }
    out.extend_from_slice(&receipt.state_digest);
    out.extend_from_slice(&receipt.utxo_digest);
    out.extend_from_slice(&receipt.dag_digest);
    out.extend_from_slice(&u32::try_from(templates.len()).unwrap_or(0).to_le_bytes());
    for id in templates {
        encode_len_prefixed(&mut out, id.as_bytes());
    }
    encode_len_prefixed(&mut out, receipt.result.as_str().as_bytes());
    Ok(out)
}

pub fn verify_receipt_digest_v1(
    receipt: &VerifyReceiptV1,
) -> Result<[u8; 32], VerifyReceiptV1Error> {
    Ok(Sha256::digest(verify_receipt_canonical_bytes_v1(receipt)?).into())
}

pub fn compare_reconstructed_v1(
    expected_state: [u8; 32],
    expected_utxo: [u8; 32],
    expected_dag: [u8; 32],
    observed_state: [u8; 32],
    observed_utxo: [u8; 32],
    observed_dag: [u8; 32],
) -> VerifyResultV1 {
    if expected_state == observed_state
        && expected_utxo == observed_utxo
        && expected_dag == observed_dag
    {
        VerifyResultV1::Match
    } else {
        VerifyResultV1::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> VerifyReceiptV1 {
        VerifyReceiptV1 {
            receipt_version: VERIFY_RECEIPT_VERSION_V1,
            chain_id: "pulsedag-testnet".into(),
            binary_digest: [7u8; 32],
            from: "genesis".into(),
            to_selected_tip: "tip".into(),
            to_pulse_height: Some(3),
            state_digest: [1u8; 32],
            utxo_digest: [2u8; 32],
            dag_digest: [3u8; 32],
            admitted_templates: vec![],
            result: VerifyResultV1::Match,
        }
    }

    #[test]
    fn same_inputs_same_digest_regardless_of_template_order() {
        let mut a = receipt();
        a.admitted_templates = vec!["vault_v1".into(), "htlc_v1".into()];
        let mut b = a.clone();
        b.admitted_templates = vec!["htlc_v1".into(), "vault_v1".into()];
        assert_eq!(
            verify_receipt_digest_v1(&a).unwrap(),
            verify_receipt_digest_v1(&b).unwrap()
        );
    }

    #[test]
    fn mutated_state_is_mismatch_and_changes_digest() {
        let expected = [1u8; 32];
        assert_eq!(
            compare_reconstructed_v1(
                expected, [2u8; 32], [3u8; 32], expected, [2u8; 32], [3u8; 32]
            ),
            VerifyResultV1::Match
        );
        assert_eq!(
            compare_reconstructed_v1(
                expected, [2u8; 32], [3u8; 32], [9u8; 32], [2u8; 32], [3u8; 32]
            ),
            VerifyResultV1::Mismatch
        );
        let match_receipt = receipt();
        let mut mismatch = receipt();
        mismatch.state_digest = [9u8; 32];
        mismatch.result = VerifyResultV1::Mismatch;
        assert_ne!(
            verify_receipt_digest_v1(&match_receipt).unwrap(),
            verify_receipt_digest_v1(&mismatch).unwrap()
        );
    }

    #[test]
    fn chain_id_and_binary_bind_the_receipt() {
        let base = verify_receipt_digest_v1(&receipt()).unwrap();
        let mut other = receipt();
        other.chain_id = "pulsedag-private".into();
        assert_ne!(base, verify_receipt_digest_v1(&other).unwrap());
        other = receipt();
        other.binary_digest = [8u8; 32];
        assert_ne!(base, verify_receipt_digest_v1(&other).unwrap());
    }

    #[test]
    fn empty_identity_fails_closed() {
        let mut receipt = receipt();
        receipt.chain_id.clear();
        assert_eq!(
            verify_receipt_digest_v1(&receipt),
            Err(VerifyReceiptV1Error::EmptyChainId)
        );
    }

    #[test]
    fn default_admitted_templates_are_empty() {
        let receipt = receipt();
        assert!(receipt.admitted_templates.is_empty());
        assert_eq!(receipt.result, VerifyResultV1::Match);
    }

    #[test]
    fn host_time_is_rejected() {
        assert_eq!(
            reject_host_time_v1(Some(1_700_000_000)),
            Err(VerifyReceiptV1Error::HostTimeForbidden)
        );
        reject_host_time_v1(None).unwrap();
    }
}
