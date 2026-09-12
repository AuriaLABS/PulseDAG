use crate::contract_v3::{
    ContractV3Error, CovenantDescriptorV1, CovenantKindV1, COVENANT_DESCRIPTOR_VERSION_V1,
    MAX_COVENANT_WITNESS_BYTES,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const COVENANT_EXECUTION_SEMANTICS_VERSION_V1: u16 = 1;
pub const MAX_COVENANT_AUTHORIZATION_FACTS_V1: usize = 16;
pub const MAX_COVENANT_PREIMAGE_BYTES_V1: usize = 64;

const COVENANT_PARAMETERS_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-parameters:v1";
const COVENANT_WITNESS_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-witness:v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CovenantExecutionError {
    #[error(transparent)]
    Contract(#[from] ContractV3Error),
    #[error("unsupported covenant execution semantics version {0}")]
    UnsupportedExecutionVersion(u16),
    #[error("{field} commitment must not be all zero")]
    EmptyCommitment { field: &'static str },
    #[error("{left} and {right} commitments must differ")]
    DuplicateRoleCommitment {
        left: &'static str,
        right: &'static str,
    },
    #[error("commitment set count {actual} exceeds maximum {max}")]
    CommitmentSetTooLarge { actual: usize, max: usize },
    #[error("commitment set must contain at least one active entry")]
    EmptyCommitmentSet,
    #[error("active commitment-set entries must be non-zero, strictly sorted and unique")]
    NonCanonicalCommitmentSet,
    #[error("unused commitment-set entries must be all zero")]
    NonZeroUnusedCommitment,
    #[error("preimage length {actual} exceeds maximum {max}")]
    PreimageTooLong { actual: usize, max: usize },
    #[error("unused preimage bytes must be all zero")]
    NonZeroUnusedPreimage,
    #[error("multisig threshold {threshold} is invalid for {participants} participants")]
    InvalidMultisigThreshold { threshold: u8, participants: u8 },
    #[error("descriptor covenant kind {descriptor:?} does not match parameters {parameters:?}")]
    DescriptorKindMismatch {
        descriptor: CovenantKindV1,
        parameters: CovenantKindV1,
    },
    #[error("covenant parameters commitment mismatch")]
    ParametersCommitmentMismatch,
    #[error("covenant witness kind {witness:?} does not match parameters {parameters:?}")]
    WitnessKindMismatch {
        witness: CovenantKindV1,
        parameters: CovenantKindV1,
    },
    #[error("canonical covenant witness size {actual} exceeds descriptor budget {budget}")]
    WitnessBudgetExceeded { actual: usize, budget: u32 },
    #[error("proposed spend commitment does not match the covenant path")]
    SpendCommitmentMismatch,
    #[error("timelock height {current} has not reached required height {required}")]
    TimelockNotMature { current: u64, required: u64 },
    #[error("vault recovery height {current} has not reached required height {required}")]
    VaultRecoveryNotMature { current: u64, required: u64 },
    #[error("required authorization fact for {role} is missing")]
    AuthorizationMissing { role: &'static str },
    #[error("multisig witness contains an authorization outside the participant set")]
    UnauthorizedMultisigSigner,
    #[error("multisig threshold not met: provided {provided}, required {required}")]
    MultisigThresholdNotMet { provided: u8, required: u8 },
    #[error("escrow authorization facts are invalid for the selected path")]
    InvalidEscrowAuthorization,
    #[error("atomic-swap claim expired at height {refund_height}; current height is {current}")]
    AtomicSwapClaimExpired { current: u64, refund_height: u64 },
    #[error("atomic-swap refund height {current} has not reached {required}")]
    AtomicSwapRefundNotMature { current: u64, required: u64 },
    #[error("atomic-swap preimage does not match the committed secret hash")]
    AtomicSwapPreimageMismatch,
    #[error("atomic-swap refund witness must not carry a preimage")]
    UnexpectedAtomicSwapPreimage,
    #[error("payment-channel state commitment mismatch")]
    PaymentChannelStateMismatch,
    #[error("payment-channel sequence mismatch: provided {provided}, required {required}")]
    PaymentChannelSequenceMismatch { provided: u64, required: u64 },
    #[error("payment-channel unilateral settlement height {current} has not reached {required}")]
    PaymentChannelSettlementNotMature { current: u64, required: u64 },
    #[error("payment-channel authorization facts are invalid for the selected path")]
    InvalidPaymentChannelAuthorization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundedCommitmentSetV1 {
    pub count: u8,
    pub entries: [[u8; 32]; MAX_COVENANT_AUTHORIZATION_FACTS_V1],
}

impl BoundedCommitmentSetV1 {
    pub fn from_sorted(entries: &[[u8; 32]]) -> Result<Self, CovenantExecutionError> {
        if entries.len() > MAX_COVENANT_AUTHORIZATION_FACTS_V1 {
            return Err(CovenantExecutionError::CommitmentSetTooLarge {
                actual: entries.len(),
                max: MAX_COVENANT_AUTHORIZATION_FACTS_V1,
            });
        }
        if entries.is_empty() {
            return Err(CovenantExecutionError::EmptyCommitmentSet);
        }

        let mut fixed = [[0_u8; 32]; MAX_COVENANT_AUTHORIZATION_FACTS_V1];
        fixed[..entries.len()].copy_from_slice(entries);
        let value = Self {
            count: entries.len() as u8,
            entries: fixed,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), CovenantExecutionError> {
        let count = usize::from(self.count);
        if count > MAX_COVENANT_AUTHORIZATION_FACTS_V1 {
            return Err(CovenantExecutionError::CommitmentSetTooLarge {
                actual: count,
                max: MAX_COVENANT_AUTHORIZATION_FACTS_V1,
            });
        }
        if count == 0 {
            return Err(CovenantExecutionError::EmptyCommitmentSet);
        }

        let active = &self.entries[..count];
        if active
            .iter()
            .any(|entry| entry.iter().all(|byte| *byte == 0))
            || active.windows(2).any(|window| window[0] >= window[1])
        {
            return Err(CovenantExecutionError::NonCanonicalCommitmentSet);
        }
        if self.entries[count..]
            .iter()
            .any(|entry| entry.iter().any(|byte| *byte != 0))
        {
            return Err(CovenantExecutionError::NonZeroUnusedCommitment);
        }
        Ok(())
    }

    pub fn active(&self) -> &[[u8; 32]] {
        &self.entries[..usize::from(self.count)]
    }

    pub fn contains(&self, value: &[u8; 32]) -> bool {
        self.active().binary_search(value).is_ok()
    }

    fn encode_canonical(&self, out: &mut Vec<u8>) -> Result<(), CovenantExecutionError> {
        self.validate()?;
        out.push(self.count);
        for entry in self.active() {
            out.extend_from_slice(entry);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundedPreimageV1 {
    pub len: u8,
    pub bytes: [u8; MAX_COVENANT_PREIMAGE_BYTES_V1],
}

impl BoundedPreimageV1 {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CovenantExecutionError> {
        if bytes.len() > MAX_COVENANT_PREIMAGE_BYTES_V1 {
            return Err(CovenantExecutionError::PreimageTooLong {
                actual: bytes.len(),
                max: MAX_COVENANT_PREIMAGE_BYTES_V1,
            });
        }
        let mut fixed = [0_u8; MAX_COVENANT_PREIMAGE_BYTES_V1];
        fixed[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            len: bytes.len() as u8,
            bytes: fixed,
        })
    }

    pub fn validate(&self) -> Result<(), CovenantExecutionError> {
        let len = usize::from(self.len);
        if len > MAX_COVENANT_PREIMAGE_BYTES_V1 {
            return Err(CovenantExecutionError::PreimageTooLong {
                actual: len,
                max: MAX_COVENANT_PREIMAGE_BYTES_V1,
            });
        }
        if self.bytes[len..].iter().any(|byte| *byte != 0) {
            return Err(CovenantExecutionError::NonZeroUnusedPreimage);
        }
        Ok(())
    }

    pub fn active(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    fn encode_canonical(&self, out: &mut Vec<u8>) -> Result<(), CovenantExecutionError> {
        self.validate()?;
        out.push(self.len);
        out.extend_from_slice(self.active());
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimelockParametersV1 {
    pub not_before_height: u64,
    pub spend_commitment: [u8; 32],
    pub authorization_key_commitment: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VaultParametersV1 {
    pub normal_spend_commitment: [u8; 32],
    pub recovery_spend_commitment: [u8; 32],
    pub spend_key_commitment: [u8; 32],
    pub recovery_key_commitment: [u8; 32],
    pub recovery_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultisigParametersV1 {
    pub spend_commitment: [u8; 32],
    pub threshold: u8,
    pub participants: BoundedCommitmentSetV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EscrowParametersV1 {
    pub release_spend_commitment: [u8; 32],
    pub refund_spend_commitment: [u8; 32],
    pub buyer_key_commitment: [u8; 32],
    pub seller_key_commitment: [u8; 32],
    pub arbiter_key_commitment: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AtomicSwapParametersV1 {
    pub claim_spend_commitment: [u8; 32],
    pub refund_spend_commitment: [u8; 32],
    pub secret_hash: [u8; 32],
    pub claim_key_commitment: [u8; 32],
    pub refund_key_commitment: [u8; 32],
    pub refund_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaymentChannelParametersV1 {
    pub cooperative_spend_commitment: [u8; 32],
    pub unilateral_spend_commitment: [u8; 32],
    pub party_a_key_commitment: [u8; 32],
    pub party_b_key_commitment: [u8; 32],
    pub state_commitment: [u8; 32],
    pub sequence: u64,
    pub settlement_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CovenantParametersV1 {
    Timelock(TimelockParametersV1),
    Vault(VaultParametersV1),
    Multisig(MultisigParametersV1),
    Escrow(EscrowParametersV1),
    AtomicSwap(AtomicSwapParametersV1),
    PaymentChannel(PaymentChannelParametersV1),
}

impl CovenantParametersV1 {
    pub fn kind(&self) -> CovenantKindV1 {
        match self {
            Self::Timelock(_) => CovenantKindV1::Timelock,
            Self::Vault(_) => CovenantKindV1::Vault,
            Self::Multisig(_) => CovenantKindV1::Multisig,
            Self::Escrow(_) => CovenantKindV1::Escrow,
            Self::AtomicSwap(_) => CovenantKindV1::AtomicSwap,
            Self::PaymentChannel(_) => CovenantKindV1::PaymentChannel,
        }
    }

    pub fn validate(&self) -> Result<(), CovenantExecutionError> {
        match self {
            Self::Timelock(parameters) => {
                ensure_nonzero("timelock spend", &parameters.spend_commitment)?;
                ensure_nonzero(
                    "timelock authorization key",
                    &parameters.authorization_key_commitment,
                )?;
            }
            Self::Vault(parameters) => {
                ensure_nonzero("vault normal spend", &parameters.normal_spend_commitment)?;
                ensure_nonzero(
                    "vault recovery spend",
                    &parameters.recovery_spend_commitment,
                )?;
                ensure_nonzero("vault spend key", &parameters.spend_key_commitment)?;
                ensure_nonzero("vault recovery key", &parameters.recovery_key_commitment)?;
                ensure_distinct(
                    "vault normal spend",
                    &parameters.normal_spend_commitment,
                    "vault recovery spend",
                    &parameters.recovery_spend_commitment,
                )?;
                ensure_distinct(
                    "vault spend key",
                    &parameters.spend_key_commitment,
                    "vault recovery key",
                    &parameters.recovery_key_commitment,
                )?;
            }
            Self::Multisig(parameters) => {
                ensure_nonzero("multisig spend", &parameters.spend_commitment)?;
                parameters.participants.validate()?;
                if parameters.threshold == 0 || parameters.threshold > parameters.participants.count {
                    return Err(CovenantExecutionError::InvalidMultisigThreshold {
                        threshold: parameters.threshold,
                        participants: parameters.participants.count,
                    });
                }
            }
            Self::Escrow(parameters) => {
                ensure_nonzero("escrow release spend", &parameters.release_spend_commitment)?;
                ensure_nonzero("escrow refund spend", &parameters.refund_spend_commitment)?;
                ensure_nonzero("escrow buyer key", &parameters.buyer_key_commitment)?;
                ensure_nonzero("escrow seller key", &parameters.seller_key_commitment)?;
                ensure_nonzero("escrow arbiter key", &parameters.arbiter_key_commitment)?;
                ensure_distinct(
                    "escrow release spend",
                    &parameters.release_spend_commitment,
                    "escrow refund spend",
                    &parameters.refund_spend_commitment,
                )?;
                ensure_all_distinct_three(
                    ("escrow buyer key", &parameters.buyer_key_commitment),
                    ("escrow seller key", &parameters.seller_key_commitment),
                    ("escrow arbiter key", &parameters.arbiter_key_commitment),
                )?;
            }
            Self::AtomicSwap(parameters) => {
                ensure_nonzero("atomic-swap claim spend", &parameters.claim_spend_commitment)?;
                ensure_nonzero("atomic-swap refund spend", &parameters.refund_spend_commitment)?;
                ensure_nonzero("atomic-swap secret hash", &parameters.secret_hash)?;
                ensure_nonzero("atomic-swap claim key", &parameters.claim_key_commitment)?;
                ensure_nonzero("atomic-swap refund key", &parameters.refund_key_commitment)?;
                ensure_distinct(
                    "atomic-swap claim spend",
                    &parameters.claim_spend_commitment,
                    "atomic-swap refund spend",
                    &parameters.refund_spend_commitment,
                )?;
                ensure_distinct(
                    "atomic-swap claim key",
                    &parameters.claim_key_commitment,
                    "atomic-swap refund key",
                    &parameters.refund_key_commitment,
                )?;
            }
            Self::PaymentChannel(parameters) => {
                ensure_nonzero(
                    "payment-channel cooperative spend",
                    &parameters.cooperative_spend_commitment,
                )?;
                ensure_nonzero(
                    "payment-channel unilateral spend",
                    &parameters.unilateral_spend_commitment,
                )?;
                ensure_nonzero(
                    "payment-channel party A key",
                    &parameters.party_a_key_commitment,
                )?;
                ensure_nonzero(
                    "payment-channel party B key",
                    &parameters.party_b_key_commitment,
                )?;
                ensure_nonzero("payment-channel state", &parameters.state_commitment)?;
                ensure_distinct(
                    "payment-channel cooperative spend",
                    &parameters.cooperative_spend_commitment,
                    "payment-channel unilateral spend",
                    &parameters.unilateral_spend_commitment,
                )?;
                ensure_distinct(
                    "payment-channel party A key",
                    &parameters.party_a_key_commitment,
                    "payment-channel party B key",
                    &parameters.party_b_key_commitment,
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultSpendPathV1 {
    Normal,
    Recovery,
}

impl VaultSpendPathV1 {
    fn canonical_id(self) -> u8 {
        match self {
            Self::Normal => 1,
            Self::Recovery => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EscrowSpendPathV1 {
    Release,
    Refund,
}

impl EscrowSpendPathV1 {
    fn canonical_id(self) -> u8 {
        match self {
            Self::Release => 1,
            Self::Refund => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AtomicSwapSpendPathV1 {
    Claim,
    Refund,
}

impl AtomicSwapSpendPathV1 {
    fn canonical_id(self) -> u8 {
        match self {
            Self::Claim => 1,
            Self::Refund => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentChannelSpendPathV1 {
    Cooperative,
    Unilateral,
}

impl PaymentChannelSpendPathV1 {
    fn canonical_id(self) -> u8 {
        match self {
            Self::Cooperative => 1,
            Self::Unilateral => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CovenantWitnessV1 {
    Timelock {
        authorization_key_commitment: [u8; 32],
    },
    Vault {
        path: VaultSpendPathV1,
        authorization_key_commitment: [u8; 32],
    },
    Multisig {
        authorization_key_commitments: BoundedCommitmentSetV1,
    },
    Escrow {
        path: EscrowSpendPathV1,
        authorization_key_commitments: BoundedCommitmentSetV1,
    },
    AtomicSwap {
        path: AtomicSwapSpendPathV1,
        authorization_key_commitment: [u8; 32],
        preimage: BoundedPreimageV1,
    },
    PaymentChannel {
        path: PaymentChannelSpendPathV1,
        state_commitment: [u8; 32],
        sequence: u64,
        authorization_key_commitments: BoundedCommitmentSetV1,
    },
}

impl CovenantWitnessV1 {
    pub fn kind(&self) -> CovenantKindV1 {
        match self {
            Self::Timelock { .. } => CovenantKindV1::Timelock,
            Self::Vault { .. } => CovenantKindV1::Vault,
            Self::Multisig { .. } => CovenantKindV1::Multisig,
            Self::Escrow { .. } => CovenantKindV1::Escrow,
            Self::AtomicSwap { .. } => CovenantKindV1::AtomicSwap,
            Self::PaymentChannel { .. } => CovenantKindV1::PaymentChannel,
        }
    }

    pub fn validate(&self) -> Result<(), CovenantExecutionError> {
        match self {
            Self::Timelock {
                authorization_key_commitment,
            }
            | Self::Vault {
                authorization_key_commitment,
                ..
            } => ensure_nonzero("covenant authorization key", authorization_key_commitment),
            Self::Multisig {
                authorization_key_commitments,
            }
            | Self::Escrow {
                authorization_key_commitments,
                ..
            } => authorization_key_commitments.validate(),
            Self::AtomicSwap {
                authorization_key_commitment,
                preimage,
                ..
            } => {
                ensure_nonzero("atomic-swap authorization key", authorization_key_commitment)?;
                preimage.validate()
            }
            Self::PaymentChannel {
                state_commitment,
                authorization_key_commitments,
                ..
            } => {
                ensure_nonzero("payment-channel witness state", state_commitment)?;
                authorization_key_commitments.validate()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantExecutionContextV1 {
    pub version: u16,
    pub block_height: u64,
    pub spend_commitment: [u8; 32],
}

impl CovenantExecutionContextV1 {
    pub fn validate(&self) -> Result<(), CovenantExecutionError> {
        if self.version != COVENANT_EXECUTION_SEMANTICS_VERSION_V1 {
            return Err(CovenantExecutionError::UnsupportedExecutionVersion(
                self.version,
            ));
        }
        ensure_nonzero("covenant execution spend", &self.spend_commitment)
    }
}

pub fn canonical_covenant_parameters_bytes_v1(
    parameters: &CovenantParametersV1,
) -> Result<Vec<u8>, CovenantExecutionError> {
    parameters.validate()?;
    let mut out = Vec::with_capacity(768);
    encode_domain(&mut out, COVENANT_PARAMETERS_DOMAIN_V1);
    out.extend_from_slice(&COVENANT_EXECUTION_SEMANTICS_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&covenant_kind_id(parameters.kind()).to_le_bytes());

    match parameters {
        CovenantParametersV1::Timelock(parameters) => {
            out.extend_from_slice(&parameters.not_before_height.to_le_bytes());
            out.extend_from_slice(&parameters.spend_commitment);
            out.extend_from_slice(&parameters.authorization_key_commitment);
        }
        CovenantParametersV1::Vault(parameters) => {
            out.extend_from_slice(&parameters.normal_spend_commitment);
            out.extend_from_slice(&parameters.recovery_spend_commitment);
            out.extend_from_slice(&parameters.spend_key_commitment);
            out.extend_from_slice(&parameters.recovery_key_commitment);
            out.extend_from_slice(&parameters.recovery_height.to_le_bytes());
        }
        CovenantParametersV1::Multisig(parameters) => {
            out.extend_from_slice(&parameters.spend_commitment);
            out.push(parameters.threshold);
            parameters.participants.encode_canonical(&mut out)?;
        }
        CovenantParametersV1::Escrow(parameters) => {
            out.extend_from_slice(&parameters.release_spend_commitment);
            out.extend_from_slice(&parameters.refund_spend_commitment);
            out.extend_from_slice(&parameters.buyer_key_commitment);
            out.extend_from_slice(&parameters.seller_key_commitment);
            out.extend_from_slice(&parameters.arbiter_key_commitment);
        }
        CovenantParametersV1::AtomicSwap(parameters) => {
            out.extend_from_slice(&parameters.claim_spend_commitment);
            out.extend_from_slice(&parameters.refund_spend_commitment);
            out.extend_from_slice(&parameters.secret_hash);
            out.extend_from_slice(&parameters.claim_key_commitment);
            out.extend_from_slice(&parameters.refund_key_commitment);
            out.extend_from_slice(&parameters.refund_height.to_le_bytes());
        }
        CovenantParametersV1::PaymentChannel(parameters) => {
            out.extend_from_slice(&parameters.cooperative_spend_commitment);
            out.extend_from_slice(&parameters.unilateral_spend_commitment);
            out.extend_from_slice(&parameters.party_a_key_commitment);
            out.extend_from_slice(&parameters.party_b_key_commitment);
            out.extend_from_slice(&parameters.state_commitment);
            out.extend_from_slice(&parameters.sequence.to_le_bytes());
            out.extend_from_slice(&parameters.settlement_height.to_le_bytes());
        }
    }
    Ok(out)
}

pub fn covenant_parameters_commitment_v1(
    parameters: &CovenantParametersV1,
) -> Result<[u8; 32], CovenantExecutionError> {
    Ok(sha256_array(&canonical_covenant_parameters_bytes_v1(
        parameters,
    )?))
}

pub fn descriptor_for_covenant_parameters_v1(
    parameters: &CovenantParametersV1,
    witness_budget_bytes: u32,
) -> Result<CovenantDescriptorV1, CovenantExecutionError> {
    parameters.validate()?;
    let descriptor = CovenantDescriptorV1 {
        version: COVENANT_DESCRIPTOR_VERSION_V1,
        kind: parameters.kind(),
        parameters_commitment: covenant_parameters_commitment_v1(parameters)?,
        witness_budget_bytes,
    };
    descriptor.validate()?;
    Ok(descriptor)
}

pub fn canonical_covenant_witness_bytes_v1(
    witness: &CovenantWitnessV1,
) -> Result<Vec<u8>, CovenantExecutionError> {
    witness.validate()?;
    let mut out = Vec::with_capacity(768);
    encode_domain(&mut out, COVENANT_WITNESS_DOMAIN_V1);
    out.extend_from_slice(&COVENANT_EXECUTION_SEMANTICS_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&covenant_kind_id(witness.kind()).to_le_bytes());

    match witness {
        CovenantWitnessV1::Timelock {
            authorization_key_commitment,
        } => out.extend_from_slice(authorization_key_commitment),
        CovenantWitnessV1::Vault {
            path,
            authorization_key_commitment,
        } => {
            out.push(path.canonical_id());
            out.extend_from_slice(authorization_key_commitment);
        }
        CovenantWitnessV1::Multisig {
            authorization_key_commitments,
        } => authorization_key_commitments.encode_canonical(&mut out)?,
        CovenantWitnessV1::Escrow {
            path,
            authorization_key_commitments,
        } => {
            out.push(path.canonical_id());
            authorization_key_commitments.encode_canonical(&mut out)?;
        }
        CovenantWitnessV1::AtomicSwap {
            path,
            authorization_key_commitment,
            preimage,
        } => {
            out.push(path.canonical_id());
            out.extend_from_slice(authorization_key_commitment);
            preimage.encode_canonical(&mut out)?;
        }
        CovenantWitnessV1::PaymentChannel {
            path,
            state_commitment,
            sequence,
            authorization_key_commitments,
        } => {
            out.push(path.canonical_id());
            out.extend_from_slice(state_commitment);
            out.extend_from_slice(&sequence.to_le_bytes());
            authorization_key_commitments.encode_canonical(&mut out)?;
        }
    }
    Ok(out)
}

pub fn evaluate_covenant_v1(
    descriptor: &CovenantDescriptorV1,
    parameters: &CovenantParametersV1,
    witness: &CovenantWitnessV1,
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    descriptor.validate()?;
    context.validate()?;
    parameters.validate()?;
    witness.validate()?;

    let parameters_kind = parameters.kind();
    if descriptor.kind != parameters_kind {
        return Err(CovenantExecutionError::DescriptorKindMismatch {
            descriptor: descriptor.kind,
            parameters: parameters_kind,
        });
    }
    if descriptor.parameters_commitment != covenant_parameters_commitment_v1(parameters)? {
        return Err(CovenantExecutionError::ParametersCommitmentMismatch);
    }
    if witness.kind() != parameters_kind {
        return Err(CovenantExecutionError::WitnessKindMismatch {
            witness: witness.kind(),
            parameters: parameters_kind,
        });
    }

    let witness_len = canonical_covenant_witness_bytes_v1(witness)?.len();
    if witness_len > descriptor.witness_budget_bytes as usize {
        return Err(CovenantExecutionError::WitnessBudgetExceeded {
            actual: witness_len,
            budget: descriptor.witness_budget_bytes,
        });
    }
    if witness_len > MAX_COVENANT_WITNESS_BYTES as usize {
        return Err(CovenantExecutionError::WitnessBudgetExceeded {
            actual: witness_len,
            budget: MAX_COVENANT_WITNESS_BYTES,
        });
    }

    match (parameters, witness) {
        (
            CovenantParametersV1::Timelock(parameters),
            CovenantWitnessV1::Timelock {
                authorization_key_commitment,
            },
        ) => evaluate_timelock(parameters, authorization_key_commitment, context),
        (
            CovenantParametersV1::Vault(parameters),
            CovenantWitnessV1::Vault {
                path,
                authorization_key_commitment,
            },
        ) => evaluate_vault(parameters, *path, authorization_key_commitment, context),
        (
            CovenantParametersV1::Multisig(parameters),
            CovenantWitnessV1::Multisig {
                authorization_key_commitments,
            },
        ) => evaluate_multisig(parameters, authorization_key_commitments, context),
        (
            CovenantParametersV1::Escrow(parameters),
            CovenantWitnessV1::Escrow {
                path,
                authorization_key_commitments,
            },
        ) => evaluate_escrow(parameters, *path, authorization_key_commitments, context),
        (
            CovenantParametersV1::AtomicSwap(parameters),
            CovenantWitnessV1::AtomicSwap {
                path,
                authorization_key_commitment,
                preimage,
            },
        ) => evaluate_atomic_swap(
            parameters,
            *path,
            authorization_key_commitment,
            preimage,
            context,
        ),
        (
            CovenantParametersV1::PaymentChannel(parameters),
            CovenantWitnessV1::PaymentChannel {
                path,
                state_commitment,
                sequence,
                authorization_key_commitments,
            },
        ) => evaluate_payment_channel(
            parameters,
            *path,
            state_commitment,
            *sequence,
            authorization_key_commitments,
            context,
        ),
        _ => Err(CovenantExecutionError::WitnessKindMismatch {
            witness: witness.kind(),
            parameters: parameters.kind(),
        }),
    }
}

fn evaluate_timelock(
    parameters: &TimelockParametersV1,
    authorization: &[u8; 32],
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    require_spend(&context.spend_commitment, &parameters.spend_commitment)?;
    if context.block_height < parameters.not_before_height {
        return Err(CovenantExecutionError::TimelockNotMature {
            current: context.block_height,
            required: parameters.not_before_height,
        });
    }
    require_authorization(
        authorization,
        &parameters.authorization_key_commitment,
        "timelock key",
    )
}

fn evaluate_vault(
    parameters: &VaultParametersV1,
    path: VaultSpendPathV1,
    authorization: &[u8; 32],
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    match path {
        VaultSpendPathV1::Normal => {
            require_spend(
                &context.spend_commitment,
                &parameters.normal_spend_commitment,
            )?;
            require_authorization(authorization, &parameters.spend_key_commitment, "vault spend key")
        }
        VaultSpendPathV1::Recovery => {
            require_spend(
                &context.spend_commitment,
                &parameters.recovery_spend_commitment,
            )?;
            if context.block_height < parameters.recovery_height {
                return Err(CovenantExecutionError::VaultRecoveryNotMature {
                    current: context.block_height,
                    required: parameters.recovery_height,
                });
            }
            require_authorization(
                authorization,
                &parameters.recovery_key_commitment,
                "vault recovery key",
            )
        }
    }
}

fn evaluate_multisig(
    parameters: &MultisigParametersV1,
    authorizations: &BoundedCommitmentSetV1,
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    require_spend(&context.spend_commitment, &parameters.spend_commitment)?;
    for authorization in authorizations.active() {
        if !parameters.participants.contains(authorization) {
            return Err(CovenantExecutionError::UnauthorizedMultisigSigner);
        }
    }
    if authorizations.count < parameters.threshold {
        return Err(CovenantExecutionError::MultisigThresholdNotMet {
            provided: authorizations.count,
            required: parameters.threshold,
        });
    }
    Ok(())
}

fn evaluate_escrow(
    parameters: &EscrowParametersV1,
    path: EscrowSpendPathV1,
    authorizations: &BoundedCommitmentSetV1,
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    let valid = match path {
        EscrowSpendPathV1::Release => {
            require_spend(
                &context.spend_commitment,
                &parameters.release_spend_commitment,
            )?;
            exact_pair(
                authorizations,
                &parameters.buyer_key_commitment,
                &parameters.seller_key_commitment,
            ) || exact_pair(
                authorizations,
                &parameters.seller_key_commitment,
                &parameters.arbiter_key_commitment,
            )
        }
        EscrowSpendPathV1::Refund => {
            require_spend(
                &context.spend_commitment,
                &parameters.refund_spend_commitment,
            )?;
            exact_pair(
                authorizations,
                &parameters.buyer_key_commitment,
                &parameters.seller_key_commitment,
            ) || exact_pair(
                authorizations,
                &parameters.buyer_key_commitment,
                &parameters.arbiter_key_commitment,
            )
        }
    };
    if !valid {
        return Err(CovenantExecutionError::InvalidEscrowAuthorization);
    }
    Ok(())
}

fn evaluate_atomic_swap(
    parameters: &AtomicSwapParametersV1,
    path: AtomicSwapSpendPathV1,
    authorization: &[u8; 32],
    preimage: &BoundedPreimageV1,
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    match path {
        AtomicSwapSpendPathV1::Claim => {
            require_spend(
                &context.spend_commitment,
                &parameters.claim_spend_commitment,
            )?;
            if context.block_height >= parameters.refund_height {
                return Err(CovenantExecutionError::AtomicSwapClaimExpired {
                    current: context.block_height,
                    refund_height: parameters.refund_height,
                });
            }
            require_authorization(
                authorization,
                &parameters.claim_key_commitment,
                "atomic-swap claim key",
            )?;
            if preimage.len == 0 || sha256_array(preimage.active()) != parameters.secret_hash {
                return Err(CovenantExecutionError::AtomicSwapPreimageMismatch);
            }
        }
        AtomicSwapSpendPathV1::Refund => {
            require_spend(
                &context.spend_commitment,
                &parameters.refund_spend_commitment,
            )?;
            if context.block_height < parameters.refund_height {
                return Err(CovenantExecutionError::AtomicSwapRefundNotMature {
                    current: context.block_height,
                    required: parameters.refund_height,
                });
            }
            require_authorization(
                authorization,
                &parameters.refund_key_commitment,
                "atomic-swap refund key",
            )?;
            if preimage.len != 0 {
                return Err(CovenantExecutionError::UnexpectedAtomicSwapPreimage);
            }
        }
    }
    Ok(())
}

fn evaluate_payment_channel(
    parameters: &PaymentChannelParametersV1,
    path: PaymentChannelSpendPathV1,
    state_commitment: &[u8; 32],
    sequence: u64,
    authorizations: &BoundedCommitmentSetV1,
    context: &CovenantExecutionContextV1,
) -> Result<(), CovenantExecutionError> {
    if state_commitment != &parameters.state_commitment {
        return Err(CovenantExecutionError::PaymentChannelStateMismatch);
    }
    if sequence != parameters.sequence {
        return Err(CovenantExecutionError::PaymentChannelSequenceMismatch {
            provided: sequence,
            required: parameters.sequence,
        });
    }

    match path {
        PaymentChannelSpendPathV1::Cooperative => {
            require_spend(
                &context.spend_commitment,
                &parameters.cooperative_spend_commitment,
            )?;
            if !exact_pair(
                authorizations,
                &parameters.party_a_key_commitment,
                &parameters.party_b_key_commitment,
            ) {
                return Err(CovenantExecutionError::InvalidPaymentChannelAuthorization);
            }
        }
        PaymentChannelSpendPathV1::Unilateral => {
            require_spend(
                &context.spend_commitment,
                &parameters.unilateral_spend_commitment,
            )?;
            if context.block_height < parameters.settlement_height {
                return Err(CovenantExecutionError::PaymentChannelSettlementNotMature {
                    current: context.block_height,
                    required: parameters.settlement_height,
                });
            }
            if authorizations.count != 1
                || !(authorizations.contains(&parameters.party_a_key_commitment)
                    || authorizations.contains(&parameters.party_b_key_commitment))
            {
                return Err(CovenantExecutionError::InvalidPaymentChannelAuthorization);
            }
        }
    }
    Ok(())
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

fn encode_domain(out: &mut Vec<u8>, domain: &[u8]) {
    out.extend_from_slice(&(domain.len() as u32).to_le_bytes());
    out.extend_from_slice(domain);
}

fn sha256_array(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn ensure_nonzero(field: &'static str, value: &[u8; 32]) -> Result<(), CovenantExecutionError> {
    if value.iter().all(|byte| *byte == 0) {
        return Err(CovenantExecutionError::EmptyCommitment { field });
    }
    Ok(())
}

fn ensure_distinct(
    left_name: &'static str,
    left: &[u8; 32],
    right_name: &'static str,
    right: &[u8; 32],
) -> Result<(), CovenantExecutionError> {
    if left == right {
        return Err(CovenantExecutionError::DuplicateRoleCommitment {
            left: left_name,
            right: right_name,
        });
    }
    Ok(())
}

fn ensure_all_distinct_three(
    first: (&'static str, &[u8; 32]),
    second: (&'static str, &[u8; 32]),
    third: (&'static str, &[u8; 32]),
) -> Result<(), CovenantExecutionError> {
    ensure_distinct(first.0, first.1, second.0, second.1)?;
    ensure_distinct(first.0, first.1, third.0, third.1)?;
    ensure_distinct(second.0, second.1, third.0, third.1)
}

fn require_spend(actual: &[u8; 32], expected: &[u8; 32]) -> Result<(), CovenantExecutionError> {
    if actual != expected {
        return Err(CovenantExecutionError::SpendCommitmentMismatch);
    }
    Ok(())
}

fn require_authorization(
    actual: &[u8; 32],
    expected: &[u8; 32],
    role: &'static str,
) -> Result<(), CovenantExecutionError> {
    if actual != expected {
        return Err(CovenantExecutionError::AuthorizationMissing { role });
    }
    Ok(())
}

fn exact_pair(set: &BoundedCommitmentSetV1, first: &[u8; 32], second: &[u8; 32]) -> bool {
    set.count == 2 && set.contains(first) && set.contains(second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commitment(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn set(values: &[[u8; 32]]) -> BoundedCommitmentSetV1 {
        BoundedCommitmentSetV1::from_sorted(values).unwrap()
    }

    fn context(height: u64, spend: [u8; 32]) -> CovenantExecutionContextV1 {
        CovenantExecutionContextV1 {
            version: COVENANT_EXECUTION_SEMANTICS_VERSION_V1,
            block_height: height,
            spend_commitment: spend,
        }
    }

    #[test]
    fn timelock_parameters_have_frozen_commitment_vector() {
        let parameters = CovenantParametersV1::Timelock(TimelockParametersV1 {
            not_before_height: 100,
            spend_commitment: commitment(0x11),
            authorization_key_commitment: commitment(0x22),
        });
        assert_eq!(
            hex::encode(covenant_parameters_commitment_v1(&parameters).unwrap()),
            "16ddbcef24217eb9bc63084ae900ad45bc7a34f9af6acba58cf141d46ef4230c"
        );
    }

    #[test]
    fn timelock_executes_only_after_height_and_for_committed_spend() {
        let parameters = CovenantParametersV1::Timelock(TimelockParametersV1 {
            not_before_height: 100,
            spend_commitment: commitment(0x11),
            authorization_key_commitment: commitment(0x22),
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let witness = CovenantWitnessV1::Timelock {
            authorization_key_commitment: commitment(0x22),
        };

        assert!(matches!(
            evaluate_covenant_v1(&descriptor, &parameters, &witness, &context(99, commitment(0x11))),
            Err(CovenantExecutionError::TimelockNotMature { .. })
        ));
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &witness,
            &context(100, commitment(0x11))
        )
        .is_ok());
        assert!(matches!(
            evaluate_covenant_v1(&descriptor, &parameters, &witness, &context(100, commitment(0x12))),
            Err(CovenantExecutionError::SpendCommitmentMismatch)
        ));
    }

    #[test]
    fn vault_normal_and_delayed_recovery_paths_are_enforced() {
        let parameters = CovenantParametersV1::Vault(VaultParametersV1 {
            normal_spend_commitment: commitment(0x31),
            recovery_spend_commitment: commitment(0x32),
            spend_key_commitment: commitment(0x11),
            recovery_key_commitment: commitment(0x22),
            recovery_height: 500,
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let normal = CovenantWitnessV1::Vault {
            path: VaultSpendPathV1::Normal,
            authorization_key_commitment: commitment(0x11),
        };
        let recovery = CovenantWitnessV1::Vault {
            path: VaultSpendPathV1::Recovery,
            authorization_key_commitment: commitment(0x22),
        };

        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &normal,
            &context(1, commitment(0x31))
        )
        .is_ok());
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &recovery,
                &context(499, commitment(0x32))
            ),
            Err(CovenantExecutionError::VaultRecoveryNotMature { .. })
        ));
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &recovery,
            &context(500, commitment(0x32))
        )
        .is_ok());
    }

    #[test]
    fn multisig_requires_bounded_canonical_threshold_authorizations() {
        let participants = set(&[commitment(0x11), commitment(0x22), commitment(0x33)]);
        let parameters = CovenantParametersV1::Multisig(MultisigParametersV1 {
            spend_commitment: commitment(0x41),
            threshold: 2,
            participants,
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let witness = CovenantWitnessV1::Multisig {
            authorization_key_commitments: set(&[commitment(0x11), commitment(0x22)]),
        };
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &witness,
            &context(1, commitment(0x41))
        )
        .is_ok());

        let insufficient = CovenantWitnessV1::Multisig {
            authorization_key_commitments: set(&[commitment(0x11)]),
        };
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &insufficient,
                &context(1, commitment(0x41))
            ),
            Err(CovenantExecutionError::MultisigThresholdNotMet { .. })
        ));
    }

    #[test]
    fn escrow_accepts_cooperative_or_arbiter_paths_only() {
        let parameters = CovenantParametersV1::Escrow(EscrowParametersV1 {
            release_spend_commitment: commitment(0x51),
            refund_spend_commitment: commitment(0x52),
            buyer_key_commitment: commitment(0x11),
            seller_key_commitment: commitment(0x22),
            arbiter_key_commitment: commitment(0x33),
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let release = CovenantWitnessV1::Escrow {
            path: EscrowSpendPathV1::Release,
            authorization_key_commitments: set(&[commitment(0x22), commitment(0x33)]),
        };
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &release,
            &context(1, commitment(0x51))
        )
        .is_ok());

        let invalid = CovenantWitnessV1::Escrow {
            path: EscrowSpendPathV1::Release,
            authorization_key_commitments: set(&[commitment(0x11), commitment(0x33)]),
        };
        assert_eq!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &invalid,
                &context(1, commitment(0x51))
            ),
            Err(CovenantExecutionError::InvalidEscrowAuthorization)
        );
    }

    #[test]
    fn atomic_swap_claim_and_refund_paths_are_height_and_secret_bound() {
        let preimage = BoundedPreimageV1::from_bytes(b"secret").unwrap();
        let parameters = CovenantParametersV1::AtomicSwap(AtomicSwapParametersV1 {
            claim_spend_commitment: commitment(0x61),
            refund_spend_commitment: commitment(0x62),
            secret_hash: sha256_array(preimage.active()),
            claim_key_commitment: commitment(0x11),
            refund_key_commitment: commitment(0x22),
            refund_height: 100,
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let claim = CovenantWitnessV1::AtomicSwap {
            path: AtomicSwapSpendPathV1::Claim,
            authorization_key_commitment: commitment(0x11),
            preimage,
        };
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &claim,
            &context(99, commitment(0x61))
        )
        .is_ok());
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &claim,
                &context(100, commitment(0x61))
            ),
            Err(CovenantExecutionError::AtomicSwapClaimExpired { .. })
        ));

        let refund = CovenantWitnessV1::AtomicSwap {
            path: AtomicSwapSpendPathV1::Refund,
            authorization_key_commitment: commitment(0x22),
            preimage: BoundedPreimageV1::from_bytes(&[]).unwrap(),
        };
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &refund,
            &context(100, commitment(0x62))
        )
        .is_ok());
    }

    #[test]
    fn payment_channel_supports_cooperative_and_delayed_unilateral_settlement() {
        let parameters = CovenantParametersV1::PaymentChannel(PaymentChannelParametersV1 {
            cooperative_spend_commitment: commitment(0x71),
            unilateral_spend_commitment: commitment(0x72),
            party_a_key_commitment: commitment(0x11),
            party_b_key_commitment: commitment(0x22),
            state_commitment: commitment(0x81),
            sequence: 7,
            settlement_height: 100,
        });
        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        let cooperative = CovenantWitnessV1::PaymentChannel {
            path: PaymentChannelSpendPathV1::Cooperative,
            state_commitment: commitment(0x81),
            sequence: 7,
            authorization_key_commitments: set(&[commitment(0x11), commitment(0x22)]),
        };
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &cooperative,
            &context(1, commitment(0x71))
        )
        .is_ok());

        let unilateral = CovenantWitnessV1::PaymentChannel {
            path: PaymentChannelSpendPathV1::Unilateral,
            state_commitment: commitment(0x81),
            sequence: 7,
            authorization_key_commitments: set(&[commitment(0x11)]),
        };
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &unilateral,
                &context(99, commitment(0x72))
            ),
            Err(CovenantExecutionError::PaymentChannelSettlementNotMature { .. })
        ));
        assert!(evaluate_covenant_v1(
            &descriptor,
            &parameters,
            &unilateral,
            &context(100, commitment(0x72))
        )
        .is_ok());
    }

    #[test]
    fn descriptor_binding_and_witness_budget_fail_closed() {
        let parameters = CovenantParametersV1::Timelock(TimelockParametersV1 {
            not_before_height: 1,
            spend_commitment: commitment(0x11),
            authorization_key_commitment: commitment(0x22),
        });
        let witness = CovenantWitnessV1::Timelock {
            authorization_key_commitment: commitment(0x22),
        };

        let mut descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1024).unwrap();
        descriptor.parameters_commitment[0] ^= 1;
        assert_eq!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &witness,
                &context(1, commitment(0x11))
            ),
            Err(CovenantExecutionError::ParametersCommitmentMismatch)
        );

        let descriptor = descriptor_for_covenant_parameters_v1(&parameters, 1).unwrap();
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor,
                &parameters,
                &witness,
                &context(1, commitment(0x11))
            ),
            Err(CovenantExecutionError::WitnessBudgetExceeded { .. })
        ));
    }

    #[test]
    fn bounded_fixed_storage_rejects_noncanonical_sets_and_preimages() {
        let mut duplicate_entries = [[0_u8; 32]; MAX_COVENANT_AUTHORIZATION_FACTS_V1];
        duplicate_entries[0] = commitment(0x11);
        duplicate_entries[1] = commitment(0x11);
        let duplicate = BoundedCommitmentSetV1 {
            count: 2,
            entries: duplicate_entries,
        };
        assert_eq!(
            duplicate.validate(),
            Err(CovenantExecutionError::NonCanonicalCommitmentSet)
        );

        let mut preimage = BoundedPreimageV1::from_bytes(b"a").unwrap();
        preimage.bytes[2] = 1;
        assert_eq!(
            preimage.validate(),
            Err(CovenantExecutionError::NonZeroUnusedPreimage)
        );
    }
}
