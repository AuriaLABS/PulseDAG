use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PROGRAMMABILITY_IDENTITY_VERSION_V1: u16 = 1;
pub const CONTRACT_TRANSACTION_VERSION_V3: u16 = 3;
pub const PROOF_COMMITMENT_METADATA_VERSION_V1: u16 = 1;
pub const RESOURCE_BUDGET_VERSION_V1: u16 = 1;
pub const COVENANT_DESCRIPTOR_VERSION_V1: u16 = 1;
pub const CONTRACT_V3_COMPATIBILITY_SCHEMA_VERSION: u16 = 1;

pub const PROOF_SYSTEM_NONE: u16 = 0;
pub const PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1: u16 = 1;

pub const MAX_PROGRAMMABILITY_CHAIN_ID_BYTES: usize = 64;
pub const MAX_CONTRACT_NAMESPACE_BYTES: usize = 64;
pub const MAX_COVENANT_WITNESS_BYTES: u32 = 64 * 1024;

pub const CONTRACT_V3_MAX_COMPUTE_UNITS: u64 = 10_000_000;
pub const CONTRACT_V3_MAX_READ_BYTES: u64 = 16 * 1024 * 1024;
pub const CONTRACT_V3_MAX_WRITE_BYTES: u64 = 4 * 1024 * 1024;
pub const CONTRACT_V3_MAX_PROOF_BYTES: u32 = 8 * 1024 * 1024;
pub const CONTRACT_V3_MAX_EVENT_BYTES: u32 = 1024 * 1024;

const PROGRAMMABILITY_IDENTITY_DOMAIN: &[u8] = b"PulseDAG:programmability-identity:v1";
const CONTRACT_ENVELOPE_DOMAIN: &[u8] = b"PulseDAG:contract-envelope:v3";
const CONTRACT_SIGNING_DOMAIN: &[u8] = b"PulseDAG:contract-signing:v3";
const CONTRACT_TXID_DOMAIN: &[u8] = b"PulseDAG:contract-txid:v3";
const CONTRACT_COMPATIBILITY_DOMAIN: &[u8] = b"PulseDAG:contract-compatibility:v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContractV3Error {
    #[error("unsupported programmability identity version {0}")]
    UnsupportedIdentityVersion(u16),
    #[error("application programmability domain requires a non-zero application id")]
    InvalidApplicationDomain,
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} length {actual} exceeds maximum {max}")]
    FieldTooLong {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("{field} contains unsupported byte 0x{byte:02x}")]
    InvalidFieldByte { field: &'static str, byte: u8 },
    #[error("programmability identity mismatch")]
    IdentityMismatch,
    #[error("unsupported contract transaction version {0}")]
    UnsupportedContractTransactionVersion(u16),
    #[error("unsupported proof metadata version {0}")]
    UnsupportedProofMetadataVersion(u16),
    #[error("unsupported proof system {0}")]
    UnsupportedProofSystem(u16),
    #[error("invalid proof-system version {actual} for proof system {system}")]
    InvalidProofSystemVersion { system: u16, actual: u16 },
    #[error("invalid proof commitment for proof system {0}")]
    InvalidProofCommitment(u16),
    #[error("unsupported resource budget version {0}")]
    UnsupportedResourceBudgetVersion(u16),
    #[error("{field} resource limit {actual} exceeds maximum {max}")]
    ResourceLimitExceeded {
        field: &'static str,
        actual: u64,
        max: u64,
    },
    #[error("unsupported covenant descriptor version {0}")]
    UnsupportedCovenantVersion(u16),
    #[error("covenant witness budget {actual} exceeds maximum {max}")]
    CovenantWitnessBudgetExceeded { actual: u32, max: u32 },
    #[error("covenant parameters commitment must not be all zero")]
    EmptyCovenantParametersCommitment,
    #[error("covenant kind {0:?} is reserved but not executable in the #1041 foundation")]
    UnsupportedCovenantKind(CovenantKindV1),
    #[error("{field} commitment must not be all zero")]
    EmptyCommitment { field: &'static str },
    #[error("canonical field length exceeds u32::MAX")]
    CanonicalLengthOverflow,
    #[error("unsupported contract compatibility schema version {0}")]
    UnsupportedCompatibilitySchemaVersion(u16),
    #[error("contract compatibility metadata contains an unexpected frozen version")]
    CompatibilityVersionMismatch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgrammabilityDomain {
    Mainnet,
    Testnet,
    Application(u32),
}

impl ProgrammabilityDomain {
    pub fn validate(self) -> Result<(), ContractV3Error> {
        if matches!(self, Self::Application(0)) {
            return Err(ContractV3Error::InvalidApplicationDomain);
        }
        Ok(())
    }

    fn encode_canonical(self, out: &mut Vec<u8>) {
        match self {
            Self::Mainnet => out.push(1),
            Self::Testnet => out.push(2),
            Self::Application(application_id) => {
                out.push(3);
                out.extend_from_slice(&application_id.to_le_bytes());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgrammabilityActivationIdentity {
    pub version: u16,
    pub domain: ProgrammabilityDomain,
    #[serde(deserialize_with = "deserialize_chain_id")]
    pub chain_id: String,
    pub genesis_hash: [u8; 32],
    pub base_protocol_fingerprint: [u8; 32],
}

impl ProgrammabilityActivationIdentity {
    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.version != PROGRAMMABILITY_IDENTITY_VERSION_V1 {
            return Err(ContractV3Error::UnsupportedIdentityVersion(self.version));
        }
        self.domain.validate()?;
        validate_chain_id(&self.chain_id)?;
        ensure_nonzero_commitment("genesis_hash", &self.genesis_hash)?;
        ensure_nonzero_commitment(
            "base_protocol_fingerprint",
            &self.base_protocol_fingerprint,
        )?;
        Ok(())
    }

    pub fn validate_against(
        &self,
        expected: &ProgrammabilityActivationIdentity,
    ) -> Result<(), ContractV3Error> {
        self.validate()?;
        expected.validate()?;
        if self != expected {
            return Err(ContractV3Error::IdentityMismatch);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractV3Error> {
        canonical_programmability_identity_bytes_v1(self)
    }

    pub fn fingerprint(&self) -> Result<[u8; 32], ContractV3Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProofCommitmentMetadataV1 {
    pub version: u16,
    pub proof_system: u16,
    pub proof_system_version: u16,
    pub proof_commitment: [u8; 32],
}

impl ProofCommitmentMetadataV1 {
    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.version != PROOF_COMMITMENT_METADATA_VERSION_V1 {
            return Err(ContractV3Error::UnsupportedProofMetadataVersion(
                self.version,
            ));
        }

        match self.proof_system {
            PROOF_SYSTEM_NONE => {
                if self.proof_system_version != 0 {
                    return Err(ContractV3Error::InvalidProofSystemVersion {
                        system: self.proof_system,
                        actual: self.proof_system_version,
                    });
                }
                if self.proof_commitment.iter().any(|byte| *byte != 0) {
                    return Err(ContractV3Error::InvalidProofCommitment(
                        self.proof_system,
                    ));
                }
            }
            PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1 => {
                if self.proof_system_version != 1 {
                    return Err(ContractV3Error::InvalidProofSystemVersion {
                        system: self.proof_system,
                        actual: self.proof_system_version,
                    });
                }
                if self.proof_commitment.iter().all(|byte| *byte == 0) {
                    return Err(ContractV3Error::InvalidProofCommitment(
                        self.proof_system,
                    ));
                }
            }
            unknown => return Err(ContractV3Error::UnsupportedProofSystem(unknown)),
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceBudgetV1 {
    pub version: u16,
    pub compute_units: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub proof_bytes: u32,
    pub event_bytes: u32,
}

impl ResourceBudgetV1 {
    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.version != RESOURCE_BUDGET_VERSION_V1 {
            return Err(ContractV3Error::UnsupportedResourceBudgetVersion(
                self.version,
            ));
        }
        ensure_resource_limit(
            "compute_units",
            self.compute_units,
            CONTRACT_V3_MAX_COMPUTE_UNITS,
        )?;
        ensure_resource_limit("read_bytes", self.read_bytes, CONTRACT_V3_MAX_READ_BYTES)?;
        ensure_resource_limit(
            "write_bytes",
            self.write_bytes,
            CONTRACT_V3_MAX_WRITE_BYTES,
        )?;
        ensure_resource_limit(
            "proof_bytes",
            u64::from(self.proof_bytes),
            u64::from(CONTRACT_V3_MAX_PROOF_BYTES),
        )?;
        ensure_resource_limit(
            "event_bytes",
            u64::from(self.event_bytes),
            u64::from(CONTRACT_V3_MAX_EVENT_BYTES),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u16)]
pub enum CovenantKindV1 {
    Timelock = 1,
    Vault = 2,
    Multisig = 3,
    Escrow = 4,
    AtomicSwap = 5,
    PaymentChannel = 6,
}

impl CovenantKindV1 {
    fn canonical_id(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantDescriptorV1 {
    pub version: u16,
    pub kind: CovenantKindV1,
    pub parameters_commitment: [u8; 32],
    pub witness_budget_bytes: u32,
}

impl CovenantDescriptorV1 {
    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.version != COVENANT_DESCRIPTOR_VERSION_V1 {
            return Err(ContractV3Error::UnsupportedCovenantVersion(self.version));
        }
        if self.witness_budget_bytes > MAX_COVENANT_WITNESS_BYTES {
            return Err(ContractV3Error::CovenantWitnessBudgetExceeded {
                actual: self.witness_budget_bytes,
                max: MAX_COVENANT_WITNESS_BYTES,
            });
        }
        if self.parameters_commitment.iter().all(|byte| *byte == 0) {
            return Err(ContractV3Error::EmptyCovenantParametersCommitment);
        }
        Ok(())
    }

    pub fn reject_unsupported_execution(&self) -> Result<(), ContractV3Error> {
        self.validate()?;
        Err(ContractV3Error::UnsupportedCovenantKind(self.kind))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractTransactionV3Envelope {
    pub version: u16,
    pub identity: ProgrammabilityActivationIdentity,
    #[serde(deserialize_with = "deserialize_namespace")]
    pub namespace: String,
    pub payload_commitment: [u8; 32],
    pub state_commitment: [u8; 32],
    pub authorization_commitment: [u8; 32],
    pub proof: ProofCommitmentMetadataV1,
    pub resources: ResourceBudgetV1,
    pub nonce: u64,
    pub replay_nonce: u64,
    pub covenant: Option<CovenantDescriptorV1>,
}

impl ContractTransactionV3Envelope {
    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.version != CONTRACT_TRANSACTION_VERSION_V3 {
            return Err(ContractV3Error::UnsupportedContractTransactionVersion(
                self.version,
            ));
        }
        self.identity.validate()?;
        validate_namespace(&self.namespace)?;
        ensure_nonzero_commitment("payload_commitment", &self.payload_commitment)?;
        ensure_nonzero_commitment("state_commitment", &self.state_commitment)?;
        ensure_nonzero_commitment(
            "authorization_commitment",
            &self.authorization_commitment,
        )?;
        self.proof.validate()?;
        self.resources.validate()?;
        if let Some(covenant) = &self.covenant {
            covenant.validate()?;
        }
        Ok(())
    }

    pub fn validate_identity(
        &self,
        expected: &ProgrammabilityActivationIdentity,
    ) -> Result<(), ContractV3Error> {
        self.validate()?;
        self.identity.validate_against(expected)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractV3Error> {
        canonical_contract_envelope_bytes_v3(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractV3CompatibilityMetadataV1 {
    pub schema_version: u16,
    pub identity_fingerprint: [u8; 32],
    pub contract_transaction_version: u16,
    pub proof_metadata_version: u16,
    pub resource_budget_version: u16,
    pub covenant_descriptor_version: u16,
}

impl ContractV3CompatibilityMetadataV1 {
    pub fn from_identity(
        identity: &ProgrammabilityActivationIdentity,
    ) -> Result<Self, ContractV3Error> {
        Ok(Self {
            schema_version: CONTRACT_V3_COMPATIBILITY_SCHEMA_VERSION,
            identity_fingerprint: identity.fingerprint()?,
            contract_transaction_version: CONTRACT_TRANSACTION_VERSION_V3,
            proof_metadata_version: PROOF_COMMITMENT_METADATA_VERSION_V1,
            resource_budget_version: RESOURCE_BUDGET_VERSION_V1,
            covenant_descriptor_version: COVENANT_DESCRIPTOR_VERSION_V1,
        })
    }

    pub fn validate(&self) -> Result<(), ContractV3Error> {
        if self.schema_version != CONTRACT_V3_COMPATIBILITY_SCHEMA_VERSION {
            return Err(ContractV3Error::UnsupportedCompatibilitySchemaVersion(
                self.schema_version,
            ));
        }
        if self.contract_transaction_version != CONTRACT_TRANSACTION_VERSION_V3
            || self.proof_metadata_version != PROOF_COMMITMENT_METADATA_VERSION_V1
            || self.resource_budget_version != RESOURCE_BUDGET_VERSION_V1
            || self.covenant_descriptor_version != COVENANT_DESCRIPTOR_VERSION_V1
        {
            return Err(ContractV3Error::CompatibilityVersionMismatch);
        }
        ensure_nonzero_commitment("identity_fingerprint", &self.identity_fingerprint)?;
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<[u8; 32], ContractV3Error> {
        Ok(sha256_array(
            &canonical_contract_v3_compatibility_metadata_bytes_v1(self)?,
        ))
    }
}

pub fn canonical_programmability_identity_bytes_v1(
    identity: &ProgrammabilityActivationIdentity,
) -> Result<Vec<u8>, ContractV3Error> {
    identity.validate()?;

    let mut out = Vec::with_capacity(160);
    encode_len_prefixed(&mut out, PROGRAMMABILITY_IDENTITY_DOMAIN)?;
    out.extend_from_slice(&identity.version.to_le_bytes());
    identity.domain.encode_canonical(&mut out);
    encode_len_prefixed(&mut out, identity.chain_id.as_bytes())?;
    out.extend_from_slice(&identity.genesis_hash);
    out.extend_from_slice(&identity.base_protocol_fingerprint);
    Ok(out)
}

pub fn canonical_contract_envelope_bytes_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<Vec<u8>, ContractV3Error> {
    envelope.validate()?;

    let identity_bytes = envelope.identity.canonical_bytes()?;
    let mut out = Vec::with_capacity(512);
    encode_len_prefixed(&mut out, CONTRACT_ENVELOPE_DOMAIN)?;
    out.extend_from_slice(&envelope.version.to_le_bytes());
    encode_len_prefixed(&mut out, &identity_bytes)?;
    encode_len_prefixed(&mut out, envelope.namespace.as_bytes())?;
    out.extend_from_slice(&envelope.payload_commitment);
    out.extend_from_slice(&envelope.state_commitment);
    out.extend_from_slice(&envelope.authorization_commitment);

    out.extend_from_slice(&envelope.proof.version.to_le_bytes());
    out.extend_from_slice(&envelope.proof.proof_system.to_le_bytes());
    out.extend_from_slice(&envelope.proof.proof_system_version.to_le_bytes());
    out.extend_from_slice(&envelope.proof.proof_commitment);

    out.extend_from_slice(&envelope.resources.version.to_le_bytes());
    out.extend_from_slice(&envelope.resources.compute_units.to_le_bytes());
    out.extend_from_slice(&envelope.resources.read_bytes.to_le_bytes());
    out.extend_from_slice(&envelope.resources.write_bytes.to_le_bytes());
    out.extend_from_slice(&envelope.resources.proof_bytes.to_le_bytes());
    out.extend_from_slice(&envelope.resources.event_bytes.to_le_bytes());

    out.extend_from_slice(&envelope.nonce.to_le_bytes());
    out.extend_from_slice(&envelope.replay_nonce.to_le_bytes());

    match &envelope.covenant {
        None => out.push(0),
        Some(covenant) => {
            out.push(1);
            out.extend_from_slice(&covenant.version.to_le_bytes());
            out.extend_from_slice(&covenant.kind.canonical_id().to_le_bytes());
            out.extend_from_slice(&covenant.parameters_commitment);
            out.extend_from_slice(&covenant.witness_budget_bytes.to_le_bytes());
        }
    }

    Ok(out)
}

pub fn canonical_contract_signing_preimage_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<Vec<u8>, ContractV3Error> {
    let envelope_bytes = envelope.canonical_bytes()?;
    let identity_fingerprint = envelope.identity.fingerprint()?;

    let mut out = Vec::with_capacity(envelope_bytes.len() + 96);
    encode_len_prefixed(&mut out, CONTRACT_SIGNING_DOMAIN)?;
    out.extend_from_slice(&identity_fingerprint);
    encode_len_prefixed(&mut out, &envelope_bytes)?;
    Ok(out)
}

pub fn canonical_contract_txid_preimage_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<Vec<u8>, ContractV3Error> {
    let envelope_bytes = envelope.canonical_bytes()?;

    let mut out = Vec::with_capacity(envelope_bytes.len() + 64);
    encode_len_prefixed(&mut out, CONTRACT_TXID_DOMAIN)?;
    encode_len_prefixed(&mut out, &envelope_bytes)?;
    Ok(out)
}

pub fn contract_envelope_hash_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<[u8; 32], ContractV3Error> {
    Ok(sha256_array(&envelope.canonical_bytes()?))
}

pub fn contract_signing_digest_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<[u8; 32], ContractV3Error> {
    Ok(sha256_array(&canonical_contract_signing_preimage_v3(
        envelope,
    )?))
}

pub fn compute_contract_txid_v3(
    envelope: &ContractTransactionV3Envelope,
) -> Result<[u8; 32], ContractV3Error> {
    Ok(sha256_array(&canonical_contract_txid_preimage_v3(
        envelope,
    )?))
}

pub fn canonical_contract_v3_compatibility_metadata_bytes_v1(
    metadata: &ContractV3CompatibilityMetadataV1,
) -> Result<Vec<u8>, ContractV3Error> {
    metadata.validate()?;

    let mut out = Vec::with_capacity(96);
    encode_len_prefixed(&mut out, CONTRACT_COMPATIBILITY_DOMAIN)?;
    out.extend_from_slice(&metadata.schema_version.to_le_bytes());
    out.extend_from_slice(&metadata.identity_fingerprint);
    out.extend_from_slice(&metadata.contract_transaction_version.to_le_bytes());
    out.extend_from_slice(&metadata.proof_metadata_version.to_le_bytes());
    out.extend_from_slice(&metadata.resource_budget_version.to_le_bytes());
    out.extend_from_slice(&metadata.covenant_descriptor_version.to_le_bytes());
    Ok(out)
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ContractV3Error> {
    let len = u32::try_from(bytes.len()).map_err(|_| ContractV3Error::CanonicalLengthOverflow)?;
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

fn ensure_nonzero_commitment(
    field: &'static str,
    value: &[u8; 32],
) -> Result<(), ContractV3Error> {
    if value.iter().all(|byte| *byte == 0) {
        return Err(ContractV3Error::EmptyCommitment { field });
    }
    Ok(())
}

fn ensure_resource_limit(
    field: &'static str,
    actual: u64,
    max: u64,
) -> Result<(), ContractV3Error> {
    if actual > max {
        return Err(ContractV3Error::ResourceLimitExceeded { field, actual, max });
    }
    Ok(())
}

fn validate_chain_id(value: &str) -> Result<(), ContractV3Error> {
    validate_text_length(
        value,
        "programmability chain_id",
        MAX_PROGRAMMABILITY_CHAIN_ID_BYTES,
    )?;
    if let Some(byte) = value.bytes().find(|byte| {
        !matches!(
            byte,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b':'
        )
    }) {
        return Err(ContractV3Error::InvalidFieldByte {
            field: "programmability chain_id",
            byte,
        });
    }
    Ok(())
}

fn validate_namespace(value: &str) -> Result<(), ContractV3Error> {
    validate_text_length(value, "contract namespace", MAX_CONTRACT_NAMESPACE_BYTES)?;
    if let Some(byte) = value.bytes().find(|byte| {
        !matches!(
            byte,
            b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'/'
                | b':'
        )
    }) {
        return Err(ContractV3Error::InvalidFieldByte {
            field: "contract namespace",
            byte,
        });
    }
    Ok(())
}

fn validate_text_length(
    value: &str,
    field: &'static str,
    max: usize,
) -> Result<(), ContractV3Error> {
    if value.is_empty() {
        return Err(ContractV3Error::EmptyField { field });
    }
    if value.len() > max {
        return Err(ContractV3Error::FieldTooLong {
            field,
            actual: value.len(),
            max,
        });
    }
    Ok(())
}

fn deserialize_chain_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_chain_id(&value).map_err(D::Error::custom)?;
    Ok(value)
}

fn deserialize_namespace<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_namespace(&value).map_err(D::Error::custom)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_identity(domain: ProgrammabilityDomain) -> ProgrammabilityActivationIdentity {
        ProgrammabilityActivationIdentity {
            version: PROGRAMMABILITY_IDENTITY_VERSION_V1,
            domain,
            chain_id: "pulsedag-testnet-v3".into(),
            genesis_hash: [0x11; 32],
            base_protocol_fingerprint: [0x22; 32],
        }
    }

    fn sample_envelope() -> ContractTransactionV3Envelope {
        ContractTransactionV3Envelope {
            version: CONTRACT_TRANSACTION_VERSION_V3,
            identity: sample_identity(ProgrammabilityDomain::Testnet),
            namespace: "payments.v1".into(),
            payload_commitment: [0x33; 32],
            state_commitment: [0x44; 32],
            authorization_commitment: [0x55; 32],
            proof: ProofCommitmentMetadataV1 {
                version: PROOF_COMMITMENT_METADATA_VERSION_V1,
                proof_system: PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1,
                proof_system_version: 1,
                proof_commitment: [0x66; 32],
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

    fn hex_digest(value: [u8; 32]) -> String {
        hex::encode(value)
    }

    #[test]
    fn contract_v3_golden_vectors_are_frozen() {
        let envelope = sample_envelope();
        assert_eq!(
            hex_digest(envelope.identity.fingerprint().unwrap()),
            "59cb43d2b3cf6bde15c04651572415fa8503d83445820c8ebde3b877f84859df"
        );
        assert_eq!(
            hex_digest(contract_envelope_hash_v3(&envelope).unwrap()),
            "1a17f90e82ad12b7ef7f37c41589a5a9cbb08ecee1e24c119d7d390b0bd9f611"
        );
        assert_eq!(
            hex_digest(contract_signing_digest_v3(&envelope).unwrap()),
            "1579121abe104cefc3a2bdc37191ca24697a3e22dc980026925e333dc9f2d9c0"
        );
        assert_eq!(
            hex_digest(compute_contract_txid_v3(&envelope).unwrap()),
            "cded937ad08c7ba7cdb6beb35ac922376a9810bb57a3a03ac6d8583ddbeffdb2"
        );
    }

    #[test]
    fn canonical_encoding_is_replay_stable_across_serde_roundtrip() {
        let envelope = sample_envelope();
        let expected = envelope.canonical_bytes().unwrap();

        let encoded = serde_json::to_vec(&envelope).unwrap();
        let decoded: ContractTransactionV3Envelope = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(expected, decoded.canonical_bytes().unwrap());
        assert_eq!(
            contract_signing_digest_v3(&envelope).unwrap(),
            contract_signing_digest_v3(&decoded).unwrap()
        );
        assert_eq!(
            compute_contract_txid_v3(&envelope).unwrap(),
            compute_contract_txid_v3(&decoded).unwrap()
        );
    }

    #[test]
    fn network_and_application_domains_are_distinct() {
        let mainnet = sample_identity(ProgrammabilityDomain::Mainnet);
        let testnet = sample_identity(ProgrammabilityDomain::Testnet);
        let application = sample_identity(ProgrammabilityDomain::Application(7));

        assert_ne!(mainnet.fingerprint().unwrap(), testnet.fingerprint().unwrap());
        assert_ne!(
            mainnet.fingerprint().unwrap(),
            application.fingerprint().unwrap()
        );
        assert_ne!(
            testnet.fingerprint().unwrap(),
            application.fingerprint().unwrap()
        );
    }

    #[test]
    fn cross_chain_substitution_fails_closed() {
        let envelope = sample_envelope();
        let mut expected = envelope.identity.clone();
        expected.chain_id = "pulsedag-other-v3".into();

        assert_eq!(
            envelope.validate_identity(&expected),
            Err(ContractV3Error::IdentityMismatch)
        );
    }

    #[test]
    fn unknown_versions_fail_closed() {
        let mut identity = sample_identity(ProgrammabilityDomain::Testnet);
        identity.version = 9;
        assert!(matches!(
            identity.validate(),
            Err(ContractV3Error::UnsupportedIdentityVersion(9))
        ));

        let mut envelope = sample_envelope();
        envelope.version = 4;
        assert!(matches!(
            envelope.validate(),
            Err(ContractV3Error::UnsupportedContractTransactionVersion(4))
        ));

        let mut resources = sample_envelope().resources;
        resources.version = 2;
        assert!(matches!(
            resources.validate(),
            Err(ContractV3Error::UnsupportedResourceBudgetVersion(2))
        ));
    }

    #[test]
    fn unknown_domain_is_rejected_by_strict_serde() {
        let mut value = serde_json::to_value(sample_identity(ProgrammabilityDomain::Testnet)).unwrap();
        value["domain"] = json!("future_network");

        assert!(serde_json::from_value::<ProgrammabilityActivationIdentity>(value).is_err());
    }

    #[test]
    fn oversized_namespace_and_resources_are_rejected() {
        let mut envelope = sample_envelope();
        envelope.namespace = "a".repeat(MAX_CONTRACT_NAMESPACE_BYTES + 1);
        assert!(matches!(
            envelope.validate(),
            Err(ContractV3Error::FieldTooLong {
                field: "contract namespace",
                ..
            })
        ));

        let mut envelope = sample_envelope();
        envelope.resources.compute_units = CONTRACT_V3_MAX_COMPUTE_UNITS + 1;
        assert!(matches!(
            envelope.validate(),
            Err(ContractV3Error::ResourceLimitExceeded {
                field: "compute_units",
                ..
            })
        ));
    }

    #[test]
    fn proof_metadata_is_versioned_and_fail_closed() {
        let mut proof = sample_envelope().proof;
        proof.proof_system = 99;
        assert_eq!(
            proof.validate(),
            Err(ContractV3Error::UnsupportedProofSystem(99))
        );

        let mut proof = sample_envelope().proof;
        proof.proof_system_version = 2;
        assert_eq!(
            proof.validate(),
            Err(ContractV3Error::InvalidProofSystemVersion {
                system: PROOF_SYSTEM_EXTERNAL_COMMITMENT_V1,
                actual: 2,
            })
        );
    }

    #[test]
    fn covenant_v1_kinds_are_bounded_but_execution_is_unsupported() {
        let descriptor = CovenantDescriptorV1 {
            version: COVENANT_DESCRIPTOR_VERSION_V1,
            kind: CovenantKindV1::Timelock,
            parameters_commitment: [0x77; 32],
            witness_budget_bytes: 1_024,
        };
        assert!(descriptor.validate().is_ok());
        assert_eq!(
            descriptor.reject_unsupported_execution(),
            Err(ContractV3Error::UnsupportedCovenantKind(
                CovenantKindV1::Timelock
            ))
        );

        let mut value = serde_json::to_value(&descriptor).unwrap();
        value["kind"] = json!("future_covenant");
        assert!(serde_json::from_value::<CovenantDescriptorV1>(value).is_err());
    }

    #[test]
    fn strict_serde_rejects_unknown_fields_and_oversized_namespace() {
        let mut value = serde_json::to_value(sample_envelope()).unwrap();
        value["future_field"] = json!(1);
        assert!(serde_json::from_value::<ContractTransactionV3Envelope>(value).is_err());

        let mut value = serde_json::to_value(sample_envelope()).unwrap();
        value["namespace"] = json!("a".repeat(MAX_CONTRACT_NAMESPACE_BYTES + 1));
        assert!(serde_json::from_value::<ContractTransactionV3Envelope>(value).is_err());
    }

    #[test]
    fn compatibility_metadata_binds_identity_and_frozen_versions() {
        let identity = sample_identity(ProgrammabilityDomain::Testnet);
        let metadata = ContractV3CompatibilityMetadataV1::from_identity(&identity).unwrap();
        assert!(metadata.validate().is_ok());

        let mut other_identity = identity;
        other_identity.chain_id = "pulsedag-other-v3".into();
        let other_metadata =
            ContractV3CompatibilityMetadataV1::from_identity(&other_identity).unwrap();

        assert_ne!(metadata.fingerprint().unwrap(), other_metadata.fingerprint().unwrap());
    }
}
