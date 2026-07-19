# v2.3.0 Task 07 — Private-testnet bootstrap contract

## Objective

Turn the existing `private` node profile into a reproducible multi-host bootstrap contract without changing consensus, PoW, release version, or public-testnet state.

## Deliverables

1. Seed and ordinary-node environment templates under `configs/private-testnet/`.
2. A fail-closed preflight at `scripts/v2_3_0_private_testnet_preflight.sh`.
3. A contract regression covering valid templates and unsafe/mismatched configurations.
4. An active `docs/ROADMAP_V2_3_0.md` describing the remaining PR sequence.

## Required invariants

- `PULSEDAG_CONFIG_PROFILE=private`.
- Stable network and chain identifiers: `private-testnet-v2.3.0` and `pulsedag-private-v2.3.0`.
- `libp2p-real`, Kademlia enabled, mDNS disabled.
- Persistent, absolute, non-temporary identity and RocksDB paths.
- Every ordinary node has at least one valid TCP bootnode and does not bootstrap to its own advertised address.
- Seed nodes advertise a valid public P2P multiaddr and may start without a bootnode.
- RPC remains loopback-only in this task; remote operator access belongs in a later security-reviewed PR.
- Admin endpoints remain disabled by default; enabling them requires a token of at least 16 characters.
- Snapshot-gated pruning remains enabled.
- `public_testnet_ready=false` and the public-testnet clock remains not started.

## Validation

```bash
bash -n scripts/v2_3_0_private_testnet_preflight.sh
bash -n scripts/tests/test_v2_3_0_private_testnet_preflight.sh
bash scripts/tests/test_v2_3_0_private_testnet_preflight.sh
```

Normal repository lint, workspace, RPC/release, and pre-burn-in checks remain mandatory in GitHub Actions.

## Out of scope

- Starting persistent services.
- Firewall automation.
- Remote RPC exposure.
- DNS or cloud provisioning.
- Version bump or release tag.
- Public-testnet launch or burn-in clock.
