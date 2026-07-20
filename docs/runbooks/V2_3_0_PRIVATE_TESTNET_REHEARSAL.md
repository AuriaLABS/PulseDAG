# PulseDAG v2.3.0 Multi-Host Private-Testnet Rehearsal

## Purpose

Run the final v2.3.0 private-testnet operations rehearsal against one exact candidate commit and produce a checksummed GO/NO-GO bundle. This procedure validates the Task 07 bootstrap contract, Task 09 lifecycle controller, Task 10 observability fields, Task 11 operating procedures, external mining, restart, partition, and rejoin as one coherent system.

A successful rehearsal is a private-testnet operations decision only. It does not authorize a public testnet, a release tag, a version bump, smart contracts, or the start of the 30-day public-testnet clock.

## Required topology

Use exactly five nodes:

- one seed;
- four ordinary nodes;
- separate hosts or genuinely isolated Linux network namespaces;
- persistent and unique identity and RocksDB paths for every node;
- `libp2p-real`, Kademlia enabled, and mDNS disabled;
- loopback-only node RPC;
- at least one external miner operating independently of the node processes.

Do not count five processes sharing one host network namespace as the Task 12 topology.

## Safety prerequisites

Before fault injection, require all of the following:

1. An operator owns the rehearsal and a second person knows the rollback path.
2. Every host has working out-of-band or console access.
3. SSH host keys are pinned; password prompts and first-use trust prompts are not allowed.
4. The candidate binary, source checkout, and inventory all refer to the same 40-character commit SHA.
5. Each host passes `scripts/v2_3_0_private_testnet_preflight.sh` and Task 09 `verify`.
6. The external miner is already attached and its logs are stored outside the repository.
7. No SEV-1 incident, integrity alarm, replay gap, or unresolved credential exposure is active.
8. The fault hook has been reviewed on the target host and changes only the PulseDAG P2P path.

## Inventory preparation

Copy `configs/private-testnet/rehearsal.inventory.example.json` to an operator-owned location outside the repository. Replace every example hostname and the zero candidate SHA.

Each `transport` is an argv prefix and `transport_mode` is either `ssh` or `argv`. A normal SSH definition is:

```json
{"transport": ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", "pulsedag@node-1.example.net"], "transport_mode": "ssh"}
```

The controller models every remote action as argv. In `ssh` mode it applies standard shell quoting before handing one command to OpenSSH; in `argv` mode it appends arguments directly, which is suitable for wrappers such as `ip netns exec`. Operator-provided shell command strings are not accepted. Keep RPC URLs on `127.0.0.1`, `localhost`, or `::1`; collection executes on each remote host.

Validate before running:

```bash
python3 scripts/private_testnet/multi_host_rehearsal.py \
  validate-inventory \
  --inventory /secure/pulsedag/v2.3.0-rehearsal.json
```

## Fault-hook contract

Install an operator-reviewed executable on the selected ordinary node. The inventory points to two argv forms, for example:

```json
{
  "target": "node-4",
  "isolate_command": ["/usr/local/sbin/pulsedag-rehearsal-network", "isolate"],
  "restore_command": ["/usr/local/sbin/pulsedag-rehearsal-network", "restore"]
}
```

The hook must:

- be idempotent;
- isolate only the node's PulseDAG P2P traffic;
- preserve SSH, loopback RPC, monitoring, and recovery access;
- record the exact rule or namespace link it changed in host-local operator logs;
- restore only the rule it owns;
- fail non-zero when it cannot prove the requested state;
- never flush an entire firewall table or replace unrelated routes.

Test `restore` before the live run. Do not start Task 12 without a known recovery path that does not depend on the affected P2P link.

## Run sequence

Create a new, non-existing evidence directory and run:

```bash
python3 scripts/private_testnet/multi_host_rehearsal.py \
  run \
  --inventory /secure/pulsedag/v2.3.0-rehearsal.json \
  --out-dir /var/lib/pulsedag/rehearsals/<candidate-sha>-<utc-run-id>
```

The controller performs these phases in order:

1. Run Task 07 preflight and Task 09 lifecycle verification on all five hosts.
2. Start the seed first and ordinary nodes second.
3. Require five-node convergence using stable read-only RPC fields.
4. Require the external miner to advance the network by the configured minimum.
5. Restart the selected ordinary node and require rejoin.
6. Apply the bounded P2P isolation hook.
7. Require zero peers on the target while the other four nodes remain converged.
8. Restore the exact hook and require five-node rejoin and convergence.
9. Require external mining to advance again after the fault sequence.
10. Capture final endpoint evidence and write the decision.
11. Hash every evidence file into `SHA256SUMS`.

The controller attempts the restore hook in a `finally` path after every isolation attempt. An operator must still verify host networking independently after any interrupted run.

## Evidence verification

Verify a completed bundle before copying or reviewing it:

```bash
python3 scripts/private_testnet/multi_host_rehearsal.py \
  verify-evidence \
  --evidence-dir /var/lib/pulsedag/rehearsals/<candidate-sha>-<utc-run-id>
```

A valid `GO` bundle requires PASS records for preflight, start, baseline convergence, pre-fault external-mining progress, restart/rejoin, partition, partition/rejoin, final convergence, and post-fault external-mining progress. `decision.json` must retain:

```json
{
  "version_bump_authorized": false,
  "public_testnet_ready": false,
  "thirty_day_public_testnet_clock_started": false
}
```

Copy the immutable bundle to the protected release evidence location. Never edit a bundle in place; rerun with a new directory when evidence is incomplete or incorrect.

## GO criteria

Record private-testnet `GO` only when:

- the exact candidate SHA matches all five clean source checkouts and the inventory;
- every mandatory controller phase passed;
- all five nodes end healthy, fresh, consistent, and converged;
- external mining progresses before and after the fault sequence;
- the target restarted, isolated, restored, and caught up;
- checksums cover every evidence file exactly once and verify;
- no SEV-1 blocker or destructive recovery occurred;
- the evidence reviewer confirms no secret or private-key material is present.

## NO-GO criteria

Any of the following is an immediate `NO-GO`:

- candidate mismatch, dirty source checkout, or unpinned deployment;
- preflight or lifecycle verification failure;
- non-real P2P mode, stale/degraded status, or unexpected chain ID;
- peer topology failure outside the intended isolation;
- sync inconsistency, live sync error, or non-zero replay gap;
- missing external-mining progress before or after the fault sequence;
- failed restart, failed isolation proof, failed restore, or failed rejoin;
- height spread above the configured bound after the phase timeout;
- missing phase evidence, invalid checksum, unchecksummed file, or evidence mutation;
- loss of SSH/out-of-band recovery access;
- any SEV-1 integrity or security incident.

After `NO-GO`, preserve evidence, restore networking, stabilize the nodes, and follow `docs/runbooks/V2_3_0_INCIDENT_RESPONSE.md`. Do not advance Task 13.

## Protected Actions execution

`.github/workflows/v2_3_0_multi_host_rehearsal.yml` always runs the deterministic contract regression. The live job is manual, uses a protected self-hosted environment, and requires these environment secrets:

- `PULSEDAG_V2_3_0_REHEARSAL_INVENTORY_B64`;
- `PULSEDAG_V2_3_0_REHEARSAL_SSH_KEY`;
- `PULSEDAG_V2_3_0_REHEARSAL_KNOWN_HOSTS`.

The live gate compares the inventory candidate SHA with the checked-out commit, uploads evidence even on `NO-GO`, and succeeds only when the controller exits zero, the bundle verifies, and `decision.json` says `GO`.
