# PulseDAG Version Matrix

This matrix keeps release positioning clear across v2.2.x and v2.3.x.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `2.2.12` |
| Current milestone | v2.2.12 full private-testnet rehearsal and hardening |
| Next milestone | v2.2.13 consensus/DAG safety audit |
| Following milestone | v2.3.0 private-testnet readiness decision |
| Private-testnet readiness milestone | v2.3.0 |
| Miner architecture | External standalone miner |
| Smart contracts | Out of scope |
| Pool logic in miner | Out of scope / not allowed |

## Release boundaries

| Version | Purpose | Status framing |
| --- | --- | --- |
| v2.2.8 | Hardening baseline closure | Pre-private-testnet hardening |
| v2.2.9 | Private-testnet rehearsal closure | Rehearsal only |
| v2.2.10 | Final PoW completion | PoW finalized, P2P not yet complete |
| v2.2.11 | P2P completion | Networking/sync completion closure; not official readiness |
| v2.2.12 | Full private-testnet rehearsal and hardening | Multi-node/operator rehearsal, sustained validation, runbook hardening, and evidence capture |
| v2.2.13 | Consensus/DAG safety audit | DAG invariant checks, deterministic tip selection tests, parent/height/timestamp validation tests, missing-parent/orphan adoption safety tests, replay/order-independence evidence where practical, and compatibility-claim review |
| v2.3.0 | Official complete private-testnet readiness milestone | Readiness decision milestone |

## v2.2.11 closeout scope

v2.2.11 closed the P2P completion path for block announce/request/data flow, transaction relay, tip exchange, missing parent recovery, orphan handling, peer scoring/backoff, duplicate suppression, P2P diagnostics, and the reproducible three-node rehearsal scripts.

## v2.2.12 current scope

v2.2.12 consumes the v2.2.11 P2P completion outputs and performs the full private-testnet rehearsal and hardening pass. It should validate longer-running multi-node and multi-operator scenarios, restart/rejoin behavior, sync convergence, diagnostics quality, operational runbooks, and release evidence without claiming v2.3.0 readiness early.

## v2.2.13 consensus/DAG safety audit

v2.2.13 follows v2.2.12 as an intermediate consensus/DAG safety audit before the v2.3.0 readiness decision. It should audit DAG invariant checks, deterministic tip selection, parent/height/timestamp validation, missing-parent and orphan adoption safety, and replay/order independence where practical. It must clearly document that PulseDAG is not claiming full Kaspa or GHOSTDAG compatibility.

## v2.3.0 readiness decision

v2.3.0 remains the private-testnet readiness decision milestone. Evidence gathered during v2.2.12 and v2.2.13 can inform that decision, but v2.2.12 itself must remain a rehearsal and hardening milestone and v2.2.13 itself must remain a consensus/DAG safety audit milestone.

## Guardrails

- Do not move smart contracts into the v2.2.x line.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep miner external and node-facing through documented interfaces.
- Do not claim official private-testnet readiness before v2.3.0.
- Do not claim full Kaspa or GHOSTDAG compatibility unless that compatibility is explicitly implemented, tested, and documented.
- Avoid consensus rule changes in v2.2.13 unless they fix a clear safety bug and include documented test evidence.
