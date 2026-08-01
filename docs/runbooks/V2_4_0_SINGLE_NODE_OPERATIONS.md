# v2.4.0 single-node operations

## Scope

This runbook covers the explicit single-node operator profile introduced by v2.4.0 Task 14. It is intended for local development, deterministic burn-in, and operator validation on an intentionally isolated node.

It does not authorize public-testnet launch, start the 30-day public-testnet clock, enable smart contracts, or replace the ordinary private multi-host topology.

The currently approved software version and private-chain identity remain v2.3.0 until a separate release decision authorizes a version change.

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

An empty bootnode list or `role=seed` does not activate this mode.

## Prepare configuration

Copy the reference configuration and change only deployment-specific persistent paths:

```bash
cp configs/single-node/single-node.env.example single-node.env
```

Do not commit the resulting operator file when it contains host-specific paths, credentials, wallet material, or runtime data.

## Validate before startup

Run the fail-closed preflight:

```bash
bash scripts/v2_4_0_single_node_preflight.sh single-node.env
```

For evidence collection:

```bash
OUT_DIR=evidence/task14 \
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

## Expected runtime identity

The node startup and status surfaces must make intentional isolation unambiguous. Task 14 requires the runtime configuration layer to expose the active operator mode, P2P policy, peer expectation, RPC bind policy, network and chain identifiers, and whether isolated mining is authorized.

Task 15 owns the mining-template guard change. Until Task 15 is implemented, passing this preflight alone does not guarantee that the unmodified node binary will issue mining templates with zero peers.

## Transition to ordinary private multi-host operation

1. Stop the isolated node cleanly.
2. Preserve required backup and evidence artifacts.
3. Set `PULSEDAG_SINGLE_NODE_MODE=false` or remove it.
4. Replace `PULSEDAG_PRIVATE_TESTNET_ROLE=single` with a valid `seed` or `node` role.
5. Re-enable real P2P and configure persistent identity, listen, advertisement, and bootnode settings for the selected role.
6. Run the ordinary private-testnet preflight:

```bash
bash scripts/v2_3_0_private_testnet_preflight.sh <private-env-file>
```

7. Verify that the ordinary zero-peer mining protection is active again.
8. Reuse storage only when the documented chain identity and migration rules explicitly permit it.

## Prohibited shortcuts

- Do not infer single-node mode from an empty bootnode list.
- Do not use `role=seed` as an isolation bypass.
- Do not expose RPC publicly.
- Do not advertise a public P2P address.
- Do not enable public-testnet readiness or backdate its clock.
- Do not commit private keys, wallets, tokens, generated data, or operator-specific environment files.
