use pulsedag_core::contract_v3::{
    ResourceBudgetV1, CONTRACT_V3_MAX_COMPUTE_UNITS, RESOURCE_BUDGET_VERSION_V1,
};
use pulsedag_core::pulsescript_vm_v1::{
    compile_pulsescript_v1, execute_pulse_bytecode_v1, validate_pulse_bytecode_v1, PulseValueV1,
    PulseVmV1Error, PULSESCRIPT_COMPILER_VERSION_V1, PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1,
    PULSESCRIPT_MAX_SOURCE_BYTES_V1, PULSE_BYTECODE_VERSION_V1, PULSE_VM_MAX_BYTECODE_BYTES_V1,
    PULSE_VM_MAX_INSTRUCTIONS_V1, PULSE_VM_MAX_LOCAL_SLOTS_V1, PULSE_VM_MAX_STACK_VALUES_V1,
};

const GOLDEN_BYTECODE_HEX: &str =
    "5044505301000100060000000102000000000000000103000000000000001001050000000000000013ff";

const KNOWN_OPCODES: [u8; 14] = [
    0x01, 0x02, 0x10, 0x11, 0x12, 0x13, 0x20, 0x21, 0x22, 0x30, 0x31, 0x40, 0x41, 0xff,
];

fn header(instruction_count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(b"PDPS");
    out.extend_from_slice(&PULSE_BYTECODE_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&PULSESCRIPT_COMPILER_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&instruction_count.to_le_bytes());
    out
}

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
fn pulsescript_vm_v1_unknown_opcode_space_is_exhaustive() {
    for opcode in u8::MIN..=u8::MAX {
        if KNOWN_OPCODES.contains(&opcode) {
            continue;
        }

        let mut bytecode = header(1);
        bytecode.push(opcode);

        assert_eq!(
            validate_pulse_bytecode_v1(&bytecode),
            Err(PulseVmV1Error::UnknownOpcode(opcode)),
            "unexpected classification for opcode 0x{opcode:02x}"
        );
    }
}

#[test]
fn pulsescript_vm_v1_every_golden_truncation_fails_closed() {
    let golden = hex::decode(GOLDEN_BYTECODE_HEX).unwrap();

    for end in 0..golden.len() {
        assert!(
            validate_pulse_bytecode_v1(&golden[..end]).is_err(),
            "truncation at byte {end} unexpectedly validated"
        );
    }

    assert!(validate_pulse_bytecode_v1(&golden).is_ok());
}

#[test]
fn pulsescript_vm_v1_every_single_trailing_byte_rejects() {
    let golden = hex::decode(GOLDEN_BYTECODE_HEX).unwrap();

    for trailing in u8::MIN..=u8::MAX {
        let mut mutated = golden.clone();
        mutated.push(trailing);
        assert_eq!(
            validate_pulse_bytecode_v1(&mutated),
            Err(PulseVmV1Error::TrailingBytecode),
            "trailing byte 0x{trailing:02x} was not rejected canonically"
        );
    }
}

#[test]
fn pulsescript_vm_v1_instruction_count_rejects_before_body_allocation() {
    let bytecode = header(u32::MAX);
    assert_eq!(
        validate_pulse_bytecode_v1(&bytecode),
        Err(PulseVmV1Error::InvalidInstructionCount {
            actual: u32::MAX,
            max: PULSE_VM_MAX_INSTRUCTIONS_V1,
        })
    );
}

#[test]
fn pulsescript_vm_v1_oversized_bytecode_rejects_before_decode() {
    let oversized = vec![0u8; PULSE_VM_MAX_BYTECODE_BYTES_V1 as usize + 1];
    assert_eq!(
        validate_pulse_bytecode_v1(&oversized),
        Err(PulseVmV1Error::BytecodeTooLarge {
            actual: PULSE_VM_MAX_BYTECODE_BYTES_V1 as usize + 1,
            max: PULSE_VM_MAX_BYTECODE_BYTES_V1,
        })
    );
}

#[test]
fn pulsescript_vm_v1_noncanonical_boolean_and_local_index_reject() {
    let mut boolean = header(2);
    boolean.extend_from_slice(&[0x02, 2, 0xff]);
    assert_eq!(
        validate_pulse_bytecode_v1(&boolean),
        Err(PulseVmV1Error::InvalidBooleanEncoding(2))
    );

    let mut local = header(2);
    local.extend_from_slice(&[0x40, PULSE_VM_MAX_LOCAL_SLOTS_V1 as u8, 0xff]);
    assert_eq!(
        validate_pulse_bytecode_v1(&local),
        Err(PulseVmV1Error::LocalIndexOutOfRange {
            index: PULSE_VM_MAX_LOCAL_SLOTS_V1,
            max: PULSE_VM_MAX_LOCAL_SLOTS_V1 - 1,
        })
    );
}

#[test]
fn pulsescript_vm_v1_stack_limit_is_deterministic() {
    let mut source = String::from("pulsescript-v1\n");
    for _ in 0..=PULSE_VM_MAX_STACK_VALUES_V1 {
        source.push_str("push_u64 1\n");
    }
    source.push_str("halt\n");

    assert_eq!(
        compile_pulsescript_v1(&source),
        Err(PulseVmV1Error::StackLimitExceeded {
            max: PULSE_VM_MAX_STACK_VALUES_V1,
        })
    );
}

#[test]
fn pulsescript_vm_v1_source_limits_are_bounded_and_line_ending_invariant() {
    let mut lf = String::from("pulsescript-v1\npush_u64 1\nhalt\n");
    while lf.len() + 2 <= PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize {
        lf.push_str("#\n");
    }
    lf.push_str(&"#".repeat(PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize - lf.len()));
    assert_eq!(lf.len(), PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize);

    let crlf = lf.replace('\n', "\r\n");
    assert!(crlf.len() > PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize);
    assert!(crlf.len() <= PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize);

    let a = compile_pulsescript_v1(&lf).unwrap();
    let b = compile_pulsescript_v1(&crlf).unwrap();
    assert_eq!(a.bytecode, b.bytecode);
    assert_eq!(a.bytecode_fingerprint, b.bytecode_fingerprint);

    let canonical_oversized = format!("{lf}#");
    assert_eq!(
        canonical_oversized.len(),
        PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize + 1
    );
    assert!(canonical_oversized.len() < PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize);
    assert_eq!(
        compile_pulsescript_v1(&canonical_oversized),
        Err(PulseVmV1Error::SourceTooLarge {
            actual: PULSESCRIPT_MAX_SOURCE_BYTES_V1 as usize + 1,
            max: PULSESCRIPT_MAX_SOURCE_BYTES_V1,
        })
    );

    let oversized = "x".repeat(PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize + 1);
    assert_eq!(
        compile_pulsescript_v1(&oversized),
        Err(PulseVmV1Error::SourceTooLarge {
            actual: PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1 as usize + 1,
            max: PULSESCRIPT_MAX_RAW_SOURCE_BYTES_V1,
        })
    );
}

#[test]
fn pulsescript_vm_v1_compute_budget_preflight_is_exact() {
    let compiled = compile_pulsescript_v1(
        "pulsescript-v1\npush_u64 2\npush_u64 3\nadd_u64\npush_u64 5\neq_u64\nhalt\n",
    )
    .unwrap();

    assert_eq!(
        execute_pulse_bytecode_v1(&compiled.bytecode, &budget(5)),
        Err(PulseVmV1Error::ComputeBudgetExceeded { actual: 6, max: 5 })
    );

    let execution = execute_pulse_bytecode_v1(&compiled.bytecode, &budget(6)).unwrap();
    assert_eq!(execution.result, PulseValueV1::Bool(true));
    assert_eq!(execution.usage.compute_units, 6);
}

#[test]
fn pulsescript_vm_v1_checked_underflow_is_stable() {
    let compiled =
        compile_pulsescript_v1("pulsescript-v1\npush_u64 0\npush_u64 1\nsub_u64\nhalt\n").unwrap();

    let err = execute_pulse_bytecode_v1(&compiled.bytecode, &budget(10)).unwrap_err();
    assert_eq!(err, PulseVmV1Error::ArithmeticOverflow { instruction: 2 });
    assert_eq!(err.rejection_code(), 22);
}

#[test]
fn pulsescript_vm_v1_contract_compute_ceiling_stays_external_and_fail_closed() {
    let compiled = compile_pulsescript_v1("pulsescript-v1\npush_u64 1\nhalt\n").unwrap();
    let invalid_budget = budget(CONTRACT_V3_MAX_COMPUTE_UNITS + 1);

    assert!(matches!(
        execute_pulse_bytecode_v1(&compiled.bytecode, &invalid_budget),
        Err(PulseVmV1Error::ContractV3(_))
    ));
}

#[test]
fn pulsescript_vm_v1_repeated_validation_is_identical() {
    let malformed_cases = [
        Vec::new(),
        header(PULSE_VM_MAX_INSTRUCTIONS_V1 + 1),
        {
            let mut b = header(1);
            b.push(0x7f);
            b
        },
        {
            let mut b = header(2);
            b.extend_from_slice(&[0x02, 2, 0xff]);
            b
        },
    ];

    for case in malformed_cases {
        let first = validate_pulse_bytecode_v1(&case);
        for _ in 0..32 {
            assert_eq!(validate_pulse_bytecode_v1(&case), first);
        }
    }
}
