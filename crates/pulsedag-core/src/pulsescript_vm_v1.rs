//! Inactive PulseScript compiler kernel and deterministic bounded Contract VM foundation.
//!
//! This module is the first narrow #1042 slice. It freezes a small, typed,
//! branch-free PulseScript v1 kernel and a canonical bytecode/VM contract so
//! compiler and execution semantics can be tested before any activation or
//! node wiring exists.
//!
//! Consensus-safety properties of this slice:
//! - integer-only values and checked u64 arithmetic;
//! - bounded source, bytecode, instruction count, stack and local memory;
//! - no calls, recursion, state access, filesystem, network, host time,
//!   randomness or hardware-dependent behavior;
//! - deterministic bytecode, program fingerprints and rejection codes;
//! - malformed bytecode is bounded before allocation and fails closed.
//!
//! This module is not connected to transaction admission, state application,
//! RPC, storage, mining or the programmability activation gate.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::contract_v3::{ContractV3Error, ResourceBudgetV1, CONTRACT_V3_MAX_COMPUTE_UNITS};

pub const PULSESCRIPT_COMPILER_VERSION_V1: u16 = 1;
pub const PULSE_BYTECODE_VERSION_V1: u16 = 1;
pub const PULSE_VM_VERSION_V1: u16 = 1;

pub const PULSESCRIPT_MAX_SOURCE_BYTES_V1: u32 = 16 * 1024;
pub const PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1: u32 = PULSESCRIPT_MAX_SOURCE_BYTES_V1 * 2;
pub const PULSE_VM_MAX_BYTECODE_BYTES_V1: u32 = 64 * 1024;
pub const PULSE_VM_MAX_INSTRUCTIONS_V1: u32 = 1024;
pub const PULSE_VM_MAX_STACK_VALUES_V1: u16 = 256;
pub const PULSE_VM_MAX_LOCAL_SLOTS_V1: u16 = 64;
pub const PULSE_VM_MAX_CALL_DEPTH_V1: u16 = 0;
pub const PULSE_VM_MAX_STATE_ACCESSES_V1: u16 = 0;
pub const PULSE_VM_CONSENSUS_VALUE_BYTES_V1: u16 = 9;

const PULSE_BYTECODE_MAGIC_V1: [u8; 4] = *b"PDPS";
const PULSE_BYTECODE_FINGERPRINT_DOMAIN_V1: &[u8] = b"PulseDAG:pulsescript-bytecode:v1";
const PULSE_TOOLCHAIN_FINGERPRINT_DOMAIN_V1: &[u8] = b"PulseDAG:pulsescript-vm-toolchain:v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PulseOpcodeKindV1 {
    PushU64,
    PushBool,
    AddU64,
    SubU64,
    MulU64,
    EqU64,
    AndBool,
    OrBool,
    NotBool,
    Dup,
    Drop,
    StoreLocal,
    LoadLocal,
    Halt,
}

#[derive(Debug, Clone, Copy)]
struct PulseOpcodeSpecV1 {
    kind: PulseOpcodeKindV1,
    opcode: u8,
    name: &'static str,
    operand_bytes: u8,
    compute_cost: u64,
}

const PULSE_OPCODE_SPECS_V1: [PulseOpcodeSpecV1; 14] = [
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::PushU64,
        opcode: 0x01,
        name: "push_u64",
        operand_bytes: 8,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::PushBool,
        opcode: 0x02,
        name: "push_bool",
        operand_bytes: 1,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::AddU64,
        opcode: 0x10,
        name: "add_u64",
        operand_bytes: 0,
        compute_cost: 2,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::SubU64,
        opcode: 0x11,
        name: "sub_u64",
        operand_bytes: 0,
        compute_cost: 2,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::MulU64,
        opcode: 0x12,
        name: "mul_u64",
        operand_bytes: 0,
        compute_cost: 2,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::EqU64,
        opcode: 0x13,
        name: "eq_u64",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::AndBool,
        opcode: 0x20,
        name: "and_bool",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::OrBool,
        opcode: 0x21,
        name: "or_bool",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::NotBool,
        opcode: 0x22,
        name: "not_bool",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::Dup,
        opcode: 0x30,
        name: "dup",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::Drop,
        opcode: 0x31,
        name: "drop",
        operand_bytes: 0,
        compute_cost: 1,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::StoreLocal,
        opcode: 0x40,
        name: "store_local",
        operand_bytes: 1,
        compute_cost: 2,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::LoadLocal,
        opcode: 0x41,
        name: "load_local",
        operand_bytes: 1,
        compute_cost: 2,
    },
    PulseOpcodeSpecV1 {
        kind: PulseOpcodeKindV1::Halt,
        opcode: 0xff,
        name: "halt",
        operand_bytes: 0,
        compute_cost: 0,
    },
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PulseValueTypeV1 {
    U64,
    Bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PulseValueV1 {
    U64(u64),
    Bool(bool),
}

impl PulseValueV1 {
    pub fn value_type(self) -> PulseValueTypeV1 {
        match self {
            Self::U64(_) => PulseValueTypeV1::U64,
            Self::Bool(_) => PulseValueTypeV1::Bool,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseInstructionV1 {
    PushU64(u64),
    PushBool(bool),
    AddU64,
    SubU64,
    MulU64,
    EqU64,
    AndBool,
    OrBool,
    NotBool,
    Dup,
    Drop,
    StoreLocal(u8),
    LoadLocal(u8),
    Halt,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PulseStaticAnalysisV1 {
    pub instruction_count: u32,
    pub compute_units: u64,
    pub max_stack_values: u16,
    pub local_slots_used: u16,
    pub result_type: PulseValueTypeV1,
    pub max_call_depth: u16,
    pub state_accesses: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPulseScriptV1 {
    pub bytecode: Vec<u8>,
    pub bytecode_fingerprint: [u8; 32],
    pub analysis: PulseStaticAnalysisV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PulseVmResourceUsageV1 {
    pub compute_units: u64,
    pub peak_stack_values: u16,
    pub local_slots_used: u16,
    pub call_depth_peak: u16,
    pub state_accesses: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PulseVmExecutionV1 {
    pub result: PulseValueV1,
    pub executed_instructions: u32,
    pub usage: PulseVmResourceUsageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PulseVmV1Error {
    #[error("PulseScript source length {actual} exceeds maximum {max}")]
    SourceTooLarge { actual: usize, max: u32 },
    #[error("PulseScript source must begin with exact header pulsescript-v1")]
    UnsupportedCompilerHeader,
    #[error("PulseScript syntax error on source line {line}")]
    SourceSyntax { line: u32 },
    #[error("invalid PulseScript literal on source line {line}")]
    InvalidLiteral { line: u32 },
    #[error("PulseScript program exceeds maximum instruction count {max}")]
    TooManyInstructions { max: u32 },
    #[error("PulseScript local index {index} exceeds maximum slot {max}")]
    LocalIndexOutOfRange { index: u16, max: u16 },
    #[error("PulseScript type stack underflow at instruction {instruction}")]
    TypeStackUnderflow { instruction: u32 },
    #[error(
        "PulseScript type mismatch at instruction {instruction}: expected {expected:?}, got {actual:?}"
    )]
    TypeMismatch {
        instruction: u32,
        expected: PulseValueTypeV1,
        actual: PulseValueTypeV1,
    },
    #[error("PulseScript local {index} is read before deterministic initialization")]
    UninitializedLocal { index: u8 },
    #[error("PulseScript stack depth exceeds maximum {max}")]
    StackLimitExceeded { max: u16 },
    #[error("PulseScript bytecode must contain exactly one final halt instruction")]
    MissingOrMisplacedHalt,
    #[error("PulseScript halt requires exactly one result value, found {actual}")]
    InvalidResultArity { actual: usize },
    #[error("PulseVM bytecode length {actual} exceeds maximum {max}")]
    BytecodeTooLarge { actual: usize, max: u32 },
    #[error("PulseVM bytecode header or magic is invalid")]
    InvalidBytecodeHeader,
    #[error("unsupported PulseVM bytecode version {0}")]
    UnsupportedBytecodeVersion(u16),
    #[error("unsupported PulseScript compiler version {0}")]
    UnsupportedCompilerVersion(u16),
    #[error("PulseVM bytecode instruction count {actual} exceeds maximum {max}")]
    InvalidInstructionCount { actual: u32, max: u32 },
    #[error("PulseVM bytecode is truncated")]
    TruncatedBytecode,
    #[error("PulseVM bytecode contains unknown opcode 0x{0:02x}")]
    UnknownOpcode(u8),
    #[error("PulseVM bytecode contains non-canonical boolean operand {0}")]
    InvalidBooleanEncoding(u8),
    #[error("PulseVM bytecode contains trailing bytes")]
    TrailingBytecode,
    #[error("PulseVM checked u64 arithmetic overflow or underflow at instruction {instruction}")]
    ArithmeticOverflow { instruction: u32 },
    #[error("PulseVM compute usage {actual} exceeds transaction budget {max}")]
    ComputeBudgetExceeded { actual: u64, max: u64 },
    #[error("PulseVM deterministic compute accounting overflow")]
    ComputeAccountingOverflow,
    #[error("PulseVM runtime type invariant failed at instruction {instruction}")]
    RuntimeTypeInvariant { instruction: u32 },
    #[error("invalid Contract v3 resource budget: {0}")]
    ContractV3(#[from] ContractV3Error),
    #[error("PulseVM canonical opcode table invariant failed")]
    OpcodeTableInvariant,
}

impl PulseVmV1Error {
    pub fn rejection_code(&self) -> u16 {
        match self {
            Self::SourceTooLarge { .. } => 1,
            Self::UnsupportedCompilerHeader => 2,
            Self::SourceSyntax { .. } => 3,
            Self::InvalidLiteral { .. } => 4,
            Self::TooManyInstructions { .. } => 5,
            Self::LocalIndexOutOfRange { .. } => 6,
            Self::TypeStackUnderflow { .. } => 7,
            Self::TypeMismatch { .. } => 8,
            Self::UninitializedLocal { .. } => 9,
            Self::StackLimitExceeded { .. } => 10,
            Self::MissingOrMisplacedHalt => 11,
            Self::InvalidResultArity { .. } => 12,
            Self::BytecodeTooLarge { .. } => 13,
            Self::InvalidBytecodeHeader => 14,
            Self::UnsupportedBytecodeVersion(_) => 15,
            Self::UnsupportedCompilerVersion(_) => 16,
            Self::InvalidInstructionCount { .. } => 17,
            Self::TruncatedBytecode => 18,
            Self::UnknownOpcode(_) => 19,
            Self::InvalidBooleanEncoding(_) => 20,
            Self::TrailingBytecode => 21,
            Self::ArithmeticOverflow { .. } => 22,
            Self::ComputeBudgetExceeded { .. } => 23,
            Self::ComputeAccountingOverflow => 24,
            Self::RuntimeTypeInvariant { .. } => 25,
            Self::ContractV3(_) => 26,
            Self::OpcodeTableInvariant => 27,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedPulseProgramV1 {
    instructions: Vec<PulseInstructionV1>,
    analysis: PulseStaticAnalysisV1,
}

pub fn pulse_toolchain_fingerprint_v1() -> [u8; 32] {
    let mut bytes = Vec::with_capacity(320);
    encode_len_prefixed(&mut bytes, PULSE_TOOLCHAIN_FINGERPRINT_DOMAIN_V1);
    bytes.extend_from_slice(&PULSESCRIPT_COMPILER_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_BYTECODE_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSESCRIPT_MAX_SOURCE_BYTES_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_BYTECODE_BYTES_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_INSTRUCTIONS_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_STACK_VALUES_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_LOCAL_SLOTS_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_CALL_DEPTH_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_MAX_STATE_ACCESSES_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSE_VM_CONSENSUS_VALUE_BYTES_V1.to_le_bytes());
    bytes.extend_from_slice(&CONTRACT_V3_MAX_COMPUTE_UNITS.to_le_bytes());
    for spec in PULSE_OPCODE_SPECS_V1 {
        bytes.push(spec.opcode);
        encode_len_prefixed(&mut bytes, spec.name.as_bytes());
        bytes.push(spec.operand_bytes);
        bytes.extend_from_slice(&spec.compute_cost.to_le_bytes());
    }
    sha256_array(&bytes)
}

pub fn compile_pulsescript_v1(source: &str) -> Result<CompiledPulseScriptV1, PulseVmV1Error> {
    if source.len() > PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize {
        return Err(PulseVmV1Error::SourceTooLarge {
            actual: source.len(),
            max: PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1,
        });
    }

    let canonical_source = source.replace("\r\n", "\n").replace('\r', "\n");
    if canonical_source.len() > PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize {
        return Err(PulseVmV1Error::SourceTooLarge {
            actual: canonical_source.len(),
            max: PULSESCRIPT_MAX_SOURCE_BYTES_V1,
        });
    }

    let mut header_seen = false;
    let mut instructions = Vec::new();

    for (line_index, raw_line) in canonical_source.lines().enumerate() {
        let line_number = u32::try_from(line_index + 1).unwrap_or(u32::MAX);
        let code = raw_line
            .split_once('#')
            .map_or(raw_line, |(before_comment, _)| before_comment)
            .trim();
        if code.is_empty() {
            continue;
        }

        if !header_seen {
            if code != "pulsescript-v1" {
                return Err(PulseVmV1Error::UnsupportedCompilerHeader);
            }
            header_seen = true;
            continue;
        }

        if instructions.len() >= PULSE_VM_MAX_INSTRUCTIONS_V1 as usize {
            return Err(PulseVmV1Error::TooManyInstructions {
                max: PULSE_VM_MAX_INSTRUCTIONS_V1,
            });
        }

        let tokens = code.split_whitespace().collect::<Vec<_>>();
        let instruction = parse_source_instruction(&tokens, line_number)?;
        instructions.push(instruction);
    }

    if !header_seen {
        return Err(PulseVmV1Error::UnsupportedCompilerHeader);
    }

    let analysis = analyze_instructions(&instructions)?;
    let bytecode = encode_bytecode_v1(&instructions)?;
    let bytecode_fingerprint = pulse_bytecode_fingerprint_v1(&bytecode)?;

    Ok(CompiledPulseScriptV1 {
        bytecode,
        bytecode_fingerprint,
        analysis,
    })
}

pub fn pulse_bytecode_fingerprint_v1(bytecode: &[u8]) -> Result<[u8; 32], PulseVmV1Error> {
    let _ = decode_and_validate_bytecode_v1(bytecode)?;
    let mut bytes =
        Vec::with_capacity(PULSE_BYTECODE_FINGERPRINT_DOMAIN_V1.len() + bytecode.len() + 8);
    encode_len_prefixed(&mut bytes, PULSE_BYTECODE_FINGERPRINT_DOMAIN_V1);
    encode_len_prefixed(&mut bytes, bytecode);
    Ok(sha256_array(&bytes))
}

pub fn validate_pulse_bytecode_v1(
    bytecode: &[u8],
) -> Result<PulseStaticAnalysisV1, PulseVmV1Error> {
    Ok(decode_and_validate_bytecode_v1(bytecode)?.analysis)
}

pub fn execute_pulse_bytecode_v1(
    bytecode: &[u8],
    budget: &ResourceBudgetV1,
) -> Result<PulseVmExecutionV1, PulseVmV1Error> {
    budget.validate()?;
    let program = decode_and_validate_bytecode_v1(bytecode)?;

    if program.analysis.compute_units > budget.compute_units {
        return Err(PulseVmV1Error::ComputeBudgetExceeded {
            actual: program.analysis.compute_units,
            max: budget.compute_units,
        });
    }

    let mut stack = Vec::with_capacity(PULSE_VM_MAX_STACK_VALUES_V1 as usize);
    let mut locals = [None; PULSE_VM_MAX_LOCAL_SLOTS_V1 as usize];
    let mut compute_units = 0u64;
    let mut executed_instructions = 0u32;

    for (index, instruction) in program.instructions.iter().copied().enumerate() {
        let instruction_index = u32::try_from(index).unwrap_or(u32::MAX);
        compute_units = compute_units
            .checked_add(instruction_compute_cost(instruction))
            .ok_or(PulseVmV1Error::ComputeAccountingOverflow)?;
        executed_instructions = executed_instructions
            .checked_add(1)
            .ok_or(PulseVmV1Error::ComputeAccountingOverflow)?;

        match instruction {
            PulseInstructionV1::PushU64(value) => stack.push(PulseValueV1::U64(value)),
            PulseInstructionV1::PushBool(value) => stack.push(PulseValueV1::Bool(value)),
            PulseInstructionV1::AddU64 => {
                let rhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let lhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let value = lhs
                    .checked_add(rhs)
                    .ok_or(PulseVmV1Error::ArithmeticOverflow {
                        instruction: instruction_index,
                    })?;
                stack.push(PulseValueV1::U64(value));
            }
            PulseInstructionV1::SubU64 => {
                let rhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let lhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let value = lhs
                    .checked_sub(rhs)
                    .ok_or(PulseVmV1Error::ArithmeticOverflow {
                        instruction: instruction_index,
                    })?;
                stack.push(PulseValueV1::U64(value));
            }
            PulseInstructionV1::MulU64 => {
                let rhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let lhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let value = lhs
                    .checked_mul(rhs)
                    .ok_or(PulseVmV1Error::ArithmeticOverflow {
                        instruction: instruction_index,
                    })?;
                stack.push(PulseValueV1::U64(value));
            }
            PulseInstructionV1::EqU64 => {
                let rhs = runtime_pop_u64(&mut stack, instruction_index)?;
                let lhs = runtime_pop_u64(&mut stack, instruction_index)?;
                stack.push(PulseValueV1::Bool(lhs == rhs));
            }
            PulseInstructionV1::AndBool => {
                let rhs = runtime_pop_bool(&mut stack, instruction_index)?;
                let lhs = runtime_pop_bool(&mut stack, instruction_index)?;
                stack.push(PulseValueV1::Bool(lhs && rhs));
            }
            PulseInstructionV1::OrBool => {
                let rhs = runtime_pop_bool(&mut stack, instruction_index)?;
                let lhs = runtime_pop_bool(&mut stack, instruction_index)?;
                stack.push(PulseValueV1::Bool(lhs || rhs));
            }
            PulseInstructionV1::NotBool => {
                let value = runtime_pop_bool(&mut stack, instruction_index)?;
                stack.push(PulseValueV1::Bool(!value));
            }
            PulseInstructionV1::Dup => {
                let value = stack
                    .last()
                    .copied()
                    .ok_or(PulseVmV1Error::RuntimeTypeInvariant {
                        instruction: instruction_index,
                    })?;
                stack.push(value);
            }
            PulseInstructionV1::Drop => {
                stack.pop().ok_or(PulseVmV1Error::RuntimeTypeInvariant {
                    instruction: instruction_index,
                })?;
            }
            PulseInstructionV1::StoreLocal(local) => {
                let value = stack.pop().ok_or(PulseVmV1Error::RuntimeTypeInvariant {
                    instruction: instruction_index,
                })?;
                locals[usize::from(local)] = Some(value);
            }
            PulseInstructionV1::LoadLocal(local) => {
                let value =
                    locals[usize::from(local)].ok_or(PulseVmV1Error::RuntimeTypeInvariant {
                        instruction: instruction_index,
                    })?;
                stack.push(value);
            }
            PulseInstructionV1::Halt => break,
        }
    }

    if compute_units != program.analysis.compute_units {
        return Err(PulseVmV1Error::ComputeAccountingOverflow);
    }
    if compute_units > budget.compute_units {
        return Err(PulseVmV1Error::ComputeBudgetExceeded {
            actual: compute_units,
            max: budget.compute_units,
        });
    }

    let result = stack
        .first()
        .copied()
        .ok_or(PulseVmV1Error::RuntimeTypeInvariant {
            instruction: executed_instructions.saturating_sub(1),
        })?;

    Ok(PulseVmExecutionV1 {
        result,
        executed_instructions,
        usage: PulseVmResourceUsageV1 {
            compute_units,
            peak_stack_values: program.analysis.max_stack_values,
            local_slots_used: program.analysis.local_slots_used,
            call_depth_peak: 0,
            state_accesses: 0,
        },
    })
}

fn parse_source_instruction(
    tokens: &[&str],
    line: u32,
) -> Result<PulseInstructionV1, PulseVmV1Error> {
    let opcode_name = tokens
        .first()
        .copied()
        .ok_or(PulseVmV1Error::SourceSyntax { line })?;
    let spec = opcode_spec_for_name_v1(opcode_name).ok_or(PulseVmV1Error::SourceSyntax { line })?;

    match spec.kind {
        PulseOpcodeKindV1::PushU64 if tokens.len() == 2 => {
            let value = tokens[1]
                .parse::<u64>()
                .map_err(|_| PulseVmV1Error::InvalidLiteral { line })?;
            Ok(PulseInstructionV1::PushU64(value))
        }
        PulseOpcodeKindV1::PushBool if tokens.len() == 2 => match tokens[1] {
            "true" => Ok(PulseInstructionV1::PushBool(true)),
            "false" => Ok(PulseInstructionV1::PushBool(false)),
            _ => Err(PulseVmV1Error::InvalidLiteral { line }),
        },
        PulseOpcodeKindV1::AddU64 if tokens.len() == 1 => Ok(PulseInstructionV1::AddU64),
        PulseOpcodeKindV1::SubU64 if tokens.len() == 1 => Ok(PulseInstructionV1::SubU64),
        PulseOpcodeKindV1::MulU64 if tokens.len() == 1 => Ok(PulseInstructionV1::MulU64),
        PulseOpcodeKindV1::EqU64 if tokens.len() == 1 => Ok(PulseInstructionV1::EqU64),
        PulseOpcodeKindV1::AndBool if tokens.len() == 1 => Ok(PulseInstructionV1::AndBool),
        PulseOpcodeKindV1::OrBool if tokens.len() == 1 => Ok(PulseInstructionV1::OrBool),
        PulseOpcodeKindV1::NotBool if tokens.len() == 1 => Ok(PulseInstructionV1::NotBool),
        PulseOpcodeKindV1::Dup if tokens.len() == 1 => Ok(PulseInstructionV1::Dup),
        PulseOpcodeKindV1::Drop if tokens.len() == 1 => Ok(PulseInstructionV1::Drop),
        PulseOpcodeKindV1::StoreLocal if tokens.len() == 2 => {
            let index = parse_local_index(tokens[1], line)?;
            Ok(PulseInstructionV1::StoreLocal(index))
        }
        PulseOpcodeKindV1::LoadLocal if tokens.len() == 2 => {
            let index = parse_local_index(tokens[1], line)?;
            Ok(PulseInstructionV1::LoadLocal(index))
        }
        PulseOpcodeKindV1::Halt if tokens.len() == 1 => Ok(PulseInstructionV1::Halt),
        _ => Err(PulseVmV1Error::SourceSyntax { line }),
    }
}

fn parse_local_index(token: &str, line: u32) -> Result<u8, PulseVmV1Error> {
    let index = token
        .parse::<u16>()
        .map_err(|_| PulseVmV1Error::InvalidLiteral { line })?;
    if index >= PULSE_VM_MAX_LOCAL_SLOTS_V1 {
        return Err(PulseVmV1Error::LocalIndexOutOfRange {
            index,
            max: PULSE_VM_MAX_LOCAL_SLOTS_V1 - 1,
        });
    }
    Ok(index as u8)
}

fn analyze_instructions(
    instructions: &[PulseInstructionV1],
) -> Result<PulseStaticAnalysisV1, PulseVmV1Error> {
    if instructions.len() > PULSE_VM_MAX_INSTRUCTIONS_V1 as usize {
        return Err(PulseVmV1Error::TooManyInstructions {
            max: PULSE_VM_MAX_INSTRUCTIONS_V1,
        });
    }

    let mut stack = Vec::with_capacity(PULSE_VM_MAX_STACK_VALUES_V1 as usize);
    let mut locals = [None; PULSE_VM_MAX_LOCAL_SLOTS_V1 as usize];
    let mut compute_units = 0u64;
    let mut max_stack_values = 0u16;
    let mut local_slots_used = 0u16;
    let mut halted = false;

    for (index, instruction) in instructions.iter().copied().enumerate() {
        let instruction_index = u32::try_from(index).unwrap_or(u32::MAX);
        compute_units = compute_units
            .checked_add(instruction_compute_cost(instruction))
            .ok_or(PulseVmV1Error::ComputeAccountingOverflow)?;

        match instruction {
            PulseInstructionV1::PushU64(_) => {
                push_type(&mut stack, PulseValueTypeV1::U64, &mut max_stack_values)?
            }
            PulseInstructionV1::PushBool(_) => {
                push_type(&mut stack, PulseValueTypeV1::Bool, &mut max_stack_values)?
            }
            PulseInstructionV1::AddU64
            | PulseInstructionV1::SubU64
            | PulseInstructionV1::MulU64 => {
                pop_expected_type(&mut stack, PulseValueTypeV1::U64, instruction_index)?;
                pop_expected_type(&mut stack, PulseValueTypeV1::U64, instruction_index)?;
                push_type(&mut stack, PulseValueTypeV1::U64, &mut max_stack_values)?;
            }
            PulseInstructionV1::EqU64 => {
                pop_expected_type(&mut stack, PulseValueTypeV1::U64, instruction_index)?;
                pop_expected_type(&mut stack, PulseValueTypeV1::U64, instruction_index)?;
                push_type(&mut stack, PulseValueTypeV1::Bool, &mut max_stack_values)?;
            }
            PulseInstructionV1::AndBool | PulseInstructionV1::OrBool => {
                pop_expected_type(&mut stack, PulseValueTypeV1::Bool, instruction_index)?;
                pop_expected_type(&mut stack, PulseValueTypeV1::Bool, instruction_index)?;
                push_type(&mut stack, PulseValueTypeV1::Bool, &mut max_stack_values)?;
            }
            PulseInstructionV1::NotBool => {
                pop_expected_type(&mut stack, PulseValueTypeV1::Bool, instruction_index)?;
                push_type(&mut stack, PulseValueTypeV1::Bool, &mut max_stack_values)?;
            }
            PulseInstructionV1::Dup => {
                let value_type =
                    stack
                        .last()
                        .copied()
                        .ok_or(PulseVmV1Error::TypeStackUnderflow {
                            instruction: instruction_index,
                        })?;
                push_type(&mut stack, value_type, &mut max_stack_values)?;
            }
            PulseInstructionV1::Drop => {
                stack.pop().ok_or(PulseVmV1Error::TypeStackUnderflow {
                    instruction: instruction_index,
                })?;
            }
            PulseInstructionV1::StoreLocal(local) => {
                let value_type = stack.pop().ok_or(PulseVmV1Error::TypeStackUnderflow {
                    instruction: instruction_index,
                })?;
                let slot = usize::from(local);
                if let Some(expected) = locals[slot] {
                    if expected != value_type {
                        return Err(PulseVmV1Error::TypeMismatch {
                            instruction: instruction_index,
                            expected,
                            actual: value_type,
                        });
                    }
                } else {
                    locals[slot] = Some(value_type);
                }
                local_slots_used = local_slots_used.max(u16::from(local) + 1);
            }
            PulseInstructionV1::LoadLocal(local) => {
                let value_type = locals[usize::from(local)]
                    .ok_or(PulseVmV1Error::UninitializedLocal { index: local })?;
                push_type(&mut stack, value_type, &mut max_stack_values)?;
                local_slots_used = local_slots_used.max(u16::from(local) + 1);
            }
            PulseInstructionV1::Halt => {
                if index + 1 != instructions.len() || halted {
                    return Err(PulseVmV1Error::MissingOrMisplacedHalt);
                }
                halted = true;
                if stack.len() != 1 {
                    return Err(PulseVmV1Error::InvalidResultArity {
                        actual: stack.len(),
                    });
                }
            }
        }
    }

    if !halted {
        return Err(PulseVmV1Error::MissingOrMisplacedHalt);
    }

    let result_type = stack[0];

    Ok(PulseStaticAnalysisV1 {
        instruction_count: u32::try_from(instructions.len()).unwrap_or(u32::MAX),
        compute_units,
        max_stack_values,
        local_slots_used,
        result_type,
        max_call_depth: 0,
        state_accesses: 0,
    })
}

fn push_type(
    stack: &mut Vec<PulseValueTypeV1>,
    value_type: PulseValueTypeV1,
    max_stack_values: &mut u16,
) -> Result<(), PulseVmV1Error> {
    if stack.len() >= PULSE_VM_MAX_STACK_VALUES_V1 as usize {
        return Err(PulseVmV1Error::StackLimitExceeded {
            max: PULSE_VM_MAX_STACK_VALUES_V1,
        });
    }
    stack.push(value_type);
    *max_stack_values = (*max_stack_values).max(stack.len() as u16);
    Ok(())
}

fn pop_expected_type(
    stack: &mut Vec<PulseValueTypeV1>,
    expected: PulseValueTypeV1,
    instruction: u32,
) -> Result<(), PulseVmV1Error> {
    let actual = stack
        .pop()
        .ok_or(PulseVmV1Error::TypeStackUnderflow { instruction })?;
    if actual != expected {
        return Err(PulseVmV1Error::TypeMismatch {
            instruction,
            expected,
            actual,
        });
    }
    Ok(())
}

fn encode_bytecode_v1(instructions: &[PulseInstructionV1]) -> Result<Vec<u8>, PulseVmV1Error> {
    if instructions.len() > PULSE_VM_MAX_INSTRUCTIONS_V1 as usize {
        return Err(PulseVmV1Error::TooManyInstructions {
            max: PULSE_VM_MAX_INSTRUCTIONS_V1,
        });
    }

    let mut bytes = Vec::with_capacity(12 + instructions.len() * 9);
    bytes.extend_from_slice(&PULSE_BYTECODE_MAGIC_V1);
    bytes.extend_from_slice(&PULSE_BYTECODE_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&PULSESCRIPT_COMPILER_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(instructions.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );

    for instruction in instructions {
        let spec = opcode_spec_for_instruction_v1(*instruction);
        bytes.push(spec.opcode);
        let operand_start = bytes.len();

        match instruction {
            PulseInstructionV1::PushU64(value) => bytes.extend_from_slice(&value.to_le_bytes()),
            PulseInstructionV1::PushBool(value) => bytes.push(u8::from(*value)),
            PulseInstructionV1::StoreLocal(local) | PulseInstructionV1::LoadLocal(local) => {
                bytes.push(*local);
            }
            PulseInstructionV1::AddU64
            | PulseInstructionV1::SubU64
            | PulseInstructionV1::MulU64
            | PulseInstructionV1::EqU64
            | PulseInstructionV1::AndBool
            | PulseInstructionV1::OrBool
            | PulseInstructionV1::NotBool
            | PulseInstructionV1::Dup
            | PulseInstructionV1::Drop
            | PulseInstructionV1::Halt => {}
        }

        if bytes.len() - operand_start != usize::from(spec.operand_bytes) {
            return Err(PulseVmV1Error::OpcodeTableInvariant);
        }
    }

    if bytes.len() > PULSE_VM_MAX_BYTECODE_BYTES_V1 as usize {
        return Err(PulseVmV1Error::BytecodeTooLarge {
            actual: bytes.len(),
            max: PULSE_VM_MAX_BYTECODE_BYTES_V1,
        });
    }

    Ok(bytes)
}

fn decode_and_validate_bytecode_v1(
    bytecode: &[u8],
) -> Result<ValidatedPulseProgramV1, PulseVmV1Error> {
    if bytecode.len() > PULSE_VM_MAX_BYTECODE_BYTES_V1 as usize {
        return Err(PulseVmV1Error::BytecodeTooLarge {
            actual: bytecode.len(),
            max: PULSE_VM_MAX_BYTECODE_BYTES_V1,
        });
    }

    let mut cursor = 0usize;
    let magic = read_exact::<4>(bytecode, &mut cursor)?;
    if magic != PULSE_BYTECODE_MAGIC_V1 {
        return Err(PulseVmV1Error::InvalidBytecodeHeader);
    }

    let bytecode_version = read_u16(bytecode, &mut cursor)?;
    if bytecode_version != PULSE_BYTECODE_VERSION_V1 {
        return Err(PulseVmV1Error::UnsupportedBytecodeVersion(bytecode_version));
    }

    let compiler_version = read_u16(bytecode, &mut cursor)?;
    if compiler_version != PULSESCRIPT_COMPILER_VERSION_V1 {
        return Err(PulseVmV1Error::UnsupportedCompilerVersion(compiler_version));
    }

    let instruction_count = read_u32(bytecode, &mut cursor)?;
    if instruction_count > PULSE_VM_MAX_INSTRUCTIONS_V1 {
        return Err(PulseVmV1Error::InvalidInstructionCount {
            actual: instruction_count,
            max: PULSE_VM_MAX_INSTRUCTIONS_V1,
        });
    }

    let mut instructions = Vec::with_capacity(instruction_count as usize);
    for _ in 0..instruction_count {
        let opcode = read_u8(bytecode, &mut cursor)?;
        let spec = opcode_spec_for_byte_v1(opcode).ok_or(PulseVmV1Error::UnknownOpcode(opcode))?;
        let operand_end = cursor
            .checked_add(usize::from(spec.operand_bytes))
            .ok_or(PulseVmV1Error::TruncatedBytecode)?;
        let operand = bytecode
            .get(cursor..operand_end)
            .ok_or(PulseVmV1Error::TruncatedBytecode)?;
        cursor = operand_end;

        let instruction = match spec.kind {
            PulseOpcodeKindV1::PushU64 => {
                let raw: [u8; 8] = operand
                    .try_into()
                    .map_err(|_| PulseVmV1Error::OpcodeTableInvariant)?;
                PulseInstructionV1::PushU64(u64::from_le_bytes(raw))
            }
            PulseOpcodeKindV1::PushBool => {
                let raw = operand
                    .first()
                    .copied()
                    .ok_or(PulseVmV1Error::OpcodeTableInvariant)?;
                let value = match raw {
                    0 => false,
                    1 => true,
                    other => return Err(PulseVmV1Error::InvalidBooleanEncoding(other)),
                };
                PulseInstructionV1::PushBool(value)
            }
            PulseOpcodeKindV1::AddU64 => PulseInstructionV1::AddU64,
            PulseOpcodeKindV1::SubU64 => PulseInstructionV1::SubU64,
            PulseOpcodeKindV1::MulU64 => PulseInstructionV1::MulU64,
            PulseOpcodeKindV1::EqU64 => PulseInstructionV1::EqU64,
            PulseOpcodeKindV1::AndBool => PulseInstructionV1::AndBool,
            PulseOpcodeKindV1::OrBool => PulseInstructionV1::OrBool,
            PulseOpcodeKindV1::NotBool => PulseInstructionV1::NotBool,
            PulseOpcodeKindV1::Dup => PulseInstructionV1::Dup,
            PulseOpcodeKindV1::Drop => PulseInstructionV1::Drop,
            PulseOpcodeKindV1::StoreLocal => {
                let local = operand
                    .first()
                    .copied()
                    .ok_or(PulseVmV1Error::OpcodeTableInvariant)?;
                validate_bytecode_local(local)?;
                PulseInstructionV1::StoreLocal(local)
            }
            PulseOpcodeKindV1::LoadLocal => {
                let local = operand
                    .first()
                    .copied()
                    .ok_or(PulseVmV1Error::OpcodeTableInvariant)?;
                validate_bytecode_local(local)?;
                PulseInstructionV1::LoadLocal(local)
            }
            PulseOpcodeKindV1::Halt => PulseInstructionV1::Halt,
        };
        instructions.push(instruction);
    }

    if cursor != bytecode.len() {
        return Err(PulseVmV1Error::TrailingBytecode);
    }

    let analysis = analyze_instructions(&instructions)?;
    Ok(ValidatedPulseProgramV1 {
        instructions,
        analysis,
    })
}

fn validate_bytecode_local(local: u8) -> Result<(), PulseVmV1Error> {
    if u16::from(local) >= PULSE_VM_MAX_LOCAL_SLOTS_V1 {
        return Err(PulseVmV1Error::LocalIndexOutOfRange {
            index: u16::from(local),
            max: PULSE_VM_MAX_LOCAL_SLOTS_V1 - 1,
        });
    }
    Ok(())
}

fn opcode_spec_for_name_v1(name: &str) -> Option<&'static PulseOpcodeSpecV1> {
    PULSE_OPCODE_SPECS_V1.iter().find(|spec| spec.name == name)
}

fn opcode_spec_for_byte_v1(opcode: u8) -> Option<&'static PulseOpcodeSpecV1> {
    PULSE_OPCODE_SPECS_V1
        .iter()
        .find(|spec| spec.opcode == opcode)
}

fn opcode_spec_for_kind_v1(kind: PulseOpcodeKindV1) -> &'static PulseOpcodeSpecV1 {
    PULSE_OPCODE_SPECS_V1
        .iter()
        .find(|spec| spec.kind == kind)
        .expect("every PulseScript v1 opcode kind must have one canonical specification")
}

fn instruction_opcode_kind_v1(instruction: PulseInstructionV1) -> PulseOpcodeKindV1 {
    match instruction {
        PulseInstructionV1::PushU64(_) => PulseOpcodeKindV1::PushU64,
        PulseInstructionV1::PushBool(_) => PulseOpcodeKindV1::PushBool,
        PulseInstructionV1::AddU64 => PulseOpcodeKindV1::AddU64,
        PulseInstructionV1::SubU64 => PulseOpcodeKindV1::SubU64,
        PulseInstructionV1::MulU64 => PulseOpcodeKindV1::MulU64,
        PulseInstructionV1::EqU64 => PulseOpcodeKindV1::EqU64,
        PulseInstructionV1::AndBool => PulseOpcodeKindV1::AndBool,
        PulseInstructionV1::OrBool => PulseOpcodeKindV1::OrBool,
        PulseInstructionV1::NotBool => PulseOpcodeKindV1::NotBool,
        PulseInstructionV1::Dup => PulseOpcodeKindV1::Dup,
        PulseInstructionV1::Drop => PulseOpcodeKindV1::Drop,
        PulseInstructionV1::StoreLocal(_) => PulseOpcodeKindV1::StoreLocal,
        PulseInstructionV1::LoadLocal(_) => PulseOpcodeKindV1::LoadLocal,
        PulseInstructionV1::Halt => PulseOpcodeKindV1::Halt,
    }
}

fn opcode_spec_for_instruction_v1(instruction: PulseInstructionV1) -> &'static PulseOpcodeSpecV1 {
    opcode_spec_for_kind_v1(instruction_opcode_kind_v1(instruction))
}

fn instruction_compute_cost(instruction: PulseInstructionV1) -> u64 {
    opcode_spec_for_instruction_v1(instruction).compute_cost
}

fn runtime_pop_u64(stack: &mut Vec<PulseValueV1>, instruction: u32) -> Result<u64, PulseVmV1Error> {
    match stack.pop() {
        Some(PulseValueV1::U64(value)) => Ok(value),
        _ => Err(PulseVmV1Error::RuntimeTypeInvariant { instruction }),
    }
}

fn runtime_pop_bool(
    stack: &mut Vec<PulseValueV1>,
    instruction: u32,
) -> Result<bool, PulseVmV1Error> {
    match stack.pop() {
        Some(PulseValueV1::Bool(value)) => Ok(value),
        _ => Err(PulseVmV1Error::RuntimeTypeInvariant { instruction }),
    }
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, PulseVmV1Error> {
    Ok(read_exact::<1>(bytes, cursor)?[0])
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, PulseVmV1Error> {
    Ok(u16::from_le_bytes(read_exact::<2>(bytes, cursor)?))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, PulseVmV1Error> {
    Ok(u32::from_le_bytes(read_exact::<4>(bytes, cursor)?))
}

fn read_exact<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], PulseVmV1Error> {
    let end = cursor
        .checked_add(N)
        .ok_or(PulseVmV1Error::TruncatedBytecode)?;
    let slice = bytes
        .get(*cursor..end)
        .ok_or(PulseVmV1Error::TruncatedBytecode)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    *cursor = end;
    Ok(out)
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn sha256_array(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_v3::RESOURCE_BUDGET_VERSION_V1;

    const GOLDEN_SOURCE: &str =
        "pulsescript-v1\npush_u64 2\npush_u64 3\nadd_u64\npush_u64 5\neq_u64\nhalt\n";
    const GOLDEN_BYTECODE_HEX: &str =
        "5044505301000100060000000102000000000000000103000000000000001001050000000000000013ff";
    const GOLDEN_BYTECODE_FINGERPRINT_HEX: &str =
        "dd9c436647ddcd2af828f1c3dd60ac13ca6102ce436adc1d8e67494da409908d";
    const GOLDEN_TOOLCHAIN_FINGERPRINT_HEX: &str =
        "e898d6af8e68f56840be8a283c1da920179bb1b2daeff71fa70260b3a0cc23a2";

    fn budget(compute_units: u64) -> ResourceBudgetV1 {
        ResourceBudgetV1 {
            version: RESOURCE_BUDGET_VERSION_V1,
            compute_units,
            read_bytes: 0,
            write_bytes: 0,
            proof_bytes: 0,
            event_bytes: 0,
        }
    }

    #[test]
    fn golden_compilation_and_execution_vector_is_frozen() {
        let compiled = compile_pulsescript_v1(GOLDEN_SOURCE).unwrap();
        assert_eq!(hex::encode(&compiled.bytecode), GOLDEN_BYTECODE_HEX);
        assert_eq!(
            hex::encode(compiled.bytecode_fingerprint),
            GOLDEN_BYTECODE_FINGERPRINT_HEX
        );
        assert_eq!(compiled.analysis.instruction_count, 6);
        assert_eq!(compiled.analysis.compute_units, 6);
        assert_eq!(compiled.analysis.max_stack_values, 2);
        assert_eq!(compiled.analysis.local_slots_used, 0);
        assert_eq!(compiled.analysis.result_type, PulseValueTypeV1::Bool);
        assert_eq!(compiled.analysis.max_call_depth, 0);
        assert_eq!(compiled.analysis.state_accesses, 0);

        let execution = execute_pulse_bytecode_v1(&compiled.bytecode, &budget(6)).unwrap();
        assert_eq!(execution.result, PulseValueV1::Bool(true));
        assert_eq!(execution.executed_instructions, 6);
        assert_eq!(execution.usage.compute_units, 6);
        assert_eq!(execution.usage.peak_stack_values, 2);
        assert_eq!(execution.usage.call_depth_peak, 0);
        assert_eq!(execution.usage.state_accesses, 0);
    }

    #[test]
    fn toolchain_identity_is_frozen() {
        assert_eq!(
            hex::encode(pulse_toolchain_fingerprint_v1()),
            GOLDEN_TOOLCHAIN_FINGERPRINT_HEX
        );
    }

    #[test]
    fn compiler_output_is_line_ending_and_comment_stable() {
        let lf = compile_pulsescript_v1(GOLDEN_SOURCE).unwrap();
        let crlf = compile_pulsescript_v1(&GOLDEN_SOURCE.replace('\n', "\r\n")).unwrap();
        let comments = compile_pulsescript_v1(
            "pulsescript-v1 # header\npush_u64 2\npush_u64 3 # rhs\nadd_u64\npush_u64 5\neq_u64\nhalt\n",
        )
        .unwrap();

        assert_eq!(lf.bytecode, crlf.bytecode);
        assert_eq!(lf.bytecode, comments.bytecode);
        assert_eq!(lf.bytecode_fingerprint, crlf.bytecode_fingerprint);
        assert_eq!(lf.bytecode_fingerprint, comments.bytecode_fingerprint);
    }

    #[test]
    fn source_limit_is_line_ending_invariant() {
        let mut lf = String::from("pulsescript-v1\npush_u64 1\nhalt\n");
        while lf.len() + 2 <= PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize {
            lf.push_str("#\n");
        }
        lf.push_str(&"#".repeat(PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize - lf.len()));
        assert_eq!(lf.len(), PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize);

        let crlf = lf.replace('\n', "\r\n");
        assert!(crlf.len() > PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize);
        assert!(crlf.len() <= PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize);

        let lf_compiled = compile_pulsescript_v1(&lf).unwrap();
        let crlf_compiled = compile_pulsescript_v1(&crlf).unwrap();
        assert_eq!(lf_compiled.bytecode, crlf_compiled.bytecode);
        assert_eq!(
            lf_compiled.bytecode_fingerprint,
            crlf_compiled.bytecode_fingerprint
        );
    }

    #[test]
    fn opcode_table_is_unique_and_drives_costs() {
        for (index, spec) in PULSE_OPCODE_SPECS_V1.iter().enumerate() {
            assert_eq!(
                opcode_spec_for_byte_v1(spec.opcode).unwrap().kind,
                spec.kind
            );
            assert_eq!(opcode_spec_for_name_v1(spec.name).unwrap().kind, spec.kind);
            assert_eq!(opcode_spec_for_kind_v1(spec.kind).opcode, spec.opcode);
            assert!(PULSE_OPCODE_SPECS_V1[..index]
                .iter()
                .all(|earlier| earlier.opcode != spec.opcode && earlier.kind != spec.kind));
        }

        let samples = [
            PulseInstructionV1::PushU64(0),
            PulseInstructionV1::PushBool(false),
            PulseInstructionV1::AddU64,
            PulseInstructionV1::SubU64,
            PulseInstructionV1::MulU64,
            PulseInstructionV1::EqU64,
            PulseInstructionV1::AndBool,
            PulseInstructionV1::OrBool,
            PulseInstructionV1::NotBool,
            PulseInstructionV1::Dup,
            PulseInstructionV1::Drop,
            PulseInstructionV1::StoreLocal(0),
            PulseInstructionV1::LoadLocal(0),
            PulseInstructionV1::Halt,
        ];
        for sample in samples {
            let spec = opcode_spec_for_instruction_v1(sample);
            assert_eq!(instruction_compute_cost(sample), spec.compute_cost);
        }
    }

    #[test]
    fn static_typing_rejects_invalid_stack_programs() {
        let err =
            compile_pulsescript_v1("pulsescript-v1\npush_bool true\npush_u64 1\nadd_u64\nhalt\n")
                .unwrap_err();
        assert!(matches!(err, PulseVmV1Error::TypeMismatch { .. }));

        let err = compile_pulsescript_v1("pulsescript-v1\nload_local 0\nhalt\n").unwrap_err();
        assert_eq!(err, PulseVmV1Error::UninitializedLocal { index: 0 });

        let err = compile_pulsescript_v1(
            "pulsescript-v1\npush_u64 1\nstore_local 0\npush_bool true\nstore_local 0\nload_local 0\nhalt\n",
        )
        .unwrap_err();
        assert_eq!(
            err,
            PulseVmV1Error::TypeMismatch {
                instruction: 3,
                expected: PulseValueTypeV1::U64,
                actual: PulseValueTypeV1::Bool,
            }
        );
    }

    #[test]
    fn locals_have_deterministic_typed_semantics() {
        let source =
            "pulsescript-v1\npush_u64 7\nstore_local 3\nload_local 3\npush_u64 5\nadd_u64\nhalt\n";
        let compiled = compile_pulsescript_v1(source).unwrap();
        assert_eq!(compiled.analysis.local_slots_used, 4);
        assert_eq!(compiled.analysis.result_type, PulseValueTypeV1::U64);

        let execution = execute_pulse_bytecode_v1(&compiled.bytecode, &budget(20)).unwrap();
        assert_eq!(execution.result, PulseValueV1::U64(12));
        assert_eq!(execution.usage.local_slots_used, 4);
    }

    #[test]
    fn checked_arithmetic_overflow_is_deterministic() {
        let source = format!(
            "pulsescript-v1\npush_u64 {}\npush_u64 1\nadd_u64\nhalt\n",
            u64::MAX
        );
        let compiled = compile_pulsescript_v1(&source).unwrap();
        assert_eq!(
            execute_pulse_bytecode_v1(&compiled.bytecode, &budget(10)),
            Err(PulseVmV1Error::ArithmeticOverflow { instruction: 2 })
        );
    }

    #[test]
    fn compute_budget_is_checked_before_execution() {
        let compiled = compile_pulsescript_v1(GOLDEN_SOURCE).unwrap();
        assert_eq!(
            execute_pulse_bytecode_v1(&compiled.bytecode, &budget(5)),
            Err(PulseVmV1Error::ComputeBudgetExceeded { actual: 6, max: 5 })
        );
    }

    #[test]
    fn malformed_bytecode_fails_closed_and_bounded() {
        let compiled = compile_pulsescript_v1(GOLDEN_SOURCE).unwrap();

        let mut unknown_opcode = compiled.bytecode.clone();
        unknown_opcode[12] = 0x7f;
        assert_eq!(
            validate_pulse_bytecode_v1(&unknown_opcode),
            Err(PulseVmV1Error::UnknownOpcode(0x7f))
        );

        let mut trailing = compiled.bytecode.clone();
        trailing.push(0);
        assert_eq!(
            validate_pulse_bytecode_v1(&trailing),
            Err(PulseVmV1Error::TrailingBytecode)
        );

        let truncated = &compiled.bytecode[..compiled.bytecode.len() - 1];
        assert_eq!(
            validate_pulse_bytecode_v1(truncated),
            Err(PulseVmV1Error::TruncatedBytecode)
        );

        let oversized = vec![0u8; PULSE_VM_MAX_BYTECODE_BYTES_V1 as usize + 1];
        assert!(matches!(
            validate_pulse_bytecode_v1(&oversized),
            Err(PulseVmV1Error::BytecodeTooLarge { .. })
        ));
    }

    #[test]
    fn noncanonical_boolean_encoding_rejects() {
        let source = "pulsescript-v1\npush_bool true\nhalt\n";
        let compiled = compile_pulsescript_v1(source).unwrap();
        let mut bytecode = compiled.bytecode;
        assert_eq!(
            bytecode[12],
            opcode_spec_for_kind_v1(PulseOpcodeKindV1::PushBool).opcode
        );
        bytecode[13] = 2;
        assert_eq!(
            validate_pulse_bytecode_v1(&bytecode),
            Err(PulseVmV1Error::InvalidBooleanEncoding(2))
        );
    }

    #[test]
    fn stack_and_instruction_limits_are_enforced() {
        let mut source = String::from("pulsescript-v1\n");
        for _ in 0..=PULSE_VM_MAX_STACK_VALUES_V1 {
            source.push_str("push_u64 1\n");
        }
        source.push_str("halt\n");
        assert!(matches!(
            compile_pulsescript_v1(&source),
            Err(PulseVmV1Error::StackLimitExceeded { .. })
        ));

        let mut bytecode = Vec::new();
        bytecode.extend_from_slice(&PULSE_BYTECODE_MAGIC_V1);
        bytecode.extend_from_slice(&PULSE_BYTECODE_VERSION_V1.to_le_bytes());
        bytecode.extend_from_slice(&PULSESCRIPT_COMPILER_VERSION_V1.to_le_bytes());
        bytecode.extend_from_slice(&(PULSE_VM_MAX_INSTRUCTIONS_V1 + 1).to_le_bytes());
        assert_eq!(
            validate_pulse_bytecode_v1(&bytecode),
            Err(PulseVmV1Error::InvalidInstructionCount {
                actual: PULSE_VM_MAX_INSTRUCTIONS_V1 + 1,
                max: PULSE_VM_MAX_INSTRUCTIONS_V1,
            })
        );
    }

    #[test]
    fn unknown_source_and_missing_halt_reject() {
        assert_eq!(
            compile_pulsescript_v1("pulsescript-v1\nfuture_opcode\nhalt\n"),
            Err(PulseVmV1Error::SourceSyntax { line: 2 })
        );
        assert_eq!(
            compile_pulsescript_v1("pulsescript-v1\npush_u64 1\n"),
            Err(PulseVmV1Error::MissingOrMisplacedHalt)
        );
    }

    #[test]
    fn rejection_codes_are_stable() {
        assert_eq!(
            PulseVmV1Error::ArithmeticOverflow { instruction: 0 }.rejection_code(),
            22
        );
        assert_eq!(
            PulseVmV1Error::ComputeBudgetExceeded { actual: 2, max: 1 }.rejection_code(),
            23
        );
        assert_eq!(PulseVmV1Error::UnknownOpcode(0xaa).rejection_code(), 19);
    }
}
