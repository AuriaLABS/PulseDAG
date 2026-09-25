//! Planning matcher for `multisig_m_n_v1`.
//!
//! Threshold spend over a fixed key list. In-tree signatures are Ed25519
//! (same primitive as vault/htlc matchers). The planning spec mentions
//! Schnorr/secp256k1; curve choice stays outside this fail-closed slice.
//! No PulseClock delay — compose with `vault_v1` if one is required.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

pub const MULTISIG_TEMPLATE_ID_V1: &str = "multisig_m_n_v1";
pub const MULTISIG_DOMAIN_V1: &str = "PulseDAG:covenant:multisig:v1";
pub const MULTISIG_MAX_KEYS_V1: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultisigV1Output {
    pub template_id: String,
    pub threshold_m: u8,
    pub keys: Vec<String>,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultisigSpendWitnessV1 {
    pub chain_id: String,
    pub outpoint_txid: String,
    pub outpoint_index: u32,
    pub successor_commitment: String,
    pub key_indexes: Vec<u8>,
    pub signature_hexes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultisigAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl MultisigAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultisigV1Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    InvalidThreshold { m: u8, n: usize },
    TooManyKeys { n: usize },
    EmptyKey,
    KeysNotCanonical,
    EmptyChainId,
    IndexListNotCanonical,
    SignatureCountMismatch { indexes: usize, signatures: usize },
    ThresholdNotMet { required: u8, actual: usize },
    KeyIndexOutOfRange { index: u8 },
    InvalidSignature { index: u8 },
}

impl std::fmt::Display for MultisigV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "multisig_m_n_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(
                    f,
                    "unknown covenant template {observed}; not multisig_m_n_v1"
                )
            }
            Self::InvalidThreshold { m, n } => {
                write!(f, "threshold {m} is invalid for {n} keys")
            }
            Self::TooManyKeys { n } => write!(f, "key count {n} exceeds maximum 16"),
            Self::EmptyKey => write!(f, "multisig key must not be empty"),
            Self::KeysNotCanonical => {
                write!(f, "multisig keys must be unique and strictly sorted")
            }
            Self::EmptyChainId => write!(f, "multisig spend chain_id must not be empty"),
            Self::IndexListNotCanonical => {
                write!(
                    f,
                    "signing key indexes must be unique and strictly increasing"
                )
            }
            Self::SignatureCountMismatch {
                indexes,
                signatures,
            } => write!(
                f,
                "signature count {signatures} does not match index count {indexes}"
            ),
            Self::ThresholdNotMet { required, actual } => {
                write!(f, "got {actual} signatures; need {required}")
            }
            Self::KeyIndexOutOfRange { index } => {
                write!(f, "key index {index} is out of range")
            }
            Self::InvalidSignature { index } => {
                write!(
                    f,
                    "signature at key index {index} failed domain verification"
                )
            }
        }
    }
}

impl std::error::Error for MultisigV1Error {}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn validate_multisig_output_v1(output: &MultisigV1Output) -> Result<(), MultisigV1Error> {
    if output.template_id != MULTISIG_TEMPLATE_ID_V1 {
        return Err(MultisigV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    let n = output.keys.len();
    if n == 0 || n > MULTISIG_MAX_KEYS_V1 {
        return Err(MultisigV1Error::TooManyKeys { n });
    }
    if output.threshold_m < 1 || usize::from(output.threshold_m) > n {
        return Err(MultisigV1Error::InvalidThreshold {
            m: output.threshold_m,
            n,
        });
    }
    let mut previous: Option<&String> = None;
    for key in &output.keys {
        if key.is_empty() {
            return Err(MultisigV1Error::EmptyKey);
        }
        if previous.is_some_and(|prev| key <= prev) {
            return Err(MultisigV1Error::KeysNotCanonical);
        }
        previous = Some(key);
    }
    Ok(())
}

pub fn multisig_spend_signing_message_v1(
    witness: &MultisigSpendWitnessV1,
) -> Result<Vec<u8>, MultisigV1Error> {
    if witness.chain_id.is_empty() {
        return Err(MultisigV1Error::EmptyChainId);
    }
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, MULTISIG_DOMAIN_V1.as_bytes());
    encode_len_prefixed(&mut out, witness.chain_id.as_bytes());
    encode_len_prefixed(&mut out, witness.outpoint_txid.as_bytes());
    out.extend_from_slice(&witness.outpoint_index.to_le_bytes());
    encode_len_prefixed(&mut out, b"spend");
    encode_len_prefixed(&mut out, witness.successor_commitment.as_bytes());
    Ok(out)
}

fn indexes_are_canonical(indexes: &[u8]) -> bool {
    indexes.windows(2).all(|pair| pair[0] < pair[1])
}

pub fn evaluate_multisig_spend_v1(
    admission: MultisigAdmissionV1,
    output: &MultisigV1Output,
    witness: &MultisigSpendWitnessV1,
) -> Result<(), MultisigV1Error> {
    if !admission.admitted() {
        return Err(MultisigV1Error::TemplateDisabled);
    }
    validate_multisig_output_v1(output)?;
    if witness.key_indexes.len() != witness.signature_hexes.len() {
        return Err(MultisigV1Error::SignatureCountMismatch {
            indexes: witness.key_indexes.len(),
            signatures: witness.signature_hexes.len(),
        });
    }
    if witness.key_indexes.is_empty() || !indexes_are_canonical(&witness.key_indexes) {
        return Err(MultisigV1Error::IndexListNotCanonical);
    }
    if witness.key_indexes.len() != usize::from(output.threshold_m) {
        return Err(MultisigV1Error::ThresholdNotMet {
            required: output.threshold_m,
            actual: witness.key_indexes.len(),
        });
    }
    let message = multisig_spend_signing_message_v1(witness)?;
    for (index, signature_hex) in witness
        .key_indexes
        .iter()
        .zip(witness.signature_hexes.iter())
    {
        let key = output
            .keys
            .get(usize::from(*index))
            .ok_or(MultisigV1Error::KeyIndexOutOfRange { index: *index })?;
        let pk_bytes =
            hex::decode(key).map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
        let sig_bytes = hex::decode(signature_hex)
            .map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
        let pk_arr: [u8; 32] = pk_bytes
            .try_into()
            .map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
        let verifying_key = VerifyingKey::from_bytes(&pk_arr)
            .map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
        let signature = Signature::from_bytes(&sig_arr);
        verifying_key
            .verify(&message, &signature)
            .map_err(|_| MultisigV1Error::InvalidSignature { index: *index })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn pk_hex(key: &SigningKey) -> String {
        hex::encode(key.verifying_key().to_bytes())
    }

    fn two_of_three() -> (Vec<SigningKey>, MultisigV1Output) {
        let keys = vec![key(1), key(2), key(3)];
        let mut pks: Vec<String> = keys.iter().map(pk_hex).collect();
        pks.sort();
        // keep signing keys aligned with sorted pks
        let mut paired: Vec<(SigningKey, String)> = keys
            .into_iter()
            .map(|k| {
                let pk = pk_hex(&k);
                (k, pk)
            })
            .collect();
        paired.sort_by(|a, b| a.1.cmp(&b.1));
        let keys: Vec<SigningKey> = paired
            .iter()
            .map(|(k, _)| SigningKey::from_bytes(&k.to_bytes()))
            .collect();
        let output = MultisigV1Output {
            template_id: MULTISIG_TEMPLATE_ID_V1.into(),
            threshold_m: 2,
            keys: paired.into_iter().map(|(_, pk)| pk).collect(),
            amount: 100,
        };
        (keys, output)
    }

    fn sign(
        keys: &[SigningKey],
        output: &MultisigV1Output,
        indexes: &[u8],
        chain_id: &str,
    ) -> MultisigSpendWitnessV1 {
        let mut witness = MultisigSpendWitnessV1 {
            chain_id: chain_id.into(),
            outpoint_txid: "cc".repeat(32),
            outpoint_index: 1,
            successor_commitment: "dd".repeat(32),
            key_indexes: indexes.to_vec(),
            signature_hexes: vec![],
        };
        let message = multisig_spend_signing_message_v1(&witness).unwrap();
        witness.signature_hexes = indexes
            .iter()
            .map(|idx| {
                let pk = &output.keys[usize::from(*idx)];
                let signer = keys.iter().find(|k| pk_hex(k) == *pk).expect("key present");
                hex::encode(signer.sign(&message).to_bytes())
            })
            .collect();
        witness
    }

    fn admitted() -> MultisigAdmissionV1 {
        MultisigAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    #[test]
    fn inactive_admission_rejects_valid_threshold() {
        let (keys, output) = two_of_three();
        let witness = sign(&keys, &output, &[0, 1], "msig-test");
        assert_eq!(
            evaluate_multisig_spend_v1(MultisigAdmissionV1::INACTIVE, &output, &witness),
            Err(MultisigV1Error::TemplateDisabled)
        );
    }

    #[test]
    fn two_of_three_is_accepted_when_admitted() {
        let (keys, output) = two_of_three();
        let witness = sign(&keys, &output, &[0, 2], "msig-test");
        evaluate_multisig_spend_v1(admitted(), &output, &witness).unwrap();
    }

    #[test]
    fn one_signature_misses_threshold() {
        let (keys, output) = two_of_three();
        let witness = sign(&keys, &output, &[1], "msig-test");
        assert_eq!(
            evaluate_multisig_spend_v1(admitted(), &output, &witness),
            Err(MultisigV1Error::ThresholdNotMet {
                required: 2,
                actual: 1
            })
        );
    }

    #[test]
    fn unsorted_or_duplicate_indexes_are_rejected() {
        let (keys, output) = two_of_three();
        let mut witness = sign(&keys, &output, &[0, 1], "msig-test");
        witness.key_indexes = vec![1, 0];
        assert_eq!(
            evaluate_multisig_spend_v1(admitted(), &output, &witness),
            Err(MultisigV1Error::IndexListNotCanonical)
        );
        witness.key_indexes = vec![1, 1];
        assert_eq!(
            evaluate_multisig_spend_v1(admitted(), &output, &witness),
            Err(MultisigV1Error::IndexListNotCanonical)
        );
    }

    #[test]
    fn output_keys_must_be_unique_and_sorted() {
        let (_, mut output) = two_of_three();
        output.keys[0] = output.keys[1].clone();
        assert_eq!(
            validate_multisig_output_v1(&output),
            Err(MultisigV1Error::KeysNotCanonical)
        );
        output = two_of_three().1;
        output.threshold_m = 4;
        assert!(matches!(
            validate_multisig_output_v1(&output),
            Err(MultisigV1Error::InvalidThreshold { m: 4, n: 3 })
        ));
    }

    #[test]
    fn wrong_chain_id_fails_signature() {
        let (keys, output) = two_of_three();
        let mut witness = sign(&keys, &output, &[0, 1], "msig-test");
        witness.chain_id = "other".into();
        assert!(matches!(
            evaluate_multisig_spend_v1(admitted(), &output, &witness),
            Err(MultisigV1Error::InvalidSignature { .. })
        ));
    }
}
