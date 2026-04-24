# PoW performance baselines and operator guidance

Date captured: **2026-04-24**

This document records measured PoW performance using the current final implementation. It is intended to set realistic operator/miner expectations before public testnet launch.

## Scope and constraints

- Miner remains **external** (`apps/pulsedag-miner`), not embedded into node runtime.
- No pool logic is introduced.
- Performance guidance is grounded in measured benchmark output (not theoretical-only estimates).
- Optimization work should only proceed after measuring these baselines on the target deployment class.

## Repeatable benchmark commands

From repo root:

```bash
scripts/pow-bench.sh
```

Manual equivalent:

```bash
cargo bench -p pulsedag-core --bench pow_core -- --sample-size 20 --warm-up-time 1 --measurement-time 2
cargo run -p pulsedag-core --release --example pow_thread_baseline
```

## Benchmark environment

- CPU model: Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz
- Visible CPUs: 3
- Threads per core: 1
- Hypervisor: KVM
- OS/containerized CI-style Linux environment

## Captured outputs (from `scripts/pow-bench.sh`)

Raw benchmark transcript: `docs/benchmarks/POW_BENCHMARK_OUTPUT_2026-04-24.md`.

### Core microbenchmarks (criterion)

`cargo bench -p pulsedag-core --bench pow_core -- --sample-size 20 --warm-up-time 1 --measurement-time 2`

- `pow_preimage_bytes`: **[109.36 ns, 111.56 ns, 114.84 ns]** (~2.49–2.61 GiB/s)
- `pow_hash_score_u64`: **[496.19 ns, 498.98 ns, 501.40 ns]** (~1.99–2.02 Mhash/s)
- `pow_accepts/1`: **[473.58 ns, 474.76 ns, 475.99 ns]**
- `pow_accepts/64`: **[496.55 ns, 499.21 ns, 502.09 ns]**
- `pow_accepts/512`: **[513.62 ns, 533.81 ns, 550.32 ns]**

### Thread-scaling baseline

`cargo run -p pulsedag-core --release --example pow_thread_baseline`

- `threads=1 hps=1605357`
- `threads=2 hps=3051085`
- `threads=4 hps=2928877`
- `threads=8 hps=3402374`

## Operator guidance (testnet planning)

1. **Expect non-linear thread scaling.**
   - In this measured environment, 8 worker threads delivered ~2.12x throughput over 1 thread.
2. **Use measured local baseline before setting expectations.**
   - Start with 1, 2, 4, and 8 threads and retain the best sustained H/s value.
3. **Do not assume this environment equals bare-metal.**
   - VM/container scheduling and CPU quotas can materially shift throughput and scaling shape.
4. **Difficulty and block timing are network-level controls.**
   - Higher per-operator H/s raises solve probability but does not guarantee short solve intervals.
5. **No pool assumptions in public docs or operations runbooks.**
   - This stack currently documents only solo/external miner operation.

## Practical pre-launch checklist for operators

- Run `scripts/pow-bench.sh` on target hardware.
- Record 1/2/4/8-thread H/s values and choose the best sustained setting.
- Keep miner external and point it to node `POST /mining/template` + `POST /mining/submit` flow.
- Re-run the baseline after hardware, kernel, container, or compiler upgrades.
