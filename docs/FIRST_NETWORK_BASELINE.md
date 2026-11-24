# PulseDAG first-network baseline

PulseDAG has no previously deployed network. The first public network is native v3 testnet, followed by independently authorized mainnet. Software version v3.0.0 is not a network migration.

## Active authority

- [Launch roadmap](ROADMAP_V3_0_0.md), [monetary policy](MONETARY_POLICY_V3_0_0.md), [testnet-first runbook](runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md).
- [Launch control #781](https://github.com/AuriaLABS/PulseDAG/issues/781), [cleanup #1524](https://github.com/AuriaLABS/PulseDAG/issues/1524).
- Native monetary authority #1466/#1410, recovery #1465, identity #1049, public testnet #1418, bounded contract core #1333.

## Cleanup rules

1. No legacy network compatibility, migration, private live network or old chain identity is required. Historical source remains recoverable through Git history.
2. Consolidate applicable requirements from ROADMAP_V2_3_0.md through ROADMAP_V2_6_0.md before removing those documents and their references. Historical installation, activation, private-testnet and release checklists must not be active operator instructions.
3. Replace obsolete runtime paths with native equivalents before deleting them. Preserve independent cryptographic vectors, invalid-input tests and security controls; adapt fixtures to the first-network protocol.
4. Close superseded issues and PRs only with explicit replacement links and retained evidence. Do not treat administrative closure as technical acceptance.
5. Validate links, configs, scripts, full CI and startup boundaries on each cleanup candidate. No activation changes in documentation-only cleanup.

## Launch boundaries

PRE-GO preparation authorized; public testnet runtime not yet authorized. Mainnet NO-GO. Monetary mining OFF until testnet-specific GO. Contracts INACTIVE on both networks; bounded core readiness remains a mainnet requirement. No private live network. Prolonged live tests occur on public testnet after minimum safety acceptance and GO.
