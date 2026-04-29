# PoW benchmark methodology

Date: **2026-04-24**

This methodology is intended to make PoW performance captures repeatable across development and operator environments while preserving current consensus behavior and RPC shape.

## Guardrails

- No consensus-rule changes are introduced for benchmarking.
- No public RPC format changes are introduced for benchmarking.
- Miner stays external (`apps/pulsedag-miner`).
- No pool logic is introduced.

## What gets measured

1. **Core hash path micro-costs** (`crates/pulsedag-core/benches/pow_core.rs`)
   - `pow_preimage_bytes`
   - `pow_hash_score_u64`
   - `pow_accepts` at representative difficulties
2. **External miner equivalent sweep behavior** (`crates/pulsedag-core/examples/pow_thread_baseline.rs`)
   - Real threaded hash sweep over fixed total hashes

## Commands

From repository root:

```bash
scripts/pow-bench.sh
```

Manual equivalent:

```bash
cargo bench -p pulsedag-core --bench pow_core -- --sample-size 20 --warm-up-time 1 --measurement-time 2
cargo run -p pulsedag-core --release --example pow_thread_baseline
```

## Practical operator run guidance

Use benchmark output to choose an initial miner runtime profile, then validate with external-miner smoke:

1. Run `scripts/pow-bench.sh` on target hardware.
2. Pick a conservative starting thread count (`--threads`) based on stable throughput and host contention.
3. Run standalone operator smoke:

```bash
scripts/release/standalone_operator_smoke.sh --miner-address YOUR_ADDRESS
```

4. Move to sustained loop mode only after smoke success:

```bash
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address YOUR_ADDRESS --threads 4 --max-tries 50000 --loop --sleep-ms 1500
```

This keeps miner operation practical and repeatable while preserving current architecture boundaries (external miner, pool-free flow).

## Capture hygiene

- Record CPU and virtualization details (`lscpu` excerpt).
- Keep benchmark input fixture constant between runs.
- Re-run after changes to compiler version, CPU quota, thread pinning, or host class.
- Treat these values as baselines for planning, not guarantees of real network solve cadence.

## Multithread scheduling note

The standalone miner uses deterministic strided worker partitioning (`nonce = tid + k * threads`) so thread work is partitioned with minimal overlap and benchmark sweeps are easier to reproduce run-to-run.
