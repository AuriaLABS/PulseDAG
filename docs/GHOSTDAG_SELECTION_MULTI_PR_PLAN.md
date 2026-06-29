# Kaspa-inspired selected-parent / GHOSTDAG-style DAG selection multi-PR plan

This plan introduces selected-parent and GHOSTDAG-style DAG selection incrementally. It is intentionally scoped to avoid high block cadence until deterministic selected-parent selection, deterministic DAG ordering, and convergence evidence exist.

## Non-goals and claim boundaries

- Do not claim full Kaspa compatibility until PulseDAG has implemented and tested blue-set handling, merge-set handling, selected-parent selection, deterministic DAG ordering, pruning/finality rules, and harness evidence.
- Do not increase block production speed before deterministic selected-parent selection and deterministic DAG ordering have merged.
- Do not couple high-cadence experiments to foundational consensus PRs.
- Do not weaken existing orphan recovery, zero-peer startup, or sync safety behavior.
- Keep every PR small, reviewable, reversible where practical, and independently testable.

## Shared terminology to stabilize first

The first PR must define the exact PulseDAG meaning of the following terms before implementation PRs rely on them:

- `selected_parent`: the parent chosen by deterministic consensus scoring as the block's main selected-chain predecessor.
- `selected_tip`: the current best DAG tip according to selected-parent traversal and tie-breaking rules.
- `blue_score`: the count-like monotonic score representing accepted blue ancestry under the bounded classification rules defined for PulseDAG.
- `blue_work`: the work-weighted score used for selected-parent comparison when cumulative work must dominate count-only tie breaks.
- `merge-set`: blocks referenced by a candidate that are not on the selected-parent chain segment being extended and must be classified for acceptance.
- Blue/red classification: deterministic merge-set partitioning into accepted blue blocks and non-selected red blocks under bounded rules.
- Deterministic DAG ordering: a total order for accepted blocks derived from selected-chain traversal plus deterministic merge-set ordering.

## Safety invariants for all PRs

Each PR must preserve or explicitly strengthen these invariants:

1. Consensus replay over the same block set produces identical selected-parent metadata, selected tip, DAG order, and transaction inclusion decisions.
2. Block arrival order, orphan adoption order, peer order, and restart timing do not change consensus results.
3. Tie breaks are deterministic and use canonical data such as block hash, work, score, timestamp only where already consensus-valid, and stable parent ordering.
4. Missing-parent handling never finalizes a selected tip from incomplete DAG knowledge.
5. Zero-peer and orphan recovery never reintroduce unsafe final selected-sync behavior.
6. Mining templates never include duplicate transactions across selected-parent and parallel-parent context.
7. Blue/red classification is bounded so adversarial merge-sets cannot cause unbounded validation work.
8. High-cadence block production remains disabled until selected-parent and DAG ordering evidence is merged.

## PR 1 — Documentation/spec for selected-parent and GHOSTDAG-style terms

**Goal:** Establish the consensus vocabulary, scope, invariants, and compatibility boundaries before code changes.

**Changes:**

- Add a consensus design document defining `selected_parent`, `selected_tip`, `blue_score`, `blue_work`, merge-set, blue/red classification, deterministic DAG ordering, and safety invariants.
- Document deterministic tie-break requirements and forbidden sources of nondeterminism.
- Add explicit language that this is Kaspa-inspired, not a full Kaspa compatibility claim.
- Document that high block cadence is blocked until PRs 3 and 5 are merged and tested.

**Tests/checks:**

- Documentation link check or repository documentation check if available.
- Reviewer checklist confirming all downstream PRs cite this spec.

**Exit criteria:**

- Maintainers agree on terminology and invariants.
- No production behavior changes.

## PR 2 — Core data model for selected-parent metadata

**Goal:** Add storage and in-memory fields needed by selection while preserving compatibility with existing data.

**Changes:**

- Add optional selected-parent metadata fields to block index/state records.
- Track `selected_parent`, `blue_score`, `blue_work`, and classification placeholders without changing selection behavior yet.
- Add schema/version compatibility handling so older data can replay with default/derived metadata.
- Add serialization compatibility tests for old and new records.

**Tests/checks:**

- Unit tests for metadata defaults and round-trip serialization.
- Replay of existing fixtures or snapshots, if present.
- Existing workspace tests.

**Exit criteria:**

- Existing behavior remains unchanged when metadata is absent.
- Persisted data can be loaded without forcing a network reset.

## PR 3 — Deterministic selected-parent selection

**Goal:** Select a deterministic parent for every accepted block using the data model from PR 2.

**Changes:**

- Implement selected-parent scoring using cumulative work/score and deterministic tie breaks.
- Persist selected-parent metadata after validation.
- Keep existing block cadence and mining behavior unchanged.
- Add replay and order-independence tests that feed the same DAG in multiple arrival orders.

**Tests/checks:**

- Unit tests for selected-parent tie breaks.
- Replay/order-independence tests over small forks and multi-parent DAGs.
- Restart/reload test proving persisted selection matches recomputation.

**Exit criteria:**

- Same block set always yields the same selected tip regardless of arrival order.
- No high-cadence configuration is introduced or enabled.

## PR 4 — Bounded merge-set blue/red classification

**Goal:** Classify merge-set blocks deterministically under bounded validation costs.

**Changes:**

- Implement merge-set discovery for a candidate block relative to its selected parent.
- Add bounded blue/red classification with explicit limit constants and rejection behavior for oversized merge-sets.
- Persist or derive classification metadata as specified in PR 1.
- Add small DAG fixtures covering blue merges, red merges, boundary limits, and adversarial ordering.

**Tests/checks:**

- Fixture tests for classification determinism.
- Limit tests for oversized merge-sets.
- Replay/order-independence tests combining selected-parent and classification metadata.

**Exit criteria:**

- Blue/red outcomes are deterministic and bounded.
- Red blocks cannot affect transaction ordering or selected-tip scoring except as explicitly specified.

## PR 5 — Consensus DAG ordering from the selected chain

**Goal:** Derive a deterministic total order for accepted blocks using selected-chain traversal and classified merge-sets.

**Changes:**

- Implement deterministic DAG order derivation from the selected chain.
- Define merge-set ordering rules using canonical sort keys.
- Ensure transaction validation applies blocks in derived DAG order, not arrival order.
- Add duplicate transaction and conflict handling at ordering boundaries.

**Tests/checks:**

- DAG order fixture tests for forks, diamonds, and multi-level merges.
- Replay/order-independence tests asserting identical ordered block lists.
- Transaction conflict tests proving deterministic winner selection.

**Exit criteria:**

- Consensus state is derived from deterministic DAG order.
- This PR unlocks later high-cadence experiments but does not enable them.

## PR 6 — Mining templates using selected parent and valid parallel parents

**Goal:** Update mining templates to build on the selected parent while safely including valid parallel parents.

**Changes:**

- Build templates from the current `selected_tip`/selected parent.
- Include eligible parallel parents that are known, valid, and within merge-set bounds.
- Filter duplicate transactions across selected-parent context, parallel parents, mempool candidates, and template body.
- Expose enough template diagnostics for miners to understand selected parent and included parents.

**Tests/checks:**

- Mining-template unit tests for selected-parent choice.
- Duplicate transaction filtering tests.
- Submit-validation tests for templates with parallel parents.

**Exit criteria:**

- Existing external miner contract remains compatible or has documented additive fields.
- Templates do not produce invalid duplicate transaction sets.

## PR 7 — P2P sync with selected-chain locator and DAG frontier

**Goal:** Synchronize DAG state using selected-chain locators plus frontier data without unsafe final selected-sync during zero-peer or orphan recovery.

**Changes:**

- Add selected-chain locator exchange for efficient common-ancestor discovery.
- Add DAG frontier exchange for parallel tips and missing-parent discovery.
- Keep zero-peer startup and orphan adoption recovery conservative: no final selected-sync from incomplete peer data.
- Add peer-order-independent sync tests where possible.

**Tests/checks:**

- Unit tests for locator construction and matching.
- Integration tests for orphan recovery and delayed parent arrival.
- Regression test that zero-peer recovery does not finalize unsafe selected-sync state.

**Exit criteria:**

- Nodes converge on selected tip and DAG frontier without relying on unsafe peer timing.
- Sync remains safe when a node starts with zero peers and later reconnects.

## PR 8 — Experimental fast block cadence behind a feature flag

**Goal:** Introduce high-cadence experimentation only after deterministic selection and ordering are already merged.

**Changes:**

- Add a disabled-by-default feature flag or experimental config for faster block cadence.
- Gate all high-cadence defaults behind explicit opt-in testnet/dev settings.
- Add warnings that this is experimental and not a compatibility claim.
- Keep production defaults unchanged.

**Tests/checks:**

- Config tests proving the feature is disabled by default.
- Short convergence tests under the experimental setting.
- Regression tests proving normal cadence remains unchanged without the flag.

**Exit criteria:**

- Fast cadence cannot be enabled accidentally.
- Deterministic ordering evidence from PR 5 exists before this PR is reviewed.

## PR 9 — Multi-node and adversarial harness validation

**Goal:** Prove the selected-parent/DAG-ordering stack under realistic network and recovery conditions.

**Changes:**

- Add harness scenarios for 5 nodes / 1 miner and 5 nodes / 2 miners.
- Add orphan storm scenarios with delayed and reordered parent delivery.
- Add zero-peer recovery scenarios.
- Add high-cadence convergence scenarios that run only when the experimental feature from PR 8 is enabled.
- Record selected tip, selected chain, DAG frontier, blue/red counts, and deterministic order digest for comparison.

**Tests/checks:**

- 5N/1M convergence harness.
- 5N/2M convergence harness.
- Orphan storm convergence harness.
- Zero-peer recovery harness.
- Feature-flagged high-cadence convergence harness.

**Exit criteria:**

- Harness evidence demonstrates convergence of selected tip, selected chain, DAG frontier, classification results, and order digest.
- Remaining limitations are documented before any stronger compatibility or cadence claims are made.

## Dependency graph

```text
PR 1 spec
  -> PR 2 data model
    -> PR 3 selected parent
      -> PR 4 blue/red classification
        -> PR 5 DAG ordering
          -> PR 6 mining template
          -> PR 7 P2P sync
            -> PR 8 experimental cadence
              -> PR 9 harness evidence
```

PRs 6 and 7 may be developed in parallel after PR 5 if they do not change the same consensus-ordering code. PR 8 must wait for PR 5. PR 9 can add non-cadence harness scenarios earlier, but high-cadence scenarios must remain feature-gated until PR 8.
