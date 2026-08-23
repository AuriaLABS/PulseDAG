# PulseDAG documentation

Active release documentation now targets the `v2.4.0` Task31 candidate constructed from `main`. The exact candidate is not yet frozen: activated-v2 startup/storage/P2P wiring and final exact-SHA validation remain in progress. This state does not authorize a release tag, GitHub Release publication, public-testnet launch, Day 0, default high-cadence activation, or smart contracts.

The `v2.5.0` and `v2.6.0` roadmaps remain future planning documents. They do not bypass the v2.4.0 release/activation gates.

## Current v2.4.0 authority

- [`ROADMAP_V2_4_0.md`](ROADMAP_V2_4_0.md)
- [`PROTOCOL_ACTIVATION_V2_4_0.md`](PROTOCOL_ACTIVATION_V2_4_0.md)
- [`BLOCK_HEADER_V2_CANONICALIZATION.md`](BLOCK_HEADER_V2_CANONICALIZATION.md)
- [`TRANSACTION_PROTOCOL_V2.md`](TRANSACTION_PROTOCOL_V2.md)
- [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md)
- [`VERSION_MATRIX.md`](VERSION_MATRIX.md)

## Operator documentation

- [`RUNBOOK.md`](RUNBOOK.md)
- [`API_V1.md`](API_V1.md)
- [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md)
- [`POW_CURRENT_PATH.md`](POW_CURRENT_PATH.md)

The v2.4.0 packaged-binary installation/recovery guide must be frozen from the final exact candidate. Existing v2.3.0 installation and private-testnet documents are historical/compatibility inputs, not the current v2.4.0 release identity.

## Evidence and launch gates

- [`RELEASE_EVIDENCE.md`](RELEASE_EVIDENCE.md)
- [`BURN_IN_GATE.md`](BURN_IN_GATE.md)
- [`checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md`](checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md)

Current authorization remains:

- Task31 decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- default high cadence experimental/disabled;
- `contracts_enabled=false`.

## Future planning

- [`ROADMAP_V2_5_0.md`](ROADMAP_V2_5_0.md)
- [`ROADMAP_V2_6_0.md`](ROADMAP_V2_6_0.md)

## Maintenance and history

- [`REPOSITORY_STANDARDS.md`](REPOSITORY_STANDARDS.md)
- [`archive/README.md`](archive/README.md)
- [`codex_tasks/`](codex_tasks/)

Historical v2.3.x and v2.2.x evidence remains immutable provenance. Consensus identity changes require a fresh, explicitly versioned chain/activation boundary and exact-candidate evidence; they must never be inferred from an old release branch or old private-testnet database.
