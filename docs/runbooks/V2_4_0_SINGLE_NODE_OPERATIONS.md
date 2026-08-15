# v2.4.0 single-node operations

## Scope

This runbook covers the explicit v2.4.0 single-node operator profile for local development, deterministic validation and the single-node phase of the private burn-in.

It does not authorize public-testnet launch, start either the private 24-hour clock or the public 30-day clock, enable smart contracts, or replace the mandatory multi-node validation phases.

The final v2.4.0 source SHA and release identity remain `TBD` until all intended repository changes pass the final candidate gate.

## Safety contract

Single-node operation requires all of the following:

- `PULSEDAG_SINGLE_NODE_MODE=true`;
- `PULSEDAG_PRIVATE_TESTNET_ROLE=single`;
- P2P disabled;
- no bootnodes;
- no public P2P advertisement;
- loopback-only RPC;
- persistent RocksDB storage outside `/tmp` and `/run`;
- public-testnet readiness false;
- the 30-day public-testnet clock not started;
- smart contracts disabled.

An empty bootnode list or `role=seed` does not activate this mode. Contradictory public, multi-host or readiness settings must fail before startup.

## Prepare configuration

Copy the reference configuration and change only deployment-specific persistent paths:

```bash
cp configs/single-node/single-node.env.example single-node.env
```

Do not commit the resulting operator file when it contains host-specific paths, credentials, wallet material, tokens, unrestricted endpoints or runtime data.

For an official burn-in, archive a sanitized copy and record its SHA-256 digest before starting the clock.

## Validate before startup

Run the fail-closed preflight:

```bash
bash scripts/v2_4_0_single_node_preflight.sh single-node.env
```

For evidence collection:

```bash
OUT_DIR=evidence/single-node-preflight \
  bash scripts/v2_4_0_single_node_preflight.sh single-node.env
```

A valid manifest reports:

- `operator_mode=single-node`;
- `p2p_enabled=false`;
- `connected_peers_expected=false`;
- `isolated_mining_authorized=true`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

Do not start the node when the preflight returns `FAIL`.

## Expected runtime behavior

Startup and status surfaces must identify intentional isolation, active operator mode, P2P policy, peer expectation, RPC bind policy, network/chain identifiers and isolated-mining authorization.

The topology-aware mining-template guard is implemented:

- an explicit single-node profile may receive mining templates with `peer_count=0`;
- an ordinary private node unexpectedly isolated with zero peers remains fail-closed;
- seed role or empty bootnodes alone do not bypass the guard;
- degraded sync, missing-parent recovery and orphan-recovery conditions continue to block mining in every profile.

The official metrics inventory uses `/mempool`; route/inventory contract checks must remain green and exporter health must be verified against the candidate binary.

## Mining-submit finality

The external miner must follow [`../V2_4_0_MINING_SUBMIT_FINALITY.md`](../V2_4_0_MINING_SUBMIT_FINALITY.md).

Operationally:

- `submit_timeout_before_acceptance` is definitive non-acceptance;
- `submit_finality_unknown` is not a rejection;
- the miner reconciles the submitted block hash through the node lookup surface;
- a matching stored block is recorded as reconciled acceptance;
- `NOT_FOUND` alone is not definitive rejection during the bounded reconciliation window;
- unresolved finality is recorded separately, followed by fresh work;
- the same submitted hash is never blindly resubmitted.

Any unresolved or growing unknown-finality count during burn-in must be preserved in evidence and investigated before GO.

## Minimum evidence for the #789 single-node phase

Before the 24-hour clock starts, record:

- exact unchanged source SHA;
- successful final pre-burn-in workflow and artifact digest;
- clean database/network initialization;
- sanitized fixed configuration and digest;
- node, miner, exporter and monitoring process/container inventory;
- image and binary digests;
- UTC start timestamp entered by the operator.

During the run, retain:

- beginning, periodic, pre-restart, post-restart, pre-prune, post-prune and final snapshots;
- accepted, rejected, finality-unknown and reconciled submit counters;
- observed block intervals, current/suggested bits and target movement;
- memory, disk, RPC latency, lock contention, snapshot age and active-alert telemetry;
- node, miner and exporter logs;
- snapshot verification, retained-set, restore and incident reports.

Do not combine evidence from different SHAs. A repository commit, consensus/configuration change, unexplained restart or invalidating database reset restarts the clock.

## Transition to ordinary private multi-host operation

1. Stop the isolated node cleanly.
2. Preserve backups and the required evidence bundle.
3. Set `PULSEDAG_SINGLE_NODE_MODE=false` or remove it.
4. Replace `PULSEDAG_PRIVATE_TESTNET_ROLE=single` with a valid `seed` or `node` role.
5. Re-enable real P2P and configure persistent identity, listen, advertisement and bootnode settings.
6. Run the ordinary private-testnet preflight:

```bash
bash scripts/v2_3_0_private_testnet_preflight.sh <private-env-file>
```

7. Verify that the ordinary zero-peer mining protection is active again.
8. Reuse storage only when the documented chain identity, snapshot and migration rules explicitly permit it.
9. Add the independent node required by #789 and complete real P2P sync, offline/rejoin and consensus-value comparison.

## Prohibited shortcuts

- Do not infer single-node mode from an empty bootnode list.
- Do not use `role=seed` as an isolation bypass.
- Do not expose admin/operator RPC publicly.
- Do not advertise a public P2P address during private burn-in.
- Do not start or backdate readiness clocks.
- Do not count unknown finality as rejection or acceptance before reconciliation.
- Do not combine evidence from intermediate candidate SHAs.
- Do not commit private keys, wallets, tokens, generated data or operator-specific environment files.
