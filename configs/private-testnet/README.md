# Transitional development fixtures

This directory contains retained prototype configuration inputs for existing isolated development and CI harnesses. It is not an operator entrypoint and does not authorize a separate private live network. The first deployed network is the native-v3 public testnet.

Use [public-testnet preparation templates](../public-testnet/README.md) for first-network planning. They remain fail-closed pending the approved network identity and separate GO decision.

Retained fixtures must not be used as public configuration or evidence of native monetary acceptance. Preserve isolation, bounded RPC, validated bootnode identities and exact-candidate evidence in CI. Remove these fixtures after native test replacements are validated.

The old v2.2.18 and v2.2.19 topology directories have been retired. Earlier history is recoverable from Git.

Seed/node prototype inputs now live only in `scripts/tests/fixtures/prototype_bootstrap/` for parser regression tests. The old miner environment example and private bootstrap workflow are retired. Use the public-testnet PRE-GO templates for current preparation; never deploy the test fixtures.
