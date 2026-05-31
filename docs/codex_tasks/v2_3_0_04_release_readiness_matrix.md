# Codex task: v2.3.0 readiness matrix

Priority: P4 after convergence recovery work.

Goal: define when v2.3.0 can start and when public-testnet readiness can be considered.

Scope:

- update VERSION_MATRIX with v2.2.19 hardening status and v2.3.0 prerequisites;
- update known limitations with latest 5N gate evidence;
- add final checklist for v2.3.0 start criteria;
- document that public_testnet_ready remains false until gates pass;
- document evidence required for 30-day testnet burn-in.

Required gates before v2.3.0:

- workspace build and tests pass;
- 3N/1M PASS;
- 5N/1M PASS;
- 5N/2M PASS;
- 5N/4M stress PASS or accepted known-limitation with metrics;
- no public readiness claim without evidence.

Guardrails: no version bump unless explicitly approved after evidence.
