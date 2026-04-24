# Miner final consolidado

Estado: **cerrado**

Especificación canónica congelada: `docs/POW_SPEC_FINAL.md`.
Ruta PoW actual (validación del nodo, superficies provisionales y límites de upgrade): `docs/POW_CURRENT_PATH.md`.

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

## Algoritmo PoW (testnet pública)

- El identificador activo se mantiene como `kHeavyHash`.
- Para esta testnet, su definición exacta está congelada en `docs/POW_SPEC_FINAL.md`.

## Expectativas operativas y baseline

Para baseline de rendimiento PoW, metodología repetible y guía para operadores, ver `docs/POW_OPERATOR_BASELINES.md`.
