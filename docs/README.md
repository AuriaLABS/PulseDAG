# PulseDAG documentation

This directory is organized around the active `v2.3.0` private-testnet release candidate.

## Current operator documentation

- [`RUNBOOK.md`](RUNBOOK.md) — local node/miner operation and health checks.
- [`runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md`](runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md) — private-testnet operations.
- [`INSTALL_BINARIES_V2_3_0.md`](INSTALL_BINARIES_V2_3_0.md) — binary installation and checksum verification.
- [`API_V1.md`](API_V1.md) — RPC API reference.
- [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md) — canonical PoW specification.
- [`POW_CURRENT_PATH.md`](POW_CURRENT_PATH.md) — current mining path.

## Current planning and decisions

- [`ROADMAP_V2_3_0.md`](ROADMAP_V2_3_0.md) — current roadmap.
- [`VERSION_MATRIX.md`](VERSION_MATRIX.md) — authoritative active version state.
- [`V2_3_0_GITHUB_ACTIONS_GATES.md`](V2_3_0_GITHUB_ACTIONS_GATES.md) — active CI/evidence gates.
- [`release/V2_3_0_RELEASE_NOTES.md`](release/V2_3_0_RELEASE_NOTES.md) — release notes.
- [`release/V2_3_0_RELEASE_DECISION.md`](release/V2_3_0_RELEASE_DECISION.md) — current authorization boundary.
- [`release/V2_3_0_RELEASE_APPROVAL_RECORD.md`](release/V2_3_0_RELEASE_APPROVAL_RECORD.md) — candidate approval record.

## Governance and maintenance

- [`REPOSITORY_STANDARDS.md`](REPOSITORY_STANDARDS.md) — repository standards.
- [`REPOSITORY_CLEANUP_PLAN_V2_3_0.md`](REPOSITORY_CLEANUP_PLAN_V2_3_0.md) — cleanup policy and execution status.
- [`archive/README.md`](archive/README.md) — historical evidence and superseded documentation.
- [`codex_tasks/`](codex_tasks/) — implementation task records; not operator documentation.

## Documentation rules

1. Root-level and current documentation must describe `v2.3.0` as the active version.
2. v2.2.x material is historical evidence and belongs in the archive or in immutable Git history.
3. Historical baselines may be referenced from current documents only when clearly labelled as historical.
4. Public-testnet readiness must remain false until a separate explicit launch decision.
5. Broken links and stale active-version claims fail repository hygiene.
