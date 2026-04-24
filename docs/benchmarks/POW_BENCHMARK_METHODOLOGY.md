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

## Capture hygiene

- Record CPU and virtualization details (`lscpu` excerpt).
- Keep benchmark input fixture constant between runs.
- Re-run after changes to compiler version, CPU quota, thread pinning, or host class.
- Treat these values as baselines for planning, not guarantees of real network solve cadence.
