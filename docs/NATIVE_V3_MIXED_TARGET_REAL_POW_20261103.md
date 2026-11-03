# Native v3 mixed-target real-PoW GHOSTDAG trial (UNAPPROVED)

**Scope:** Diagnostic-only independent evidence for [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) V3-GD-03 (work contribution), V3-GD-04 (selected-parent comparison) and V3-GD-05/06 (K anticone, checked work and blue scores). **Not a policy freeze or adopted consensus rule.**

Current baseline: `main@63bd3fae384aec50e2011a7941036d42a98c8edc` after merged #1364. This PR adds **only a new test, a narrow exact-head CI extension and this note**, not production code or network activation.

## Exact existing candidate arithmetic independently challenged

The separate, existing Python stdlib oracle `scripts/native_v3_work_policy_candidates.py --self-test` computes the proposed **integer** `floor(2^256 / (compact_target + 1))`, not floating point. The dormant Rust `CandidateWork512` candidate uses explicit checked accumulation and separate KHeavyHash PoW authentication. These are **alternative research rules** until V3-GD-03 explicitly freezes the numerator, denominator, work width, overflow behavior and supporting independent goldens.

| Compact bits | Python candidate own block work | Trial treatment |
| --- | ---: | --- |
| `0x207fffff` | 2 | Easy sibling branch; 2 consecutive blocks sum to 4 |
| `0x1f7fffff` | 512 | Harder one-block sibling outweighs easy 2-block branch |

The new Rust regression mines **real canonical KHeavyHash**, including the harder block (bounded 65,536 test-only nonce budget). It authenticates a complete frozen-genesis parent DAG with `G → E1 → E2` and `G → H`, and PoW-valid join `parents=[E2,H]`. On all trial variants it asserts that **H is selected despite lower height**, because its independently derived proposed work is 512 rather than 4. It also checks the full selected-parent/merge-blue/merge-red/score/checked-work relationship:

| Experimental K | Side blues | Side reds | Join score | Proposed join work |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 2 | 2 | 514 |
| 1 | 1 | 1 | 3 | 516 |
| 2 | 2 | 0 | 4 | 518 |

A whole-closure negative case removes `E1` and requires **failure**, even though that path is not selected. A separate *correctly mined* header with forged committed blue score also must reject. Every block and Merkle/state-root field in this test is synthetic; the `state_root` is deliberately **not** a transaction-state commitment. No transaction or reward is spent, settled or minted.

The exact-head `.github/workflows/v3-dormant-ghostdag-core-probe.yml` now runs the Python independent work self-test, the real mixed-target Rust case and strict Clippy. The dedicated no-production-callsites scanner allows the added **test-only** Rust path; it does not authorize any production GHOSTDAG consumer.

## What remains BLOCKED

This demonstrates candidate internal consistency and Python-vs-Rust integer-work arithmetic at two authenticated targets. It does **not** freeze the choice between `2^256` / `2^256-1` / unit work, numeric K (including K32) or eligible parent cap (including 8). It does not prove arbitrary-changing-target retarget validity or native economic ordering, red block transaction treatment, coinbase/reward finality, committed UTXO state roots, checkpoints, 500ms propagation or protocol-owner approval. All V3-GD-01..10 items remain open until separate explicit approval/evidence, as required by `docs/NATIVE_V3_GHOSTDAG_POLICY_FREEZE_RFC.md`.

**Native-v3 monetary mining OFF. Smart-contract deployment and execution INACTIVE/fail-closed. Public mainnet and parallel public testnet #781 NO-GO.** 
