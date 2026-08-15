# PulseDAG private-testnet configurations

## Current v2.4.0 private identity

All active v2.4.0 private burn-in and rehearsal profiles use exactly:

- `PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0`
- `PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0`

The chain ID namespaces P2P topics and messages. Do not reuse a v2.3.0 database, P2P identity directory, rendered configuration or evidence bundle.

Before using a configuration:

1. verify the exact candidate SHA;
2. initialize a clean database and private network;
3. confirm every participating node uses the exact v2.4.0 chain ID;
4. validate complete bootnode addresses, including `/p2p/<peer-id>`;
5. bind RPC according to the intended operator profile;
6. capture the rendered configuration and binary/image digests in the evidence bundle.

The single-node profile intentionally disables P2P. Phase C of the private burn-in must use the seed/node examples with new persistent peer identities and the same v2.4.0 chain ID.

See [`../../docs/V2_4_0_PRIVATE_BURN_IN_OPERATOR_PROFILE.md`](../../docs/V2_4_0_PRIVATE_BURN_IN_OPERATOR_PROFILE.md).

## Historical configurations

Directories, scripts and documents explicitly named `v2_2_*` or `v2_3_0_*` are retained only to reproduce historical evidence. They are not current v2.4.0 defaults and must not be copied into the replacement burn-in.

Their classification is documented in [`LEGACY_COMPATIBILITY_V2_3_0.md`](LEGACY_COMPATIBILITY_V2_3_0.md).

## Guardrails

A private-testnet configuration does not authorize a public-testnet launch, does not set `public_testnet_ready=true`, and does not start the 30-day public-testnet clock.
