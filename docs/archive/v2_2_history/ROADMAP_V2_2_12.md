# Roadmap v2.2.12 — Full Private-Testnet Rehearsal and Hardening

v2.2.12 is the handoff milestone after v2.2.11 P2P completion. It should rehearse the completed P2P path across a fuller private-testnet operating model and harden the runbooks, diagnostics, and evidence capture needed before the v2.3.0 readiness decision.

## Positioning

- **v2.2.11** closes P2P completion.
- **v2.2.12** runs the full private-testnet rehearsal and hardening pass.
- **v2.3.0** remains the official private-testnet readiness milestone.

## Focus areas

- Multi-node and multi-operator rehearsal beyond the local three-node smoke path.
- Sustained block propagation, tx relay, tip exchange, restart/rejoin, and catch-up validation.
- Missing parent recovery, orphan handling, duplicate suppression, invalid block rejection, and chain-id mismatch evidence.
- Peer scoring/backoff and P2P diagnostics review under realistic churn.
- Operator runbook hardening, evidence collection, rollback/recovery notes, and closeout criteria.

## Guardrails

- Do not claim official private-testnet readiness in v2.2.12.
- Do not add smart contracts.
- Do not add mining-pool logic.
- Keep the miner external.
- Keep v2.3.0 as the readiness decision milestone.
