# v2.2.20 5N/1M Baseline Evidence

Date: 2026-06-06

## Decision

`v2.2.20` Docker/GitHub Actions `5N/1M baseline` evidence is accepted as **PASS** for this stage.

This evidence does **not** declare public-testnet readiness, does **not** start the 30-day burn-in clock, does **not** enable smart contracts, and does **not** change miner architecture. The miner remains an external application.

## Evidence source

Uploaded artifact:

- `v2_2_20_5n_1m_baseline_evidence (3).zip`

Archive verification:

- inner archive: `evidence.tar.gz`
- sha256: `4de50edfba42e11bd75abac0ae242baf1d9239fbfe6aa6104d5c325fa2f18c6e`

This supersedes the earlier successful `5N/1M` evidence at commit `6633962c07bb1ccfc8c9e15b8763faf0402f45a6` and confirms that the baseline still passes on the later v2.2.20 hardening commit below.

## Runtime metadata

- stage: `5N/1M baseline`
- git ref: `main`
- commit: `85c3b521cb79da7bddb9f0cc52dbbac52f3730ba`
- short commit: `85c3b521cb79`
- version: `pulsedagd 2.2.20`
- `VERSION`: `v2.2.20`
- Cargo workspace version: `2.2.20`
- result: `PASS`
- exit code: `0`
- failure classification: `none`
- runtime: `840s`
- global deadline: `2700s`
- quiescence wait: `180s`

## Topology

- nodes launched: `5`
- miners launched: `1`
- network profile: `private`
- chain id: `pulsedag-private`

## Preflight

Preflight passed:

```text
SUMMARY: PASS (13/13 explicit checks passed)
```

Checks included:

- `VERSION == v2.2.20`
- Cargo workspace version `2.2.20`
- required v2.2.20 docs exist
- required v2.2.20 rehearsal scripts exist
- no v2.3.0 readiness claim
- no v3.0 readiness claim
- no public testnet live/ready claim
- no production GPU mining claim unless explicitly implemented and tested

## Final node state

All nodes ended healthy, ready, connected, and converged.

| Node | Height | Peer count | Orphans | Pending missing parents | Tip |
|---|---:|---:|---:|---:|---|
| n1 | 400 | 4 | 0 | 0 | `099e3265fa9d44849ae7a613c95eac90c98869cd062b6e1c49f90299fca8a163` |
| n2 | 400 | 4 | 0 | 0 | `099e3265fa9d44849ae7a613c95eac90c98869cd062b6e1c49f90299fca8a163` |
| n3 | 400 | 4 | 0 | 0 | `099e3265fa9d44849ae7a613c95eac90c98869cd062b6e1c49f90299fca8a163` |
| n4 | 400 | 4 | 0 | 0 | `099e3265fa9d44849ae7a613c95eac90c98869cd062b6e1c49f90299fca8a163` |
| n5 | 400 | 4 | 0 | 0 | `099e3265fa9d44849ae7a613c95eac90c98869cd062b6e1c49f90299fca8a163` |

Final convergence:

- distinct final tips after quiescence: `1`
- worst lag before quiescence from max height: `1`
- worst lag after quiescence from max height: `0`
- lag improved during quiescence: `true`
- total orphan count after quiescence: `0`
- total pending missing parents after quiescence: `0`
- convergence before quiescence: `FAIL`
- convergence after quiescence: `PASS`

## Miner summary

- miner count: `1`
- miner templates: `1`
- miner submits: `1`
- miner accepted: `1`
- miner rejected: `1`

## Block counters

- accepted blocks: `400`
- rejected blocks: `2001`

## Recovery / diagnostics

The run captured the v2.2.20 diagnostic fields and confirms healthy peer lifecycle semantics under the baseline load.

P2P status:

| Node | Peer count | Inbound | Outbound |
|---|---:|---:|---:|
| n1 | 4 | 4 | 0 |
| n2 | 4 | 0 | 1 |
| n3 | 4 | 0 | 1 |
| n4 | 4 | 0 | 1 |
| n5 | 4 | 0 | 1 |

P2P diagnostics:

- disconnect reason counts: `{}` on all nodes
- last error by peer: `{}` on all nodes
- peer lifecycle showed connected peers across the topology

Sync recovery counters after quiescence:

| Node | orphan_count | pending_missing_parents | missing_parent_entries | inv_hashes_requested | peer_recovery_success_count |
|---|---:|---:|---:|---:|---:|
| n1 | 0 | 0 | 0 | 0 | 4 |
| n2 | 0 | 0 | 0 | 0 | 1 |
| n3 | 0 | 0 | 0 | 0 | 1 |
| n4 | 0 | 0 | 0 | 0 | 1 |
| n5 | 0 | 0 | 0 | 0 | 1 |

## Gate status

| Gate | Status |
|---|---|
| 5N/1M baseline | PASS |
| 5N/2M intermediate | NOT_RUN in this artifact |
| 5N/4M stress | NOT_RUN in this artifact |

## Guardrails preserved

- No consensus-rule change is accepted by this evidence record.
- No PoW semantic change is accepted by this evidence record.
- No smart-contract runtime is enabled.
- No pool logic is added.
- Miner remains external.
- `public_testnet_ready` remains false.
- No v2.3.0 or v3.0 readiness claim is made.

## Next steps

1. Keep this `5N/1M` result as the baseline regression guard for v2.2.20.
2. Fix/close strict warning cleanup for `apps/pulsedagd/src/block_request.rs` so all CI paths can pass with `-D warnings`.
3. Recover `v2.2.20 5N/2M intermediate` to PASS.
4. Re-run `v2.2.20 5N/4M stress` after the 5N/2M recovery fix.
