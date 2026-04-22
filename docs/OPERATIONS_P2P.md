# OPERATIONS_P2P

## Objetivo
Evitar ambigüedad operativa entre simulación local, skeleton de desarrollo y red real.

## Modos canónicos

| mode | significado | `connected_peers` representa conectividad real |
|---|---|---|
| `memory-simulated` | simulación en memoria/in-process | no |
| `libp2p-dev-loopback-skeleton` | esqueleto de desarrollo con loopback y wiring parcial de libp2p | no |
| `libp2p-real` | red libp2p real (reservado/futuro o implementación real) | sí |

## Reglas de honestidad de status
- Nunca interpretar `connected_peers` como red real cuando `connected_peers_are_real_network=false`.
- Si `mode=libp2p-dev-loopback-skeleton`, los endpoints deben evitar cualquier afirmación implícita de conectividad P2P real.
- No usar etiquetas ambiguas como `libp2p` “a secas” en status operativo.

## Endpoints relevantes
- `GET /status`
  - `p2p_mode`
  - `connected_peers_are_real_network`
  - `peer_count`
- `GET /p2p/status`
  - `mode`
  - `connected_peers_are_real_network`
  - `connected_peers`
- `GET /p2p/topology`
  - `mode`
  - `connected_peers_are_real_network`
  - `peer_count`

## Logs de arranque
En startup se registra:
- `configured_mode`
- `effective_mode`
- `runtime_mode_detail`
- `connected_peers_are_real_network`

Esto permite distinguir inmediatamente entre:
- simulación (`memory-simulated`)
- skeleton de desarrollo (`libp2p-dev-loopback-skeleton`)
- red real (`libp2p-real`, cuando exista/esté habilitada)
