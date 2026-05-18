# pulsedag-miner

Minero externo oficial de PulseDAG.

Especificación PoW canónica congelada: `docs/POW_SPEC_FINAL.md`.
Guía operativa/auditoría del flujo actual: `docs/POW_CURRENT_PATH.md`.

## Alcance cerrado

Este binario **no** contiene lógica de pool.

Solo hace tres cosas:
1. pedir un template al nodo
2. resolver PoW fuera del nodo
3. enviar el bloque resuelto al nodo

## Empaquetado de release

En los releases oficiales, este binario se publica como artefacto **standalone** separado de `pulsedagd`, con nombre `pulsedag-miner-<tag>-<target>.*`, checksum `.sha256` y manifiesto `.json` por artefacto.

## Uso

```bash
cargo run -p pulsedag-miner -- --miner-address TU_DIRECCION
```

Modo bucle:

```bash
cargo run -p pulsedag-miner -- --miner-address TU_DIRECCION --loop --sleep-ms 1500 \
  --refresh-before-expiry-ms 1000
```

Con nodo explícito:

```bash
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address TU_DIRECCION --loop
```

Con multi-thread explícito:

```bash
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address TU_DIRECCION --threads 4 --max-tries 500000 --loop --sleep-ms 1000 --refresh-before-expiry-ms 1000
```

## Uso como binario standalone de release

Después de descargar el artefacto oficial del release (`pulsedag-miner-<tag>-<target>.*`):

```bash
tar -xzf pulsedag-miner-v2.2.5-x86_64-unknown-linux-gnu.tar.gz
./pulsedag-miner-v2.2.5-x86_64-unknown-linux-gnu/pulsedag-miner --help
./pulsedag-miner-v2.2.5-x86_64-unknown-linux-gnu/pulsedag-miner \
  --node http://127.0.0.1:8080 \
  --miner-address TU_DIRECCION \
  --threads 4 \
  --max-tries 50000 \
  --loop \
  --sleep-ms 1500 \
  --refresh-before-expiry-ms 1000
```

Notas de operador:
- El binario se puede ejecutar de forma independiente del árbol de código (`cargo` no es requerido para operación en release).
- El flujo oficial sigue siendo template -> PoW -> submit contra el nodo.
- No existe soporte de pool en este binario.
- El backend CPU es el predeterminado; el backend GPU opcional se documenta en [`GPU.md`](GPU.md).

## Minería GPU opcional

La minería GPU es **opcional**. La minería CPU sigue siendo el backend predeterminado y la referencia obligatoria para operación normal y evidencia de v2.2.16.

El backend GPU es experimental hasta completar la evidencia de cierre de v2.2.16, está protegido por feature flag y solo vive en el minero externo standalone. No agrega lógica de pool, shares ni payouts.

Documentación completa: [`GPU.md`](GPU.md).

Build experimental con GPU:

```bash
cargo build -p pulsedag-miner --release --features gpu
```

Ejemplo de ejecución con GPU:

```bash
./target/release/pulsedag-miner --node http://127.0.0.1:18080 --miner-address <addr> --backend gpu --loop
```

Regla de seguridad: cada nonce encontrado por GPU debe verificarse por la ruta PoW canónica CPU/core antes de enviarse al nodo.

## Flags soportadas

- `--node`
- `--miner-address`
- `--max-tries`
- `--loop`
- `--sleep-ms`
- `--threads`
- `--refresh-before-expiry-ms`
- `--backend` (`cpu` por defecto; `gpu` requiere `--features gpu`)
- `--gpu-device` (solo para backend GPU)
- `--heartbeat` / `--no-heartbeat`
- `--worker-id`

## Fuera de alcance

- pool
- shares
- payouts
- accounting
- lógica de servidor

## Algoritmo PoW

- El identificador de algoritmo permanece como `kHeavyHash`.
- La codificación exacta del preimage, endianness, target y regla de aceptación está congelada en `docs/POW_SPEC_FINAL.md`.

## Benchmark y baseline de operador

Para ejecutar benchmarks repetibles y revisar baseline CPU/hilos, ver `docs/POW_OPERATOR_BASELINES.md` y `scripts/pow-bench.sh`.

## Smoke flow de operador (node + miner standalone)

Para un smoke reproducible de empaquetado + flujo externo:

```bash
scripts/release/standalone_operator_smoke.sh --miner-address TU_DIRECCION
```

Este helper valida artefactos standalone y corre un smoke corto de nodo + minero externo, sin introducir lógica de pool.

## Coordinación multithread (determinística)

La búsqueda de nonce usa una programación *strided* por worker:

- worker `t` explora `nonce = t, t + T, t + 2T, ...` (siendo `T = --threads` efectivo)
- esto reduce solapamiento obvio de búsqueda entre hilos
- conserva comportamiento repetible para smoke/benchmark cuando no hay solución (mismo fallback al último nonce intentado)

El flujo operativo no cambia: **template -> mine -> submit**.
