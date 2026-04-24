# pulsedag-miner

Minero externo oficial de PulseDAG.

Especificación PoW canónica congelada: `docs/POW_SPEC_FINAL.md`.

## Alcance cerrado

Este binario **no** contiene lógica de pool.

Solo hace tres cosas:
1. pedir un template al nodo
2. resolver PoW fuera del nodo
3. enviar el bloque resuelto al nodo

## Uso

```bash
cargo run -p pulsedag-miner -- --miner-address TU_DIRECCION
```

Modo bucle:

```bash
cargo run -p pulsedag-miner -- --miner-address TU_DIRECCION --loop --sleep-ms 1500
```

Con nodo explícito:

```bash
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address TU_DIRECCION --loop
```

Con multi-thread explícito:

```bash
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address TU_DIRECCION --threads 4 --max-tries 500000 --loop --sleep-ms 1000
```

## Flags soportadas

- `--node`
- `--miner-address`
- `--max-tries`
- `--loop`
- `--sleep-ms`
- `--threads`

## Fuera de alcance

- pool
- shares
- payouts
- accounting
- coordinación de workers
- lógica de servidor

## Algoritmo PoW

- El identificador de algoritmo permanece como `kHeavyHash`.
- La codificación exacta del preimage, endianness, target y regla de aceptación está congelada en `docs/POW_SPEC_FINAL.md`.
