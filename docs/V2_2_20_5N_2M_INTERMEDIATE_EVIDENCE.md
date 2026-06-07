# v2.2.20 5N/2M Intermediate Evidence

Date: 2026-06-07

## Decision

`v2.2.20` `5N/2M intermediate` evidence was rerun after PR 1 (`p2p/sync: reprocess stale orphan backlog after rate-limited parent recovery`) landed at merge commit `b6950201cd24ed8067c0b5dd228486047a1c27e0`.

The run is recorded as **FAIL** because the accepted-block gate failed: both miners received templates and submitted work, but no blocks were accepted. However, the previous `85c3b521cb79` failure signature is no longer present in this artifact: peers remain visible, final tips converge to one tip, and orphan/pending-missing-parent backlogs are zero after quiescence.

This evidence does **not** declare public-testnet readiness, does **not** start burn-in, does **not** change consensus or PoW semantics, and does **not** enable smart contracts or pool logic.

## Evidence source

Local artifact:

- run directory: `artifacts/v2_2_20/private_5n_2m_rehearsal/20260607T074320Z/`
- packaged archive: `artifacts/v2_2_20/private_5n_2m_rehearsal/20260607T074320Z/evidence.tar.gz`
- archive sha256: `98ac709013b051f85ba400c050b971f606e6feda96c249480754e9928a541d5d`

Archive verification completed successfully:

```text
/workspace/PulseDAG/artifacts/v2_2_20/private_5n_2m_rehearsal/evidence.tar.gz: OK
/workspace/PulseDAG/artifacts/v2_2_20/private_5n_2m_rehearsal/20260607T074320Z/evidence.tar.gz: OK
```

## Runtime metadata

| Field | Value |
|---|---|
| Stage | `5N/2M intermediate` |
| Commit tested | `b6950201cd24ed8067c0b5dd228486047a1c27e0` |
| Short commit | `b6950201cd24` |
| Version | `pulsedagd 2.2.20` |
| Network profile | `private` |
| Chain id | `pulsedag-private` |
| Result | `FAIL` |
| Exit code | `1` |
| Failure classification | `MINER_NO_ACCEPTED_BLOCKS`, `STAGED_GATE_5N_1M`, `STAGED_GATE_5N_2M` |
| Runtime | `886s` |
| Global deadline | `2700s` |
| Quiescence wait | `180s` |
| Miners stopped for quiescence | `1` |

Failure reasons recorded by the harness:

- `STAGED_GATE_5N_1M`: 5N/1M baseline gate failed after quiescence.
- `STAGED_GATE_5N_2M`: 5N/2M intermediate gate failed after quiescence.
- `MINER_NO_ACCEPTED_BLOCKS`: accepted blocks is zero.

## Topology

| Field | Value |
|---|---:|
| Node count | 5 |
| Miner count | 2 |

All five nodes were healthy and ready at final capture.

## Final node state after quiescence

| Node | Final height | Peer count | Orphans | Pending missing parents | Missing-parent entries | Inv hashes requested | Peer recovery success count | Final tip |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| n1 | 0 | 4 | 0 | 0 | 0 | 0 | 4 | `0828edfca48b1a43e407c4d9d7f63650552d9b86587f2565eba0c05977484047` |
| n2 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | `0828edfca48b1a43e407c4d9d7f63650552d9b86587f2565eba0c05977484047` |
| n3 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | `0828edfca48b1a43e407c4d9d7f63650552d9b86587f2565eba0c05977484047` |
| n4 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | `0828edfca48b1a43e407c4d9d7f63650552d9b86587f2565eba0c05977484047` |
| n5 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | `0828edfca48b1a43e407c4d9d7f63650552d9b86587f2565eba0c05977484047` |

Final convergence:

- convergence before quiescence: `PASS`
- convergence after quiescence: `PASS`
- worst lag before quiescence from max height: `0`
- worst lag after quiescence from max height: `0`
- distinct final tips after quiescence: `1`
- total orphan count after quiescence: `0`
- total pending missing parents after quiescence: `0`
- lag improved during quiescence: `false`

## Miner summary

| Miner | Templates | Submits | Accepted | Rejected |
|---|---:|---:|---:|---:|
| miner-1 | 1 | 1 | 0 | 1 |
| miner-2 | 1 | 1 | 0 | 1 |
| Total | 2 | 2 | 0 | 2 |

Block counters:

- accepted blocks: `0`
- rejected blocks: `2`

## P2P and sync recovery observations

P2P peer visibility did not collapse to zero. Final peer counts were `4, 1, 2, 1, 1` across `n1` through `n5`.

Disconnect diagnostics remained empty because the final peer lifecycle state was healthy/connected:

| Node | disconnect_reason_counts | last_error_by_peer |
|---|---|---|
| n1 | `{}` | `{}` |
| n2 | `{}` | `{}` |
| n3 | `{}` | `{}` |
| n4 | `{}` | `{}` |
| n5 | `{}` | `{}` |

Final sync states:

| Node | sync_state | catchup_stage |
|---|---|---|
| n1 | `synced` | `steady` |
| n2 | `requesting_tips` | `steady` |
| n3 | `synced` | `steady` |
| n4 | `requesting_tips` | `steady` |
| n5 | `requesting_tips` | `steady` |

## Supersession of previous `85c3b521cb79` failure

This artifact **partially supersedes** the previous `85c3b521cb79` `5N/2M` failure.

Superseded failure symptoms:

| Symptom from `85c3b521cb79` | New `b6950201cd24` result |
|---|---|
| Peer-count ambiguity / peer visibility degradation | Final peer counts are non-zero on all nodes. |
| `2` distinct final tips after quiescence | `1` distinct final tip after quiescence. |
| Orphan backlog of `66` per node | `0` orphans on every node. |
| Pending missing parents of `66` per node | `0` pending missing parents on every node. |
| Ambiguous stuck backlog after quiescence | Backlog is clear after quiescence. |

Not superseded as a full gate recovery:

- The run is not a `PASS` because accepted blocks remained `0`.
- Heights stayed at genesis height `0`, so this artifact proves peer/orphan/final-tip recovery behavior under the captured run conditions but does not prove successful mined-chain advancement.
- The next evidence run must restore `accepted blocks > 0` while preserving the non-zero peers, one final tip, and zero orphan/pending-missing-parent backlog shown here.

## Guardrails preserved

- No consensus-rule change is accepted by this evidence record.
- No PoW semantic change is accepted by this evidence record.
- No smart-contract runtime is enabled.
- No pool logic is added.
- Miner remains external.
- `public_testnet_ready` remains false.
- No v2.3.0 or v3.0 readiness claim is made.
