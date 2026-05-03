# Block Propagation Rehearsal v2.2.9 (A → B → C)

## Objetivo
Hacer observable y verificable el flujo de propagación de bloques entre nodos de ensayo.

## Flujo esperado
1. Nodo A acepta un bloque minado localmente.
2. Nodo A anuncia el hash a pares (`block_announced`).
3. Nodo B/C reciben anuncio.
4. Si no conocen el hash, registran `unknown_block_announced` y `block_request_sent` (intención GetBlock).
5. El emisor responde con `BlockData` (observado por `block_data_received`/`block_data_sent` a nivel de protocolo/operación).
6. Receptor enruta el bloque completo a la ruta central de aceptación (`accept_block_with_result`).
7. Si se acepta, registrar `block_accepted_from_peer`; si falla, `block_rejected_from_peer`.
8. Si faltan padres, registrar `missing_parent_detected` y cola de huérfanos.
9. Anuncios duplicados deben ignorarse (`duplicate_block_ignored`).

## Señales operativas (logs/eventos)
- `block_announced`
- `unknown_block_announced`
- `block_request_sent`
- `block_accepted_from_peer`
- `block_rejected_from_peer`
- `duplicate_block_ignored`
- `missing_parent_detected`

## Seguridad de rebroadcast
- Deduplicación inbound/outbound por ID de mensaje.
- Supresión de anuncios/bloques duplicados para evitar bucles de rebroadcast.
- No reprocesar hashes ya vistos.

## Cobertura de pruebas
- Roundtrip serialización de inventario de bloque.
- Ignorar anuncio duplicado.
- Emitir evento inbound ante anuncio de bloque desconocido (habilita GetBlock).
- `BlockData` entrega bloque completo a la ruta de aceptación central.

## Rehearsal manual A → B → C
1. Iniciar tres nodos con mismo `chain_id` y conectividad P2P.
2. Minar en A.
3. Ver en B/C:
   - `block_announced`
   - `unknown_block_announced`
   - `block_request_sent`
   - `block_accepted_from_peer` (o `missing_parent_detected`/`block_rejected_from_peer`)
4. Repetir anuncio del mismo hash y verificar `duplicate_block_ignored`.
