# PulseDAG Version Matrix

This matrix keeps release positioning clear across v2.2.x and v2.3.x.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `2.2.10` |
| Current milestone | v2.2.10 final PoW completion |
| Next milestone | v2.2.11 P2P completion |
| Private-testnet readiness milestone | v2.3.0 |
| Miner architecture | External standalone miner |
| Smart contracts | Out of scope |
| Pool logic in miner | Out of scope / not allowed |

## Release boundaries

| Version | Purpose | Status framing |
| --- | --- | --- |
| v2.2.8 | Hardening baseline closure | Pre-private-testnet hardening |
| v2.2.9 | Private-testnet rehearsal closure | Rehearsal only |
| v2.2.10 | Final PoW completion | PoW finalized, P2P not yet complete |
| v2.2.11 | P2P completion | Networking/sync completion focus |
| v2.3.0 | Official complete private-testnet readiness milestone | Readiness decision milestone |

## Guardrails

- Do not move smart contracts into the v2.2.x line.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep miner external and node-facing through documented interfaces.
