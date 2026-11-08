# PulseDAG v3.0 — testnet-first public launch runbook

Status: **PROPOSED / NO PUBLIC GO**  
Owner sequencing decision: 2026-11-08  
Authority: [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) (sole public release authorization record)  
Implementation program: [#794](https://github.com/AuriaLABS/PulseDAG/issues/794)

## Superseded model

The former coordinated mainnet + parallel public-testnet release, including the prohibition on early public-testnet launch, is superseded. Decisions `GO_V3_DUAL_LAUNCH`, `DELAY_V3_DUAL_LAUNCH` and `NO_GO_V3_DUAL_LAUNCH` are retired from future authorization. They do not retroactively grant GO.

## Network phases — no implicit escalation

| Phase | Public? | Monetary native-v3 mining | Mainnet value | Smart contracts | GO decision |
|---|---|---|---|---|---|
| 0. Offline/dev harness | No | Synthetic/replay evidence only | None | INACTIVE network; offline VM tests allowed | No public GO |
| 1. Isolated private monetary rehearsal | No | Actual, only in uniquely identified, operator-approved isolated chain | None | INACTIVE | Private activation authorization |
| 2. Public testnet | Yes | Actual after testnet identity-bound gate passes | Test coins only, no value | INACTIVE | `GO_PUBLIC_TESTNET` on #781 |
| 3. Mainnet | Yes | Actual on independent production chain | Real chain | INACTIVE | `GO_V3_MAINNET` on #781 |

Do not reuse genesis, chain ID, network name, signing domain, persistent node/peer keys, wallet-key namespaces, data directories, seed/bootnode sets, address formats or RPC credentials among networks. Enforce fail-closed cross-network handshake, replay, signing and config/sidecar checks. **No phase is permission to enable contracts.** A planning/CI merge is never release authorization.

## Gate T0 — prepare and authorize **public testnet** separately

All must be evidenced on a named exact source SHA/artifact/config bundle. Checkboxes remain open until verified:

- [ ] Protocol owner has frozen GD-01..10, including selected parent, K, blue/red ordering, blue-work arithmetic, target/retarget and canonical signed monetary replay/rewards/fees/UTXO/conflicts (#1309, #1410).
- [ ] Native-v3 node/miner actually accept correct block production and signed transfers; PoW/consensus and historical-vs-pruned replay agree; reorg, restart, offline recovery, adversarial P2P and cross-node convergence have reproducible evidence (#1367).
- [ ] Isolated non-public monetary rehearsal has **actually** activated and tested the new runtime without bypassing public/mainnet deny gates.
- [ ] Exact-source testnet binaries and immutable config digest, chain ID, genesis hash, independent bootnodes, signing and wallet domains, datastores, RPC/TLS/rate limits, version support and reset policy are frozen (#1049).
- [ ] Reproducible CI/security and dependency reachability review of exact testnet artifacts is green; no known reachable launch-blocking vulnerabilities, consensus partition or funds-integrity problem (#1387).
- [ ] Public node and CPU miner provisioning, peer diversity, metrics and alerts, backup/restore, on-call owners, abuse/DoS mitigations, incident and emergency-stop/rollback procedure are tested (#1050).
- [ ] Public docs: node installation, sync, mining and wallet operations, explorer/RPC endpoints, test-coin non-value disclaimer, known limitations, support/security reporting, permitted testnet reset rules and chain identifiers.
- [ ] Contract deployment/execution/storage/activation paths are absent or fail closed with `contracts_enabled=false` in each testnet role and through config/restart/replay/RPC.
- [ ] #781 records one explicit `GO_PUBLIC_TESTNET` decision with operator, decision UTC, exact artifact/config digest, incident route and network IDs.

**Only after GO:** start testnet bootnodes, independent observer nodes, authorized CPU miners and public-safe RPC/explorer/wallet interfaces. Record first accepted monetary-v3 block height/hash/time, signed transfer, fee/reward reconciliation, independent node convergence, contract-INACTIVE evidence and published incident route. If any check fails, keep `NO_GO_PUBLIC_TESTNET` or `DELAY_PUBLIC_TESTNET`.

## Gate T1 — public testnet stabilization (before mainnet)

- [ ] Accumulate **>=30 consecutive calendar days** of meaningful, monitored public testnet operation after the first verified public block. Record outage windows, node diversity, accepted/rejected transaction and miner counts, canonical state/reward/supply, retarget, DAG blue/red behavior, propagation, P2P abuse, sync, pruning, rejoin and wallet reconciliation.
- [ ] Record at least one versioned upgrade/restart/backup/restore/rollback and incident-response drill; testnets may have disclosed resets but such resets interrupt any claimed continuous evidence for the affected span.
- [ ] No unresolved critical or high-severity consensus, monetary-integrity or chain-isolation issue. Document every significant incident, remedy, re-test and residual risk.
- [ ] Freeze the final mainnet protocol and monetary policy; reproduce results using exact expected semantics and separately frozen mainnet genesis and network credentials.

This testnet period provides evidence and is not a calendar-based authorization. Running longer is allowed. Testnet can remain operational permanently after mainnet.

## Gate M0 — independent **mainnet** GO

- [ ] All #781/#794 final mainnet core, production wallet/custody, security, supply-chain, release, API, infrastructure and operator readiness gates pass for the exact final mainnet tree/build.
- [ ] Native-v3 production consensus and monetary state satisfy full-history-vs-authenticated-pruned restore equivalence, golden vectors, external mining and worst-case load checks.
- [ ] Complete >=25-node / >=16-CPU-miner adversarial rehearsal and >=168 contiguous hours on one **unchanged exact candidate**, with immutable evidence. Physical GPU production validation is separately deferred, not advertised as PASS.
- [ ] #1333 defines and proves the bounded core implementation `CONTRACT_IMPLEMENTATION_READY` while **all deployment/execution/state transitions remain INACTIVE**. This core implementation readiness is a mainnet GO requirement; it is not required to falsely declare the earlier testnet READY.
- [ ] Mainnet chain ID/genesis/signing identity/bootnodes/data/wallet and RPC endpoints are independently frozen and tested against testnet; mainnet launch/rollback/on-call/time UTC and public communications are finalized.
- [ ] #781 records explicit `GO_V3_MAINNET` with operator decision, exact artifact and identity evidence. An earlier `GO_PUBLIC_TESTNET` does not satisfy this item.

**Only after mainnet GO:** start mainnet nodes/miners with production-safe profiles, verify first accepted block and legitimate monetary transactions, monitor status and publish checksums/bootnodes/wallet/explorer, known limitations and contract-INACTIVE status. Keep public testnet distinct and running for future upgrade rehearsals.

## Failure/rollback policy

Any failure of identity separation, incorrect rewards/supply/UTXO, consensus divergence, bypassable contract activation, unresolved exploitable reachable dependency or secrets exposure is STOP/NO-GO for the affected phase. Do not bypass fail-closed guards, waive CI, silently reset a public testnet or migrate test coins into mainnet supply. Mainnet issuance/genesis cannot be replayed or imported from testnet. Record incident and require a fresh exact-head test and authorization decision.

## Current decision state

As of 2026-11-08: **Public testnet NO-GO; mainnet NO-GO; monetary-v3 public activation OFF; smart contracts INACTIVE.** This runbook is a plan, not a deployed network, activated miner, release, clock start or GO certificate.
