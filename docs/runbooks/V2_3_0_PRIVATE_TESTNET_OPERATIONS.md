# PulseDAG v2.3.0 Private-Testnet Operations

## Purpose

Provide the canonical day-to-day operating sequence for a multi-host PulseDAG private testnet. This runbook ties together the Task 07 configuration contract, Task 09 lifecycle controller, Task 10 observability package, external miner workflow, state protection, and evidence collection.

## Scope and guardrails

- Private testnet only.
- Node RPC remains loopback-only.
- Mining remains external to the node; no embedded pool logic.
- Smart contracts remain disabled and out of scope.
- The active candidate identifies itself as `VERSION=v2.3.0` with Cargo workspace version `2.3.0`.
- This runbook does not authorize a tag, publication, public-testnet launch, or the start/backdating of the 30-day public-testnet clock.
- `public_testnet_ready=false` remains mandatory until a separate public-testnet decision explicitly changes it.

## Candidate binding and evidence

Before private-testnet release closeout, record one exact candidate SHA and run the required lifecycle, recovery, packaging, smoke, evidence, and repository gates against that SHA. Evidence from an earlier candidate remains historical and must not be silently relabelled after active documentation, workflow, packaging, or operator-entrypoint changes.

An evidence-only closeout update may reference the already tested candidate, but it must not change runtime code or packaged release inputs. Any later change to runtime code, `README.md`, `docs/INSTALL_BINARIES_V2_3_0.md`, packaging scripts, release workflows, or release metadata requires a new exact-candidate evidence run.

## Host layout

Each node host should separate source checkout, release binaries, configuration, state, logs, and monitoring:

```text
/etc/pulsedag/private-testnet.env
/var/lib/pulsedag/lifecycle/
/var/lib/pulsedag/identity.key
/var/lib/pulsedag/rocksdb/
/var/lib/pulsedag/incident-evidence/
/var/log/pulsedag/                 # optional external log shipping target
```

The lifecycle controller owns its own release, PID, log, lock, and state paths beneath `/var/lib/pulsedag/lifecycle/`.

## 1. Bootstrap a node host

1. Create the operator account and persistent directories.
2. Copy the appropriate template from `configs/private-testnet/` into `/etc/pulsedag/private-testnet.env`.
3. Replace all example DNS names and persistent paths.
4. Create or restore the node identity key with mode `0600`.
5. Run the configuration contract:

```bash
bash scripts/v2_3_0_private_testnet_preflight.sh \
  /etc/pulsedag/private-testnet.env
```

6. Install the first node release:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  install \
  --binary ./dist/pulsedagd \
  --release-id v2.3.0-rc1
```

7. Verify before starting:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  verify
```

8. Start and inspect:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  start

python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  status
```

## 2. Attach the external miner

Before attaching a miner, require all of the following:

- `/health` succeeds;
- `/status` shows the expected chain ID and progressing height;
- `/sync/status` reports consistency and acceptable lag;
- at least one real P2P peer is connected for an ordinary node;
- Task 10 monitoring reports successful RPC collection.

Run a one-shot external-miner smoke:

```bash
scripts/release/standalone_operator_smoke.sh \
  --node-url http://127.0.0.1:8280 \
  --miner-address <PRIVATE_TESTNET_ADDRESS>
```

For continuous mining, use the standalone miner binary and preserve its logs separately from node logs. Never place payout secrets, wallet material, or miner credentials in repository files or metric labels.

## 3. Routine health check

At the beginning of each operator shift:

```bash
curl --fail --silent http://127.0.0.1:8280/health | jq
curl --fail --silent http://127.0.0.1:8280/status | jq
curl --fail --silent http://127.0.0.1:8280/sync/status | jq
curl --fail --silent http://127.0.0.1:8280/sync/verify | jq
curl --fail --silent http://127.0.0.1:8280/p2p/status | jq
curl --fail --silent http://127.0.0.1:8280/tx/mempool | jq
curl --fail --silent http://127.0.0.1:8280/pow/health | jq
```

Confirm in Prometheus/Grafana:

- all five exporters are up;
- `pulsedag_exporter_scrape_success == 1`;
- peer counts, selected height, and sync lag are credible;
- no critical alert is active;
- snapshots exist and storage replay gap is zero.

Use `docs/runbooks/MAINTENANCE_SELF_CHECK.md` for the broader read-only checklist.

## 4. Upgrade and rollback

Capture baseline evidence before any change. Then run:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  upgrade \
  --binary ./dist/pulsedagd-next \
  --release-id v2.3.0-rc2
```

The controller waits for health and automatically restores the previous release when the new binary fails. An explicit rollback is:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  rollback
```

After upgrade or rollback, verify height monotonicity, sync consistency, peer reconnection, exporter collection, and external miner attachment. Retain the prior release until the maintenance ticket is closed.

## 5. Snapshot, prune, restore, and backup

- Use `docs/runbooks/SNAPSHOT_RESTORE.md` for snapshot restore and candidate-scoped RTO evidence.
- Use `docs/runbooks/SNAPSHOT_PRUNE_RESTORE_DRILL.md` for the guarded private-testnet drill.
- Use `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` when replay or persisted-state divergence requires rebuild.
- Run the selected recovery drill against the exact release candidate and attach pre-action, post-action, timing, consistency, and readiness evidence to the closeout record.
- Do not prune without a valid snapshot when `PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true`.
- Back up identity material separately from RocksDB and snapshot data.
- Never copy a live RocksDB directory without the supported snapshot/backup procedure.

## 6. Partition, lag, and rejoin

- Peer loss or topology failure: `docs/runbooks/P2P_RECOVERY.md`.
- Lag, missing-parent backlog, or consistency failure: `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
- Critical lag or replay gap: `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md`.
- Preserve pre-action and post-action evidence and avoid restarting every node simultaneously.

## 7. Incident evidence

Create one evidence bundle per affected node:

```bash
python3 scripts/private_testnet/collect_incident_evidence.py \
  --node-url http://127.0.0.1:8280 \
  --incident-id INC-2026-0001-node-1 \
  --severity SEV-2 \
  --operator <OPERATOR_ID> \
  --out-dir /var/lib/pulsedag/incident-evidence
```

The collector saves redacted JSON responses, a manifest, UTC timestamps, and SHA-256 checksums. Copy the immutable bundle to the incident record before making destructive changes.

## 8. Stop and decommission

Stop idempotently:

```bash
python3 scripts/private_testnet/node_lifecycle.py \
  --root /var/lib/pulsedag/lifecycle \
  --env-file /etc/pulsedag/private-testnet.env \
  stop
```

Before decommissioning, archive required evidence, revoke monitoring access, preserve or intentionally destroy identity material according to the incident/security decision, and record the final state. Do not reuse a retired identity unintentionally.

## Exit criteria for an operator action

An action is complete only when:

- lifecycle command exits successfully;
- node health and sync consistency are restored;
- expected peers reconnect;
- observability collection is green;
- external mining is reattached when required;
- evidence and operator notes are attached;
- no unsupported readiness or launch claim was made.
