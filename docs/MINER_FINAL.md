# Miner final consolidado

Estado: **cerrado**

## Contrato funcional

El miner es una aplicación externa separada del nodo.

### Entrada
- URL del nodo
- dirección del minero
- parámetros locales de loop

### Flujo
- `POST /mining/template`
- minado local PoW
- `POST /mining/submit`

### Responsabilidades
- calcular nonces
- intentar resolver el bloque
- reenviar el bloque al nodo

### No responsabilidades
- no coordina pool
- no reparte trabajo entre workers
- no lleva shares
- no lleva payouts
- no mantiene estado de pool

## Decisión de arquitectura

Toda la lógica de pool, si existe, vive en el nodo o en servicios del lado servidor.
