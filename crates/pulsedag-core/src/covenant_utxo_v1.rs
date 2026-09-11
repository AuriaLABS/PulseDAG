use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    contract_v3::{CovenantDescriptorV1, CovenantKindV1},
    covenant_v1::{
        evaluate_covenant_v1, CovenantEvaluationV1, CovenantProgramV1, CovenantResourceUsageV1,
        CovenantSpendContextV1, CovenantV1Error, CovenantWitnessV1,
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
    #[error("covenant UTXO commitment does not match the spent UTXO")]
    UtxoCommitmentMismatch,
    #[error("canonical covenant UTXO field length exceeds u32::MAX")]
    CanonicalLengthOverflow,
    #[error("covenant evaluation failed: {0}")]
    Covenant(#[from] CovenantV1Error),
}

impl CovenantUtxoV1Error {
    pub fn rejection_code(&self) -> u16 {
        match self {
            Self::UnsupportedAttachmentVersion(_) => 1,
            Self::EmptyUtxoCommitment => 2,
            Self::UtxoCommitmentMismatch => 3,
            Self::CanonicalLengthOverflow => 4,
            Self::Covenant(error) => 100_u16.saturating_add(error.rejection_code()),
        }
    }
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
        self.covenant.validate().map_err(CovenantV1Error::from)?;
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
    pub program_commitment: [u8; 32],
    pub witness_commitment: [u8; 32],
    pub resource_usage: CovenantResourceUsageV1,
}

pub fn attach_covenant_to_utxo_v1(
    utxo: &Utxo,
    program: &CovenantProgramV1,
    witness_budget_bytes: u32,
) -> Result<CovenantUtxoAttachmentV1, CovenantUtxoV1Error> {
    let covenant = program.descriptor(witness_budget_bytes)?;
    let attachment = CovenantUtxoAttachmentV1 {
        version: COVENANT_UTXO_ATTACHMENT_VERSION_V1,
        utxo_commitment: covenant_utxo_commitment_v1(utxo)?,
        covenant,
    };
    attachment.validate()?;
    Ok(attachment)
}

pub fn validate_covenanted_utxo_spend_v1(
    attachment: &CovenantUtxoAttachmentV1,
    utxo: &Utxo,
    program: &CovenantProgramV1,
    witness: &CovenantWitnessV1,
    context: &CovenantSpendContextV1,
) -> Result<CovenantUtxoSpendValidationV1, CovenantUtxoV1Error> {
    attachment.validate()?;
    let utxo_commitment = covenant_utxo_commitment_v1(utxo)?;
    if attachment.utxo_commitment != utxo_commitment {
        return Err(CovenantUtxoV1Error::UtxoCommitmentMismatch);
    }

    let CovenantEvaluationV1 {
        program_commitment,
        witness_commitment,
        resource_usage,
    } = evaluate_covenant_v1(&attachment.covenant, program, witness, context)?;

    Ok(CovenantUtxoSpendValidationV1 {
        attachment_commitment: attachment.commitment()?,
        utxo_commitment,
        program_commitment,
        witness_commitment,
        resource_usage,
    })
}

pub fn covenant_utxo_commitment_v1(utxo: &Utxo) -> Result<[u8; 32], CovenantUtxoV1Error> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed(&mut out, COVENANT_UTXO_COMMITMENT_DOMAIN_V1)?;
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

fn encode_len_prefixed(
    out: &mut Vec<u8>,
    bytes: &[u8],
) -> Result<(), CovenantUtxoV1Error> {
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
        covenant_v1::{
            CovenantLockV1, CovenantProgramBodyV1, CovenantWitnessBranchV1,
            COVENANT_PROGRAM_VERSION_V1, COVENANT_SPEND_CONTEXT_VERSION_V1,
            COVENANT_WITNESS_VERSION_V1, TimelockCovenantV1,
        },
        types::OutPoint,
    };

    fn signer(byte: u8) -> [u8; 32] {
        [byte; 32]
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

    fn timelock_program() -> CovenantProgramV1 {
        CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::Height(200),
            }),
        }
    }

    fn witness() -> CovenantWitnessV1 {
        CovenantWitnessV1 {
            version: COVENANT_WITNESS_VERSION_V1,
            branch: CovenantWitnessBranchV1::Timelock,
        }
    }

    fn context() -> CovenantSpendContextV1 {
        CovenantSpendContextV1 {
            version: COVENANT_SPEND_CONTEXT_VERSION_V1,
            current_height: 200,
            median_time_seconds: 0,
            authenticated_signers: vec![signer(1)],
        }
    }

    #[test]
    fn attachment_binds_exact_utxo_and_accepts_matching_spend() {
        let utxo = sample_utxo();
        let program = timelock_program();
        let attachment = attach_covenant_to_utxo_v1(&utxo, &program, 4_096).unwrap();
        let result = validate_covenanted_utxo_spend_v1(
            &attachment,
            &utxo,
            &program,
            &witness(),
            &context(),
        )
        .unwrap();
        assert_eq!(result.utxo_commitment, attachment.utxo_commitment);
        assert_eq!(result.program_commitment, program.commitment().unwrap());
    }

    #[test]
    fn attachment_rejects_cross_utxo_substitution() {
        let utxo = sample_utxo();
        let program = timelock_program();
        let attachment = attach_covenant_to_utxo_v1(&utxo, &program, 4_096).unwrap();
        let mut substituted = utxo;
        substituted.amount += 1;
        assert!(matches!(
            validate_covenanted_utxo_spend_v1(
                &attachment,
                &substituted,
                &program,
                &witness(),
                &context(),
            ),
            Err(CovenantUtxoV1Error::UtxoCommitmentMismatch)
        ));
    }

    #[test]
    fn attachment_rejects_program_substitution() {
        let utxo = sample_utxo();
        let program = timelock_program();
        let attachment = attach_covenant_to_utxo_v1(&utxo, &program, 4_096).unwrap();
        let different_program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::Height(201),
            }),
        };
        assert!(matches!(
            validate_covenanted_utxo_spend_v1(
                &attachment,
                &utxo,
                &different_program,
                &witness(),
                &context(),
            ),
            Err(CovenantUtxoV1Error::Covenant(
                CovenantV1Error::DescriptorCommitmentMismatch
            ))
        ));
    }

    #[test]
    fn legacy_utxo_shape_is_not_modified_by_attachment_roundtrip() {
        let utxo = sample_utxo();
        let before = serde_json::to_vec(&utxo).unwrap();
        let attachment = attach_covenant_to_utxo_v1(&utxo, &timelock_program(), 4_096).unwrap();
        let encoded = serde_json::to_vec(&attachment).unwrap();
        let decoded: CovenantUtxoAttachmentV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(attachment, decoded);
        assert_eq!(before, serde_json::to_vec(&utxo).unwrap());
    }

    #[test]
    fn strict_serde_rejects_unknown_attachment_fields() {
        let utxo = sample_utxo();
        let mut value = serde_json::to_value(
            attach_covenant_to_utxo_v1(&utxo, &timelock_program(), 4_096).unwrap(),
        )
        .unwrap();
        value["future_field"] = serde_json::json!(1);
        assert!(serde_json::from_value::<CovenantUtxoAttachmentV1>(value).is_err());
    }

    #[test]
    fn sample_utxo_and_attachment_commitments_are_frozen() {
        let utxo = sample_utxo();
        let attachment = attach_covenant_to_utxo_v1(&utxo, &timelock_program(), 4_096).unwrap();
        assert_eq!(
            hex::encode(covenant_utxo_commitment_v1(&utxo).unwrap()),
            "d963255da4b2a57170093a53ae4fd332e679a5642dfae4dab0f3942b48dc3cd1"
        );
        assert_eq!(
            hex::encode(attachment.commitment().unwrap()),
            "e2ea41ec14721200fd59643c6cecd3505172548a2c572756215e475a0bb85a9b"
        );
    }
}
