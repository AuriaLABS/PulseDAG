# Codex task: harness and readiness gates

Priority: P1 after PR #545.

Goal: make v2.2.19/v2.3.0 rehearsal scripts deterministic, bounded, and useful for evidence.

Scope:

- clean `scripts/v2_2_19_private_5n_4m_rehearsal.sh`;
- add curl connect/max-time deadlines everywhere;
- add one global deadline and remove duplicate watchdog logic;
- make cleanup idempotent;
- fix `record_fail` usage;
- add quiescence after miners stop;
- keep 5N/4M as stress until convergence recovery is proven;
- add or validate 5N/1M and 5N/2M staged gates.

Acceptance:

- bad runs finish with evidence, never hang;
- `bash -n` passes for all changed scripts;
- preflight passes;
- 3N/1M remains PASS;
- 5N/1M baseline can be run locally with clear PASS/FAIL.

Guardrails: no consensus changes, no public readiness claim, no smart contracts.
