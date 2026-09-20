# PulseScript / deterministic Contract VM v1 foundation (#1042)

This document records the first narrow implementation slice for issue #1042. It is an **inactive foundation only**: it does not activate programmability and is not connected to node transaction admission, state application, RPC, storage, mining, wallet behavior, P2P, or protocol selection.

## Frozen toolchain identity

The implementation lives in:

- `crates/pulsedag-core/src/pulsescript_vm_v1.rs`

Frozen versions:

- PulseScript compiler: `1`
- Pulse bytecode: `1`
- Pulse VM: `1`

The toolchain fingerprint binds those versions, all hard limits below, the Contract v3 maximum compute ceiling, and the exact opcode cost schedule under the domain:

`PulseDAG:pulsescript-vm-toolchain:v1`

Golden toolchain fingerprint:

`e898d6af8e68f56840be8a283c1da920179bb1b2daeff71fa70260b3a0cc23a2`

## Source kernel

The first compiler kernel is intentionally small and branch-free. A source file starts with:

`pulsescript-v1`

The accepted instructions are:

- `push_u64 <decimal>`
- `push_bool true|false`
- `add_u64`
- `sub_u64`
- `mul_u64`
- `eq_u64`
- `and_bool`
- `or_bool`
- `not_bool`
- `dup`
- `drop`
- `store_local <0..63>`
- `load_local <0..63>`
- `halt`

Blank lines and text following `#` are ignored by the compiler. CRLF and lone CR are canonicalized to LF before the 16 KiB semantic source limit is applied, so equivalent line endings produce identical acceptance and bytecode. Raw input is separately capped at 32 KiB before canonicalization.

The compiler performs deterministic stack typing before emission. Local slots acquire a type on first store; reading an uninitialized local fails closed. A valid v1 program terminates with one final `halt` and exactly one typed result value.

## Canonical bytecode

Header:

- magic: ASCII `PDPS`
- bytecode version: little-endian `u16`
- compiler version: little-endian `u16`
- instruction count: little-endian `u32`

Opcode operands have fixed widths. Booleans are canonical bytes `0` or `1`; any other encoding rejects. Unknown opcodes, unsupported versions, truncated encodings, trailing bytes, oversized bytecode, excessive instruction counts, invalid local indexes and invalid typed programs all fail closed with stable rejection codes.

Bytecode fingerprints are SHA-256 over the domain-separated canonical byte sequence:

`PulseDAG:pulsescript-bytecode:v1`

## Deterministic arithmetic and execution

Consensus-visible arithmetic in this slice is `u64` only.

- addition uses checked addition;
- subtraction uses checked subtraction;
- multiplication uses checked multiplication;
- overflow and underflow return the stable arithmetic rejection code;
- there is no floating point or host-dependent numeric representation.

The canonical opcode table is the single source of truth for opcode byte, operand width and compute cost; that same table is serialized into the toolchain fingerprint and is used by encoding, decoding and execution accounting. Execution validates the Contract v3 `ResourceBudgetV1` before running and refuses programs whose statically determined compute cost exceeds the declared budget.

There are no jumps, loops, calls, recursion or state-access instructions in this first slice. This is intentional: the first bytecode surface is statically bounded by construction while later #1042 slices can add control flow/state semantics only with separately frozen limits and differential vectors.

## Hard limits

- canonical source bytes: 16 KiB
- raw source bytes before line-ending canonicalization: 32 KiB
- bytecode bytes: 64 KiB
- instructions: 1,024
- stack values: 256
- local slots: 64
- abstract consensus value width: 9 bytes
- call depth: 0
- state accesses: 0
- compute ceiling: inherited from frozen Contract v3 maximum compute units

Runtime heap layout is not consensus input. Consensus-visible memory behavior is defined by the fixed stack/local slot limits and typed value representation rather than host pointer size or allocator behavior.

## Golden compilation vector

Source:

```text
pulsescript-v1
push_u64 2
push_u64 3
add_u64
push_u64 5
eq_u64
halt
```

Canonical bytecode hex:

`5044505301000100060000000102000000000000000103000000000000001001050000000000000013ff`

Bytecode fingerprint:

`dd9c436647ddcd2af828f1c3dd60ac13ca6102ce436adc1d8e67494da409908d`

Static compute units: `6`

Result: `bool(true)`

The same vector is executed by CI on Linux and Windows so compiler output, validation, rejection behavior and execution are compared across supported host families.

## Explicitly deferred

This slice does **not** claim completion of #1042. Still deferred include:

- richer PulseScript syntax and ABI/interface description;
- bounded control flow;
- calls and call-depth semantics above zero;
- memory model beyond fixed typed stack/local slots;
- state read/write opcodes and access declarations;
- event/write-set integration with Contract State Transition v1;
- cross-contract calls and reentrancy policy;
- source maps/debug metadata;
- fuzzing and adversarial corpora;
- broader cross-platform differential evidence;
- activation and any node/runtime wiring.

Current v3.0 programmability remains inactive. Completion or merge of this foundation does not imply #781/#794 or mainnet GO.
