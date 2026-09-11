use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::contract_v3::{
    ContractV3Error, CovenantDescriptorV1, CovenantKindV1, COVENANT_DESCRIPTOR_VERSION_V1,
    MAX_COVENANT_WITNESS_BYTES,
};

pub const COVENANT_PROGRAM_VERSION_V1: u16 = 1;
pub const COVENANT_WITNESS_VERSION_V1: u16 = 1;
pub const COVENANT_SPEND_CONTEXT_VERSION_V1: u16 = 1;
pub const COVENANT_MAX_SIGNERS_V1: usize = 16;
pub const COVENANT_MAX_PREIMAGE_BYTES_V1: usize = 64;

pub const COVENANT_COST_BASE_UNITS_V1: u64 = 100;
pub const COVENANT_COST_SIGNER_CHECK_UNITS_V1: u64 = 50;
pub const COVENANT_COST_HASH_CHECK_UNITS_V1: u64 = 200;
pub const COVENANT_COST_LOCK_CHECK_UNITS_V1: u64 = 10;
pub const COVENANT_COST_STATE_CHECK_UNITS_V1: u64 = 20;
pub const COVENANT_COST_WITNESS_BYTE_UNITS_V1: u64 = 1;

const COVENANT_PROGRAM_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-program:v1";
const COVENANT_WITNESS_DOMAIN_V1: &[u8] = b"PulseDAG:covenant-witness:v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CovenantV1Error {
    #[error("invalid covenant descriptor: {0}")]
    InvalidDescriptor(#[from] ContractV3Error),
    #[error("unsupported covenant program version {0}")]
    UnsupportedProgramVersion(u16),
    #[error("unsupported covenant witness version {0}")]
    UnsupportedWitnessVersion(u16),
    #[error("unsupported covenant spend-context version {0}")]
    UnsupportedSpendContextVersion(u16),
    #[error("covenant program kind does not match descriptor kind")]
    DescriptorKindMismatch,
    #[error("covenant program commitment does not match descriptor commitment")]
    DescriptorCommitmentMismatch,
    #[error("covenant witness size {actual} exceeds descriptor budget {max}")]
    WitnessBudgetExceeded { actual: u32, max: u32 },
    #[error("covenant signer set is empty, oversized, unsorted, duplicated, or contains zero ids")]
    InvalidSignerSet,
    #[error("covenant signer id must not be all zero")]
    InvalidSigner,
    #[error("covenant signer roles must be distinct")]
    DuplicateSignerRole,
    #[error("covenant threshold {threshold} is invalid for {signer_count} signers")]
    InvalidThreshold { threshold: u8, signer_count: usize },
    #[error("required authenticated signer is absent")]
    MissingRequiredSigner,
    #[error("multisig threshold not satisfied: required {required}, present {present}")]
    ThresholdNotMet { required: u8, present: u8 },
    #[error("covenant lock is not yet satisfied")]
    LockNotSatisfied,
    #[error("covenant witness branch is invalid for this program")]
    InvalidWitnessBranch,
    #[error("atomic-swap preimage length {actual} exceeds maximum {max}")]
    PreimageTooLarge { actual: usize, max: usize },
    #[error("atomic-swap hashlock mismatch")]
    HashlockMismatch,
    #[error("payment-channel state sequence mismatch")]
    ChannelSequenceMismatch,
    #[error("payment-channel state commitment mismatch")]
    ChannelStateCommitmentMismatch,
    #[error("payment-channel settlement window is invalid")]
    InvalidChannelWindow,
    #[error("covenant arithmetic overflow")]
    ArithmeticOverflow,
    #[error("canonical covenant field length exceeds u32::MAX")]
    CanonicalLengthOverflow,
}

impl CovenantV1Error {
    pub fn rejection_code(&self) -> u16 {
        match self {
            Self::InvalidDescriptor(_) => 1,
            Self::UnsupportedProgramVersion(_) => 2,
            Self::UnsupportedWitnessVersion(_) => 3,
            Self::UnsupportedSpendContextVersion(_) => 4,
            Self::DescriptorKindMismatch => 5,
            Self::DescriptorCommitmentMismatch => 6,
            Self::WitnessBudgetExceeded { .. } => 7,
            Self::InvalidSignerSet => 8,
            Self::InvalidSigner => 9,
            Self::DuplicateSignerRole => 10,
            Self::InvalidThreshold { .. } => 11,
            Self::MissingRequiredSigner => 12,
            Self::ThresholdNotMet { .. } => 13,
            Self::LockNotSatisfied => 14,
            Self::InvalidWitnessBranch => 15,
            Self::PreimageTooLarge { .. } => 16,
            Self::HashlockMismatch => 17,
            Self::ChannelSequenceMismatch => 18,
            Self::ChannelStateCommitmentMismatch => 19,
            Self::InvalidChannelWindow => 20,
            Self::ArithmeticOverflow => 21,
            Self::CanonicalLengthOverflow => 22,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CovenantLockV1 {
    Height(u64),
    MedianTimeSeconds(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimelockCovenantV1 {
    pub required_signer: [u8; 32],
    pub lock: CovenantLockV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VaultCovenantV1 {
    pub hot_signer: [u8; 32],
    pub recovery_signer: [u8; 32],
    pub activation_height: u64,
    pub delay_blocks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultisigCovenantV1 {
    pub threshold: u8,
    #[serde(deserialize_with = "deserialize_bounded_signers")]
    pub signers: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EscrowCovenantV1 {
    pub buyer_signer: [u8; 32],
    pub seller_signer: [u8; 32],
    pub arbiter_signer: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AtomicSwapCovenantV1 {
    pub redeem_signer: [u8; 32],
    pub refund_signer: [u8; 32],
    pub hashlock: [u8; 32],
    pub refund_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaymentChannelCovenantV1 {
    pub settlement_signer: [u8; 32],
    pub refund_signer: [u8; 32],
    pub state_commitment: [u8; 32],
    pub state_sequence: u64,
    pub settle_not_before_height: u64,
    pub refund_after_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CovenantProgramBodyV1 {
    Timelock(TimelockCovenantV1),
    Vault(VaultCovenantV1),
    Multisig(MultisigCovenantV1),
    Escrow(EscrowCovenantV1),
    AtomicSwap(AtomicSwapCovenantV1),
    PaymentChannel(PaymentChannelCovenantV1),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantProgramV1 {
    pub version: u16,
    pub body: CovenantProgramBodyV1,
}

impl CovenantProgramV1 {
    pub fn validate(&self) -> Result<(), CovenantV1Error> {
        if self.version != COVENANT_PROGRAM_VERSION_V1 {
            return Err(CovenantV1Error::UnsupportedProgramVersion(self.version));
        }
        match &self.body {
            CovenantProgramBodyV1::Timelock(program) => {
                validate_signer(&program.required_signer)?;
            }
            CovenantProgramBodyV1::Vault(program) => {
                validate_distinct_signers(&[program.hot_signer, program.recovery_signer])?;
                program
                    .activation_height
                    .checked_add(program.delay_blocks)
                    .ok_or(CovenantV1Error::ArithmeticOverflow)?;
            }
            CovenantProgramBodyV1::Multisig(program) => {
                validate_sorted_unique_signers(&program.signers)?;
                if program.threshold == 0 || usize::from(program.threshold) > program.signers.len() {
                    return Err(CovenantV1Error::InvalidThreshold {
                        threshold: program.threshold,
                        signer_count: program.signers.len(),
                    });
                }
            }
            CovenantProgramBodyV1::Escrow(program) => {
                validate_distinct_signers(&[
                    program.buyer_signer,
                    program.seller_signer,
                    program.arbiter_signer,
                ])?;
            }
            CovenantProgramBodyV1::AtomicSwap(program) => {
                validate_distinct_signers(&[program.redeem_signer, program.refund_signer])?;
                ensure_nonzero_32(&program.hashlock)
                    .map_err(|_| CovenantV1Error::HashlockMismatch)?;
            }
            CovenantProgramBodyV1::PaymentChannel(program) => {
                validate_distinct_signers(&[program.settlement_signer, program.refund_signer])?;
                ensure_nonzero_32(&program.state_commitment)
                    .map_err(|_| CovenantV1Error::ChannelStateCommitmentMismatch)?;
                if program.settle_not_before_height >= program.refund_after_height {
                    return Err(CovenantV1Error::InvalidChannelWindow);
                }
            }
        }
        Ok(())
    }

    pub fn kind(&self) -> CovenantKindV1 {
        match &self.body {
            CovenantProgramBodyV1::Timelock(_) => CovenantKindV1::Timelock,
            CovenantProgramBodyV1::Vault(_) => CovenantKindV1::Vault,
            CovenantProgramBodyV1::Multisig(_) => CovenantKindV1::Multisig,
            CovenantProgramBodyV1::Escrow(_) => CovenantKindV1::Escrow,
            CovenantProgramBodyV1::AtomicSwap(_) => CovenantKindV1::AtomicSwap,
            CovenantProgramBodyV1::PaymentChannel(_) => CovenantKindV1::PaymentChannel,
        }
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CovenantV1Error> {
        canonical_covenant_program_bytes_v1(self)
    }

    pub fn commitment(&self) -> Result<[u8; 32], CovenantV1Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }

    pub fn descriptor(
        &self,
        witness_budget_bytes: u32,
    ) -> Result<CovenantDescriptorV1, CovenantV1Error> {
        if witness_budget_bytes > MAX_COVENANT_WITNESS_BYTES {
            return Err(CovenantV1Error::InvalidDescriptor(
                ContractV3Error::CovenantWitnessBudgetExceeded {
                    actual: witness_budget_bytes,
                    max: MAX_COVENANT_WITNESS_BYTES,
                },
            ));
        }
        Ok(CovenantDescriptorV1 {
            version: COVENANT_DESCRIPTOR_VERSION_V1,
            kind: self.kind(),
            parameters_commitment: self.commitment()?,
            witness_budget_bytes,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EscrowResolutionV1 {
    Cooperative,
    BuyerAward,
    SellerAward,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CovenantWitnessBranchV1 {
    Timelock,
    VaultHot,
    VaultRecovery,
    Multisig,
    Escrow(EscrowResolutionV1),
    AtomicSwapRedeem {
        #[serde(deserialize_with = "deserialize_bounded_preimage")]
        preimage: Vec<u8>,
    },
    AtomicSwapRefund,
    PaymentChannelCooperative,
    PaymentChannelSettle {
        state_sequence: u64,
        state_commitment: [u8; 32],
    },
    PaymentChannelRefund,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantWitnessV1 {
    pub version: u16,
    pub branch: CovenantWitnessBranchV1,
}

impl CovenantWitnessV1 {
    pub fn validate(&self) -> Result<(), CovenantV1Error> {
        if self.version != COVENANT_WITNESS_VERSION_V1 {
            return Err(CovenantV1Error::UnsupportedWitnessVersion(self.version));
        }
        if let CovenantWitnessBranchV1::AtomicSwapRedeem { preimage } = &self.branch {
            if preimage.len() > COVENANT_MAX_PREIMAGE_BYTES_V1 {
                return Err(CovenantV1Error::PreimageTooLarge {
                    actual: preimage.len(),
                    max: COVENANT_MAX_PREIMAGE_BYTES_V1,
                });
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CovenantV1Error> {
        canonical_covenant_witness_bytes_v1(self)
    }

    pub fn commitment(&self) -> Result<[u8; 32], CovenantV1Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantSpendContextV1 {
    pub version: u16,
    pub current_height: u64,
    pub median_time_seconds: u64,
    #[serde(deserialize_with = "deserialize_bounded_signers")]
    pub authenticated_signers: Vec<[u8; 32]>,
}

impl CovenantSpendContextV1 {
    pub fn validate(&self) -> Result<(), CovenantV1Error> {
        if self.version != COVENANT_SPEND_CONTEXT_VERSION_V1 {
            return Err(CovenantV1Error::UnsupportedSpendContextVersion(
                self.version,
            ));
        }
        if !self.authenticated_signers.is_empty() {
            validate_sorted_unique_signers(&self.authenticated_signers)?;
        }
        Ok(())
    }

    fn has_signer(&self, signer: &[u8; 32]) -> bool {
        self.authenticated_signers.binary_search(signer).is_ok()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct CovenantResourceUsageV1 {
    pub compute_units: u64,
    pub witness_bytes: u32,
    pub signer_checks: u16,
    pub hash_checks: u16,
    pub lock_checks: u16,
    pub state_checks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CovenantEvaluationV1 {
    pub program_commitment: [u8; 32],
    pub witness_commitment: [u8; 32],
    pub resource_usage: CovenantResourceUsageV1,
}

pub fn evaluate_covenant_v1(
    descriptor: &CovenantDescriptorV1,
    program: &CovenantProgramV1,
    witness: &CovenantWitnessV1,
    context: &CovenantSpendContextV1,
) -> Result<CovenantEvaluationV1, CovenantV1Error> {
    descriptor.validate()?;
    program.validate()?;
    witness.validate()?;
    context.validate()?;

    if descriptor.kind != program.kind() {
        return Err(CovenantV1Error::DescriptorKindMismatch);
    }
    let program_commitment = program.commitment()?;
    if descriptor.parameters_commitment != program_commitment {
        return Err(CovenantV1Error::DescriptorCommitmentMismatch);
    }

    let witness_bytes = witness.canonical_bytes()?;
    let witness_size =
        u32::try_from(witness_bytes.len()).map_err(|_| CovenantV1Error::CanonicalLengthOverflow)?;
    if witness_size > descriptor.witness_budget_bytes {
        return Err(CovenantV1Error::WitnessBudgetExceeded {
            actual: witness_size,
            max: descriptor.witness_budget_bytes,
        });
    }

    let mut meter = CostMeterV1::new(witness_size)?;
    match (&program.body, &witness.branch) {
        (CovenantProgramBodyV1::Timelock(program), CovenantWitnessBranchV1::Timelock) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.required_signer)?;
            meter.lock_checks(1)?;
            require_lock(context, program.lock)?;
        }
        (CovenantProgramBodyV1::Vault(program), CovenantWitnessBranchV1::VaultHot) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.hot_signer)?;
            meter.lock_checks(1)?;
            let not_before = program
                .activation_height
                .checked_add(program.delay_blocks)
                .ok_or(CovenantV1Error::ArithmeticOverflow)?;
            if context.current_height < not_before {
                return Err(CovenantV1Error::LockNotSatisfied);
            }
        }
        (CovenantProgramBodyV1::Vault(program), CovenantWitnessBranchV1::VaultRecovery) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.recovery_signer)?;
        }
        (CovenantProgramBodyV1::Multisig(program), CovenantWitnessBranchV1::Multisig) => {
            let signer_checks = u16::try_from(program.signers.len())
                .map_err(|_| CovenantV1Error::ArithmeticOverflow)?;
            meter.signer_checks(signer_checks)?;
            let present = program
                .signers
                .iter()
                .filter(|signer| context.has_signer(signer))
                .count();
            if present < usize::from(program.threshold) {
                return Err(CovenantV1Error::ThresholdNotMet {
                    required: program.threshold,
                    present: u8::try_from(present)
                        .map_err(|_| CovenantV1Error::ArithmeticOverflow)?,
                });
            }
        }
        (
            CovenantProgramBodyV1::Escrow(program),
            CovenantWitnessBranchV1::Escrow(resolution),
        ) => {
            meter.signer_checks(2)?;
            match resolution {
                EscrowResolutionV1::Cooperative => {
                    require_signer(context, &program.buyer_signer)?;
                    require_signer(context, &program.seller_signer)?;
                }
                EscrowResolutionV1::BuyerAward => {
                    require_signer(context, &program.buyer_signer)?;
                    require_signer(context, &program.arbiter_signer)?;
                }
                EscrowResolutionV1::SellerAward => {
                    require_signer(context, &program.seller_signer)?;
                    require_signer(context, &program.arbiter_signer)?;
                }
            }
        }
        (
            CovenantProgramBodyV1::AtomicSwap(program),
            CovenantWitnessBranchV1::AtomicSwapRedeem { preimage },
        ) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.redeem_signer)?;
            meter.lock_checks(1)?;
            if context.current_height >= program.refund_height {
                return Err(CovenantV1Error::LockNotSatisfied);
            }
            meter.hash_checks(1)?;
            if sha256_array(preimage) != program.hashlock {
                return Err(CovenantV1Error::HashlockMismatch);
            }
        }
        (
            CovenantProgramBodyV1::AtomicSwap(program),
            CovenantWitnessBranchV1::AtomicSwapRefund,
        ) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.refund_signer)?;
            meter.lock_checks(1)?;
            if context.current_height < program.refund_height {
                return Err(CovenantV1Error::LockNotSatisfied);
            }
        }
        (
            CovenantProgramBodyV1::PaymentChannel(program),
            CovenantWitnessBranchV1::PaymentChannelCooperative,
        ) => {
            meter.signer_checks(2)?;
            require_signer(context, &program.settlement_signer)?;
            require_signer(context, &program.refund_signer)?;
        }
        (
            CovenantProgramBodyV1::PaymentChannel(program),
            CovenantWitnessBranchV1::PaymentChannelSettle {
                state_sequence,
                state_commitment,
            },
        ) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.settlement_signer)?;
            meter.lock_checks(2)?;
            if context.current_height < program.settle_not_before_height
                || context.current_height >= program.refund_after_height
            {
                return Err(CovenantV1Error::LockNotSatisfied);
            }
            meter.state_checks(2)?;
            if *state_sequence != program.state_sequence {
                return Err(CovenantV1Error::ChannelSequenceMismatch);
            }
            if *state_commitment != program.state_commitment {
                return Err(CovenantV1Error::ChannelStateCommitmentMismatch);
            }
        }
        (
            CovenantProgramBodyV1::PaymentChannel(program),
            CovenantWitnessBranchV1::PaymentChannelRefund,
        ) => {
            meter.signer_checks(1)?;
            require_signer(context, &program.refund_signer)?;
            meter.lock_checks(1)?;
            if context.current_height < program.refund_after_height {
                return Err(CovenantV1Error::LockNotSatisfied);
            }
        }
        _ => return Err(CovenantV1Error::InvalidWitnessBranch),
    }

    Ok(CovenantEvaluationV1 {
        program_commitment,
        witness_commitment: sha256_array(&witness_bytes),
        resource_usage: meter.finish(),
    })
}

pub fn canonical_covenant_program_bytes_v1(
    program: &CovenantProgramV1,
) -> Result<Vec<u8>, CovenantV1Error> {
    program.validate()?;
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed(&mut out, COVENANT_PROGRAM_DOMAIN_V1)?;
    out.extend_from_slice(&program.version.to_le_bytes());
    out.extend_from_slice(&covenant_kind_id(program.kind()).to_le_bytes());

    match &program.body {
        CovenantProgramBodyV1::Timelock(program) => {
            out.extend_from_slice(&program.required_signer);
            encode_lock(&mut out, program.lock);
        }
        CovenantProgramBodyV1::Vault(program) => {
            out.extend_from_slice(&program.hot_signer);
            out.extend_from_slice(&program.recovery_signer);
            out.extend_from_slice(&program.activation_height.to_le_bytes());
            out.extend_from_slice(&program.delay_blocks.to_le_bytes());
        }
        CovenantProgramBodyV1::Multisig(program) => {
            out.push(program.threshold);
            let count = u16::try_from(program.signers.len())
                .map_err(|_| CovenantV1Error::CanonicalLengthOverflow)?;
            out.extend_from_slice(&count.to_le_bytes());
            for signer in &program.signers {
                out.extend_from_slice(signer);
            }
        }
        CovenantProgramBodyV1::Escrow(program) => {
            out.extend_from_slice(&program.buyer_signer);
            out.extend_from_slice(&program.seller_signer);
            out.extend_from_slice(&program.arbiter_signer);
        }
        CovenantProgramBodyV1::AtomicSwap(program) => {
            out.extend_from_slice(&program.redeem_signer);
            out.extend_from_slice(&program.refund_signer);
            out.extend_from_slice(&program.hashlock);
            out.extend_from_slice(&program.refund_height.to_le_bytes());
        }
        CovenantProgramBodyV1::PaymentChannel(program) => {
            out.extend_from_slice(&program.settlement_signer);
            out.extend_from_slice(&program.refund_signer);
            out.extend_from_slice(&program.state_commitment);
            out.extend_from_slice(&program.state_sequence.to_le_bytes());
            out.extend_from_slice(&program.settle_not_before_height.to_le_bytes());
            out.extend_from_slice(&program.refund_after_height.to_le_bytes());
        }
    }
    Ok(out)
}

pub fn canonical_covenant_witness_bytes_v1(
    witness: &CovenantWitnessV1,
) -> Result<Vec<u8>, CovenantV1Error> {
    witness.validate()?;
    let mut out = Vec::with_capacity(128);
    encode_len_prefixed(&mut out, COVENANT_WITNESS_DOMAIN_V1)?;
    out.extend_from_slice(&witness.version.to_le_bytes());
    match &witness.branch {
        CovenantWitnessBranchV1::Timelock => out.push(1),
        CovenantWitnessBranchV1::VaultHot => out.push(2),
        CovenantWitnessBranchV1::VaultRecovery => out.push(3),
        CovenantWitnessBranchV1::Multisig => out.push(4),
        CovenantWitnessBranchV1::Escrow(resolution) => {
            out.push(5);
            out.push(match resolution {
                EscrowResolutionV1::Cooperative => 1,
                EscrowResolutionV1::BuyerAward => 2,
                EscrowResolutionV1::SellerAward => 3,
            });
        }
        CovenantWitnessBranchV1::AtomicSwapRedeem { preimage } => {
            out.push(6);
            encode_len_prefixed(&mut out, preimage)?;
        }
        CovenantWitnessBranchV1::AtomicSwapRefund => out.push(7),
        CovenantWitnessBranchV1::PaymentChannelCooperative => out.push(8),
        CovenantWitnessBranchV1::PaymentChannelSettle {
            state_sequence,
            state_commitment,
        } => {
            out.push(9);
            out.extend_from_slice(&state_sequence.to_le_bytes());
            out.extend_from_slice(state_commitment);
        }
        CovenantWitnessBranchV1::PaymentChannelRefund => out.push(10),
    }
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

fn encode_lock(out: &mut Vec<u8>, lock: CovenantLockV1) {
    match lock {
        CovenantLockV1::Height(height) => {
            out.push(1);
            out.extend_from_slice(&height.to_le_bytes());
        }
        CovenantLockV1::MedianTimeSeconds(seconds) => {
            out.push(2);
            out.extend_from_slice(&seconds.to_le_bytes());
        }
    }
}

fn require_lock(
    context: &CovenantSpendContextV1,
    lock: CovenantLockV1,
) -> Result<(), CovenantV1Error> {
    let satisfied = match lock {
        CovenantLockV1::Height(height) => context.current_height >= height,
        CovenantLockV1::MedianTimeSeconds(seconds) => context.median_time_seconds >= seconds,
    };
    if !satisfied {
        return Err(CovenantV1Error::LockNotSatisfied);
    }
    Ok(())
}

fn require_signer(
    context: &CovenantSpendContextV1,
    signer: &[u8; 32],
) -> Result<(), CovenantV1Error> {
    if !context.has_signer(signer) {
        return Err(CovenantV1Error::MissingRequiredSigner);
    }
    Ok(())
}

fn validate_signer(signer: &[u8; 32]) -> Result<(), CovenantV1Error> {
    if signer.iter().all(|byte| *byte == 0) {
        return Err(CovenantV1Error::InvalidSigner);
    }
    Ok(())
}

fn validate_distinct_signers(signers: &[[u8; 32]]) -> Result<(), CovenantV1Error> {
    for signer in signers {
        validate_signer(signer)?;
    }
    for left in 0..signers.len() {
        for right in (left + 1)..signers.len() {
            if signers[left] == signers[right] {
                return Err(CovenantV1Error::DuplicateSignerRole);
            }
        }
    }
    Ok(())
}

fn validate_sorted_unique_signers(signers: &[[u8; 32]]) -> Result<(), CovenantV1Error> {
    if signers.is_empty() || signers.len() > COVENANT_MAX_SIGNERS_V1 {
        return Err(CovenantV1Error::InvalidSignerSet);
    }
    for signer in signers {
        if validate_signer(signer).is_err() {
            return Err(CovenantV1Error::InvalidSignerSet);
        }
    }
    if signers.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CovenantV1Error::InvalidSignerSet);
    }
    Ok(())
}

fn ensure_nonzero_32(value: &[u8; 32]) -> Result<(), ()> {
    if value.iter().all(|byte| *byte == 0) {
        return Err(());
    }
    Ok(())
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), CovenantV1Error> {
    let len = u32::try_from(bytes.len()).map_err(|_| CovenantV1Error::CanonicalLengthOverflow)?;
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

fn deserialize_bounded_preimage<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Vec::<u8>::deserialize(deserializer)?;
    if value.len() > COVENANT_MAX_PREIMAGE_BYTES_V1 {
        return Err(D::Error::custom(format!(
            "preimage length {} exceeds maximum {}",
            value.len(),
            COVENANT_MAX_PREIMAGE_BYTES_V1
        )));
    }
    Ok(value)
}

fn deserialize_bounded_signers<'de, D>(deserializer: D) -> Result<Vec<[u8; 32]>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Vec::<[u8; 32]>::deserialize(deserializer)?;
    if value.len() > COVENANT_MAX_SIGNERS_V1 {
        return Err(D::Error::custom(format!(
            "signer count {} exceeds maximum {}",
            value.len(),
            COVENANT_MAX_SIGNERS_V1
        )));
    }
    Ok(value)
}

struct CostMeterV1 {
    usage: CovenantResourceUsageV1,
}

impl CostMeterV1 {
    fn new(witness_bytes: u32) -> Result<Self, CovenantV1Error> {
        let witness_cost = u64::from(witness_bytes)
            .checked_mul(COVENANT_COST_WITNESS_BYTE_UNITS_V1)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        let compute_units = COVENANT_COST_BASE_UNITS_V1
            .checked_add(witness_cost)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        Ok(Self {
            usage: CovenantResourceUsageV1 {
                compute_units,
                witness_bytes,
                ..Default::default()
            },
        })
    }

    fn signer_checks(&mut self, count: u16) -> Result<(), CovenantV1Error> {
        self.usage.signer_checks = self
            .usage
            .signer_checks
            .checked_add(count)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        self.charge(u64::from(count), COVENANT_COST_SIGNER_CHECK_UNITS_V1)
    }

    fn hash_checks(&mut self, count: u16) -> Result<(), CovenantV1Error> {
        self.usage.hash_checks = self
            .usage
            .hash_checks
            .checked_add(count)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        self.charge(u64::from(count), COVENANT_COST_HASH_CHECK_UNITS_V1)
    }

    fn lock_checks(&mut self, count: u16) -> Result<(), CovenantV1Error> {
        self.usage.lock_checks = self
            .usage
            .lock_checks
            .checked_add(count)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        self.charge(u64::from(count), COVENANT_COST_LOCK_CHECK_UNITS_V1)
    }

    fn state_checks(&mut self, count: u16) -> Result<(), CovenantV1Error> {
        self.usage.state_checks = self
            .usage
            .state_checks
            .checked_add(count)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        self.charge(u64::from(count), COVENANT_COST_STATE_CHECK_UNITS_V1)
    }

    fn charge(&mut self, count: u64, unit_cost: u64) -> Result<(), CovenantV1Error> {
        let delta = count
            .checked_mul(unit_cost)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        self.usage.compute_units = self
            .usage
            .compute_units
            .checked_add(delta)
            .ok_or(CovenantV1Error::ArithmeticOverflow)?;
        Ok(())
    }

    fn finish(self) -> CovenantResourceUsageV1 {
        self.usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn context(height: u64, signers: Vec<[u8; 32]>) -> CovenantSpendContextV1 {
        CovenantSpendContextV1 {
            version: COVENANT_SPEND_CONTEXT_VERSION_V1,
            current_height: height,
            median_time_seconds: 10_000,
            authenticated_signers: signers,
        }
    }

    fn descriptor(program: &CovenantProgramV1) -> CovenantDescriptorV1 {
        program.descriptor(4_096).unwrap()
    }

    fn witness(branch: CovenantWitnessBranchV1) -> CovenantWitnessV1 {
        CovenantWitnessV1 {
            version: COVENANT_WITNESS_VERSION_V1,
            branch,
        }
    }

    #[test]
    fn timelock_height_and_median_time_paths_are_deterministic() {
        let height_program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::Height(100),
            }),
        };
        let height_witness = witness(CovenantWitnessBranchV1::Timelock);
        assert!(evaluate_covenant_v1(
            &descriptor(&height_program),
            &height_program,
            &height_witness,
            &context(100, vec![signer(1)])
        )
        .is_ok());
        assert_eq!(
            evaluate_covenant_v1(
                &descriptor(&height_program),
                &height_program,
                &height_witness,
                &context(99, vec![signer(1)])
            )
            .unwrap_err()
            .rejection_code(),
            CovenantV1Error::LockNotSatisfied.rejection_code()
        );

        let time_program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::MedianTimeSeconds(9_999),
            }),
        };
        assert!(evaluate_covenant_v1(
            &descriptor(&time_program),
            &time_program,
            &height_witness,
            &context(1, vec![signer(1)])
        )
        .is_ok());
    }

    #[test]
    fn vault_hot_delay_and_recovery_paths_pass() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Vault(VaultCovenantV1 {
                hot_signer: signer(1),
                recovery_signer: signer(2),
                activation_height: 100,
                delay_blocks: 20,
            }),
        };
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::VaultHot),
            &context(120, vec![signer(1)])
        )
        .is_ok());
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::VaultRecovery),
            &context(1, vec![signer(2)])
        )
        .is_ok());
    }

    #[test]
    fn multisig_threshold_is_bounded_and_order_independent_at_authentication_boundary() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Multisig(MultisigCovenantV1 {
                threshold: 2,
                signers: vec![signer(1), signer(2), signer(3)],
            }),
        };
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::Multisig),
            &context(0, vec![signer(1), signer(3)])
        )
        .is_ok());
        assert!(matches!(
            evaluate_covenant_v1(
                &descriptor(&program),
                &program,
                &witness(CovenantWitnessBranchV1::Multisig),
                &context(0, vec![signer(1)])
            ),
            Err(CovenantV1Error::ThresholdNotMet { .. })
        ));
    }

    #[test]
    fn escrow_cooperative_and_arbitrated_paths_pass() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Escrow(EscrowCovenantV1 {
                buyer_signer: signer(1),
                seller_signer: signer(2),
                arbiter_signer: signer(3),
            }),
        };
        for (resolution, signers) in [
            (
                EscrowResolutionV1::Cooperative,
                vec![signer(1), signer(2)],
            ),
            (EscrowResolutionV1::BuyerAward, vec![signer(1), signer(3)]),
            (
                EscrowResolutionV1::SellerAward,
                vec![signer(2), signer(3)],
            ),
        ] {
            assert!(evaluate_covenant_v1(
                &descriptor(&program),
                &program,
                &witness(CovenantWitnessBranchV1::Escrow(resolution)),
                &context(0, signers)
            )
            .is_ok());
        }
    }

    #[test]
    fn atomic_swap_redeem_and_refund_paths_pass() {
        let preimage = b"pulse-secret".to_vec();
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::AtomicSwap(AtomicSwapCovenantV1 {
                redeem_signer: signer(1),
                refund_signer: signer(2),
                hashlock: sha256_array(&preimage),
                refund_height: 500,
            }),
        };
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::AtomicSwapRedeem { preimage }),
            &context(499, vec![signer(1)])
        )
        .is_ok());
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::AtomicSwapRefund),
            &context(500, vec![signer(2)])
        )
        .is_ok());
    }

    #[test]
    fn payment_channel_cooperative_settle_and_refund_paths_pass() {
        let state = [9_u8; 32];
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::PaymentChannel(PaymentChannelCovenantV1 {
                settlement_signer: signer(1),
                refund_signer: signer(2),
                state_commitment: state,
                state_sequence: 7,
                settle_not_before_height: 100,
                refund_after_height: 200,
            }),
        };
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::PaymentChannelCooperative),
            &context(1, vec![signer(1), signer(2)])
        )
        .is_ok());
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::PaymentChannelSettle {
                state_sequence: 7,
                state_commitment: state,
            }),
            &context(150, vec![signer(1)])
        )
        .is_ok());
        assert!(evaluate_covenant_v1(
            &descriptor(&program),
            &program,
            &witness(CovenantWitnessBranchV1::PaymentChannelRefund),
            &context(200, vec![signer(2)])
        )
        .is_ok());
    }

    #[test]
    fn descriptor_commitment_and_witness_budget_fail_closed() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::Height(1),
            }),
        };
        let mut bad_descriptor = descriptor(&program);
        bad_descriptor.parameters_commitment = [0xaa; 32];
        assert!(matches!(
            evaluate_covenant_v1(
                &bad_descriptor,
                &program,
                &witness(CovenantWitnessBranchV1::Timelock),
                &context(1, vec![signer(1)])
            ),
            Err(CovenantV1Error::DescriptorCommitmentMismatch)
        ));

        let mut tiny_budget = descriptor(&program);
        tiny_budget.witness_budget_bytes = 1;
        assert!(matches!(
            evaluate_covenant_v1(
                &tiny_budget,
                &program,
                &witness(CovenantWitnessBranchV1::Timelock),
                &context(1, vec![signer(1)])
            ),
            Err(CovenantV1Error::WitnessBudgetExceeded { .. })
        ));
    }

    #[test]
    fn multisig_program_commitment_golden_vector_is_frozen() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Multisig(MultisigCovenantV1 {
                threshold: 2,
                signers: vec![signer(1), signer(2), signer(3)],
            }),
        };
        assert_eq!(
            hex::encode(program.commitment().unwrap()),
            "eece867dffa1c1f3b124c224a326457604ce0571640fa821f63d4d649f7163fb"
        );
    }

    #[test]
    fn resource_accounting_is_replay_stable() {
        let program = CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Multisig(MultisigCovenantV1 {
                threshold: 2,
                signers: vec![signer(1), signer(2), signer(3)],
            }),
        };
        let descriptor = descriptor(&program);
        let witness = witness(CovenantWitnessBranchV1::Multisig);
        let context = context(0, vec![signer(1), signer(2)]);
        let first = evaluate_covenant_v1(&descriptor, &program, &witness, &context).unwrap();
        let second = evaluate_covenant_v1(&descriptor, &program, &witness, &context).unwrap();
        assert_eq!(first, second);
        assert!(first.resource_usage.compute_units > 0);
    }

    #[test]
    fn strict_serde_rejects_unknown_fields_and_oversized_signer_sets() {
        let mut value = serde_json::to_value(CovenantProgramV1 {
            version: COVENANT_PROGRAM_VERSION_V1,
            body: CovenantProgramBodyV1::Timelock(TimelockCovenantV1 {
                required_signer: signer(1),
                lock: CovenantLockV1::Height(1),
            }),
        })
        .unwrap();
        value["future_field"] = serde_json::json!(1);
        assert!(serde_json::from_value::<CovenantProgramV1>(value).is_err());

        let value = serde_json::json!({
            "threshold": 1,
            "signers": vec![signer(1); COVENANT_MAX_SIGNERS_V1 + 1],
        });
        assert!(serde_json::from_value::<MultisigCovenantV1>(value).is_err());
    }
}
