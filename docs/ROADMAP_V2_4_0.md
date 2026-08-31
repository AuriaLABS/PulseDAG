# ROADMAP v2.4.0 — Runtime Resilience, Protocol v2 and Deterministic DAG Consensus

Date: 2026-08-15 UTC

> **Historical v2.4 program / not current public-launch authority.** The standalone-public-testnet-first sequence, separate v2.4 public-launch decision, 30-day public-testnet clock and no-smart-contract launch assumptions documented below are retained as v2.4 provenance and regression context only. They are superseded for the definitive public launch by [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md): coordinated **v3.0.0 mainnet + parallel public testnet in Q4 2026**, with the v2.5 scale/resilience and v2.6 programmability workstreams incorporated as mandatory technical inputs. Final v3 launch authority is issue `#781` under the v3 freeze/manifest contract.

## Starting point

The v2.3.0 private-testnet line established repeatable multi-host bootstrap, lifecycle tooling, observability, incident runbooks, and protected rehearsal evidence.

The first v2.4.0 program then focused on operator modes, route-contract enforcement, control-plane resilience, submission finality, target-based retargeting, public-safe RPC, wallet hardening, pruning-aware sync, dependency security, and public-testnet release readiness.

The v2.4.0 scope is now intentionally extended before final release/activation to include two protocol-level changes that were previously deferred:

1. a versioned transaction/signing v2 contract with cryptographic chain/network binding and explicit replay/replacement/submission semantics; and
2. the deterministic GHOSTDAG-style consensus path already described by `GHOSTDAG_SELECTION_DESIGN_SPEC.md` and `GHOSTDAG_SELECTION_MULTI_PR_PLAN.md`, including selected-parent selection, bounded blue/red classification, canonical DAG ordering, state application, finality/pruning, P2P sync and mining integration.

This scope expansion means no earlier v2.4.0 candidate SHA, burn-in artifact, launch rehearsal, release identity or GO decision can be treated as final if it predates required Tasks 22–30. Evidence must be regenerated on the final exact candidate whenever the activation contract says the old evidence is invalidated.

Historically, v2.4.0 was defined as a no-smart-contract release with a separate public-testnet launch decision controlled by issue `#781`. That sequencing is **not** the current launch plan; for v3, programmability is mandatory scope and #781 controls the coordinated mainnet + parallel-testnet decision described by `ROADMAP_V3_0_0.md`.

## Guardrails

The guardrails below are retained as v2.4 implementation/provenance constraints. Public-testnet clock/readiness markers in this section are legacy compatibility state, not v3 launch requirements.

- `VERSION`, Cargo package versions and release identity remain unchanged until the final Task 31 decision authorizes the v2.4.0 release/activation candidate.
- No `v2.4.0` tag or release artifact may be published from roadmap/implementation work without explicit maintainer approval.
- `public_testnet_ready=false` remains mandatory until the separate launch-control gate authorizes public launch.
- The 30-day public-testnet clock must not start or be backdated before the actual authorized public launch.
- Smart contracts remain disabled and require a separate later approval after the accepted public-testnet policy is satisfied.
- Multi-node safety remains fail-closed by default.
- Any single-node mode must be explicit and impossible to activate accidentally.
- Consensus, transaction, signing, header and storage-format changes require explicit versioning/activation rules; no canonical semantics may be silently mutated in place.
- Historical blocks, transactions, signatures and snapshots must remain deterministically decodable/replayable under their original version rules.
- Mining remains an external application; no embedded pool logic is introduced.
- High cadence remains disabled by default until Tasks 24–28 are complete and Task 29 is intentionally enabled for controlled experimentation.
- Code comments, developer documentation, commits, and pull-request descriptions remain English-only.
- Credentials, private keys, wallet seeds, local runtime state, generated burn-in output, and operator-specific configuration must not be committed.

## Dependency spine

```text
Tasks 14–20 runtime/release foundation
        |
        v
Task 21 pre-protocol readiness checkpoint
        |
        v
Task 22 activation contract
        |
        +--> Task 23 transaction/signing v2
        |
        +--> Tasks 24–28 deterministic GHOSTDAG / high-cadence / sync / mining path
        |
        v
Task 29 controlled high-cadence experimentation
        |
        v
Task 30 adversarial/replay evidence
        |
        v
Task 31 final v2.4.0 release/activation decision
```

## Task status

The detailed v2.4 task definitions and acceptance criteria below remain historical implementation authority for their respective source changes and evidence. They do not override the v3 launch model above.

<!-- Existing task detail intentionally retained below this point in repository history. -->
