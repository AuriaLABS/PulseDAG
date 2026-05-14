# PulseDAG v2.2.15 release notes

PulseDAG v2.2.15 opens as the sustained P2P multi-node rehearsal release after the v2.2.14 storage/replay hardening milestone.

## Highlights

- Bumps repository version metadata to `v2.2.15` and Cargo workspace version metadata to `2.2.15` while keeping license metadata as `ISC`.
- Positions v2.2.15 as the current milestone for sustained P2P operation across multiple nodes.
- Adds release documentation for three-node and optional five-node rehearsals, restart/rejoin, lag recovery, churn, convergence, peer diagnostics, and chain-id isolation.
- Keeps v2.2.14 as the storage/replay hardening closure.
- Keeps v2.2.16 as miner/node contract hardening.
- Keeps v2.3.0 as a readiness decision only, not an automatic launch.

## Scope

v2.2.15 focuses on evidence for:

- 3-node local P2P rehearsal.
- 5-node local P2P rehearsal when practical.
- Node restart/rejoin behavior.
- Lagging-node recovery.
- Peer churn.
- Chain-id isolation.
- Sync convergence.
- Peer diagnostics and operator endpoint quality.
- No unresolved Sev-1 consensus or sync defects at closeout.

## Guardrails

- No smart contracts are added.
- No contract runtime is enabled.
- No pool logic is added.
- The miner remains a standalone external application.
- Consensus-rule changes remain out of scope unless they fix a documented safety bug and include tests.
- v2.2.15 is not a v2.3.0 readiness claim.

## Required validation

Before closing v2.2.15, collect output for:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
./scripts/v2-2-14-release-evidence.sh
```

If the v2.2.14 release evidence script is still the latest evidence script, label its output as the inherited baseline for v2.2.15 and attach any additional P2P rehearsal evidence separately.

## Operator documents

- Roadmap: `docs/ROADMAP_V2_2_15.md`.
- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_15.md`.
- P2P rehearsal plan: `docs/P2P_REHEARSAL_PLAN_V2_2_15.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
