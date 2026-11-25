# Retained development and incident utilities — not a private network

This historical directory name does not define a network, release target or supported deployment profile. PulseDAG has no deployed predecessor. Its first live network will be the native public testnet, subject to #781. Do not run the old remote rehearsal or node-start examples from Git history.

## Retained utilities

- `collect_incident_evidence.py`: bounded redacted evidence collection; validate against the exact native runtime and preserve partial-collection failures.
- `runtime_metrics_exporter.py`: current v3 monitoring integration, restricted to the active v3 inventory.
- Lifecycle, namespace, multi-host, cadence, packaged recovery and PoW accounting modules: temporary development/CI dependencies. Some still exercise prototype protocol carriers. They are not a native monetary acceptance path or public operator deployment command. Replace them with native equivalents before deleting their remaining regression coverage.

## Required transferable safeguards

Immutable release directories and checksum verification; atomic activation and rollback; exclusive lock; no shell interpretation of environment files; verified ownership and permissions; DNS validation; persistent identities; PID reuse protection; bounded scoped network faults; preserved management and recovery access; no secret collection; deterministic redacted evidence and explicit incomplete/failure status. Preserve the existing isolated fixture tests while replacing the underlying harnesses. These safeguards apply to first-network tooling without requiring a predecessor protocol.

Current authorities: [first-network baseline](../../docs/FIRST_NETWORK_BASELINE.md), [testnet-first launch](../../docs/runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md), [runbook index](../../docs/runbooks/INDEX.md), [security](../../SECURITY.md). No script may set public readiness, start the 30-day clock, enable mining, or activate contracts.

Historical operator examples are available at [the pre-retirement commit](https://github.com/AuriaLABS/PulseDAG/blob/b97280e434d357bcc8336a19a73137a5ce90c7dd/scripts/private_testnet/README.md), not in active instructions.
