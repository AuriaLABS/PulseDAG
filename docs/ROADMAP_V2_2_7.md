# Roadmap v2.2.7 (proposed PR queue)

This roadmap proposes the next docs/validation queue after v2.2.6 status sync.

## Proposed next-step PR queue
1. Startup/recovery tests
   - Expand restart-path assertions and operator runbook validation for cold/warm boot recovery behavior.
2. Snapshot/prune restore drills
   - Add repeatable restore drills that exercise snapshot import, prune windows, and post-restore health checks.
3. Miner stale-template safety
   - Add explicit stale-template safety test coverage for the external standalone miner flow.
4. P2P real-network guardrail tests
   - Add guardrail tests that verify real-network mode semantics and prevent simulated-mode confusion in operator signals.
5. Burn-in evidence package
   - Standardize evidence bundle structure/checklists for release gating and 30-day stable testnet burn-in proof.

## Scope guardrails
- Docs/process/test coverage planning only.
- No consensus, miner behavior, API behavior, storage behavior, or P2P protocol behavior changes are implied by this queue.
