# v2.3.0 Task 11 — Operator and incident runbooks

## Goal

Publish one coherent private-testnet operations and incident-response package tied to the Task 07 configuration contract, Task 09 lifecycle controller, Task 10 observability signals, existing recovery procedures, and redacted evidence collection.

## Deliverables

- A canonical v2.3.0 runbook index.
- A private-testnet operations runbook covering bootstrap, external miner attachment, routine checks, lifecycle, upgrade/rollback, state protection, evidence, and decommissioning.
- An incident-response runbook with SEV-1 through SEV-4, explicit roles, evidence custody, containment, recovery, communications, and closure.
- A security/capacity runbook for RPC abuse, disk pressure, identity rotation, token rotation, and monitoring access.
- A standard-library incident evidence collector with recursive redaction, immutable bundle identifiers, UTC metadata, and SHA-256 checksums.
- A validator that checks required sections, repository paths, alert/runbook links, evidence contract markers, and readiness guardrails.
- Deterministic collector regression coverage and a dedicated Actions evidence gate.
- English comments, diagnostics, and operator documentation.

## Acceptance criteria

1. The active runbook index is labeled v2.3.0 and retains required compatibility references.
2. Bootstrap, external miner, upgrade/rollback, snapshot/restore, partition recovery, evidence, and decommissioning are covered.
3. SEV-1 through SEV-4 definitions, roles, no-go rules, and closure criteria are explicit.
4. RPC abuse, disk pressure, identity rotation, operator-token rotation, and monitoring access have bounded procedures.
5. Every versioned alert runbook link resolves.
6. Evidence collection redacts sensitive keys recursively and never writes them to the manifest.
7. Existing incident bundle identifiers cannot be overwritten.
8. Every response and manifest has a SHA-256 checksum.
9. Partial collection exits non-zero and records the failed endpoint count.
10. Repository hygiene, Lint, RPC/release, and pre-burn-in checks pass.

## Guardrails

- Evidence collection uses read-only RPC endpoints.
- No secrets, private keys, wallet data, authorization headers, or tokens are retained.
- No consensus, storage-format, node RPC, miner protocol, pool, or smart-contract change.
- No version bump, tag, release-readiness claim, public-testnet launch, or 30-day clock start.

## Follow-up

Task 12 runs the complete topology in separate hosts or isolated network namespaces and produces a private-testnet GO/NO-GO evidence set.
