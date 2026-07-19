# PulseDAG v2.3.0 Security and Capacity Operations

## Purpose

Provide bounded operator procedures for RPC abuse, disk pressure, node identity rotation, credential exposure, and monitoring-network access. These procedures apply to the private testnet only.

## Baseline security posture

- Node RPC binds to loopback.
- The Prometheus exporter is the only monitoring endpoint exposed to the private monitoring network.
- Administrative RPC remains disabled unless explicitly required, authenticated, time-bounded, and approved.
- `PULSEDAG_API_PROFILE=private_operator` and RPC rate limiting remain enabled.
- Node identities, operator tokens, wallet material, and miner secrets live outside the source tree.
- Managed directories are operator-owned and not group/world writable.

## RPC abuse or request pressure

### Indicators

- `pulsedag_node_rpc_response_degraded == 1`;
- `pulsedag_node_rpc_response_stale == 1`;
- `pulsedag_exporter_endpoint_success{endpoint=...} == 0`;
- elevated CPU/memory or event-loop delay correlated with RPC sources;
- repeated rate-limit responses or large request bodies;
- operator/admin endpoint probes.

### Immediate actions

1. Preserve incident evidence and request-source logs.
2. Confirm RPC bind and API profile from the host-local environment file.
3. Block abusive source addresses at the private firewall or reverse proxy.
4. Reduce exposure before increasing process resources.
5. Keep node RPC private; do not solve monitoring failures by binding RPC publicly.
6. If administrative endpoints are enabled, disable them unless required for containment.
7. Rotate the operator token when exposure is suspected.

### Validation

- `/health`, `/status`, and `/sync/status` recover without stale/degraded flags;
- request rate returns to the expected private-testnet baseline;
- no unauthorized admin action occurred;
- monitoring exporter collection returns to success;
- configuration and firewall changes are recorded.

### Escalation

Declare SEV-1 for confirmed remote administrative compromise or credential exposure. Declare SEV-2 when abuse materially disrupts multiple nodes or block/transaction propagation.

## Disk pressure

### Indicators

- filesystem use above 80 percent: warning;
- filesystem use above 90 percent or projected exhaustion within one shift: critical;
- snapshot missing or snapshot creation failing;
- log growth accelerating;
- RocksDB compaction/write errors;
- prune operation unable to complete;
- `pulsedag_sync_storage_replay_gap > 0`.

### Immediate actions

1. Preserve filesystem, snapshot, prune, and lifecycle evidence.
2. Stop nonessential local log duplication and temporary evidence generation.
3. Move completed evidence bundles to approved external storage after checksum verification.
4. Verify a valid snapshot before considering prune changes.
5. Never delete RocksDB files manually.
6. Never remove the only known-good snapshot.
7. Stop the external miner if storage errors threaten node consistency.
8. If exhaustion is imminent, stop the affected node cleanly and expand storage or restore on a prepared host.

### Safe cleanup order

1. temporary package/build output outside the repository;
2. duplicated logs already shipped and checksummed;
3. completed incident/rehearsal bundles copied to approved storage;
4. obsolete immutable release directories, retaining `current`, `previous`, and the last known-good release;
5. protocol-aware prune/restore actions through supported scripts and runbooks only.

### Recovery validation

- free space is above the warning threshold with projected headroom;
- snapshot exists and is readable;
- storage replay gap is zero;
- node restarts and sync consistency passes;
- height and peer convergence resume;
- external mining is reattached only after integrity checks.

Use `docs/runbooks/SNAPSHOT_RESTORE.md` and `docs/runbooks/REBUILD_FROM_SNAPSHOT_AND_DELTA.md` when storage integrity is uncertain.

## Node identity rotation

### Rotate when

- the identity key is exposed, copied to an untrusted host, or has unknown custody;
- a host is decommissioned or transferred;
- duplicate peer identity is detected;
- filesystem ownership or permissions were compromised;
- the incident commander requires isolation from the prior peer identity.

Do not rotate identity merely to hide a connectivity problem. Diagnose DNS, firewall, bootnodes, and P2P health first.

### Procedure

1. Declare/record the maintenance or incident decision.
2. Stop the external miner attached to the node.
3. Collect pre-rotation evidence.
4. Stop the node with the Task 09 lifecycle controller.
5. Back up the old identity into restricted incident custody when investigation requires preservation; otherwise destroy it according to policy.
6. Generate a new identity using the supported node identity mechanism.
7. Set owner-only permissions and verify the configured persistent path.
8. Update inventory/DNS/peer allowlists that explicitly reference the old identity.
9. Run Task 07 preflight.
10. Start the node and verify peer discovery, chain ID, sync consistency, and convergence.
11. Collect post-rotation evidence and record old/new public peer identifiers without storing private key bytes.
12. Reattach the external miner only after node consistency is established.

### Failure response

If the new identity cannot join:

- keep the old identity retired when exposure triggered rotation;
- verify bootnode and firewall configuration;
- compare P2P status and logs;
- do not copy another active node's identity key;
- escalate through `docs/runbooks/P2P_RECOVERY.md`.

## Operator token rotation

1. Disable or restrict the affected administrative surface.
2. Generate a new high-entropy token outside shell history and repository files.
3. Update the host-local secret store.
4. Restart only the required service.
5. Verify old-token rejection and new-token acceptance through a private path.
6. Redact tokens from evidence; retain only rotation timestamp and credential identifier.

## Monitoring-network access

- Restrict exporter port `9108` to Prometheus collectors.
- Do not forward node RPC through the exporter host.
- Use stable `node`, `role`, and `network` labels only.
- Do not add block hashes, transaction IDs, peer IDs, wallet addresses, tokens, or keys as labels.
- Treat missing exporter data as an observability failure, not proof that the node is down.

## Exit criteria

A security/capacity action is complete only when:

- affected credentials/identities are contained;
- storage and process health are stable;
- node consistency and peer convergence pass;
- monitoring collection is restored;
- evidence is redacted and checksummed;
- incident severity and final private-testnet state are reviewed;
- no public-testnet or release-readiness claim was introduced;
- the 30-day public-testnet clock remains not started and is never backdated by an operational action.
