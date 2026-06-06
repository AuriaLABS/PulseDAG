# v2.2.20 First 5N/4M Stress Evidence Record

Date: 2026-06-06

## Scope

This record documents the first `v2.2.20` comparison point after the technical PRs that added bounded stress diagnostics for the private `5N/4M` rehearsal path.

This is **not** a public-testnet readiness declaration, does **not** start burn-in, and does **not** change the `v2.2.20` hardening scope.

## v2.2.19 comparison baseline

The previous `v2.2.19` private `5N/4M` stress evidence closed as `OBSERVE_FAIL` with these stress signals:

| Metric | v2.2.19 `5N/4M` value |
|---|---:|
| Peer visibility | peers collapsed to `0` |
| Orphans | `512` |
| Pending missing parents | `512` |
| Final tips after quiescence | `4` |
| Result | `OBSERVE_FAIL` |

Interpretation: the v2.2.19 stress run did not prove recoverability after quiescence. The concrete blocking symptoms were peer drop-to-zero, orphan saturation, missing-parent backlog saturation, and divergent final tips.

## v2.2.20 first post-PR result

No committed `artifacts/v2_2_20/private_5n_4m_rehearsal/` runtime bundle is present in this tree yet. Therefore the first v2.2.20 stress record is:

| Area | v2.2.20 first record |
|---|---|
| Runtime result | `PENDING_EVIDENCE` |
| Improvement claim | **No improvement can be claimed yet** |
| Concrete cause | Missing committed v2.2.20 `5N/4M` evidence bundle; there is no post-PR measurement for peer count, orphan count, pending missing parents, or final-tip convergence in this tree. |
| Required next evidence | Run `scripts/v2_2_20_private_5n_4m_rehearsal.sh` or the Docker wrapper and attach `evidence-summary.md`, `p2p_convergence.json`, `quiescence-metrics.json`, `evidence.tar.gz`, and `evidence.tar.gz.sha256`. |

This means the documented status is **not improved yet** by evidence. The technical PRs improved the measurement surface, but the stress outcome remains unproven until a v2.2.20 bundle is attached.

## New v2.2.20 metrics to compare against v2.2.19

The v2.2.20 stress harness must record the old v2.2.19 comparison signals plus the new recovery diagnostics below:

| Metric family | Required v2.2.20 fields | Why it matters |
|---|---|---|
| Peer visibility | per-node `peer_count`; network non-zero peer gate | Confirms whether the v2.2.19 peer drop-to-zero failure is fixed or still present. |
| Orphan pressure | per-node `orphan_count`; pre/post total orphan count | Shows whether orphan saturation remains at or near the v2.2.19 `512` ceiling. |
| Missing-parent pressure | per-node `pending_missing_parents`; `missing_parents_entries`; pre/post total missing-parent count | Separates queue backlog from missing-parent entry cardinality and shows whether quiescence drains the backlog. |
| Recovery activity | `inv_hashes_requested`; `peer_recovery_success_count` | Distinguishes an idle failure from an active-but-unsuccessful recovery attempt. |
| Quiescence behavior | pre/post convergence, worst lag, distinct tips, lag-improved flag | Shows whether stopping miners allows recovery even if the stress interval diverges. |
| Miner pressure | templates, submits, accepted blocks, rejected blocks for all four miners | Confirms the run actually exercised the 4-miner stress path rather than failing before load generation. |

## Improvement decision rule

A future v2.2.20 `5N/4M` run may record improvement only if the attached evidence proves at least one of these measured deltas against v2.2.19:

- peer visibility is not all-zero after quiescence;
- orphan count is below `512` on the previously saturated nodes, or drains during quiescence;
- pending missing parents are below `512` on the previously saturated nodes, or drain during quiescence;
- final tips improve from `4` distinct tips toward a bounded smaller number or `1` converged tip;
- the run still fails, but the failure has a deterministic classification backed by the new recovery counters.

If none of those deltas is present, the record must state **no improvement** and cite the concrete cause, such as persistent peer collapse, persistent orphan/missing-parent saturation, unchanged four-tip divergence, RPC starvation during capture, or missing/invalid evidence artifacts.

## Current conclusion

`v2.2.20` has the required metric surface to compare against the v2.2.19 `OBSERVE_FAIL`, but the first committed post-PR record in this tree is `PENDING_EVIDENCE`, not an improved stress result.

Next action: attach a real v2.2.20 `5N/4M` evidence bundle and update this record with the measured peer counts, orphan counts, pending missing-parent counts, final tips, recovery counters, and PASS/FAIL/OBSERVE classification.
