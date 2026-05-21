# Risk Register v2.2.18

This register tracks key risks for `v2.2.18` private-testnet RC preparation.

## Fields

- risk id
- area
- severity
- likelihood
- owner
- mitigation
- evidence required
- status

## Register

| risk id | area | severity | likelihood | owner | mitigation | evidence required | status |
|---|---|---|---|---|---|---|---|
| R-218-001 | Consensus / DAG | High | Medium | Core protocol maintainers | Keep deterministic DAG selection checks in CI/rehearsal; block compatibility claims unless specific Kaspa/GHOSTDAG parity tests exist. | Deterministic DAG test outputs plus explicit compatibility test suite/results (if claim is made). | Open |
| R-218-002 | Mining / GPU | Medium | High | Mining maintainers | Treat GPU mining as optional path; gate RC on CPU/external miner baseline unless canonical kernel is implemented and verified. | Benchmark + functional evidence showing GPU path is canonical and stable, or explicit waiver documenting scaffold-only status. | Open |
| R-218-003 | Release positioning | High | Medium | Release manager | Enforce language guardrails in release notes/checklists: v2.2.18 != v2.3.0 readiness. | Reviewed release notes/checklist/go-no-go artifacts with explicit non-readiness statements. | Open |
| R-218-004 | Network launch governance | High | Medium | Program owner | Keep v2.2.18 scope private; require separate public testnet launch checklist and approvals. | Signed launch checklist and governance approval package for public testnet (outside v2.2.18). | Open |
| R-218-005 | RPC security exposure | Critical | Medium | Node operators + security owner | Restrict RPC to localhost/private interfaces; add firewalling and authn/authz before any broader exposure. | Endpoint inventory, bind-address evidence, firewall rules, and security validation logs. | Open |
| R-218-006 | RC claim quality | High | Medium | QA / Release evidence owner | Accept RC status only with artifact-backed evidence and reproducibility metadata. | Complete evidence bundle with run IDs, commands, logs, summaries, and traceability links. | Open |
| R-218-007 | Restore/rebuild confidence | High | Medium | Operations owner | Run snapshot/restore/rebuild drills repeatedly; document outcomes and rollback handling. | Successful drill outputs from multiple runs with timing, integrity checks, and pass/fail results. | Open |
| R-218-008 | Long-run stability | High | Medium | SRE / Operations | Execute sustained rehearsals (multi-hour/day as defined) and track incident trends/regressions. | Long-duration rehearsal artifacts, alert timelines, incident reviews, and stability trend summaries. | Open |

## Status legend

- **Open**: risk active; mitigation/evidence incomplete.
- **Mitigating**: mitigation in progress, partial evidence present.
- **Accepted**: explicitly accepted by owner with rationale and expiry/revisit date.
- **Closed**: mitigation complete and required evidence accepted.
