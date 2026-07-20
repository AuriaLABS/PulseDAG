# PulseDAG v2.3.0 Incident Response

## Purpose

Define one private-testnet incident model for severity, ownership, evidence, communications, containment, recovery, and closure. This document governs operational incidents only and does not authorize public-testnet readiness or launch.

## Roles

| Role | Responsibility |
|---|---|
| Incident commander | Owns severity, decisions, timeline, and closure. |
| Technical lead | Diagnoses the affected surface and proposes containment/recovery. |
| Operator | Executes approved commands and records exact UTC timing. |
| Communications owner | Publishes internal status updates without unsupported readiness claims. |
| Evidence custodian | Preserves checksummed artifacts and access history. |
| Reviewer | Confirms exit criteria and follow-up ownership before closure. |

One person may hold multiple roles in a small team, but the incident commander and final reviewer should be distinct for SEV-1 and SEV-2 incidents.

## Severity model

### SEV-1 — Critical integrity or security risk

Examples:

- consensus or selected-chain divergence across healthy nodes;
- accepted invalid block or state corruption;
- private key, operator token, or signing material exposure;
- unrecoverable storage corruption affecting multiple nodes;
- active remote administrative compromise;
- any condition requiring the private testnet to stop producing blocks.

Response target: acknowledge immediately, freeze risky writes/mining, preserve evidence, and establish an incident channel. A SEV-1 is an automatic private-testnet no-go until reviewed and closed.

### SEV-2 — Major service degradation

Examples:

- multiple nodes partitioned or unable to converge;
- critical sync lag, replay gap, or missing-parent backlog;
- repeated failed upgrades/rollbacks;
- sustained loss of mining or transaction propagation;
- disk exhaustion risk within the current operator window;
- RPC abuse causing material node degradation.

Response target: acknowledge within 15 minutes, assign an incident commander, preserve evidence, and begin bounded containment.

### SEV-3 — Limited degradation

Examples:

- one node unhealthy while quorum/topology remains stable;
- warning-level lag, orphan pressure, or stale observability;
- exporter or dashboard failure without node impact;
- recoverable configuration drift.

Response target: acknowledge within one operator shift and create a tracked remediation item.

### SEV-4 — Minor operational defect

Examples:

- documentation mismatch;
- non-blocking alert noise;
- cosmetic dashboard issue;
- maintenance improvement with no active service impact.

Response target: record and prioritize through normal development workflow.

## Incident phases

### 1. Detect and declare

Record:

- incident ID;
- UTC declaration time;
- severity and rationale;
- affected nodes, roles, and network surface;
- first observed symptom and alert;
- incident commander and technical lead.

Do not silently downgrade severity. Any downgrade requires a timestamped rationale.

### 2. Preserve evidence

Before destructive action, run:

```bash
python3 scripts/private_testnet/collect_incident_evidence.py \
  --node-url http://127.0.0.1:8280 \
  --incident-id <INCIDENT_ID>-<NODE> \
  --severity <SEV-1|SEV-2|SEV-3|SEV-4> \
  --operator <OPERATOR_ID> \
  --out-dir /var/lib/pulsedag/incident-evidence
```

Also preserve:

- lifecycle state and release manifests;
- node/miner logs for the affected window;
- Prometheus alert state and relevant dashboard snapshots;
- configuration checksum, not secret values;
- deployment, upgrade, rollback, and identity-change timeline;
- Git commit, workflow run, and artifact identifiers.

Never paste operator tokens, identity keys, seed phrases, wallet material, or unredacted authorization headers into tickets or chat.

### 3. Contain

Choose the smallest safe action:

- stop the external miner before stopping a node when block production is implicated;
- isolate one affected node rather than restarting the entire network;
- revoke or rotate exposed credentials;
- rate-limit or firewall abusive RPC sources;
- stop pruning when snapshot/replay integrity is uncertain;
- prevent automated restart loops during corruption or identity incidents.

Document every command before execution for SEV-1/SEV-2 incidents unless immediate security containment is required.

### 4. Diagnose by surface

| Surface | Primary signals | Primary runbook |
|---|---|---|
| P2P / partition | peers, topology, propagation, exporter status | `docs/runbooks/P2P_RECOVERY.md` |
| Sync / missing parents | lag, consistency, selected-height gap, backlog | `docs/runbooks/RECOVERY_ORCHESTRATION.md` |
| Snapshot / replay | snapshot state, replay gap, persisted count | `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` |
| Upgrade / rollback | lifecycle state, current/previous release, health | `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md` |
| RPC abuse | request pressure, rate limit, degraded/stale response | `docs/runbooks/V2_3_0_SECURITY_AND_CAPACITY.md` |
| Disk pressure | filesystem use, snapshots, logs, prune policy | `docs/runbooks/V2_3_0_SECURITY_AND_CAPACITY.md` |
| Identity exposure | identity path, access history, peer impact | `docs/runbooks/V2_3_0_SECURITY_AND_CAPACITY.md` |
| Mining | template/submit responses, PoW health, miner logs | `docs/runbooks/V2_3_0_PRIVATE_TESTNET_OPERATIONS.md` |

### 5. Recover

Recovery must have explicit preconditions and a rollback path. After the action:

- confirm `/health`, `/status`, `/sync/status`, and `/sync/verify`;
- confirm expected peer reconnection and selected-height convergence;
- confirm snapshot/replay integrity;
- confirm exporter collection and alert recovery;
- reattach the external miner only after node consistency is established;
- capture a second evidence bundle for comparison.

### 6. Communicate

Internal updates should contain:

- severity and current impact;
- actions completed and next decision;
- known risks and blockers;
- exact UTC timestamp;
- owner of the next action.

Do not claim release readiness, public-testnet readiness, or incident resolution before exit criteria are verified.

### 7. Close

Closure requires:

- root cause or explicitly bounded unknown cause;
- impact window and affected nodes;
- final evidence bundle/checksums;
- validation that integrity and convergence are restored;
- follow-up issues with owners and deadlines;
- reviewer approval;
- confirmation that no secret was retained in incident artifacts;
- explicit private-testnet GO/NO-GO state.

## Incident record template

```text
Incident ID:
Severity:
Declared at (UTC):
Incident commander:
Technical lead:
Evidence custodian:
Affected nodes/surfaces:
Initial symptom:
Impact:
Containment:
Timeline:
Root cause:
Recovery:
Validation:
Evidence paths/checksums:
Follow-up issues/owners:
Final private-testnet state: GO | NO-GO | LIMITED
Closed at (UTC):
Reviewer:
```

## Automatic no-go conditions

The private testnet remains no-go when any of the following is unresolved:

- SEV-1 integrity/security incident;
- inconsistent chain or storage state;
- uncontained credential exposure;
- replay gap or restore uncertainty;
- repeated convergence failure;
- missing evidence for a destructive recovery action.

No incident decision starts or backdates the public-testnet 30-day clock.
