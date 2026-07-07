# Kaspa-inspired GHOSTDAG selection design/spec for PulseDAG

Status: design-only proposal. This document changes no consensus, networking, mining, storage, or RPC behavior.

## Compatibility and scope boundaries

PulseDAG may reuse GHOSTDAG vocabulary and reimplement Kaspa-inspired concepts, but this document does **not** claim full Kaspa compatibility, wire compatibility, state compatibility, or consensus compatibility. PulseDAG must implement, test, and activate its own rules inside PulseDAG with explicit versioning, replay fixtures, migration gates, and operator-facing evidence before any stronger claim is made.

This PR is documentation and architecture only. It must not copy Kaspa code literally and must not enable high block cadence.

## Current PulseDAG limitations

The current implementation should be treated as a local/simple DAG policy until the migration plan below lands:

- Current tip policy is local/simple: APIs derive a preferred tip from the local chain view rather than from a full GHOSTDAG selected chain.
- `blue_score` is currently only an existing block/header field and local tie-breaker input; it is not yet the result of full bounded blue/red classification.
- There is no full GHOSTDAG implementation yet.
- There is no merge-set ordering yet.
- There is no finality/pruning boundary based on selected-chain depth or GHOSTDAG-style acceptance rules yet.
- High block cadence must remain disabled until deterministic selected-parent selection, deterministic DAG ordering, P2P sync, mining-template, transaction-selection, and replay-determinism requirements are implemented and validated.

## Consensus terms and required semantics

### 1. `selected_parent`

`selected_parent` is the single parent of a block chosen by deterministic PulseDAG consensus scoring as that block's main selected-chain predecessor. A block may reference multiple parents, but exactly one eligible parent becomes the selected parent after all required parent metadata is available.

Required semantics:

- The selected parent must be chosen from known valid parents only.
- Selection must be independent of block arrival order, orphan adoption order, peer order, restart timing, and local cache state.
- Selection must use canonical consensus inputs: parent blue work, parent blue score where activated, parent hash, parent header work, and deterministic parent ordering.
- If any parent metadata required for selection is missing, the block may be staged but must not finalize selected-parent metadata.

### 2. `selected_tip`

`selected_tip` is the current best known block at the head of the selected chain under the activated PulseDAG selection rules. It replaces local/simple preferred-tip behavior only after activation.

Required semantics:

- The selected tip is the candidate with maximal activated selection score, using deterministic tie-breaks.
- A node must not mark selected-tip sync complete from incomplete DAG knowledge.
- RPC status may expose both legacy/local preferred-tip data and activated selected-tip data during migration, but names must make the distinction explicit.

### 3. `selected_chain`

`selected_chain` is the chain obtained by repeatedly following `selected_parent` from `selected_tip` back to genesis or the current pruning boundary.

Required semantics:

- It is a deterministic projection of the DAG, not a separate block structure.
- It anchors DAG ordering, finality/pruning checks, locators, mining templates, and replay verification.
- Reorgs are selected-chain changes and must trigger deterministic state rebuild or rollback from a safe boundary.

### 4. `blue_score`

`blue_score` is the monotonic count-like score assigned by activated PulseDAG blue/red classification. It should represent accepted blue ancestry according to PulseDAG's bounded rules, not merely local height.

Required semantics:

- Legacy blocks retain their legacy `blue_score` interpretation until an explicit activation/version boundary redefines or supplements the field.
- Any change to hash/preimage semantics for `blue_score` requires explicit header/version planning and replay tests.
- After activation, `blue_score` must be derived deterministically from selected-parent and merge-set classification.

### 5. `blue_work`

`blue_work` is the cumulative work-weighted score used for selected-parent and selected-tip comparison when work must dominate count-only scoring.

Required semantics:

- It must accumulate only consensus-valid work under activated rules.
- It must be deterministic across replay and storage restore.
- It should be the primary safety input for chain selection when count-only `blue_score` could be manipulated by low-work structure.

### 6. `merge_set`

For a candidate block, the `merge_set` is the set of referenced ancestor blocks that are not on the candidate's selected-parent chain segment and that must be considered when accepting the candidate into the DAG order.

Required semantics:

- Merge-set discovery must be bounded.
- Missing merge-set ancestors keep the candidate staged/incomplete.
- The merge set must be computed from the DAG topology, not from peer announcement order.

### 7. `merge_set_blues`

`merge_set_blues` is the deterministic subset of `merge_set` accepted as blue under PulseDAG's bounded classification rule.

Required semantics:

- Blue merge-set blocks may contribute to activated `blue_score`, `blue_work`, DAG ordering, and transaction selection only as specified by consensus.
- Classification must use deterministic candidate ordering and deterministic tie-breaks.
- Limit overflows must have explicit behavior: reject, defer, or classify as red according to the activated rule.

### 8. `merge_set_reds`

`merge_set_reds` is the deterministic subset of `merge_set` that is known and valid as a block but not accepted into the blue set for ordering/scoring priority.

Required semantics:

- Red blocks are not invalid solely because they are red.
- Red transaction effects must be handled exactly as specified by DAG ordering and transaction-selection rules.
- Red blocks must not provide nondeterministic ordering or scoring influence.

### 9. Blue/red classification

Blue/red classification is the deterministic partitioning of each candidate's merge set into accepted blue blocks and red blocks according to bounded PulseDAG rules.

Required semantics:

- The rule must define the bound parameter(s), candidate sort order, tie-break keys, and overflow behavior.
- Classification must be replay deterministic across process restarts and storage restores.
- Classification must be resistant to adversarial merge sets that attempt to force unbounded CPU, memory, or database reads.

### 10. DAG ordering

DAG ordering is the deterministic total order used to apply accepted block effects and transactions. It is derived from selected-chain traversal plus deterministic merge-set ordering.

Required semantics:

- Consensus state must be applied in DAG order, not arrival order.
- The order must be total, stable, and reproducible from stored blocks and activated metadata.
- Transaction conflicts across parallel blocks must resolve by deterministic ordered position, not by mempool timing or peer timing.
- DAG ordering must include explicit handling for blue and red blocks before activation.

### 11. Pruning/finality boundary

The pruning/finality boundary is the point behind which the node treats selected-chain history and required side-DAG data as stable enough for pruning, snapshotting, or irreversible state compaction.

Required semantics:

- PulseDAG must not prune data needed to recompute selected-parent, blue/red classification, DAG order, or transaction effects for any not-final region.
- Finality depth and pruning windows must be explicit consensus or network parameters, not local heuristics.
- Snapshot restore must prove the restored node can validate from the boundary without hidden local state.

### 12. P2P sync requirements

P2P sync must exchange enough information for nodes to converge on the same selected chain, DAG frontier, and missing-parent set without unsafe assumptions.

Required requirements:

- Capability negotiation for any new selected-chain locator, DAG frontier, or classification metadata messages.
- Selected-chain locators for common-ancestor discovery.
- DAG frontier exchange for parallel tips and missing-parent recovery.
- Orphan and missing-parent recovery that does not finalize selected sync when required parents or merge-set ancestors are absent.
- Compatibility behavior for older peers so rolling upgrades do not cause decode-failure penalties.

### 13. Mining template requirements

Mining templates must build on activated selected-tip/selected-parent data and include only safe parallel parents.

Required requirements:

- Template parent selection uses the selected tip, not local arrival-order tip preference.
- Parallel parents are included only if known, valid, and within merge-set/classification bounds.
- Template diagnostics expose selected parent, included parents, score/work metadata, and any exclusion reason.
- Templates must remain conservative until P2P sync and replay-determinism evidence prove convergence.

### 14. Transaction selection requirements

Transaction selection must be deterministic with respect to the DAG order and must avoid duplicate or conflicting effects across selected-chain and parallel-parent context.

Required requirements:

- A transaction already accepted in selected-chain or merge-set context must not be duplicated in a template.
- Conflicts across parallel blocks must resolve by deterministic DAG order.
- Mempool selection may be policy-driven, but once included in a candidate block, replay validation must resolve outcomes using consensus ordering only.
- Fee ordering, dependency ordering, and package selection rules must be documented before high cadence.

### 15. Replay determinism requirements

Replay determinism means that the same valid block set produces the same selected parent for every block, selected tip, selected chain, blue/red classification, DAG order, UTXO/state effects, and transaction conflict outcomes.

Required requirements:

- Replay must be independent of arrival order, peer order, orphan drain order, thread scheduling, database iteration order, hash-map iteration order, and wall-clock time.
- Test fixtures must feed identical DAGs in multiple permutations and compare selected-tip, selected-chain, order digest, classification digest, and state digest.
- Node restart and snapshot restore must reproduce the same metadata or safely recompute it.

## Required before high block cadence can be enabled

High block cadence remains blocked until all of the following are implemented, activated in a controlled environment, and backed by evidence:

1. Deterministic selected-parent and selected-tip selection.
2. Bounded merge-set discovery and blue/red classification.
3. Deterministic DAG ordering and transaction conflict resolution.
4. State application by DAG order with rollback/rebuild support for selected-chain changes.
5. P2P selected-chain locator and DAG frontier sync with rolling-upgrade compatibility.
6. Mining templates based on selected tip and safe parallel parents.
7. Transaction selection rules that prevent duplicates and deterministic conflicts.
8. Replay, restart, and snapshot-restore determinism tests.
9. Multi-node convergence harness evidence under delayed parents, orphan storms, reordering, and peer churn.
10. Feature flag or explicit dev/testnet-only gate proving high cadence is disabled by default.

## Migration plan

1. **Spec stabilization:** Land this design-only document and require downstream PRs to cite it when changing consensus, P2P, mining, storage, or replay behavior.
2. **Metadata model:** Add versioned selected-parent, selected-chain, blue-score, blue-work, merge-set, and ordering metadata without changing live selection behavior.
3. **Selection implementation:** Implement deterministic selected-parent and selected-tip calculation behind a non-activating compatibility gate.
4. **Classification implementation:** Add bounded merge-set discovery and blue/red classification with fixture tests and limit behavior.
5. **Ordering/state staging:** Move block effect application from arrival order to derived DAG order, with rebuild/rollback from a safe boundary.
6. **Networking:** Add capability-negotiated selected-chain locators and DAG frontier exchange.
7. **Mining and mempool:** Update mining templates and transaction selection to use selected-tip context and deterministic DAG-order conflict handling.
8. **Finality/pruning:** Define and implement a conservative finality boundary and pruning/snapshot restore requirements.
9. **Activation rehearsal:** Run replay, restart, snapshot, and multi-node convergence harnesses before enabling any high-cadence experiment.
10. **Cadence experiment:** Add disabled-by-default high-cadence settings only after deterministic selection/order/sync/mining requirements are complete.

## Proposed PR sequence

1. Documentation/spec PR: this document plus links from any roadmap/index document.
2. Storage and in-memory metadata PR with no behavior change.
3. Deterministic selected-parent/selected-tip calculator PR behind a disabled gate.
4. Bounded merge-set and blue/red classification PR.
5. DAG ordering and state-staging PR.
6. Replay determinism fixture PR covering selection, classification, ordering, and state digest permutations.
7. P2P locator/frontier capability PR.
8. Mining template selected-parent/parallel-parent PR.
9. Transaction selection and conflict-resolution PR.
10. Finality/pruning boundary PR.
11. Multi-node adversarial harness evidence PR.
12. Disabled-by-default high-cadence experiment PR.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Accidental Kaspa compatibility claim | Misleads operators and reviewers | Keep all docs and release notes explicit: Kaspa-inspired only until compatibility tests exist. |
| `blue_score` semantic migration breaks legacy blocks | Consensus split or invalid replay | Use explicit activation/versioning and preserve legacy interpretation before the boundary. |
| Arrival-order state commits leak into consensus | Nondeterministic replay | Stage effects and rebuild/apply state only in deterministic DAG order. |
| Unbounded merge-set processing | CPU, memory, or database DoS | Define hard bounds and deterministic overflow behavior before activation. |
| Peer-order-dependent sync | Divergent selected tips | Capability-negotiated locators/frontiers plus permutation tests. |
| Mining creates duplicate/conflicting transactions | Invalid blocks or nondeterministic acceptance | Template-level duplicate filtering and consensus DAG-order conflict tests. |
| Premature high cadence | Amplifies orphan and convergence failures | Require feature flag disabled by default and evidence gates listed above. |
| Over-pruning side-DAG data | Unable to replay or resolve reorgs | Conservative finality boundary and snapshot restore proof before pruning. |

## Consensus activation gates

PulseDAG exposes an explicit `consensus_mode` gate so design-only GHOSTDAG-inspired metadata cannot silently become live consensus.

| Mode | Default | Metadata behavior | High cadence | Release/readiness status |
| --- | --- | --- | --- | --- |
| `legacy` | yes | Preserves stable legacy tip behavior; selected-parent, merge-set, blue/red, and ordered-DAG fields are not activated consensus. | Disabled (`high_cadence_allowed=false`). | Normal mode, with high cadence still blocked until replay and harness evidence passes. |
| `ghostdag_dev` | no | Enables selected-parent, merge-set, blue/red diagnostics, and ordered-DAG metadata for development diagnostics only. | Disabled (`high_cadence_allowed=false`). | Experimental only; not public-testnet readiness and blocked by `ghostdag_dev_mode_not_release_ready`. |

Operators may select the mode with `PULSEDAG_CONSENSUS_MODE=legacy` or `PULSEDAG_CONSENSUS_MODE=ghostdag_dev` (or `--consensus-mode`). The default is always `legacy`.

Status and readiness surfaces must report:

- `consensus_mode`
- `ghostdag_metadata_active`
- `high_cadence_allowed=false`

Release/readiness blockers include:

- `ghostdag_dev_mode_not_release_ready`
- `high_cadence_blocked_until_replay_and_harness_pass`

`ghostdag_dev` is not public-testnet readiness. It does not implement new GHOSTDAG rules, change PoW or emission, enable high block cadence, or change the P2P protocol.
