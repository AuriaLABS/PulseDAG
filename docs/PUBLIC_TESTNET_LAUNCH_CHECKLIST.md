# Public testnet launch checklist (v2.3.0 decision gate)

## Intent and scope
- [ ] this checklist defines **required launch conditions** for v2.3.0/public testnet
- [ ] this document does **not** claim current readiness or authorize launch by itself
- [ ] readiness status must be decided via `docs/V2_3_0_PUBLIC_TESTNET_DECISION_TEMPLATE.md`

## 1) Evidence gates (mandatory)
- [ ] local 3n/1m PASS with reproducible evidence
- [ ] private 5n/4m PASS with reproducible evidence
- [ ] multi-host rehearsal PASS
- [ ] snapshot restore PASS
- [ ] replay/rebuild PASS
- [ ] RPC security smoke PASS
- [ ] release binary install verification PASS
- [ ] private burn-in PASS for **at least 7 consecutive days**
- [ ] private burn-in PASS for **14 consecutive days** before public launch (ideal target)

## 2) Network gates (mandatory)
- [ ] bootnodes are documented and version-pinned where applicable
- [ ] firewall guide is complete and operator-validated
- [ ] inbound/outbound peer limits are set for public-safe operation
- [ ] RPC/P2P rate limits are set and tested
- [ ] public-safe RPC profile is tested and documented

## 3) Consensus gates (mandatory)
- [ ] deterministic replay evidence captured and archived
- [ ] fork/reorg tests PASS
- [ ] invalid block tests PASS
- [ ] difficulty retarget tests PASS
- [ ] timestamp rule tests PASS

## 4) Mining gates (mandatory)
- [ ] external miner workflow PASS against target node profile
- [ ] CPU miner submits are accepted in expected conditions
- [ ] miner address is required and validated in documented flows
- [ ] miner reject reasons are classified and documented
- [ ] GPU mining status is clearly documented (supported/not supported/experimental)

## 5) Operations gates (mandatory)
- [ ] log coverage and retention expectations are documented
- [ ] dashboards or metrics endpoints are operational and documented
- [ ] incident response runbook is documented and exercised
- [ ] upgrade and rollback procedures are documented and rehearsed
- [ ] snapshot restore operational workflow is documented and rehearsed
- [ ] seed node recovery procedure is documented and rehearsed

## 6) Guardrail gates (mandatory)
- [ ] no smart contracts are enabled or implied for this launch
- [ ] no pool logic is embedded in the miner scope
- [ ] no public admin RPC exposure
- [ ] no v3.0 readiness claim in v2.3.0/public testnet materials
- [ ] no Kaspa parity claim without explicit evidence

## Final decision rule
- [ ] if any mandatory gate above is not PASS, decision is **NO-GO** unless explicitly waived with risk acceptance
- [ ] final decision is recorded only via `docs/V2_3_0_PUBLIC_TESTNET_DECISION_TEMPLATE.md`
