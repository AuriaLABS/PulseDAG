# PoW benchmark raw output (2026-04-24)

Captured via:

```bash
scripts/pow-bench.sh
```

## Criterion output

```text
pow_core/pow_preimage_bytes
                        time:   [84.807 ns 85.539 ns 86.458 ns]
                        thrpt:  [3.3070 GiB/s 3.3425 GiB/s 3.3714 GiB/s]

pow_core/pow_hash_score_u64
                        time:   [709.65 ns 713.08 ns 716.47 ns]
                        thrpt:  [1.3957 Melem/s 1.4024 Melem/s 1.4092 Melem/s]

pow_core/pow_accepts/1  time:   [695.37 ns 701.52 ns 708.59 ns]
                        thrpt:  [1.4113 Melem/s 1.4255 Melem/s 1.4381 Melem/s]

pow_core/pow_accepts/64 time:   [705.21 ns 706.91 ns 709.03 ns]
                        thrpt:  [1.4104 Melem/s 1.4146 Melem/s 1.4180 Melem/s]

pow_core/pow_accepts/512
                        time:   [716.17 ns 717.65 ns 719.56 ns]
                        thrpt:  [1.3897 Melem/s 1.3934 Melem/s 1.3963 Melem/s]
```

## Thread baseline output

```text
pow-thread-baseline total_hashes=2000000 difficulty=u32::MAX
threads=1 hps=1266426 checksum=3493883252879308943
threads=2 hps=2441952 checksum=3493883252879308943
threads=4 hps=2466457 checksum=3493883252879308943
threads=8 hps=2501146 checksum=3493883252879308943
```

## Environment snapshot

```text
CPU(s):                                  3
Model name:                              Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz
Thread(s) per core:                      1
Hypervisor vendor:                       KVM
```
