# PulseDAG Version Matrix

This matrix keeps release positioning clear across the v2.2.x and v2.3.x line.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `2.2.9` |
| Current milestone | v2.2.9 private-testnet rehearsal |
| Next major milestone | v2.3.0 official complete private-testnet readiness |
| Miner architecture | External standalone miner |
| Smart contracts | Out of scope |
| Pool logic in miner | Out of scope / not allowed |

## Release boundaries

| Version | Purpose | Private testnet status |
| --- | --- | --- |
| v2.2.7 | Clean foundation closure | Foundation only; manual/partial smoke checks |
| v2.2.8 | Ambitious hardening baseline closure | Pre-private-testnet hardening baseline |
| v2.2.9 | Private-testnet rehearsal closure | Rehearsal only; not official readiness |
| v2.3.0 | Official complete private-testnet readiness milestone | Target for official private testnet readiness decision |

## Guardrails

- Do not move smart contracts into the v2.2.x line.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep the miner as an external application that talks to the node through documented interfaces.
- Keep v2.3.0 as the milestone for official full P2P, multi-node PoW operation, sync/recovery, and operator readiness closure.
