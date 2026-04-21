# P2P runtime + sync lag

## Objetivo
Dar visibilidad operativa al estado de red de cada nodo antes de abrir una testnet prolongada.

## Señales mínimas
- peer_count
- last_peer_message_unix
- inbound_block_count
- inbound_tx_count
- outbound_block_count
- outbound_tx_count
- sync_lag_blocks
- sync_target_height
- selected_tip

## Fórmula base de lag
sync_lag_blocks = max(0, sync_target_height - local_best_height)

## Fuente del target
Mientras no haya consenso multi-peer completo:
- usar `highest_peer_advertised_height` si existe
- si no existe, usar `local_best_height`

## Estados recomendados
- healthy: lag <= 2
- catching_up: lag between 3 and 20
- behind: lag > 20
