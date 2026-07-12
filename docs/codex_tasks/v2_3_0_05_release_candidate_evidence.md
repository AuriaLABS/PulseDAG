# v2.3.0 Task 05 — Freeze and validate the release-candidate commit

## Goal

After Tasks 01–04 merge, freeze one exact candidate commit and produce every technical and operational artifact required to close `v2.2.20` hardening and open formal `v2.3.0` readiness review.

This task does not authorize a version bump and does not claim public-testnet readiness.

## Candidate freeze

Record:

- exact commit SHA and branch;
- UTC freeze timestamp;
- `VERSION=v2.2.20`;
- Cargo workspace version `2.2.20`;
- clean working tree/diff evidence;
- `public_testnet_ready=false`;
- reviewer/release-manager identity.

Every gate below must run on this same commit. Evidence from earlier commits may be referenced historically but cannot establish the release-candidate gate.

## Workspace validation gate

Capture complete logs and exit codes for:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

No ignored failing test or unrecorded retry is allowed.

## Required runtime gates

### Local smoke

- 3N/1M or equivalent;
- health/readiness/peer visibility;
- accepted block activity;
- current version/release metadata;
- `public_testnet_ready=false`.

### Private staged rehearsals

Run in sequence on the same candidate commit:

1. `5N/1M` — PASS;
2. `5N/2M` — PASS;
3. `5N/4M` — PASS.

Each bundle must include:

- archive and checksum;
- evaluated commit and timestamps;
- startup topology stability;
- miner templates/submits/accepts/rejects;
- pre/post-quiescence tips and lag;
- selected tip, ordered DAG tip, and state root;
- accepted/retained hash-set digest;
- storage/memory checks;
- orphan/missing-parent state;
- RPC liveness and handler timeouts;
- readiness;
- failure classes and evidence-consistency checks.

### Transaction/mempool drill

Require:

- five-node tx relay convergence;
- duplicate suppression;
- capacity/rejection taxonomy;
- confirmation cleanup;
- deterministic final mempool sets;
- metrics, manifest, archive, checksum.

### Lag-injection drill

Require:

- offline/isolated node at least 64 blocks behind;
- real remote-tip inventory;
- correlated selected-segment activation;
- block/chunk application;
- complete final convergence;
- manifest, archive, checksum.

### Prune/restart/rejoin drill

Require:

- non-zero pruning;
- retained-set equality;
- snapshot checksum;
- restart from snapshot+delta;
- offline advance and rejoin;
- final convergence;
- manifest, archive, checksum.

## Release workflow gate

Capture:

- preflight script output;
- reproducible release/archive command;
- binary/archive checksums;
- install/start verification;
- `/release`, `/status`, `/health`, `/readiness`, `/p2p/status`, `/checks`, `/metrics` captures;
- rollback target and rollback procedure.

## Security and operations gate

Attach:

- RPC listener/exposure review;
- authentication/authorization posture;
- firewall and secret-redaction checks;
- bootnode and chain-ID configuration;
- NAT/firewall assumptions;
- deployment, rollback, incident, snapshot/restore, and monitoring runbooks;
- operator dashboard/alert inventory.

## Known limitations and incidents

Create decision-scoped ledgers containing:

- every unresolved limitation;
- severity and affected gate;
- owner and reviewer;
- UTC approval date;
- scope and explicit exclusions;
- expiry date/event;
- exit criteria;
- incident status;
- explicit statement that no waiver authorizes public-testnet readiness.

Any unresolved Sev-1 consensus, sync, storage, or security incident is automatic NO-GO.

## Evidence index

Update or replace the stale `v2.2.20` evidence index so it points only to current candidate evidence for required gates.

For every artifact record:

- gate and result;
- exact commit;
- UTC start/end;
- command/workflow;
- archive path;
- sha256;
- manifest path;
- reviewer decision;
- waiver/limitation reference if applicable.

## Acceptance criteria

The task passes only if:

- all mandatory gates pass, or an explicitly permitted gate has a complete bounded waiver;
- the candidate commit is identical across all release gates;
- evidence consistency checks pass;
- no unresolved Sev-1 blocker exists;
- version remains `v2.2.20` / `2.2.20`;
- `public_testnet_ready=false` remains explicit;
- the final recommendation can truthfully be `GO_TO_START_V2_3_0_REVIEW`.

## Guardrails

- No version bump in this task.
- No public-testnet ready/live claim.
- No smart-contract enablement.
- No pool logic.
- Do not replace missing evidence with prose.

## PR report

Include the candidate SHA, matrix of every gate, artifact/checksum links, incident/waiver summary, and GO/NO-GO recommendation.