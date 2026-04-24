# PoW performance baselines and operator guidance

Date captured: **2026-04-24**

This document records measured PoW performance using the current final implementation. It is intended to set realistic operator/miner expectations before public testnet launch.

## Scope and constraints

- Miner remains **external** (`apps/pulsedag-miner`), not embedded into node runtime.
- No pool logic is introduced.
- Performance guidance is grounded in measured benchmark output (not theoretical-only estimates).

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

- CPU model: Intel(R) Xeon(R) Platinum 8171M CPU @ 2.60GHz
- Visible CPUs: 3
- Threads per core: 1
- OS/containerized CI-style Linux environment

## Captured outputs

### Core microbenchmarks (criterion)

`cargo bench -p pulsedag-core --bench pow_core -- --sample-size 20 --warm-up-time 1 --measurement-time 2`

- `pow_preimage_bytes`: **[169.98 ns, 178.15 ns, 184.56 ns]** (~1.55–1.68 GiB/s)
- `pow_hash_score_u64`: **[571.51 ns, 581.82 ns, 599.23 ns]** (~1.67–1.75 Mhash/s)
- `pow_accepts/1`: **[555.86 ns, 582.05 ns, 616.17 ns]**
- `pow_accepts/64`: **[552.96 ns, 587.83 ns, 649.00 ns]**
- `pow_accepts/512`: **[555.49 ns, 566.46 ns, 581.00 ns]**

### Thread-scaling baseline

`cargo run -p pulsedag-core --release --example pow_thread_baseline`

- `threads=1 hps=1319063`
- `threads=2 hps=2506337`
- `threads=4 hps=2509104`
- `threads=8 hps=2542339`

## Operator guidance (testnet planning)

1. **Expect sub-linear thread scaling.**
   - 8 logical worker threads achieved ~1.93x throughput over 1 thread in this environment.
2. **Use measured local baseline before setting expectations.**
   - Start with 1, 2, 4, and 8 threads and retain the best sustained H/s value.
3. **Do not assume this environment equals bare-metal.**
   - VM/container scheduling and CPU quotas can reduce throughput materially.
4. **Difficulty and block timing are network-level controls.**
   - Higher per-operator H/s raises solve probability but does not guarantee short solve intervals.
5. **No pool assumptions in public docs or operations runbooks.**
   - This stack currently documents only solo/external miner operation.

## Practical pre-launch checklist for operators

- Run `scripts/pow-bench.sh` on target hardware.
- Record 1/2/4/8-thread H/s values and choose the best sustained setting.
- Keep miner external and point it to node `POST /mining/template` + `POST /mining/submit` flow.
- Re-run the baseline after hardware, kernel, container, or compiler upgrades.
