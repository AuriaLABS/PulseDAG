# pulsedag-miner

Minero externo oficial de PulseDAG.

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

## Flags soportadas

- `--node`
- `--miner-address`
- `--max-tries`
- `--loop`
- `--sleep-ms`

## Fuera de alcance

- pool
- shares
- payouts
- accounting
- coordinación de workers
- lógica de servidor
