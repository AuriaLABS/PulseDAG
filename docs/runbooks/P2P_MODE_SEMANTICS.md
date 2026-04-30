# P2P mode semantics guardrails

## Purpose
This runbook prevents operator confusion between simulated/development P2P execution and real external network connectivity.

## Canonical mode semantics

| mode | connected peer meaning | real network connectivity |
|---|---|---|
| `memory-simulated` | in-process simulated observations | no |
| `libp2p-dev-loopback-skeleton` | development loopback/skeleton observations | no |
| `libp2p-real` | external peers discovered/maintained by real libp2p runtime | yes |

## Endpoint interpretation rules

- `GET /status`
  - Treat `connected_peers_are_real_network=true` as the only signal that peer count reflects real network.
  - Always read `p2p_mode`, `p2p_runtime_mode_detail`, and `connected_peers_semantics` together.
- `GET /p2p/status`
  - Use `mode`, `runtime_mode_detail`, `connected_peers_are_real_network`, and `connected_peers_semantics` as the semantic source of truth.
- `GET /p2p/topology`
  - Use `mode`, `runtime_mode_detail`, `connected_peers_are_real_network`, and `connected_peers_semantics` before concluding external connectivity.

## Guardrail policy

- Simulated modes (`memory-simulated`, `libp2p-dev-loopback-skeleton`) must never be interpreted as real connectivity.
- Real mode (`libp2p-real`) may report real external connectivity through connected peers.
- Do not infer real connectivity from raw peer count alone.

## Verification checklist

1. Call `/status`, `/p2p/status`, `/p2p/topology`.
2. Confirm semantic fields are explicit and consistent across endpoints.
3. Confirm simulated modes report:
   - `connected_peers_are_real_network=false`
   - `connected_peers_semantics=simulated-or-internal-peer-observations`
4. Confirm real mode reports:
   - `connected_peers_are_real_network=true`
   - `connected_peers_semantics=real-network-connected-peers`
