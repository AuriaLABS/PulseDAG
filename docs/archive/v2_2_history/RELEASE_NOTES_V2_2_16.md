# PulseDAG v2.2.16 release notes

PulseDAG v2.2.16 opens as the miner/node contract hardening milestone after v2.2.15 sustained P2P rehearsal evidence passed.

## Opening status

v2.2.16 starts from the v2.2.15 closeout line where sustained P2P evidence passed on Ubuntu/WSL and the project moved to miner/node contract hardening.

v2.2.16 is not a v2.3.0 readiness claim. It is one of the remaining hardening gates before the private-testnet readiness decision.

## Scope

v2.2.16 focuses on:

- external miner/node contract hardening for the standalone `pulsedag-miner`.
- canonical mining template fields and deterministic PoW preimage documentation/tests.
- mining template freshness, expiry, and stale-template rejection behavior.
- target/difficulty semantics and 256-bit hash-vs-target comparison evidence.
- mining submit validation with a stable rejection taxonomy for external miners.
- miner telemetry, worker metrics, and node-side mining diagnostics.
- external miner restart/reconnect evidence and multi-miner rehearsal evidence.
- CPU miner correctness as the default reference backend.
- optional experimental GPU mining backend work only after the canonical PoW adapter exists, with every GPU-found nonce/result CPU-verified before submit.
- optional miner performance evidence collection for CPU hashes/sec, GPU hashes/sec when available, accepted/rejected submits, stale skips, and average template age at submit.

## Guardrails

- No smart contracts are added.
- No contract runtime is enabled.
- No pool coordination logic is added inside `pulsedag-miner`.
- The miner remains a standalone external application.
- Miner logic must not be moved into `pulsedagd`.
- Consensus-rule changes remain out of scope; v2.2.16 must not change consensus rules or PoW semantics.
- GPU mining, if present, must live only in the external miner, be optional, be feature-gated, depend on the canonical PoW adapter, CPU-verify every GPU-found nonce/result before submit, and never be required for default builds.
- v2.2.16 is not a v2.3.0 readiness claim and does not claim public-testnet readiness.
- kHeavyHash/PoW alignment must not be presented as full Kaspa or GHOSTDAG compatibility.

## Mining submit rejection taxonomy

`POST /mining/submit` responses are machine-readable for miner clients. The response payload keeps `ok=true` for validation outcomes and uses `data.accepted` plus stable `data.reason_code` values so miners can branch without parsing human text. v2.2.16 stabilizes these submit classes:

| `reason_code` | Meaning | Miner action |
| --- | --- | --- |
| `accepted` | Block was accepted. | Continue with fresh work. |
| `stale_template` | Template, parents, selected tip, mempool view, or freshness window no longer matches node state. | Refresh template and retry. |
| `invalid_pow` | Header does not satisfy current PoW/target validation. | Discard nonce/header and verify target comparison. |
| `malformed_block` | Block failed structural validation. | Rebuild from a fresh template. |
| `invalid_height` | Submitted height does not match the template/node expectation. | Refresh template. |
| `invalid_parent` | Submitted parent set is invalid when block acceptance runs. | Refresh template. |
| `duplicate_block` | Block hash already exists in the node DAG. | Stop resubmitting that block and fetch fresh work. |
| `invalid_coinbase` | Reserved stable class for coinbase-specific submit failures when surfaced separately. | Check miner address/coinbase construction. |
| `invalid_transaction` | Template transaction set or block transactions are invalid. | Refresh template. |
| `chain_id_mismatch` | Reserved for chain/network mismatch when submit carries chain identity. | Check miner node/network configuration. |
| `internal_error` | Storage or unexpected node-side failure while processing a submit. | Check node logs and retry after recovery. |

Legacy template-specific classes such as `missing_template_id` and `unknown_template` remain machine-readable and are treated as refresh-template outcomes. Detailed stale-template subreasons remain in `pow_rejection_reason` as `reason_code=<subreason>` text for operators, while the top-level stable class remains `stale_template`.

## Miner performance evidence harness

The optional v2.2.16 miner benchmark harness is documented in `docs/MINER_BENCHMARK_V2_2_16.md` and implemented as `scripts/v2_2_16_miner_benchmark.sh`. It writes JSON, CSV, logs, and a Markdown summary under `artifacts/v2.2.16/miner-benchmark/<UTC timestamp>/`. The harness is bounded by default for local and VPS runs, keeps CPU as the default backend, does not require GPU, and does not add pool/share logic or protocol changes.

Collected fields include CPU backend hashes/sec, GPU backend hashes/sec when compiled and available, accepted submits, rejected submits, stale skips, and average template age at submit. GPU evidence may be recorded as skipped when the optional feature, driver/runtime, or canonical implementation is unavailable.

## Required validation before closeout

Before closing v2.2.16, collect output for:

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace --release
bash -n scripts/v2_2_16_miner_benchmark.sh
cargo build -p pulsedag-miner --release
bash scripts/v2-2-16-release-evidence.sh
```

The release evidence bundle should write section logs and `evidence/v2.2.16/summary.md`.

## Optional GPU validation

GPU mining is optional and experimental in v2.2.16, and it is only eligible after the canonical PoW adapter exists. When implemented and available on the host, collect smoke evidence with the GPU feature enabled and CPU-verify every GPU-found nonce before submit. When unavailable, the release evidence should record `SKIP` or `NOT_REQUESTED`, not fail the mandatory CPU miner/node contract gate.

Example optional GPU commands:

```bash
cargo build -p pulsedag-miner --release --features gpu
./target/release/pulsedag-miner --node http://127.0.0.1:18080 --miner-address <addr> --backend gpu --loop
bash scripts/v2-2-16-gpu-miner-smoke.sh
```

Operator-facing GPU notes live in `apps/pulsedag-miner/GPU.md`. The GPU path remains standalone and external: no pool logic, no shares, and no payouts are introduced.

## Known limitations at opening

- v2.2.16 starts as a contract-hardening milestone; release evidence is not yet collected.
- GPU mining is allowed only as experimental external-miner work and should not block closeout when no GPU exists. CPU mining remains the default backend.
- v2.3.0 remains a later private-testnet readiness decision.

## Operator documents

- Roadmap: `docs/ROADMAP_V2_2_16.md`.
- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_16.md`.
- Optional GPU miner guide: `apps/pulsedag-miner/GPU.md`.
- Miner/node contract: `docs/MINER_NODE_CONTRACT_V2_2_16.md`.
- Miner benchmark harness: `docs/MINER_BENCHMARK_V2_2_16.md`.
- GPU backlog: `docs/MINER_GPU_BACKLOG_V2_2_16.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
