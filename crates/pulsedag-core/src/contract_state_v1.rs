use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::contract_v3::{
    compute_contract_txid_v3, ContractTransactionV3Envelope, ContractV3Error,
    ProgrammabilityActivationIdentity, ProofCommitmentMetadataV1, ResourceBudgetV1,
    PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1, PROOF_SYSTEM_NONE,
};

pub const CONTRACT_STATE_TRANSITION_VERSION_V1: u16 = 1;
pub const PROOF_EVIDENCE_VERSION_V1: u16 = 1;
pub const PROOF_EVIDENCE_POLICY_VERSION_V1: u16 = 1;

const CONTRACT_STATE_TRANSITION_DOMAIN_V1: &[u8] = b"PulseDAG:contract-state-transition:v1";
const PROOF_EVIDENCE_BINDING_DOMAIN_V1: &[u8] = b"PulseDAG:proof-evidence-binding:v1";
const EMPTY_WRITE_SET_DOMAIN_V1: &[u8] = b"PulseDAG:contract-empty-write-set:v1";
const EMPTY_EVENT_SET_DOMAIN_V1: &[u8] = b"PulseDAG:contract-empty-event-set:v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContractStateV1Error {
    #[error("invalid contract v3 envelope or identity: {0}")]
    ContractV3(#[from] ContractV3Error),
    #[error("unsupported contract state-transition version {0}")]
    UnsupportedTransitionVersion(u16),
    #[error("unsupported proof-evidence version {0}")]
    UnsupportedEvidenceVersion(u16),
    #[error("unsupported proof-evidence policy version {0}")]
    UnsupportedEvidencePolicyVersion(u16),
    #[error("proof evidence contains an unsupported proof-system tuple")]
    UnsupportedProofTuple,
    #[error("proof evidence commitment is invalid for the selected proof system")]
    InvalidEvidenceCommitment,
    #[error("proof evidence does not match the contract envelope proof commitment")]
    EvidenceCommitmentMismatch,
    #[error("proof evidence was deterministically invalidated by policy generation")]
    EvidenceInvalidated,
    #[error("proof evidence does not match the active deterministic evidence policy")]
    EvidencePolicyMismatch,
    #[error("contract transition identity fingerprint mismatch")]
    IdentityFingerprintMismatch,
    #[error("contract transition txid mismatch")]
    ContractTxidMismatch,
    #[error("contract transition prior-state commitment mismatch")]
    PriorStateMismatch,
    #[error("contract transition next-state commitment mismatch")]
    NextStateMismatch,
    #[error("contract transition replay nonce mismatch")]
    ReplayNonceMismatch,
    #[error("{field} usage {actual} exceeds envelope budget {max}")]
    ResourceBudgetExceeded {
        field: &'static str,
        actual: u64,
        max: u64,
    },
    #[error("{field} commitment must not be all zero")]
    EmptyCommitment { field: &'static str },
    #[error("contract state canonical length exceeds u32::MAX")]
    CanonicalLengthOverflow,
    #[error("contract transition resource usage does not match deterministic measured usage")]
    ResourceUsageMismatch,
    #[error(
        "contract transition write-set commitment does not match deterministic measured output"
    )]
    WriteSetCommitmentMismatch,
    #[error(
        "contract transition event-set commitment does not match deterministic measured output"
    )]
    EventSetCommitmentMismatch,
}

impl ContractStateV1Error {
    pub fn rejection_code(&self) -> u16 {
        match self {
            Self::ContractV3(_) => 1,
            Self::UnsupportedTransitionVersion(_) => 2,
            Self::UnsupportedEvidenceVersion(_) => 3,
            Self::UnsupportedEvidencePolicyVersion(_) => 4,
            Self::UnsupportedProofTuple => 5,
            Self::InvalidEvidenceCommitment => 6,
            Self::EvidenceCommitmentMismatch => 7,
            Self::EvidenceInvalidated => 8,
            Self::EvidencePolicyMismatch => 9,
            Self::IdentityFingerprintMismatch => 10,
            Self::ContractTxidMismatch => 11,
            Self::PriorStateMismatch => 12,
            Self::NextStateMismatch => 13,
            Self::ReplayNonceMismatch => 14,
            Self::ResourceBudgetExceeded { .. } => 15,
            Self::EmptyCommitment { .. } => 16,
            Self::CanonicalLengthOverflow => 17,
            Self::ResourceUsageMismatch => 18,
            Self::WriteSetCommitmentMismatch => 19,
            Self::EventSetCommitmentMismatch => 20,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ContractResourceUsageV1 {
    pub compute_units: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub proof_bytes: u32,
    pub event_bytes: u32,
}

impl ContractResourceUsageV1 {
    pub fn validate_against_budget(
        &self,
        budget: &ResourceBudgetV1,
    ) -> Result<(), ContractStateV1Error> {
        budget.validate()?;
        ensure_budget("compute_units", self.compute_units, budget.compute_units)?;
        ensure_budget("read_bytes", self.read_bytes, budget.read_bytes)?;
        ensure_budget("write_bytes", self.write_bytes, budget.write_bytes)?;
        ensure_budget(
            "proof_bytes",
            u64::from(self.proof_bytes),
            u64::from(budget.proof_bytes),
        )?;
        ensure_budget(
            "event_bytes",
            u64::from(self.event_bytes),
            u64::from(budget.event_bytes),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProofEvidenceV1 {
    pub version: u16,
    pub proof_system: u16,
    pub proof_system_version: u16,
    pub verifier_revision: u32,
    pub evidence_generation: u64,
    pub evidence_payload_commitment: [u8; 32],
}

impl ProofEvidenceV1 {
    pub fn validate(&self) -> Result<(), ContractStateV1Error> {
        if self.version != PROOF_EVIDENCE_VERSION_V1 {
            return Err(ContractStateV1Error::UnsupportedEvidenceVersion(
                self.version,
            ));
        }

        match self.proof_system {
            PROOF_SYSTEM_NONE => {
                if self.proof_system_version != 0
                    || self.verifier_revision != 0
                    || self.evidence_generation != 0
                {
                    return Err(ContractStateV1Error::UnsupportedProofTuple);
                }
                if self
                    .evidence_payload_commitment
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(ContractStateV1Error::InvalidEvidenceCommitment);
                }
            }
            PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1 => {
                if self.proof_system_version != 1 || self.verifier_revision == 0 {
                    return Err(ContractStateV1Error::UnsupportedProofTuple);
                }
                ensure_nonzero_commitment(
                    "evidence_payload_commitment",
                    &self.evidence_payload_commitment,
                )?;
            }
            _ => return Err(ContractStateV1Error::UnsupportedProofTuple),
        }
        Ok(())
    }

    pub fn binding_commitment(&self) -> Result<[u8; 32], ContractStateV1Error> {
        compute_proof_evidence_binding_commitment_v1(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProofEvidencePolicyV1 {
    pub version: u16,
    pub proof_system: u16,
    pub proof_system_version: u16,
    pub verifier_revision: u32,
    pub minimum_valid_evidence_generation: u64,
}

impl ProofEvidencePolicyV1 {
    pub fn validate(&self) -> Result<(), ContractStateV1Error> {
        if self.version != PROOF_EVIDENCE_POLICY_VERSION_V1 {
            return Err(ContractStateV1Error::UnsupportedEvidencePolicyVersion(
                self.version,
            ));
        }

        match self.proof_system {
            PROOF_SYSTEM_NONE => {
                if self.proof_system_version != 0
                    || self.verifier_revision != 0
                    || self.minimum_valid_evidence_generation != 0
                {
                    return Err(ContractStateV1Error::UnsupportedProofTuple);
                }
            }
            PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1 => {
                if self.proof_system_version != 1 || self.verifier_revision == 0 {
                    return Err(ContractStateV1Error::UnsupportedProofTuple);
                }
            }
            _ => return Err(ContractStateV1Error::UnsupportedProofTuple),
        }
        Ok(())
    }

    pub fn validate_evidence(
        &self,
        envelope_proof: &ProofCommitmentMetadataV1,
        evidence: &ProofEvidenceV1,
    ) -> Result<(), ContractStateV1Error> {
        self.validate()?;
        evidence.validate()?;
        envelope_proof.validate()?;

        if self.proof_system != envelope_proof.proof_system
            || self.proof_system_version != envelope_proof.proof_system_version
            || evidence.proof_system != self.proof_system
            || evidence.proof_system_version != self.proof_system_version
            || evidence.verifier_revision != self.verifier_revision
        {
            return Err(ContractStateV1Error::EvidencePolicyMismatch);
        }

        if self.proof_system == PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1
            && evidence.evidence_generation < self.minimum_valid_evidence_generation
        {
            return Err(ContractStateV1Error::EvidenceInvalidated);
        }

        if evidence.binding_commitment()? != envelope_proof.proof_commitment {
            return Err(ContractStateV1Error::EvidenceCommitmentMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractStateTransitionV1 {
    pub version: u16,
    pub identity_fingerprint: [u8; 32],
    pub contract_txid: [u8; 32],
    pub prior_state_commitment: [u8; 32],
    pub next_state_commitment: [u8; 32],
    pub write_set_commitment: [u8; 32],
    pub event_set_commitment: [u8; 32],
    pub resource_usage: ContractResourceUsageV1,
    pub replay_nonce: u64,
    pub proof_evidence: ProofEvidenceV1,
}

impl ContractStateTransitionV1 {
    pub fn validate_shape(&self) -> Result<(), ContractStateV1Error> {
        if self.version != CONTRACT_STATE_TRANSITION_VERSION_V1 {
            return Err(ContractStateV1Error::UnsupportedTransitionVersion(
                self.version,
            ));
        }
        ensure_nonzero_commitment("identity_fingerprint", &self.identity_fingerprint)?;
        ensure_nonzero_commitment("contract_txid", &self.contract_txid)?;
        ensure_nonzero_commitment("prior_state_commitment", &self.prior_state_commitment)?;
        ensure_nonzero_commitment("next_state_commitment", &self.next_state_commitment)?;
        ensure_nonzero_commitment("write_set_commitment", &self.write_set_commitment)?;
        ensure_nonzero_commitment("event_set_commitment", &self.event_set_commitment)?;
        self.proof_evidence.validate()?;
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractStateV1Error> {
        canonical_contract_state_transition_bytes_v1(self)
    }

    pub fn commitment(&self) -> Result<[u8; 32], ContractStateV1Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractStateValidationV1 {
    pub transition_commitment: [u8; 32],
    pub write_set_commitment: [u8; 32],
    pub event_set_commitment: [u8; 32],
    pub resource_usage: ContractResourceUsageV1,
    pub replay_nonce: u64,
}

pub fn validate_contract_state_transition_v1(
    envelope: &ContractTransactionV3Envelope,
    expected_identity: &ProgrammabilityActivationIdentity,
    expected_prior_state_commitment: [u8; 32],
    expected_replay_nonce: u64,
    measured_write_set_commitment: [u8; 32],
    measured_event_set_commitment: [u8; 32],
    measured_resource_usage: ContractResourceUsageV1,
    transition: &ContractStateTransitionV1,
    evidence_policy: &ProofEvidencePolicyV1,
) -> Result<ContractStateValidationV1, ContractStateV1Error> {
    envelope.validate_identity(expected_identity)?;
    transition.validate_shape()?;

    let expected_fingerprint = expected_identity.fingerprint()?;
    if transition.identity_fingerprint != expected_fingerprint {
        return Err(ContractStateV1Error::IdentityFingerprintMismatch);
    }

    let expected_txid = compute_contract_txid_v3(envelope)?;
    if transition.contract_txid != expected_txid {
        return Err(ContractStateV1Error::ContractTxidMismatch);
    }
    if transition.prior_state_commitment != expected_prior_state_commitment {
        return Err(ContractStateV1Error::PriorStateMismatch);
    }
    if transition.next_state_commitment != envelope.state_commitment {
        return Err(ContractStateV1Error::NextStateMismatch);
    }
    if transition.replay_nonce != envelope.replay_nonce
        || transition.replay_nonce != expected_replay_nonce
    {
        return Err(ContractStateV1Error::ReplayNonceMismatch);
    }

    if transition.write_set_commitment != measured_write_set_commitment {
        return Err(ContractStateV1Error::WriteSetCommitmentMismatch);
    }
    if transition.event_set_commitment != measured_event_set_commitment {
        return Err(ContractStateV1Error::EventSetCommitmentMismatch);
    }
    if transition.resource_usage != measured_resource_usage {
        return Err(ContractStateV1Error::ResourceUsageMismatch);
    }
    measured_resource_usage.validate_against_budget(&envelope.resources)?;
    evidence_policy.validate_evidence(&envelope.proof, &transition.proof_evidence)?;

    Ok(ContractStateValidationV1 {
        transition_commitment: transition.commitment()?,
        write_set_commitment: measured_write_set_commitment,
        event_set_commitment: measured_event_set_commitment,
        resource_usage: measured_resource_usage,
        replay_nonce: transition.replay_nonce,
    })
}

pub fn compute_proof_evidence_binding_commitment_v1(
    evidence: &ProofEvidenceV1,
) -> Result<[u8; 32], ContractStateV1Error> {
    evidence.validate()?;
    if evidence.proof_system == PROOF_SYSTEM_NONE {
        return Ok([0; 32]);
    }

    let mut out = Vec::with_capacity(96);
    encode_len_prefixed(&mut out, PROOF_EVIDENCE_BINDING_DOMAIN_V1)?;
    out.extend_from_slice(&evidence.version.to_le_bytes());
    out.extend_from_slice(&evidence.proof_system.to_le_bytes());
    out.extend_from_slice(&evidence.proof_system_version.to_le_bytes());
    out.extend_from_slice(&evidence.verifier_revision.to_le_bytes());
    out.extend_from_slice(&evidence.evidence_generation.to_le_bytes());
    out.extend_from_slice(&evidence.evidence_payload_commitment);
    Ok(sha256_array(&out))
}

pub fn canonical_contract_state_transition_bytes_v1(
    transition: &ContractStateTransitionV1,
) -> Result<Vec<u8>, ContractStateV1Error> {
    transition.validate_shape()?;
    let mut out = Vec::with_capacity(320);
    encode_len_prefixed(&mut out, CONTRACT_STATE_TRANSITION_DOMAIN_V1)?;
    out.extend_from_slice(&transition.version.to_le_bytes());
    out.extend_from_slice(&transition.identity_fingerprint);
    out.extend_from_slice(&transition.contract_txid);
    out.extend_from_slice(&transition.prior_state_commitment);
    out.extend_from_slice(&transition.next_state_commitment);
    out.extend_from_slice(&transition.write_set_commitment);
    out.extend_from_slice(&transition.event_set_commitment);
    out.extend_from_slice(&transition.resource_usage.compute_units.to_le_bytes());
    out.extend_from_slice(&transition.resource_usage.read_bytes.to_le_bytes());
    out.extend_from_slice(&transition.resource_usage.write_bytes.to_le_bytes());
    out.extend_from_slice(&transition.resource_usage.proof_bytes.to_le_bytes());
    out.extend_from_slice(&transition.resource_usage.event_bytes.to_le_bytes());
    out.extend_from_slice(&transition.replay_nonce.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.version.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.proof_system.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.proof_system_version.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.verifier_revision.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.evidence_generation.to_le_bytes());
    out.extend_from_slice(&transition.proof_evidence.evidence_payload_commitment);
    Ok(out)
}

pub fn empty_write_set_commitment_v1() -> [u8; 32] {
    sha256_array(EMPTY_WRITE_SET_DOMAIN_V1)
}

pub fn empty_event_set_commitment_v1() -> [u8; 32] {
    sha256_array(EMPTY_EVENT_SET_DOMAIN_V1)
}

fn ensure_budget(field: &'static str, actual: u64, max: u64) -> Result<(), ContractStateV1Error> {
    if actual > max {
        return Err(ContractStateV1Error::ResourceBudgetExceeded { field, actual, max });
    }
    Ok(())
}

fn ensure_nonzero_commitment(
    field: &'static str,
    value: &[u8; 32],
) -> Result<(), ContractStateV1Error> {
    if value.iter().all(|byte| *byte == 0) {
        return Err(ContractStateV1Error::EmptyCommitment { field });
    }
    Ok(())
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ContractStateV1Error> {
    let len =
        u32::try_from(bytes.len()).map_err(|_| ContractStateV1Error::CanonicalLengthOverflow)?;
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
    use crate::contract_v3::{
        ProgrammabilityDomain, ProofCommitmentMetadataV1, ResourceBudgetV1,
        CONTRACT_TRANSACTION_VERSION_V3, PROGRAMMABILITY_IDENTITY_VERSION_V1,
        PROOF_COMMITMENT_METADATA_VERSION_V1, RESOURCE_BUDGET_VERSION_V1,
    };

    fn identity() -> ProgrammabilityActivationIdentity {
        ProgrammabilityActivationIdentity {
            version: PROGRAMMABILITY_IDENTITY_VERSION_V1,
            domain: ProgrammabilityDomain::Testnet,
            chain_id: "pulsedag-testnet-v3".into(),
            genesis_hash: [0x11; 32],
            base_protocol_fingerprint: [0x22; 32],
        }
    }

    fn external_evidence() -> ProofEvidenceV1 {
        ProofEvidenceV1 {
            version: PROOF_EVIDENCE_VERSION_V1,
            proof_system: PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1,
            proof_system_version: 1,
            verifier_revision: 3,
            evidence_generation: 5,
            evidence_payload_commitment: [0x66; 32],
        }
    }

    fn measured_usage() -> ContractResourceUsageV1 {
        ContractResourceUsageV1 {
            compute_units: 50_000,
            read_bytes: 1_024,
            write_bytes: 512,
            proof_bytes: 1_024,
            event_bytes: 128,
        }
    }

    fn validate_contract_state_transition_v1(
        envelope: &ContractTransactionV3Envelope,
        expected_identity: &ProgrammabilityActivationIdentity,
        expected_prior_state_commitment: [u8; 32],
        expected_replay_nonce: u64,
        measured_resource_usage: ContractResourceUsageV1,
        transition: &ContractStateTransitionV1,
        evidence_policy: &ProofEvidencePolicyV1,
    ) -> Result<ContractStateValidationV1, ContractStateV1Error> {
        super::validate_contract_state_transition_v1(
            envelope,
            expected_identity,
            expected_prior_state_commitment,
            expected_replay_nonce,
            empty_write_set_commitment_v1(),
            empty_event_set_commitment_v1(),
            measured_resource_usage,
            transition,
            evidence_policy,
        )
    }

    fn envelope() -> ContractTransactionV3Envelope {
        ContractTransactionV3Envelope {
            version: CONTRACT_TRANSACTION_VERSION_V3,
            identity: identity(),
            namespace: "payments.v1".into(),
            payload_commitment: [0x33; 32],
            state_commitment: [0x44; 32],
            authorization_commitment: [0x55; 32],
            proof: ProofCommitmentMetadataV1 {
                version: PROOF_COMMITMENT_METADATA_VERSION_V1,
                proof_system: PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1,
                proof_system_version: 1,
                proof_commitment: external_evidence().binding_commitment().unwrap(),
            },
            resources: ResourceBudgetV1 {
                version: RESOURCE_BUDGET_VERSION_V1,
                compute_units: 1_000_000,
                read_bytes: 65_536,
                write_bytes: 32_768,
                proof_bytes: 4_096,
                event_bytes: 2_048,
            },
            nonce: 7,
            replay_nonce: 9,
            covenant: None,
        }
    }

    fn policy(minimum_generation: u64) -> ProofEvidencePolicyV1 {
        ProofEvidencePolicyV1 {
            version: PROOF_EVIDENCE_POLICY_VERSION_V1,
            proof_system: PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1,
            proof_system_version: 1,
            verifier_revision: 3,
            minimum_valid_evidence_generation: minimum_generation,
        }
    }

    fn transition() -> ContractStateTransitionV1 {
        let envelope = envelope();
        ContractStateTransitionV1 {
            version: CONTRACT_STATE_TRANSITION_VERSION_V1,
            identity_fingerprint: envelope.identity.fingerprint().unwrap(),
            contract_txid: compute_contract_txid_v3(&envelope).unwrap(),
            prior_state_commitment: [0x77; 32],
            next_state_commitment: envelope.state_commitment,
            write_set_commitment: empty_write_set_commitment_v1(),
            event_set_commitment: empty_event_set_commitment_v1(),
            resource_usage: measured_usage(),
            replay_nonce: envelope.replay_nonce,
            proof_evidence: external_evidence(),
        }
    }

    #[test]
    fn valid_transition_binds_identity_txid_state_budget_replay_and_evidence() {
        let envelope = envelope();
        let transition = transition();
        let measured = measured_usage();
        let result = validate_contract_state_transition_v1(
            &envelope,
            &identity(),
            [0x77; 32],
            9,
            measured,
            &transition,
            &policy(4),
        )
        .unwrap();
        assert_eq!(result.replay_nonce, 9);
        assert_eq!(result.resource_usage, measured);
        assert_eq!(result.write_set_commitment, empty_write_set_commitment_v1());
        assert_eq!(result.event_set_commitment, empty_event_set_commitment_v1());
        assert_eq!(
            result.transition_commitment,
            transition.commitment().unwrap()
        );
    }

    #[test]
    fn identity_txid_state_and_replay_substitution_fail_closed() {
        let envelope = envelope();
        let measured = measured_usage();

        let mut invalid = transition();
        invalid.identity_fingerprint[0] ^= 1;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &invalid,
                &policy(4),
            ),
            Err(ContractStateV1Error::IdentityFingerprintMismatch)
        ));

        let mut invalid = transition();
        invalid.contract_txid[0] ^= 1;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &invalid,
                &policy(4),
            ),
            Err(ContractStateV1Error::ContractTxidMismatch)
        ));

        let mut invalid = transition();
        invalid.next_state_commitment[0] ^= 1;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &invalid,
                &policy(4),
            ),
            Err(ContractStateV1Error::NextStateMismatch)
        ));

        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x78; 32],
                9,
                measured,
                &transition(),
                &policy(4),
            ),
            Err(ContractStateV1Error::PriorStateMismatch)
        ));
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                10,
                measured,
                &transition(),
                &policy(4),
            ),
            Err(ContractStateV1Error::ReplayNonceMismatch)
        ));
    }

    #[test]
    fn write_and_event_commitments_must_match_deterministic_measurements() {
        let envelope = envelope();
        let measured_write = empty_write_set_commitment_v1();
        let measured_event = empty_event_set_commitment_v1();
        let measured = measured_usage();

        let mut invalid_write = transition();
        invalid_write.write_set_commitment = [0x88; 32];
        assert!(matches!(
            super::validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured_write,
                measured_event,
                measured,
                &invalid_write,
                &policy(4),
            ),
            Err(ContractStateV1Error::WriteSetCommitmentMismatch)
        ));

        let mut invalid_event = transition();
        invalid_event.event_set_commitment = [0x99; 32];
        assert!(matches!(
            super::validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured_write,
                measured_event,
                measured,
                &invalid_event,
                &policy(4),
            ),
            Err(ContractStateV1Error::EventSetCommitmentMismatch)
        ));

        assert_eq!(
            ContractStateV1Error::WriteSetCommitmentMismatch.rejection_code(),
            19
        );
        assert_eq!(
            ContractStateV1Error::EventSetCommitmentMismatch.rejection_code(),
            20
        );
    }

    #[test]
    fn resource_accounting_enforces_every_envelope_budget() {
        let budget = envelope().resources;
        let cases = [
            (
                "compute_units",
                ContractResourceUsageV1 {
                    compute_units: budget.compute_units + 1,
                    ..Default::default()
                },
            ),
            (
                "read_bytes",
                ContractResourceUsageV1 {
                    read_bytes: budget.read_bytes + 1,
                    ..Default::default()
                },
            ),
            (
                "write_bytes",
                ContractResourceUsageV1 {
                    write_bytes: budget.write_bytes + 1,
                    ..Default::default()
                },
            ),
            (
                "proof_bytes",
                ContractResourceUsageV1 {
                    proof_bytes: budget.proof_bytes + 1,
                    ..Default::default()
                },
            ),
            (
                "event_bytes",
                ContractResourceUsageV1 {
                    event_bytes: budget.event_bytes + 1,
                    ..Default::default()
                },
            ),
        ];

        for (field, usage) in cases {
            assert!(matches!(
                usage.validate_against_budget(&budget),
                Err(ContractStateV1Error::ResourceBudgetExceeded {
                    field: actual_field,
                    ..
                }) if actual_field == field
            ));
        }
    }

    #[test]
    fn claimed_resource_usage_must_match_deterministic_measurement() {
        let envelope = envelope();
        let mut transition = transition();
        transition.resource_usage = ContractResourceUsageV1::default();
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured_usage(),
                &transition,
                &policy(4),
            ),
            Err(ContractStateV1Error::ResourceUsageMismatch)
        ));
        assert_eq!(
            ContractStateV1Error::ResourceUsageMismatch.rejection_code(),
            18
        );
    }

    #[test]
    fn measured_resource_usage_must_respect_envelope_budget() {
        let envelope = envelope();
        let mut transition = transition();
        let measured = ContractResourceUsageV1 {
            compute_units: envelope.resources.compute_units + 1,
            ..measured_usage()
        };
        transition.resource_usage = measured;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &transition,
                &policy(4),
            ),
            Err(ContractStateV1Error::ResourceBudgetExceeded {
                field: "compute_units",
                ..
            })
        ));
    }

    #[test]
    fn evidence_generation_and_revision_invalidation_are_deterministic() {
        let envelope = envelope();
        let transition = transition();
        let measured = measured_usage();
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &transition,
                &policy(6),
            ),
            Err(ContractStateV1Error::EvidenceInvalidated)
        ));

        let mut changed_policy = policy(4);
        changed_policy.verifier_revision = 4;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &transition,
                &changed_policy,
            ),
            Err(ContractStateV1Error::EvidencePolicyMismatch)
        ));
    }

    #[test]
    fn evidence_generation_and_revision_are_bound_to_envelope_commitment() {
        let envelope = envelope();
        let measured = measured_usage();

        let mut relabeled_generation = transition();
        relabeled_generation.proof_evidence.evidence_generation = 6;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &relabeled_generation,
                &policy(6),
            ),
            Err(ContractStateV1Error::EvidenceCommitmentMismatch)
        ));

        let mut relabeled_revision = transition();
        relabeled_revision.proof_evidence.verifier_revision = 4;
        let mut revised_policy = policy(4);
        revised_policy.verifier_revision = 4;
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured,
                &relabeled_revision,
                &revised_policy,
            ),
            Err(ContractStateV1Error::EvidenceCommitmentMismatch)
        ));
    }

    #[test]
    fn proof_evidence_payload_commitment_must_match_contract_envelope_binding() {
        let envelope = envelope();
        let mut transition = transition();
        transition.proof_evidence.evidence_payload_commitment = [0x67; 32];
        assert!(matches!(
            validate_contract_state_transition_v1(
                &envelope,
                &identity(),
                [0x77; 32],
                9,
                measured_usage(),
                &transition,
                &policy(4),
            ),
            Err(ContractStateV1Error::EvidenceCommitmentMismatch)
        ));
    }

    #[test]
    fn no_proof_system_has_exact_zero_evidence_contract() {
        let proof = ProofCommitmentMetadataV1 {
            version: PROOF_COMMITMENT_METADATA_VERSION_V1,
            proof_system: PROOF_SYSTEM_NONE,
            proof_system_version: 0,
            proof_commitment: [0; 32],
        };
        let evidence = ProofEvidenceV1 {
            version: PROOF_EVIDENCE_VERSION_V1,
            proof_system: PROOF_SYSTEM_NONE,
            proof_system_version: 0,
            verifier_revision: 0,
            evidence_generation: 0,
            evidence_payload_commitment: [0; 32],
        };
        let policy = ProofEvidencePolicyV1 {
            version: PROOF_EVIDENCE_POLICY_VERSION_V1,
            proof_system: PROOF_SYSTEM_NONE,
            proof_system_version: 0,
            verifier_revision: 0,
            minimum_valid_evidence_generation: 0,
        };
        assert_eq!(evidence.binding_commitment().unwrap(), [0; 32]);
        assert!(policy.validate_evidence(&proof, &evidence).is_ok());
    }

    #[test]
    fn unsupported_versions_fail_closed_with_stable_codes() {
        let mut invalid_transition = transition();
        invalid_transition.version = 2;
        assert_eq!(
            invalid_transition
                .validate_shape()
                .unwrap_err()
                .rejection_code(),
            2
        );

        let mut evidence = transition().proof_evidence;
        evidence.version = 2;
        assert_eq!(evidence.validate().unwrap_err().rejection_code(), 3);

        let mut policy = policy(4);
        policy.version = 2;
        assert_eq!(policy.validate().unwrap_err().rejection_code(), 4);
    }

    #[test]
    fn transition_commitment_golden_vector_is_frozen() {
        assert_eq!(
            hex::encode(external_evidence().binding_commitment().unwrap()),
            "dc5741b0b91db0b890b10be690e153ca7783f873a28fdbb09552c465f176dc9c"
        );
        assert_eq!(
            hex::encode(transition().commitment().unwrap()),
            "bf1a81ae3e26c0a40a2b3e6acc645d38663380eaeaaaa60a2cf1214cb2e50440"
        );
        assert_eq!(
            hex::encode(empty_write_set_commitment_v1()),
            "e8181adc9a6964ee50c2afe73b5f6ca8c3945284b8016af8dac0e38383e11b1d"
        );
        assert_eq!(
            hex::encode(empty_event_set_commitment_v1()),
            "f5e7cd6592dcb69450651ce5e9e63b59ba789243057f371ca760b3180aac1a78"
        );
    }

    #[test]
    fn transition_encoding_and_serde_rejection_are_replay_stable() {
        let transition = transition();
        let first = transition.canonical_bytes().unwrap();
        let encoded = serde_json::to_vec(&transition).unwrap();
        let decoded: ContractStateTransitionV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(first, decoded.canonical_bytes().unwrap());
        assert_eq!(
            transition.commitment().unwrap(),
            decoded.commitment().unwrap()
        );

        let mut value = serde_json::to_value(&transition).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::json!(1));
        assert!(serde_json::from_value::<ContractStateTransitionV1>(value).is_err());
    }
}
