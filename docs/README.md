# PulseDAG documentation

The active software release is **v2.4.0**. Older v2.3.x and v2.2.x material remains available as historical release evidence, compatibility documentation, or archived operator context; it must not be mistaken for the active version contract.

## v2.4.0 release and operations

- [Roadmap](ROADMAP_V2_4_0.md)
- [Difficulty retarget contract](DIFFICULTY_RETARGET_V2_4_0.md)
- [Version matrix](VERSION_MATRIX.md)
- [Operator runbook](RUNBOOK.md)
- [Single-node private burn-in operations](runbooks/V2_4_0_SINGLE_NODE_OPERATIONS.md)
- [Binary installation and verification](INSTALL_BINARIES_V2_4_0.md)
- [Release evidence policy](RELEASE_EVIDENCE.md)
- [Burn-in gate](BURN_IN_GATE.md)
- [Release notes](release/V2_4_0_RELEASE_NOTES.md)
- [Release decision](release/V2_4_0_RELEASE_DECISION.md)
- [Private-testnet release closeout](checklists/V2_4_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md)
- [Public-testnet operator entry checklist](checklists/PUBLIC_TESTNET_OPERATOR_ENTRY_CHECKLIST.md)

## Release boundary

The v2.4.0 software release is authorized independently from public-testnet launch. Until the separate launch-control decision and actual public launch are recorded:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

Any source or fixed launch-configuration change after an exact-SHA validation freeze requires the affected validation and operational evidence to be regenerated on the new exact SHA.

## Historical material

- [Historical archive](archive/README.md)
- v2.3.0 release, installation, operations, evidence, and closeout documents are retained for provenance and compatibility.
- v2.2.x material remains historical unless an active document explicitly references it as a supported compatibility surface.
