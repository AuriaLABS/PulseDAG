# Public testnet NO-GO criteria (v2.3.0)

This document defines conditions that block public testnet launch until resolved or formally waived.

## Automatic NO-GO triggers

### Evidence failures
- missing or non-reproducible evidence for any mandatory checklist item
- failed local 3n/1m, private 5n/4m, or multi-host rehearsal
- failed snapshot restore or replay/rebuild validation
- failed RPC security smoke checks
- failed release binary install verification
- burn-in period shorter than 7 consecutive private-network days

### Network readiness failures
- bootnodes not documented or not reachable per declared topology
- firewall guide incomplete or unvalidated by operators
- peer limits unset or inconsistent with public-safe profile
- rate limits unset or not validated under traffic
- RPC profile exposes unsafe/public-admin behavior

### Consensus safety failures
- no deterministic replay evidence for target build
- unresolved fork/reorg instability
- unresolved invalid block acceptance risk
- unresolved difficulty retarget correctness risk
- unresolved timestamp rule correctness risk

### Mining readiness failures
- external miner cannot sustain expected workflow
- CPU miner submits not accepted in expected normal path
- miner address not required where required by policy
- reject reasons not classified (cannot distinguish expected vs critical)
- GPU status undocumented or ambiguously represented

### Operations readiness failures
- inadequate logging for incident triage
- no operational dashboards/metrics endpoint visibility
- no incident response ownership or process
- upgrade/rollback untested for release path
- snapshot restore untested in ops workflow
- seed node recovery untested or undocumented

### Guardrail violations
- any smart contract capability claimed as part of this launch
- pool logic introduced into miner scope
- public admin RPC enabled
- public messaging claims v3.0 readiness
- Kaspa parity claim without specific evidence

## Waiver policy
A NO-GO trigger may be bypassed only with an explicit **WAIVED_WITH_REASON** decision that includes:
- risk statement and blast radius
- mitigation/rollback plan
- decision owner
- dated evidence path
- explicit risk acceptance

Use `docs/V2_3_0_PUBLIC_TESTNET_DECISION_TEMPLATE.md` for all decisions.
