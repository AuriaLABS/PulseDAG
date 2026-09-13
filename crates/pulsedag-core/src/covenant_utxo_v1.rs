use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    contract_v3::{
        ContractV3Error, CovenantDescriptorV1, CovenantKindV1, ProgrammabilityActivationIdentity,
    },
    covenant_v1::{
        canonical_covenant_witness_bytes_v1, covenant_parameters_commitment_v1,
        descriptor_for_covenant_parameters_v1, evaluate_covenant_v1, CovenantExecutionContextV1,
        CovenantExecutionError, CovenantParametersV1, CovenantWitnessV1,
    },
    types::Utxo,
};

pub const COVENANT_UTXO_ATTACHMENT_VERSION_V1: u16 = 1;

const COVENANT_UTXO_COMMITMENT_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-utxo:v1";
const COVENANT_UTXO_ATTACHMENT_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-utxo-attachment:v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CovenantUtxoV1Error {
    #[error("unsupported covenant UTXO attachment version {0}")]
    UnsupportedAttachmentVersion(u16),
    #[error("covenant UTXO commitment must not be all zero")]
    EmptyUtxoCommitment,
    #[error("covenant UTXO commitment does not match the spent UTXO and activation identity")]
    UtxoCommitmentMismatch,
    #[error("canonical covenant UTXO field length exceeds u32::MAX")]
    CanonicalLengthOverflow,
    #[error("invalid covenant descriptor or activation identity: {0}")]
    Contract(#[from] ContractV3Error),
    #[error("covenant evaluation failed: {0}")]
    Covenant(#[from] CovenantExecutionError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantUtxoAttachmentV1 {
    pub version: u16,
    pub utxo_commitment: [u8; 32],
    pub covenant: CovenantDescriptorV1,
}

impl CovenantUtxoAttachmentV1 {
    pub fn validate(&self) -> Result<(), CovenantUtxoV1Error> {
        if self.version != COVENANT_UTXO_ATTACHMENT_VERSION_V1 {
            return Err(CovenantUtxoV1Error::UnsupportedAttachmentVersion(
                self.version,
            ));
        }
        if self.utxo_commitment.iter().all(|byte| *byte == 0) {
            return Err(CovenantUtxoV1Error::EmptyUtxoCommitment);
        }
        self.covenant.validate()?;
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CovenantUtxoV1Error> {
        canonical_covenant_utxo_attachment_bytes_v1(self)
    }

    pub fn commitment(&self) -> Result<[u8; 32], CovenantUtxoV1Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantUtxoSpendValidationV1 {
    pub attachment_commitment: [u8; 32],
    pub utxo_commitment: [u8; 32],
    pub parameters_commitment: [u8; 32],
    pub witness_bytes: u32,
}

pub fn attach_covenant_to_utxo_v1(
    identity: &ProgrammabilityActivationIdentity,
    utxo: &Utxo,
    parameters: &CovenantParametersV1,
    witness_budget_bytes: u32,
) -> Result<CovenantUtxoAttachmentV1, CovenantUtxoV1Error> {
    let covenant = descriptor_for_covenant_parameters_v1(parameters, witness_budget_bytes)?;
    let attachment = CovenantUtxoAttachmentV1 {
        version: COVENANT_UTXO_ATTACHMENT_VERSION_V1,
        utxo_commitment: covenant_utxo_commitment_v1(identity, utxo)?,
        covenant,
    };
    attachment.validate()?;
    Ok(attachment)
}

pub fn validate_covenanted_utxo_spend_v1(
    expected_identity: &ProgrammabilityActivationIdentity,
    attachment: &CovenantUtxoAttachmentV1,
    utxo: &Utxo,
    parameters: &CovenantParametersV1,
    witness: &CovenantWitnessV1,
    context: &CovenantExecutionContextV1,
) -> Result<CovenantUtxoSpendValidationV1, CovenantUtxoV1Error> {
    attachment.validate()?;

    let utxo_commitment = covenant_utxo_commitment_v1(expected_identity, utxo)?;
    if attachment.utxo_commitment != utxo_commitment {
        return Err(CovenantUtxoV1Error::UtxoCommitmentMismatch);
    }

    evaluate_covenant_v1(&attachment.covenant, parameters, witness, context)?;

    let witness_len = canonical_covenant_witness_bytes_v1(witness)?.len();
    let witness_bytes =
        u32::try_from(witness_len).map_err(|_| CovenantUtxoV1Error::CanonicalLengthOverflow)?;

    Ok(CovenantUtxoSpendValidationV1 {
        attachment_commitment: attachment.commitment()?,
        utxo_commitment,
        parameters_commitment: covenant_parameters_commitment_v1(parameters)?,
        witness_bytes,
    })
}

pub fn covenant_utxo_commitment_v1(
    identity: &ProgrammabilityActivationIdentity,
    utxo: &Utxo,
) -> Result<[u8; 32], CovenantUtxoV1Error> {
    let identity_fingerprint = identity.fingerprint()?;
    let mut out = Vec::with_capacity(288);
    encode_len_prefixed(&mut out, COVENANT_UTXO_COMMITMENT_DOMAIN_V1)?;
    out.extend_from_slice(&identity_fingerprint);
    encode_len_prefixed(&mut out, utxo.outpoint.txid.as_bytes())?;
    out.extend_from_slice(&utxo.outpoint.index.to_le_bytes());
    encode_len_prefixed(&mut out, utxo.address.as_bytes())?;
    out.extend_from_slice(&utxo.amount.to_le_bytes());
    out.push(u8::from(utxo.coinbase));
    out.extend_from_slice(&utxo.height.to_le_bytes());
    Ok(sha256_array(&out))
}

pub fn canonical_covenant_utxo_attachment_bytes_v1(
    attachment: &CovenantUtxoAttachmentV1,
) -> Result<Vec<u8>, CovenantUtxoV1Error> {
    attachment.validate()?;
    let mut out = Vec::with_capacity(128);
    encode_len_prefixed(&mut out, COVENANT_UTXO_ATTACHMENT_DOMAIN_V1)?;
    out.extend_from_slice(&attachment.version.to_le_bytes());
    out.extend_from_slice(&attachment.utxo_commitment);
    out.extend_from_slice(&attachment.covenant.version.to_le_bytes());
    out.extend_from_slice(&covenant_kind_id(attachment.covenant.kind).to_le_bytes());
    out.extend_from_slice(&attachment.covenant.parameters_commitment);
    out.extend_from_slice(&attachment.covenant.witness_budget_bytes.to_le_bytes());
    Ok(out)
}

fn covenant_kind_id(kind: CovenantKindV1) -> u16 {
    match kind {
        CovenantKindV1::Timelock => 1,
        CovenantKindV1::Vault => 2,
        CovenantKindV1::Multisig => 3,
        CovenantKindV1::Escrow => 4,
        CovenantKindV1::AtomicSwap => 5,
        CovenantKindV1::PaymentChannel => 6,
    }
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), CovenantUtxoV1Error> {
    let len =
        u32::try_from(bytes.len()).map_err(|_| CovenantUtxoV1Error::CanonicalLengthOverflow)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn sha256_array(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract_v3::{ProgrammabilityDomain, PROGRAMMABILITY_IDENTITY_VERSION_V1},
        covenant_v1::{TimelockParametersV1, COVENANT_EXECUTION_SEMANTICS_VERSION_V1},
        types::OutPoint,
    };

    fn sample_identity(domain: ProgrammabilityDomain) -> ProgrammabilityActivationIdentity {
        ProgrammabilityActivationIdentity {
            version: PROGRAMMABILITY_IDENTITY_VERSION_V1,
            domain,
            chain_id: "pulsedag-testnet-v3".into(),
            genesis_hash: [0x11; 32],
            base_protocol_fingerprint: [0x22; 32],
        }
    }

    fn sample_utxo() -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 7,
            },
            address: "pulse1covenanttest".into(),
            amount: 42_000,
            coinbase: false,
            height: 123,
        }
    }

    fn timelock_parameters() -> CovenantParametersV1 {
        CovenantParametersV1::Timelock(TimelockParametersV1 {
            not_before_height: 100,
            spend_commitment: [0x11; 32],
            authorization_key_commitment: [0x22; 32],
        })
    }

    fn timelock_witness() -> CovenantWitnessV1 {
        CovenantWitnessV1::Timelock {
            authorization_key_commitment: [0x22; 32],
        }
    }

    fn context() -> CovenantExecutionContextV1 {
        CovenantExecutionContextV1 {
            version: COVENANT_EXECUTION_SEMANTICS_VERSION_V1,
            block_height: 100,
            spend_commitment: [0x11; 32],
        }
    }

    #[test]
    fn attachment_binds_exact_utxo_and_accepts_matching_spend() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let parameters = timelock_parameters();
        let attachment = attach_covenant_to_utxo_v1(&identity, &utxo, &parameters, 4_096).unwrap();
        let result = validate_covenanted_utxo_spend_v1(
            &identity,
            &attachment,
            &utxo,
            &parameters,
            &timelock_witness(),
            &context(),
        )
        .unwrap();

        assert_eq!(result.utxo_commitment, attachment.utxo_commitment);
        assert_eq!(
            result.parameters_commitment,
            covenant_parameters_commitment_v1(&parameters).unwrap()
        );
        assert!(result.witness_bytes > 0);
    }

    #[test]
    fn attachment_rejects_cross_utxo_substitution() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let parameters = timelock_parameters();
        let attachment = attach_covenant_to_utxo_v1(&identity, &utxo, &parameters, 4_096).unwrap();
        let mut substituted = utxo;
        substituted.amount += 1;

        assert!(matches!(
            validate_covenanted_utxo_spend_v1(
                &identity,
                &attachment,
                &substituted,
                &parameters,
                &timelock_witness(),
                &context(),
            ),
            Err(CovenantUtxoV1Error::UtxoCommitmentMismatch)
        ));
    }

    #[test]
    fn attachment_rejects_cross_chain_substitution() {
        let testnet_identity = sample_identity(ProgrammabilityDomain::Testnet);
        let mainnet_identity = sample_identity(ProgrammabilityDomain::Mainnet);
        let utxo = sample_utxo();
        let parameters = timelock_parameters();
        let attachment =
            attach_covenant_to_utxo_v1(&testnet_identity, &utxo, &parameters, 4_096).unwrap();

        assert_ne!(
            testnet_identity.fingerprint().unwrap(),
            mainnet_identity.fingerprint().unwrap()
        );
        assert!(matches!(
            validate_covenanted_utxo_spend_v1(
                &mainnet_identity,
                &attachment,
                &utxo,
                &parameters,
                &timelock_witness(),
                &context(),
            ),
            Err(CovenantUtxoV1Error::UtxoCommitmentMismatch)
        ));
    }

    #[test]
    fn attachment_rejects_parameter_substitution() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let parameters = timelock_parameters();
        let attachment = attach_covenant_to_utxo_v1(&identity, &utxo, &parameters, 4_096).unwrap();
        let substituted = CovenantParametersV1::Timelock(TimelockParametersV1 {
            not_before_height: 101,
            spend_commitment: [0x11; 32],
            authorization_key_commitment: [0x22; 32],
        });

        assert!(matches!(
            validate_covenanted_utxo_spend_v1(
                &identity,
                &attachment,
                &utxo,
                &substituted,
                &timelock_witness(),
                &context(),
            ),
            Err(CovenantUtxoV1Error::Covenant(
                CovenantExecutionError::ParametersCommitmentMismatch
            ))
        ));
    }

    #[test]
    fn legacy_utxo_shape_is_not_modified_by_attachment_roundtrip() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let before = serde_json::to_vec(&utxo).unwrap();
        let attachment =
            attach_covenant_to_utxo_v1(&identity, &utxo, &timelock_parameters(), 4_096).unwrap();
        let encoded = serde_json::to_vec(&attachment).unwrap();
        let decoded: CovenantUtxoAttachmentV1 = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(attachment, decoded);
        assert_eq!(before, serde_json::to_vec(&utxo).unwrap());
    }

    #[test]
    fn strict_serde_rejects_unknown_attachment_fields() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let mut value = serde_json::to_value(
            attach_covenant_to_utxo_v1(&identity, &utxo, &timelock_parameters(), 4_096).unwrap(),
        )
        .unwrap();
        value["future_field"] = serde_json::json!(1);

        assert!(serde_json::from_value::<CovenantUtxoAttachmentV1>(value).is_err());
    }

    #[test]
    fn unsupported_attachment_version_fails_closed() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let mut attachment =
            attach_covenant_to_utxo_v1(&identity, &sample_utxo(), &timelock_parameters(), 4_096)
                .unwrap();
        attachment.version += 1;

        assert!(matches!(
            attachment.validate(),
            Err(CovenantUtxoV1Error::UnsupportedAttachmentVersion(2))
        ));
    }

    #[test]
    fn sample_utxo_and_attachment_commitments_are_frozen() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let utxo = sample_utxo();
        let parameters = timelock_parameters();
        let attachment = attach_covenant_to_utxo_v1(&identity, &utxo, &parameters, 4_096).unwrap();

        assert_eq!(
            hex::encode(identity.fingerprint().unwrap()),
            "59cb43d2b3cf6bde15c04651572415fa8503d83445820c8ebde3b877f84859df"
        );
        assert_eq!(
            hex::encode(covenant_utxo_commitment_v1(&identity, &utxo).unwrap()),
            "2c2c2afbe2b8a3cdc2902781525138bc7edee7a7a10abbfdf5d90be6e40a4fe2"
        );
        assert_eq!(
            hex::encode(covenant_parameters_commitment_v1(&parameters).unwrap()),
            "16ddbcef24217eb9bc63084ae900ad45bc7a34f9af6acba58cf141d46ef4230c"
        );
        assert_eq!(attachment.canonical_bytes().unwrap().len(), 114);
        assert_eq!(
            hex::encode(attachment.commitment().unwrap()),
            "6e7c5f9c1c6c189646d113b1ebdcde6661b12febd41a44cf4244e0c502fbc07d"
        );
    }
}
