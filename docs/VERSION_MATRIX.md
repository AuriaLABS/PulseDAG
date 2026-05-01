# PulseDAG Version Matrix

This matrix keeps release positioning clear across the v2.2.x and v2.3.x line.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `2.2.7` |
| Current milestone | v2.2.7 clean foundation closure |
| Next major milestone | v2.3.0 private-testnet readiness |
| Miner architecture | External standalone miner |
| Smart contracts | Out of scope |
| Pool logic in miner | Out of scope / not allowed |

## Release boundaries

| Version | Purpose | Private testnet status |
| --- | --- | --- |
| v2.2.7 | Close PoW/mining/P2P foundation cleanly and align docs/versioning | Foundation only; manual/partial smoke checks |
| v2.2.8 | Optional/possible pre-testnet hardening line | Still pre-private-testnet unless explicitly changed later |
| v2.3.0 | Complete private-testnet readiness milestone | Target for real multi-node private testnet |

## Guardrails

- Do not move smart contracts into the v2.2.x line.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep the miner as an external application that talks to the node through documented interfaces.
- Keep v2.3.0 as the milestone for full P2P, multi-node PoW operation, sync/recovery, and operator readiness.
