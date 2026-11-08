# PulseDAG v3.0 — public testnet first, mainnet later

Status: **PROPOSED / NO PUBLIC GO**
Owner decision: 2026-11-08; latest change: **the public testnet itself is the first live native-v3 monetary rehearsal — no mandatory private network**.
Authority: [#781](https://github.com/AuriaLABS/PulseDAG/issues/781) (only public authorization record).
Execution: [#1418](https://github.com/AuriaLABS/PulseDAG/issues/1418).
Integration: [#794](https://github.com/AuriaLABS/PulseDAG/issues/794).

## Sequence

1. **Prepare minimum safe testnet release:** freeze the native-v3 consensus and economic rules needed for deterministic block/transaction admission; run local unit/integration, invalid-work, inflation/double-spend, chain-domain and contract-INACTIVE regression tests; build auditable binaries/config and safe public operations. These local tests **do not require a separate private network**.
2. **Authorize and launch public testnet:** record `GO_PUBLIC_TESTNET` for exact artifacts, genesis, chain ID, signing domain and responsible operators. The public testnet is where genuine monetary mining and all full-scale node/transaction scenarios will first be exercised. Its coins have no real-world value.
3. **Test openly on public testnet:** measure accepted PoW blocks, CPU mining, subsidy, fees, signed UTXO transfers, conflict handling, selected parent/blue-work, forks/reorgs, prune/checkpoint/rejoin, restart, multi-node convergence, P2P attacks, cadence, wallet and explorer. Publish failures, resets and fixes.
4. **Stabilize testnet:** require >=30 consecutive days of meaningful monitored public operation with an incident ledger and recovery/upgrade/rollback evidence, plus final-mainnet acceptance requirements.
5. **Authorize mainnet independently:** require `GO_V3_MAINNET` on #781 for exact production artifacts, genuinely distinct mainnet identity, monetary replay equivalence, full security/release readiness, 25-node/16-miner adversarial and 168-hour unchanged-candidate gates, and bounded smart-contract implementation READY but INACTIVE (#1333).

A testnet GO is **never** a mainnet GO. Neither is granted by a documentation change.

## Testnet entry gate T0 — minimum safety before public exposure

Each item is checked only with verifiable evidence from the specific candidate. Do not require a private live monetary rehearsal or treat testnet's future long-running results as entry prerequisites.

- [ ] Protocol identity/GD-01..10 (including GD-08 ordered monetary acceptance) and PoW/target/retarget/rewards/UTXO/conflict rules sufficiently frozen and implemented for deterministic native-v3 admission; explicit unsupported-policy refusal (#1309, #1410).
- [ ] Local/offline CI smoke/regressions pass for PoW correctness, header/transaction signatures, reward/fee conservation, duplicate-spend rejection, malformed messages, state update atomicity and the selected-parent order. All tests tied to exact source SHA.
- [ ] Testnet-only monetary enablement is **strictly bound to authorized testnet chain ID, frozen genesis, signing domain and config digest**. The normal mainnet production startup path still fails closed before its own GO; no config/sidecar/peer bypass; contracts remain INACTIVE everywhere.
- [ ] Dedicated **testnet** genesis, chain ID, bootnode identities, signing/address domain, datastores, wallet key namespaces, P2P network identity, RPC/status endpoints, credentials and permitted-reset policy. No future mainnet supply, keys or peers are reused (#1049).
- [ ] Public attack-surface baseline passes: reachable critical/high security exposures resolved, public RPC auth/rate/resource limits, peer flood containment, noncustodial wallet secrets, supply-chain/CI integrity and incident stop/rollback mechanism (#1387, #1050).
- [ ] Testnet operational minimum: functioning bootnode, operator/backup contact, logs and metrics, reproducible node/CPU-miner/wallet binaries, monitor/alert and public incident route; publish clearly that coins are worthless, network may reset and API/protocol semantics may change.
- [ ] Proof that smart-contract deploy/execute/state transitions and contract RPC cannot activate through config, restart, replay, malformed requests or feature flags. Core implementation READY is **not** a testnet entry requirement, but is a mainnet entry requirement.
- [ ] #781 records `GO_PUBLIC_TESTNET` with decision UTC, operators, exact binary/source/config digests, full testnet identity and initial rollback/incident procedures.

**After T0 GO:** start public testnet bootnodes, nodes and CPU miners; log first accepted real monetary block and timestamp, signed transfer, rewards/fees and independent node consensus observations. A failed post-launch trial becomes a recorded testnet issue, not a false pre-GO success.

## Public testnet validation T1 — the reason the testnet exists

- [ ] Genuine CPU mining and PoW difficulty/retarget tested across ordinary and adversarial conditions.
- [ ] Signed transfers, UTXO double spends, fee/reward accounting, reorg/conflict ordering, blue/red classification and supply roots tested at real chain heights.
- [ ] Node sync, bootstrapping, pruning/checkpoint/restoration, cold restart, fork/rejoin, P2P/DoS resistance, network-wide convergence and measurements across nodes tested.
- [ ] Wallet/explorer/API experience and multi-node/multi-miner telemetry reviewed; publish reproducible incidents, fixes, upgrades and reset notices.
- [ ] >=30 consecutive days of **meaningful** monitored public activity after first accepted block, with documented incident/remediation/rollback. A reset that invalidates the evidence restarts the relevant contiguous period.
- [ ] Fix all consensus/security/funds-integrity problems before promotion; no automatic production release based on elapsed time.

## Mainnet gate M0 — separate production acceptance

- [ ] Final exact-candidate source, artifacts, config/genesis/chain identities, wallet/custody, CPU-mining, API, RPC, operator/release and dependency/security checks independently PASS for production.
- [ ] Full native-v3 replay and verified authenticated-pruned state equivalence, rollback/restart, PoW/retarget, supply and fee conservation and distributed consensus evidence pass.
- [ ] >=25-node / >=16-CPU-miner adversarial rehearsal, >=168 consecutive hours on one unchanged integrated production candidate, and production-specific incident/upgrade/rollback pass. Physical GPU validation remains deferred/nonproduction unless separately approved.
- [ ] #1333 bounded smart-contract core proven `CONTRACT_IMPLEMENTATION_READY` on exact candidate; smart contracts deployment/execution/state transitions/RPC **remain INACTIVE**.
- [ ] Production mainnet has independent chain ID/genesis/bootnodes/wallet/signing domains and keys with no overlap or replay between testnet and mainnet.
- [ ] Final independent `GO_V3_MAINNET` (not `GO_PUBLIC_TESTNET`) recorded in #781.

Only **after** mainnet GO may operators start mainnet and record first genuine production block, publish endpoint identities and final release checksums. Keep public testnet active for future upgrades.

## STOP / NO-GO policy

Stop the affected network or activation if there is a chain-domain isolation failure, mainnet-guard bypass, unintended smart-contract activation, exploitable reachable vulnerability, consensus split or invalid monetary creation. Never weaken fail-closed protection just to run a test; instead implement a reviewed, testnet-only authorized activation path.

As of 2026-11-08: **testnet NO-GO; mainnet NO-GO; public native-v3 monetary mining OFF; smart contracts INACTIVE.** No network deployment, release, private rehearsal or automatic permission is implied.
