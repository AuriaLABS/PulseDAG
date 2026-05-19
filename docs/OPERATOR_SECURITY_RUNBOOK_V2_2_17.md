# OPERATOR SECURITY RUNBOOK v2.2.17

## Scope

This runbook defines the secure-by-default operator posture for PulseDAG v2.2.17 in local and VPS environments. It focuses on RPC/API exposure control, access hardening, evidence collection, and incident response.

## Security Principles (v2.2.17)

- Keep RPC bound to localhost by default (`127.0.0.1:<port>`).
- Do **not** expose RPC directly to the public internet.
- Do **not** expose admin endpoints publicly.
- Prefer host-level firewalls + SSH tunneling + explicit allowlists.
- Use least privilege for API profiles and operational access.
- Treat auth tokens as secrets; never place real secrets in docs, tickets, or chat.

## Baseline Configuration Expectations

From current testnet/env examples, RPC defaults are localhost-bound:

- `PULSEDAG_RPC_BIND=127.0.0.1:8080`
- `PULSEDAG_RPC_BIND=127.0.0.1:8081`
- `PULSEDAG_RPC_BIND=127.0.0.1:8082`

Maintain this default unless a controlled private-network design requires otherwise.

## 1) Local Windows/WSL Testing

### Goals

- Keep node RPC private to the local host.
- Validate health/status/sync endpoints through localhost only.

### Recommended Steps

1. Start node with localhost bind in your env file (example):
   - `PULSEDAG_RPC_BIND=127.0.0.1:8080`
2. Run checks from the same host/WSL instance:
   - `curl -fsS http://127.0.0.1:8080/health`
   - `curl -fsS http://127.0.0.1:8080/status`
   - `curl -fsS http://127.0.0.1:8080/sync/status`
3. Confirm Windows firewall does not publish RPC inbound.
4. If using port proxies or Docker bridges, verify no unintended `0.0.0.0` mapping for RPC.

### Validation

- `ss -ltnp | rg ':8080'` should show loopback-only listener (`127.0.0.1`).

## 2) Ubuntu VPS Testing

### Goals

- Expose P2P as needed for peer connectivity.
- Keep RPC private (`127.0.0.1`) and access via secure channel only.

### Launch Posture

- RPC bind: `127.0.0.1:<rpc_port>`
- P2P bind: routable interface/port as required by topology
- Admin surfaces: restricted to trusted operators only

### Health Checks (server-local)

- `curl -fsS http://127.0.0.1:8080/health`
- `curl -fsS http://127.0.0.1:8080/status`
- `curl -fsS http://127.0.0.1:8080/p2p/status`
- `curl -fsS http://127.0.0.1:8080/sync/status`

## 3) Recommended Firewall Rules

Use default-deny inbound with explicit allows.

### Minimum inbound policy

- Allow SSH (`22/tcp`) only from trusted management IPs.
- Allow P2P port(s) required for network participation.
- Deny inbound RPC port(s) from untrusted/public networks.
- Deny inbound admin endpoint exposure from public networks.

### Example UFW workflow (adapt ports/IPs)

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow from <trusted-mgmt-ip>/32 to any port 22 proto tcp
sudo ufw allow <p2p-port>/tcp
sudo ufw deny <rpc-port>/tcp
sudo ufw enable
sudo ufw status verbose
```

> Replace placeholders with real values in your environment; do not publish internal topology details externally.

## 4) RPC Localhost-Only Default

Set RPC bind to loopback:

- `PULSEDAG_RPC_BIND=127.0.0.1:8080` (or your chosen local port)

Verification:

- `ss -ltnp | rg ':8080'`
- `curl -fsS http://127.0.0.1:8080/health`

If a check reveals `0.0.0.0:<rpc-port>`, treat as security drift and remediate immediately.

## 5) SSH Tunnel Access to RPC

When remote operator access is required, use SSH local forwarding instead of opening RPC.

### From operator workstation

```bash
ssh -N -L 18080:127.0.0.1:8080 <user>@<vps-host>
```

Then query locally:

- `curl -fsS http://127.0.0.1:18080/health`
- `curl -fsS http://127.0.0.1:18080/status`

Optional hardening:

- Use short-lived bastion sessions.
- Restrict SSH source IPs.
- Enforce key-based auth and disable password auth.

## 6) API Profiles

Define profiles by trust boundary and purpose:

- **Read-only operator profile**: health, status, sync, p2p diagnostics.
- **Maintenance profile**: snapshot/rebuild/prune/maintenance actions; restricted to on-call operators.
- **Admin profile**: highest-risk operations; limited to incident-response workflows and change windows.

Policy:

- Default all automation to read-only profile.
- Use higher-privilege profiles only with explicit ticket/change record.

## 7) Admin Endpoint Policy

- Never publish admin endpoints to the public internet.
- Restrict admin operations to localhost and/or trusted private management networks.
- Require authenticated operator context and auditable change records.
- Disable or isolate admin paths in non-maintenance periods when operationally possible.

## 8) Auth Token Setup

- Store tokens in protected environment variables or secret manager.
- Rotate tokens on personnel changes, incident suspicion, and scheduled cadence.
- Scope tokens by role/profile (read-only vs maintenance/admin).
- Avoid long-lived shared tokens.

Example placeholder-only pattern (no real secret):

```bash
export PULSEDAG_API_TOKEN="<redacted-token-value>"
```

Never commit tokens to git or include them in evidence bundles without redaction.

## 9) CORS Policy

- Use deny-by-default CORS posture.
- If browser clients are needed, allowlist exact origins (scheme + host + port).
- Avoid wildcard origin for privileged endpoints.
- Separate public read surfaces from privileged/admin surfaces.

Recommended policy model:

- `allow_origins=[]` by default.
- Add only required, explicit origins per environment.

## 10) Rate Limit Configuration

Apply conservative request limits, especially for expensive RPCs.

Recommended controls:

- Per-IP request rate ceilings.
- Burst limits with short refill windows.
- Stricter limits on write/admin endpoints than on health/status reads.
- Alert on sustained 429 spikes or abnormal request fanout.

Operational target:

- Throttle abuse without breaking expected operator polling cadence.

## 11) Evidence Collection

Collect evidence for routine audits and incidents.

### Minimum evidence set

- Listener state (`ss -ltnp`) showing RPC bind scope.
- Firewall state (`ufw status verbose` or equivalent).
- Key endpoint snapshots:
  - `/health`
  - `/status`
  - `/p2p/status`
  - `/sync/status`
- Service logs around event window.
- Change metadata (who/when/what for config or deployment changes).

### Handling

- Timestamp all captures in UTC.
- Hash/compress bundles for integrity.
- Redact secrets/tokens before sharing.

## 12) Incident Response Playbooks

### A) RPC exposed accidentally

1. Contain immediately:
   - Apply firewall deny on RPC port.
   - Rebind RPC to `127.0.0.1`.
2. Verify containment:
   - `ss -ltnp | rg ':<rpc-port>'`
   - external probe from trusted host should fail.
3. Rotate tokens/credentials potentially exposed.
4. Capture evidence (pre/post firewall, listener state, logs).
5. Perform impact review (unexpected calls, rate spikes, admin attempts).
6. File post-incident corrective actions.

### B) Admin endpoint exposed

1. Remove public exposure immediately (firewall + bind/policy).
2. Disable or isolate admin route access until review completes.
3. Rotate admin credentials/tokens.
4. Review logs for unauthorized admin calls.
5. Snapshot current chain/runtime status for integrity checks.
6. Escalate to security incident channel and complete RCA.

### C) Suspected abuse

1. Enable tighter temporary rate limits and source filtering.
2. Preserve logs and request metadata.
3. Compare baseline vs current traffic and endpoint usage.
4. Block offending IP ranges where justified.
5. Validate node health/sync and no unauthorized state-changing actions.

### D) Node degraded

1. Check:
   - `/health`, `/status`, `/sync/status`, `/p2p/status`
2. Assess resource saturation (CPU/memory/disk/IO).
3. Reduce external pressure (rate limit/temporary allowlist tightening).
4. If needed, perform controlled restart (see Section 13).
5. Track recovery metrics vs baseline.

### E) Sync stuck

1. Confirm peer connectivity and bootnode correctness (P2P, not RPC).
2. Inspect `/sync/status` and `/sync/missing` behavior.
3. Verify no firewall rule blocks required P2P paths.
4. Restart affected node in controlled order if necessary.
5. Re-validate convergence across peers.

### F) Storage warning

1. Confirm disk usage/inodes and write latency.
2. Trigger cleanup/expansion plan per storage policy.
3. Capture `/status` and maintenance diagnostics before changes.
4. Avoid abrupt kill; use safe shutdown path.
5. Resume service and verify sync/runtime health.

## 13) Safe Shutdown / Restart

1. Announce maintenance window (if applicable).
2. Capture pre-action evidence (`/health`, `/status`, `/sync/status`, logs).
3. Stop service gracefully via service manager.
4. Confirm process exit and port release.
5. Start service and wait for healthy endpoint responses.
6. Confirm sync progress and peer reconnect.
7. Capture post-action evidence and close checklist entry.

## 14) What Must Not Be Exposed Publicly

- RPC endpoints intended for operator or miner trust boundary.
- Admin/maintenance endpoints.
- Authentication tokens, headers, or secret-bearing configs.
- Internal-only diagnostics that meaningfully increase attack surface.

## 15) Documentation-Only Verification Checklist (v2.2.17)

- [ ] Commands target localhost RPC examples (`127.0.0.1:*`) by default.
- [ ] No section recommends opening RPC to the internet.
- [ ] No section recommends public admin endpoints.
- [ ] No real secrets/tokens in examples.
- [ ] Firewall and SSH-tunnel guidance reflects private-RPC posture.

