# PulseDAG Version Matrix

This matrix keeps release positioning clear across v2.2.x and v2.3.x.

## Current baseline

| Area                                         | Current value                                                    |
| -------------------------------------------- | ---------------------------------------------------------------- |
| Workspace package version                    | `2.2.10` until a release/version-bump PR changes Cargo metadata. |
| Current documentation milestone              | v2.2.11 P2P completion.                                          |
| Previous milestone                           | v2.2.10 final PoW completion.                                    |
| Next rehearsal milestone                     | v2.2.12 full private-testnet rehearsal across operators.         |
| Private-testnet readiness decision milestone | v2.3.0.                                                          |
| Miner architecture                           | External standalone `pulsedag-miner`.                            |
| Smart contracts                              | Out of scope.                                                    |
| Pool logic in miner                          | Out of scope / not allowed.                                      |
| Public mainnet readiness                     | Not claimed.                                                     |

## Release boundaries

| Version | Purpose                                                        | Status framing                                                                                                                                                                           |
| ------- | -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v2.2.8  | Hardening baseline closure                                     | Pre-private-testnet hardening.                                                                                                                                                           |
| v2.2.9  | Private-testnet rehearsal closure                              | Rehearsal only.                                                                                                                                                                          |
| v2.2.10 | Final PoW completion                                           | PoW finalized; external miner flow documented.                                                                                                                                           |
| v2.2.11 | P2P completion                                                 | Real `libp2p-real` networking, chain-id isolated topics, block/tx propagation, sync recovery, restart catch-up, and operator troubleshooting for multi-node private-testnet preparation. |
| v2.2.12 | Full private-testnet rehearsal                                 | Consumes v2.2.11 P2P outputs for broader operator rehearsal.                                                                                                                             |
| v2.3.0  | Official complete private-testnet readiness decision milestone | Future readiness decision only; not implied by v2.2.11 docs.                                                                                                                             |

## v2.2.11 P2P completion documentation set

| Document                        | Role                                                                                                                                                                         |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/P2P_SPEC_V2_2_11.md`      | P2P architecture, topics, message flow, validation, rebroadcast, duplicate suppression, tx propagation, and sync overview.                                                   |
| `docs/P2P_REHEARSAL_V2_2_11.md` | Local or external Ubuntu three-node rehearsal runbook with exact node/miner commands and endpoint checks.                                                                    |
| `docs/SYNC_RECOVERY_V2_2_11.md` | Troubleshooting guide for peer count, chain-id mismatch, block fetch failures, invalid PoW, stuck missing parents, duplicate storms, catch-up failures, and firewall issues. |

## Guardrails

- Do not move smart contracts into the v2.2.x line.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep the miner external and node-facing through documented RPC interfaces.
- Do not claim v2.3.0 readiness from v2.2.11.
- Do not claim public mainnet readiness from v2.2.x P2P completion docs.
