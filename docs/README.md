# PulseDAG documentation

Active development, CI, release evidence, and readiness work target **v3.0.0**.

`main` is the moving v3.0.0 construction line, not a frozen tag. The exact candidate is not yet frozen, and this state does not authorize a GitHub Release, public-testnet launch, mainnet launch, Day 0, default high-cadence activation, or smart contracts.

Current guardrails:

- candidate decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`;
- active release/readiness CI must identify v3.0.0;
- v2.x material is historical/compatibility evidence only.

## Current v3.0.0 authority

- [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md)
- [`ROADMAP_V3_0_LONG_LIVED_CORE.md`](ROADMAP_V3_0_LONG_LIVED_CORE.md)
- [`VERSION_MATRIX.md`](VERSION_MATRIX.md)
- [`BLOCK_HEADER_V2_CANONICALIZATION.md`](BLOCK_HEADER_V2_CANONICALIZATION.md)
- [`TRANSACTION_PROTOCOL_V2.md`](TRANSACTION_PROTOCOL_V2.md)
- [`PULSECLOCK_V1.md`](PULSECLOCK_V1.md)
- [`FINALITY_ENVELOPE_V1.md`](FINALITY_ENVELOPE_V1.md)

Transaction/header v2 documents remain protocol-component specifications inside the v3 candidate; they are not separate release targets.

## Operator documentation

- [`RUNBOOK.md`](RUNBOOK.md)
- [`API_V1.md`](API_V1.md)
- [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md)
- [`POW_CURRENT_PATH.md`](POW_CURRENT_PATH.md)

## Evidence and launch gates

- [`RELEASE_EVIDENCE.md`](RELEASE_EVIDENCE.md)
- [`BURN_IN_GATE.md`](BURN_IN_GATE.md)
- [`checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md`](checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md)

Evidence generated for v2.x may be retained for regression, compatibility, and provenance. It does not satisfy a v3.0.0 launch gate unless the corresponding v3 workflow reruns it against the exact v3 candidate SHA.

## v3 product and protocol work

- [`ACCESS_SET_V1.md`](ACCESS_SET_V1.md)
- [`COVENANT_VAULT_V1.md`](COVENANT_VAULT_V1.md)
- [`COVENANT_HTLC_V1.md`](COVENANT_HTLC_V1.md)
- [`COVENANT_PAY_STREAM_V1.md`](COVENANT_PAY_STREAM_V1.md)
- [`COVENANT_MULTISIG_V1.md`](COVENANT_MULTISIG_V1.md)
- [`COVENANT_CHANNEL_V1.md`](COVENANT_CHANNEL_V1.md)
- [`COVENANT_COLORED_UTXO_V1.md`](COVENANT_COLORED_UTXO_V1.md)
- [`BASED_APPS_V0.md`](BASED_APPS_V0.md)
- [`PULSEDAG_VERIFY_V1.md`](PULSEDAG_VERIFY_V1.md)
- [`WORK_CERTIFICATE_V1.md`](WORK_CERTIFICATE_V1.md)
- [`USER_SURFACE_V1.md`](USER_SURFACE_V1.md)

These surfaces remain independently gated. The v3 version identity does not by itself enable contracts, covenants, high cadence, GPU production claims, or any other disabled feature.

## Maintenance and history

- [`REPOSITORY_STANDARDS.md`](REPOSITORY_STANDARDS.md)
- [`archive/README.md`](archive/README.md)
- [`codex_tasks/`](codex_tasks/)

Historical v2.x evidence remains immutable provenance and compatibility material.
