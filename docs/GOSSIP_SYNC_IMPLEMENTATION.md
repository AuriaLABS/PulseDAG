
# Gossip + Sync

## Objetivo
Pasar de observabilidad de red a sincronización efectiva entre nodos.

## Flujos
1. `NewBlock`
2. `NewTransaction`
3. `GetBlock`
4. `BlockData`
5. `GetBlocksFromHeight`
6. `BlocksBatch`
7. `PeerHello`

## Reglas
- ignorar duplicados
- no reprocesar bloques ya confirmados
- aceptar huérfanos si faltan padres
- al detectar lag, pedir lotes por altura
- limitar batch size
- registrar counters runtime
