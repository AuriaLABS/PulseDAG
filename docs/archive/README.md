# PulseDAG historical archive

This index locates prototype evidence in Git history, separate from the first-network v3.0.0 source tree.

## Preservation baseline

The complete pre-cleanup repository state is preserved at commit:

`4bee533d97708d5166024839c02277d913438448`

Immutable historical trees:

- [Documentation before cleanup](https://github.com/AuriaLABS/PulseDAG/tree/4bee533d97708d5166024839c02277d913438448/docs)
- [Scripts before cleanup](https://github.com/AuriaLABS/PulseDAG/tree/4bee533d97708d5166024839c02277d913438448/scripts)
- [Workflows before cleanup](https://github.com/AuriaLABS/PulseDAG/tree/4bee533d97708d5166024839c02277d913438448/.github/workflows)
- [Versioned private-testnet configurations](https://github.com/AuriaLABS/PulseDAG/tree/4bee533d97708d5166024839c02277d913438448/configs/private-testnet)
- [Tracked historical evidence](https://github.com/AuriaLABS/PulseDAG/tree/4bee533d97708d5166024839c02277d913438448/artifacts)

## Version families

| Version | Historical scope | Status |
|---|---|---|
| v2.2.17 | API, operator, and security hardening | Archived |
| v2.2.18 | Private-testnet RC preparation, topology, perturbation, and evidence procedures | Archived |
| v2.2.19 | Multi-host/private rehearsal, operator hardening, and closeout evidence | Archived |
| v2.2.20 | Final v2.2 hardening, staged network evidence, protected rehearsal, and closeout | Archived baseline for v2.3.0 |

## Archive policy

The [retired workflow provenance index](github-actions/v2-legacy/README.md) identifies the old workflow blobs in immutable Git history. The old YAML copies are no longer tracked in the current source tree.

- Historical prototype evidence is never relabelled as first-network acceptance.
- Superseded operator guidance is removed from active indexes.
- Exact content remains available through immutable commits and tags.
- A historical helper may remain in an active path only when a current workflow still invokes it and its temporary development-test dependency is documented.
- New prototype-specific compatibility paths are not permitted. Retain only necessary regression fixtures, not duplicated archive copies.

Return to the [active documentation index](../README.md).

## Retired prototype task and operator material

The cleanup baseline [`b97280e434d3`](https://github.com/AuriaLABS/PulseDAG/commit/b97280e434d357bcc8336a19a73137a5ce90c7dd) preserves the retired [v2.x task documents](https://github.com/AuriaLABS/PulseDAG/tree/b97280e434d357bcc8336a19a73137a5ce90c7dd/docs/codex_tasks), [PowerShell prototype helpers](https://github.com/AuriaLABS/PulseDAG/tree/b97280e434d357bcc8336a19a73137a5ce90c7dd/scripts/archive/v2_2_history), [release notes](https://github.com/AuriaLABS/PulseDAG/blob/b97280e434d357bcc8336a19a73137a5ce90c7dd/docs/release/V2_4_0_RELEASE_NOTES.md), [limitations](https://github.com/AuriaLABS/PulseDAG/blob/b97280e434d357bcc8336a19a73137a5ce90c7dd/docs/release/V2_4_0_KNOWN_LIMITATIONS.md), and old [RPC smoke 17](https://github.com/AuriaLABS/PulseDAG/blob/b97280e434d357bcc8336a19a73137a5ce90c7dd/scripts/v2_2_17_rpc_security_smoke.sh) / [RPC smoke 19](https://github.com/AuriaLABS/PulseDAG/blob/b97280e434d357bcc8336a19a73137a5ce90c7dd/scripts/v2_2_19_rpc_security_smoke.sh). These are not active instructions or native-network evidence. Applicable acceptance requirements are consolidated in the first-network baseline; current RPC security integration tests remain intact.
