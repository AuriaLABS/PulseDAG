# v3.0.0 coordinated dual-network launch

Execute this runbook only after the exact-candidate evidence ledger and
`docs/V3_0_0_LAUNCH_MANIFEST.md` validate with `GO_V3_DUAL_LAUNCH`.

1. Verify exact source/tree, artifact, protocol, monetary-policy, config, and
   genesis identities on every host.
2. Start mainnet seed nodes, then parallel-testnet seed nodes with independent
   persistent P2P identities.
3. Start ordinary/observer roles and prove chain IDs, signing domains, genesis
   hashes, and peer identities cannot cross networks.
4. Start approved CPU/NVIDIA/AMD miners and verify independent production,
   state convergence, and submit reconciliation.
5. Bring up bounded public RPC/status/event surfaces with approved TLS.
6. Verify wallet transfer, asset, contract, event, proof, and reconcile flows
   against the correct network.
7. Record first accepted mainnet and testnet blocks, heights, and UTC times.
8. Publish endpoints, bootnodes, checksums, tooling, limitations, and incident
   and security reporting routes.

The first-block times may differ by operational minutes but belong to one
release window. There is no preceding public-testnet launch or clock.
