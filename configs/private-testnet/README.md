# PulseDAG private-testnet configurations

## Current configuration policy

Current v2.3.0 workflows and runbooks use v2.3.0 or version-neutral configuration paths that they reference explicitly.

Before using a configuration:

1. verify the exact candidate SHA;
2. confirm chain and network isolation;
3. validate complete bootnode addresses, including `/p2p/<peer-id>`;
4. bind RPC according to the intended operator profile;
5. capture the rendered configuration in the evidence bundle.

## Historical configurations

Directories named `v2_2_*` are retained only to reproduce historical evidence. They are not current defaults.

Their classification is documented in [`LEGACY_COMPATIBILITY_V2_3_0.md`](LEGACY_COMPATIBILITY_V2_3_0.md).

## Guardrails

A private-testnet configuration does not authorize a public-testnet launch, does not set `public_testnet_ready=true`, and does not start the 30-day public-testnet clock.
