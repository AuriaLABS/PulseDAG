# PulseDAG v2.2.15 release notes

PulseDAG v2.2.15 closes as the sustained P2P multi-node rehearsal release after the v2.2.14 storage/replay hardening milestone.

## Closeout status

v2.2.15 release evidence passed on Ubuntu/WSL at commit `aacbec8a4ad366cb258f7b6f9ff47890a44023d1`.

Evidence bundle result:

```text
Overall status: PASS
```

Passed sections:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo build --workspace`
- `bash scripts/v2-2-15-p2p-3node-rehearsal.sh`
- `bash scripts/v2-2-15-p2p-churn-rejoin-evidence.sh`
- `bash scripts/v2-2-15-p2p-lag-recovery-evidence.sh`
- `bash scripts/v2-2-15-chain-id-isolation-evidence.sh`

The evidence summary is generated at `evidence/v2.2.15/summary.md` by `bash scripts/v2-2-15-release-evidence.sh`.

## Highlights

- Bumps repository version metadata to `v2.2.15` and Cargo workspace version metadata to `2.2.15` while keeping license metadata as `ISC`.
- Positions v2.2.15 as the current sustained P2P operation milestone.
- Adds release documentation for three-node and optional five-node rehearsals, restart/rejoin, lag recovery, churn, convergence, peer diagnostics, and chain-id isolation.
- Adds a passing release evidence bundle for P2P rehearsal closeout.
- Keeps v2.2.14 as the storage/replay hardening closure.
- Keeps v2.2.16 as miner/node contract hardening.
- Keeps v2.3.0 as a readiness decision only, not an automatic launch.

## Scope

v2.2.15 is P2P rehearsal and hardening. It is not v2.3.0 private-testnet readiness and should not be presented as a launch/readiness decision by itself. v2.2.16 follows with miner/node contract hardening.

v2.2.15 focuses on evidence for:

- 3-node local P2P rehearsal.
- 5-node local P2P rehearsal when practical.
- Node restart/rejoin behavior.
- Lagging-node recovery.
- Peer churn.
- Chain-id isolation, including explicit evidence that mismatched `chain_id` peers do not become healthy compatible peers.
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
- Smart contracts remain out of scope.
- Pool logic in the miner remains out of scope.

## Required validation

v2.2.15 closeout evidence passed with:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
bash scripts/v2-2-15-p2p-3node-rehearsal.sh
bash scripts/v2-2-15-p2p-churn-rejoin-evidence.sh
bash scripts/v2-2-15-p2p-lag-recovery-evidence.sh
bash scripts/v2-2-15-chain-id-isolation-evidence.sh
bash scripts/v2-2-15-release-evidence.sh
```

## Known limitations and follow-up

- v2.2.15 is a P2P rehearsal/hardening evidence gate, not v2.3.0 readiness.
- The optional five-node local rehearsal remains a follow-up when practical or can be repeated during v2.2.18 private-testnet RC evidence.
- The next milestone is v2.2.16 miner/node contract hardening.

## Operator documents

- Roadmap: `docs/ROADMAP_V2_2_15.md`.
- Closing checklist: `docs/CLOSING_CHECKLIST_V2_2_15.md`.
- P2P rehearsal plan: `docs/P2P_REHEARSAL_PLAN_V2_2_15.md`.
- Version positioning: `docs/VERSION_MATRIX.md`.
