//! Independent wire/cost/result vectors for the frozen, inactive VM profile.
//!
//! Expected bytes and costs deliberately do not use the production opcode table.
//! Arithmetic uses a wider integer oracle so wrapping and operand-order bugs are
//! observable on every host without randomness or another dependency.

use pulsedag_core::contract_v3::{ResourceBudgetV1, RESOURCE_BUDGET_VERSION_V1};
use pulsedag_core::pulsescript_vm_v1::{
    compile_pulsescript_v1, execute_pulse_bytecode_v1, validate_pulse_bytecode_v1,
    PulseStaticAnalysisV1, PulseValueTypeV1, PulseValueV1, PulseVmV1Error,
};

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

fn wire(count: u32, body: &[u8]) -> Vec<u8> {
    // Literal version bytes keep the expected wire format independent.
    let mut bytes = b"PDPS\x01\x00\x01\x00".to_vec();
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn push_u64(body: &mut Vec<u8>, value: u64) {
    body.push(0x01);
    body.extend_from_slice(&value.to_le_bytes());
}

fn analysis(
    instruction_count: u32,
    compute_units: u64,
    max_stack_values: u16,
    local_slots_used: u16,
    result_type: PulseValueTypeV1,
) -> PulseStaticAnalysisV1 {
    PulseStaticAnalysisV1 {
        instruction_count,
        compute_units,
        max_stack_values,
        local_slots_used,
        result_type,
        max_call_depth: 0,
        state_accesses: 0,
    }
}

fn check_vector(
    source: &str,
    bytecode: &[u8],
    expected: PulseStaticAnalysisV1,
    result: Result<PulseValueV1, PulseVmV1Error>,
) {
    let commented = source
        .lines()
        .map(|line| format!("  {line} # conformance\n"))
        .collect::<String>();
    let variants = [
        source.to_owned(),
        source.replace('\n', "\r\n"),
        source.replace('\n', "\r"),
        commented,
    ];
    let mut fingerprint = None;
    for variant in variants {
        let compiled = compile_pulsescript_v1(&variant).unwrap();
        assert_eq!(compiled.bytecode, bytecode, "{source}");
        assert_eq!(compiled.analysis, expected, "{source}");
        if let Some(previous) = fingerprint {
            assert_eq!(compiled.bytecode_fingerprint, previous, "{source}");
        }
        fingerprint = Some(compiled.bytecode_fingerprint);
    }

    assert_eq!(validate_pulse_bytecode_v1(bytecode), Ok(expected));
    // Even overflow vectors must fail budget preflight before evaluating values.
    assert_eq!(
        execute_pulse_bytecode_v1(bytecode, &budget(expected.compute_units - 1)),
        Err(PulseVmV1Error::ComputeBudgetExceeded {
            actual: expected.compute_units,
            max: expected.compute_units - 1,
        }),
        "{source}"
    );
    let execution = execute_pulse_bytecode_v1(bytecode, &budget(expected.compute_units));
    match result {
        Ok(value) => {
            let execution = execution.unwrap();
            assert_eq!(execution.result, value, "{source}");
            assert_eq!(execution.executed_instructions, expected.instruction_count);
            assert_eq!(execution.usage.compute_units, expected.compute_units);
            assert_eq!(execution.usage.peak_stack_values, expected.max_stack_values);
            assert_eq!(execution.usage.local_slots_used, expected.local_slots_used);
            assert_eq!(execution.usage.call_depth_peak, 0);
            assert_eq!(execution.usage.state_accesses, 0);
        }
        Err(error) => {
            assert_eq!(execution.unwrap_err(), error, "{source}");
            assert_eq!(error.rejection_code(), 22);
        }
    }
}

#[test]
fn pulsescript_vm_v1_integer_boundary_matrix_matches_wide_oracle() {
    let values = [
        0,
        1,
        2,
        255,
        256,
        65_535,
        65_536,
        u64::from(u32::MAX),
        u64::from(u32::MAX) + 1,
        (1u64 << 63) - 1,
        1u64 << 63,
        u64::MAX - 1,
        u64::MAX,
    ];
    for lhs in values {
        for rhs in values {
            for (name, opcode) in [("add_u64", 0x10), ("sub_u64", 0x11), ("mul_u64", 0x12)] {
                let a = u128::from(lhs);
                let b = u128::from(rhs);
                let wide = match opcode {
                    0x10 => Some(a + b),
                    0x11 if a >= b => Some(a - b),
                    0x11 => None,
                    0x12 => Some(a * b),
                    _ => unreachable!(),
                };
                let result = wide
                    .and_then(|value| u64::try_from(value).ok())
                    .map(PulseValueV1::U64)
                    .ok_or(PulseVmV1Error::ArithmeticOverflow { instruction: 2 });
                let source =
                    format!("pulsescript-v1\npush_u64 {lhs}\npush_u64 {rhs}\n{name}\nhalt\n");
                let mut body = Vec::new();
                push_u64(&mut body, lhs);
                push_u64(&mut body, rhs);
                body.extend_from_slice(&[opcode, 0xff]);
                check_vector(
                    &source,
                    &wire(4, &body),
                    analysis(4, 4, 2, 0, PulseValueTypeV1::U64),
                    result,
                );
            }

            let source =
                format!("pulsescript-v1\npush_u64 {lhs}\npush_u64 {rhs}\neq_u64\nhalt\n");
            let mut body = Vec::new();
            push_u64(&mut body, lhs);
            push_u64(&mut body, rhs);
            body.extend_from_slice(&[0x13, 0xff]);
            check_vector(
                &source,
                &wire(4, &body),
                analysis(4, 3, 2, 0, PulseValueTypeV1::Bool),
                Ok(PulseValueV1::Bool(lhs == rhs)),
            );
        }
    }
}

#[test]
fn pulsescript_vm_v1_boolean_truth_tables_are_exhaustive() {
    for lhs in [false, true] {
        for rhs in [false, true] {
            for (name, opcode, value) in [
                ("and_bool", 0x20, lhs && rhs),
                ("or_bool", 0x21, lhs || rhs),
            ] {
                let source =
                    format!("pulsescript-v1\npush_bool {lhs}\npush_bool {rhs}\n{name}\nhalt\n");
                let body = [0x02, u8::from(lhs), 0x02, u8::from(rhs), opcode, 0xff];
                check_vector(
                    &source,
                    &wire(4, &body),
                    analysis(4, 3, 2, 0, PulseValueTypeV1::Bool),
                    Ok(PulseValueV1::Bool(value)),
                );
            }
        }
        let source = format!("pulsescript-v1\npush_bool {lhs}\nnot_bool\nhalt\n");
        check_vector(
            &source,
            &wire(3, &[0x02, u8::from(lhs), 0x22, 0xff]),
            analysis(3, 2, 1, 0, PulseValueTypeV1::Bool),
            Ok(PulseValueV1::Bool(!lhs)),
        );
    }
}

#[test]
fn pulsescript_vm_v1_local_slots_preserve_values_and_do_not_alias() {
    // Freeze the full v1 slot range, including the wrap from slot 63 to slot 0.
    for slot in 0u8..64 {
        let other = (slot + 1) % 64;
        let source = format!(
            "pulsescript-v1\npush_u64 17\nstore_local {slot}\npush_bool true\n\
             store_local {other}\nload_local {slot}\ndup\ndrop\nhalt\n"
        );
        let mut body = Vec::new();
        push_u64(&mut body, 17);
        body.extend_from_slice(&[0x40, slot, 0x02, 1, 0x40, other, 0x41, slot, 0x30, 0x31, 0xff]);
        check_vector(
            &source,
            &wire(8, &body),
            analysis(8, 10, 2, u16::from(slot.max(other)) + 1, PulseValueTypeV1::U64),
            Ok(PulseValueV1::U64(17)),
        );

        let source = format!(
            "pulsescript-v1\npush_bool false\nstore_local {slot}\npush_bool true\n\
             store_local {slot}\nload_local {slot}\ndup\nand_bool\nhalt\n"
        );
        let body = [
            0x02, 0, 0x40, slot, 0x02, 1, 0x40, slot, 0x41, slot, 0x30, 0x20, 0xff,
        ];
        check_vector(
            &source,
            &wire(8, &body),
            analysis(8, 10, 2, u16::from(slot) + 1, PulseValueTypeV1::Bool),
            Ok(PulseValueV1::Bool(true)),
        );
    }
}

#[test]
fn pulsescript_vm_v1_local_type_freeze_rejects_both_directions_in_every_slot() {
    for slot in 0u8..64 {
        for (first, second, expected, actual) in [
            (
                "push_u64 7",
                "push_bool true",
                PulseValueTypeV1::U64,
                PulseValueTypeV1::Bool,
            ),
            (
                "push_bool true",
                "push_u64 7",
                PulseValueTypeV1::Bool,
                PulseValueTypeV1::U64,
            ),
        ] {
            let source = format!(
                "pulsescript-v1\n{first}\nstore_local {slot}\n{second}\n\
                 store_local {slot}\nload_local {slot}\nhalt\n"
            );
            let mut body = Vec::new();
            for value_type in [expected, actual] {
                match value_type {
                    PulseValueTypeV1::U64 => push_u64(&mut body, 7),
                    PulseValueTypeV1::Bool => body.extend_from_slice(&[0x02, 1]),
                }
                body.extend_from_slice(&[0x40, slot]);
            }
            body.extend_from_slice(&[0x41, slot, 0xff]);
            let error = PulseVmV1Error::TypeMismatch {
                instruction: 3,
                expected,
                actual,
            };
            assert_eq!(error.rejection_code(), 8);
            assert_eq!(compile_pulsescript_v1(&source).unwrap_err(), error);
            assert_eq!(validate_pulse_bytecode_v1(&wire(6, &body)), Err(error.clone()));
            assert_eq!(
                execute_pulse_bytecode_v1(&wire(6, &body), &budget(20)),
                Err(error)
            );
        }
    }
}
