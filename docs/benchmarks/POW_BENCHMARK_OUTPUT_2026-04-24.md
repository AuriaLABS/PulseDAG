# PoW benchmark raw output (2026-04-24)

Captured via:

```bash
scripts/pow-bench.sh
```

## Criterion output

```text
pow_core/pow_preimage_bytes
                        time:   [109.36 ns 111.56 ns 114.84 ns]
                        thrpt:  [2.4896 GiB/s 2.5628 GiB/s 2.6146 GiB/s]

pow_core/pow_hash_score_u64
                        time:   [496.19 ns 498.98 ns 501.40 ns]
                        thrpt:  [1.9944 Melem/s 2.0041 Melem/s 2.0154 Melem/s]

pow_core/pow_accepts/1  time:   [473.58 ns 474.76 ns 475.99 ns]
                        thrpt:  [2.1009 Melem/s 2.1063 Melem/s 2.1116 Melem/s]

pow_core/pow_accepts/64 time:   [496.55 ns 499.21 ns 502.09 ns]
                        thrpt:  [1.9917 Melem/s 2.0032 Melem/s 2.0139 Melem/s]

pow_core/pow_accepts/512
                        time:   [513.62 ns 533.81 ns 550.32 ns]
                        thrpt:  [1.8171 Melem/s 1.8733 Melem/s 1.9470 Melem/s]
```

## Thread baseline output

```text
pow-thread-baseline total_hashes=2000000 difficulty=u32::MAX
threads=1 hps=1605357 checksum=3493883252879308943
threads=2 hps=3051085 checksum=3493883252879308943
threads=4 hps=2928877 checksum=3493883252879308943
threads=8 hps=3402374 checksum=3493883252879308943
```

## Environment snapshot

```text
CPU(s):                                  3
Model name:                              Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz
Thread(s) per core:                      1
Hypervisor vendor:                       KVM
```
