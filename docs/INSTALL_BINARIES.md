# Installing PulseDAG binaries

Use only artifacts from an explicitly authorized release. The first public network is native v3 testnet; no prior network migration is required. Packaging does not authorize a network launch.

## Verify before running

Download the matching platform archive, SHA-256 checksum, JSON build manifest and provenance attestation from the same release. Compare the exact archive filename and digest. On Linux use `sha256sum -c <archive>.sha256`; on macOS use `shasum -a 256 -c <archive>.sha256`; on Windows use `Get-FileHash -Algorithm SHA256 -Path <archive>` and compare with the published checksum. Reject mismatches, missing manifests or unverified provenance. Verify GitHub attestations with `gh attestation verify <archive> --repo AuriaLABS/PulseDAG`.

Extract to a fresh directory and check the binary version against the approved release. The release workflow builds node and standalone miner archives for Linux, Windows and macOS x86_64 and runs native archive smoke checks. GPU build features are not evidence of physical GPU acceptance. CPU is the required mining path.

## Network setup

Follow the [testnet-first launch runbook](https://github.com/AuriaLABS/PulseDAG/blob/main/docs/runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md) and [launch control #781](https://github.com/AuriaLABS/PulseDAG/issues/781). Do not start monetary mining before the separate testnet GO. Use only approved testnet-specific genesis, chain ID, signing domain, bootnodes, wallet and storage directories. Keep admin RPC private; ordinary RPC defaults to loopback unless a reviewed public-safe configuration is supplied. Contracts remain INACTIVE. Testnet coins have no monetary value and the network may reset.

## Operations

Retain artifact hashes, logs and manifests. Do not bypass checksum, locked dependency, smoke or launch guards. A failed installation or health check blocks deployment; use the authorized rollback procedure, preserving keys and storage. Do not assume rollback between incompatible testnet candidates is safe: follow the published reset/upgrade policy.
