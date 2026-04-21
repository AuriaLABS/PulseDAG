# Testnet 3–5 nodos

## Topología mínima recomendada
- nodo A: seed inicial
- nodo B: peer de A
- nodo C: peer de A y B
- minero 1 contra A
- minero 2 contra B (opcional)

## Objetivos de esta fase
1. propagación de bloques
2. propagación de transacciones
3. sync incremental básico
4. estabilidad tras reinicio
5. journal y alertas operativas sanas

## Criterios de éxito
- bloques nuevos visibles en varios nodos
- `sync_lag_blocks` cercano a 0 la mayor parte del tiempo
- sin crecimiento sostenido de `orphan_count`
- sin alertas críticas persistentes
- replay/rebuild correctos tras reinicio controlado
