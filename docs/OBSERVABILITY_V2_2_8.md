# OBSERVABILITY v2.2.8

## Logs clave
- `node startup identity`: confirma versión, `chain_id`, perfil/red y binds.
- `p2p initialized`: confirma modo efectivo (memory/libp2p) y semántica de peers.
- `p2p peer connected`: muestra peer y conteo observado.
- `block announced by peer` / `unknown block announced; requesting block from peers`.
- `queued inbound p2p orphan block`: bloque recibido con padres faltantes.
- `adopted ready orphan blocks after inbound block`: reintentos exitosos de huérfanos.
- `accepted inbound p2p block`: avance real de sync.
- `mining template created`: plantillas externas emitidas.
- `mining submit accepted` / `mining submit rejected: invalid PoW`.

## Métricas/counters disponibles
Se exponen como contadores internos en `NodeRuntimeStats` (endpoint runtime):
- `pulsedag_blocks_accepted_total`
- `pulsedag_blocks_rejected_total`
- `pulsedag_invalid_pow_total`
- `pulsedag_mining_templates_total`
- `pulsedag_mining_submits_total`
- `pulsedag_p2p_blocks_received_total`
- `pulsedag_sync_missing_parents_total`

Señales equivalentes ya existentes:
- `external_mining_submit_accepted` / `external_mining_submit_rejected`
- `queued_orphan_blocks`, `adopted_orphan_blocks`
- `connected_peers` vía estado P2P (`p2p.status`).
- `orphans_current` vía `chain.orphan_blocks.len()` y endpoint `/orphans`.

## Señales esperadas en smoke test
1. Startup:
   - `node startup identity` seguido de `p2p initialized`.
2. Red/P2P:
   - eventos de `p2p peer connected` y anuncios de bloques.
3. Sync:
   - bloques recibidos (`pulsedag_p2p_blocks_received_total` sube), aceptados (`pulsedag_blocks_accepted_total` sube),
     huérfanos transitorios y posterior adopción.
4. Mining:
   - `mining template created` periódicamente.
   - `mining submit accepted` para shares/bloques válidos.

## Indicadores de falla
- `pulsedag_invalid_pow_total` creciendo rápidamente (miner mal configurado o reloj/target incoherente).
- `pulsedag_sync_missing_parents_total` creciendo sin adopción de huérfanos.
- Muchos `mining submit rejected` con `stale_template` (clientes no refrescan template).
- `accepted inbound p2p block` ausente durante largos periodos con peers conectados.
