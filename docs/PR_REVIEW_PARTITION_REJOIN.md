# PR Review: Partition/Rejoin behavior (3–5 nodes)

Scope reviewed: `crates/pulsedag-p2p/tests/partition_rejoin_slo.rs` and existing p2p recovery instrumentation.

## Verdict

**PASS (test evidence added)** for v2.1 partition/rejoin validation in the integration harness.

## Recovery SLO used

- **SLO:** post-heal convergence in **<= 6 deterministic harness ticks**.
- **Convergence definition:** every node in scenario shares:
  - same best height,
  - same best hash/tip identity,
  - stable non-divergent tip for a post-recovery stability window.

## New evidence (automated)

### 1) 3-node partition -> rejoin -> convergence

- Scenario: split `[0,1]` from `[2]`, mine on both sides, heal network.
- Assertion: all 3 nodes converge to a single tip within SLO and stay converged in a stability window.

### 2) 5-node partition -> rejoin -> convergence

- Scenario: split `[0,1]` from `[2,3,4]`, mine during partition, heal network.
- Assertion: all 5 nodes converge to one tip identity within SLO and remain stable.

### 3) Moderate churn / reconnect pressure

- Scenario: repeated link drops/reconnects plus temporary node offline/online transitions.
- Assertion: cluster still reconverges after churn pressure within SLO and remains stable.

### 4) Recovery SLO is measured, not assumed

- Deterministic test explicitly measures elapsed recovery ticks and asserts a bounded value.
- In a controlled 3-node heal case the harness confirms immediate (1 tick) post-heal convergence.

### 5) No persistent fork after rejoin

- Scenario: competing partition tips are created then rejoined.
- Assertion: cluster converges to one tip, then continues advancing without re-divergence.

## Confidence change

- **Before:** blocker-level concern due to missing integration evidence for 3–5 node partition/rejoin and churn.
- **After:** materially improved confidence from deterministic, repeatable integration coverage that exercises partition, heal, churn, SLO, and no-persistent-fork checks.

## Notes

- This change is intentionally scoped to harness/tests and review evidence.
- No consensus or miner logic changes were introduced.
