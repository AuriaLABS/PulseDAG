use sha3::{Digest, Sha3_256};

use crate::{errors::PulseError, types::Address};

/// Canonical version tag for the first PulseDAG post-quantum key/signature envelope.
pub const PQC_ENVELOPE_VERSION_V1: &str = "pqc1";

/// Classical component retained in the hybrid suite.
pub const ED25519_PUBLIC_KEY_BYTES: usize = 32;
pub const ED25519_SIGNATURE_BYTES: usize = 64;

/// FIPS 204 ML-DSA-65 sizes (security category 3).
pub const ML_DSA_65_PUBLIC_KEY_BYTES: usize = 1_952;
pub const ML_DSA_65_SIGNATURE_BYTES: usize = 3_309;

/// Domain separation for post-quantum address commitments.
const PQ_ADDRESS_DOMAIN_V1: &[u8] = b"PulseDAG:pq-address:v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridPublicKeyV1 {
    pub ed25519: Vec<u8>,
    pub ml_dsa_65: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridSignatureV1 {
    pub ed25519: Vec<u8>,
    pub ml_dsa_65: Vec<u8>,
}

fn invalid_pqc_field(field: &str, reason: &str) -> PulseError {
    PulseError::InvalidTransaction(format!("invalid post-quantum {field}: {reason}"))
}

fn decode_canonical_hex(
    field: &str,
    encoded: &str,
    expected_bytes: usize,
) -> Result<Vec<u8>, PulseError> {
    if encoded.len() != expected_bytes.saturating_mul(2) {
        return Err(invalid_pqc_field(field, "unexpected encoded length"));
    }
    if !encoded
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_pqc_field(
            field,
            "must use canonical lowercase hexadecimal",
        ));
    }
    hex::decode(encoded).map_err(|_| invalid_pqc_field(field, "invalid hexadecimal"))
}

fn split_envelope<'a>(field: &str, envelope: &'a str) -> Result<(&'a str, &'a str), PulseError> {
    let mut parts = envelope.split(':');
    let version = parts
        .next()
        .ok_or_else(|| invalid_pqc_field(field, "missing version"))?;
    let classical = parts
        .next()
        .ok_or_else(|| invalid_pqc_field(field, "missing Ed25519 component"))?;
    let post_quantum = parts
        .next()
        .ok_or_else(|| invalid_pqc_field(field, "missing ML-DSA-65 component"))?;
    if parts.next().is_some() {
        return Err(invalid_pqc_field(field, "unexpected extra component"));
    }
    if version != PQC_ENVELOPE_VERSION_V1 {
        return Err(invalid_pqc_field(field, "unsupported envelope version"));
    }
    Ok((classical, post_quantum))
}

pub fn encode_hybrid_public_key_v1(ed25519: &[u8], ml_dsa_65: &[u8]) -> Result<String, PulseError> {
    if ed25519.len() != ED25519_PUBLIC_KEY_BYTES {
        return Err(invalid_pqc_field(
            "public key",
            "invalid Ed25519 public-key length",
        ));
    }
    if ml_dsa_65.len() != ML_DSA_65_PUBLIC_KEY_BYTES {
        return Err(invalid_pqc_field(
            "public key",
            "invalid ML-DSA-65 public-key length",
        ));
    }
    Ok(format!(
        "{PQC_ENVELOPE_VERSION_V1}:{}:{}",
        hex::encode(ed25519),
        hex::encode(ml_dsa_65)
    ))
}

pub fn decode_hybrid_public_key_v1(envelope: &str) -> Result<HybridPublicKeyV1, PulseError> {
    let (ed25519, ml_dsa_65) = split_envelope("public key", envelope)?;
    Ok(HybridPublicKeyV1 {
        ed25519: decode_canonical_hex("Ed25519 public key", ed25519, ED25519_PUBLIC_KEY_BYTES)?,
        ml_dsa_65: decode_canonical_hex(
            "ML-DSA-65 public key",
            ml_dsa_65,
            ML_DSA_65_PUBLIC_KEY_BYTES,
        )?,
    })
}

pub fn encode_hybrid_signature_v1(ed25519: &[u8], ml_dsa_65: &[u8]) -> Result<String, PulseError> {
    if ed25519.len() != ED25519_SIGNATURE_BYTES {
        return Err(invalid_pqc_field(
            "signature",
            "invalid Ed25519 signature length",
        ));
    }
    if ml_dsa_65.len() != ML_DSA_65_SIGNATURE_BYTES {
        return Err(invalid_pqc_field(
            "signature",
            "invalid ML-DSA-65 signature length",
        ));
    }
    Ok(format!(
        "{PQC_ENVELOPE_VERSION_V1}:{}:{}",
        hex::encode(ed25519),
        hex::encode(ml_dsa_65)
    ))
}

pub fn decode_hybrid_signature_v1(envelope: &str) -> Result<HybridSignatureV1, PulseError> {
    let (ed25519, ml_dsa_65) = split_envelope("signature", envelope)?;
    Ok(HybridSignatureV1 {
        ed25519: decode_canonical_hex("Ed25519 signature", ed25519, ED25519_SIGNATURE_BYTES)?,
        ml_dsa_65: decode_canonical_hex(
            "ML-DSA-65 signature",
            ml_dsa_65,
            ML_DSA_65_SIGNATURE_BYTES,
        )?,
    })
}

/// Derive a post-quantum address commitment from the canonical hybrid key envelope.
///
/// Unlike the legacy `pulse1` address path, this keeps the full 256-bit SHA3-256
/// digest so Grover-style generic search still targets roughly 128 bits of
/// preimage security. The address intentionally uses a distinct `pulseq1`
/// prefix so legacy and post-quantum authorization rules cannot be confused.
pub fn address_from_hybrid_public_key_v1(envelope: &str) -> Result<Address, PulseError> {
    let keys = decode_hybrid_public_key_v1(envelope)?;
    let ed25519_len =
        u32::try_from(keys.ed25519.len()).expect("fixed Ed25519 key length exceeds u32");
    let ml_dsa_65_len =
        u32::try_from(keys.ml_dsa_65.len()).expect("fixed ML-DSA-65 key length exceeds u32");

    let mut hasher = Sha3_256::new();
    hasher.update(PQ_ADDRESS_DOMAIN_V1);
    hasher.update(ed25519_len.to_le_bytes());
    hasher.update(&keys.ed25519);
    hasher.update(ml_dsa_65_len.to_le_bytes());
    hasher.update(&keys.ml_dsa_65);
    Ok(format!("pulseq1{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_public_key_envelope_round_trips_canonically() {
        let ed25519 = vec![0x11; ED25519_PUBLIC_KEY_BYTES];
        let ml_dsa_65 = vec![0x22; ML_DSA_65_PUBLIC_KEY_BYTES];
        let encoded = encode_hybrid_public_key_v1(&ed25519, &ml_dsa_65).unwrap();
        let decoded = decode_hybrid_public_key_v1(&encoded).unwrap();
        assert_eq!(decoded.ed25519, ed25519);
        assert_eq!(decoded.ml_dsa_65, ml_dsa_65);
        assert!(encoded.starts_with("pqc1:"));
    }

    #[test]
    fn hybrid_signature_envelope_round_trips_canonically() {
        let ed25519 = vec![0x33; ED25519_SIGNATURE_BYTES];
        let ml_dsa_65 = vec![0x44; ML_DSA_65_SIGNATURE_BYTES];
        let encoded = encode_hybrid_signature_v1(&ed25519, &ml_dsa_65).unwrap();
        let decoded = decode_hybrid_signature_v1(&encoded).unwrap();
        assert_eq!(decoded.ed25519, ed25519);
        assert_eq!(decoded.ml_dsa_65, ml_dsa_65);
    }

    #[test]
    fn post_quantum_address_uses_full_256_bit_commitment() {
        let encoded = encode_hybrid_public_key_v1(
            &[0x55; ED25519_PUBLIC_KEY_BYTES],
            &[0x66; ML_DSA_65_PUBLIC_KEY_BYTES],
        )
        .unwrap();
        let address = address_from_hybrid_public_key_v1(&encoded).unwrap();
        assert!(address.starts_with("pulseq1"));
        assert_eq!(address.len(), "pulseq1".len() + 64);
    }

    #[test]
    fn uppercase_or_wrong_length_envelopes_fail_closed() {
        let encoded = encode_hybrid_public_key_v1(
            &[0xaa; ED25519_PUBLIC_KEY_BYTES],
            &[0xbb; ML_DSA_65_PUBLIC_KEY_BYTES],
        )
        .unwrap();
        let uppercase = encoded.replacen("aa", "AA", 1);
        assert!(decode_hybrid_public_key_v1(&uppercase).is_err());

        let truncated = format!("pqc1:{}:{}", "11".repeat(31), "22".repeat(1_952));
        assert!(decode_hybrid_public_key_v1(&truncated).is_err());
    }
}
