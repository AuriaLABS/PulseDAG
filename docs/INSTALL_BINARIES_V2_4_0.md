# Install binaries v2.4.0

> Release-candidate documentation. Use these instructions only for an explicitly approved and published `v2.4.0` release whose archive checksums, manifests and provenance match the downloaded files.

## Verify checksums

### Linux (bash)

```bash
sha256sum -c pulsedagd-v2.4.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

### macOS

```bash
shasum -a 256 -c pulsedagd-v2.4.0-x86_64-apple-darwin.tar.gz.sha256
shasum -a 256 -c pulsedag-miner-v2.4.0-x86_64-apple-darwin.tar.gz.sha256
```

### Windows (PowerShell)

```powershell
Get-FileHash .\pulsedagd-v2.4.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Get-FileHash .\pulsedag-miner-v2.4.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Compare Windows output with the matching `.sha256` sidecars or `SHA256SUMS.txt`. Do not run an archive whose checksum does not match exactly.

## Verify install from archive

```bash
scripts/release/verify_install_from_archive.sh \
  --archive pulsedagd-v2.4.0-x86_64-unknown-linux-gnu.tar.gz \
  --timeout-secs 10
scripts/release/verify_install_from_archive.sh \
  --archive pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz \
  --timeout-secs 10
```

## Included release assets

The release workflow publishes separate archives for:

- `pulsedagd` — keyless PulseDAG node;
- `pulsedag-miner` — standalone external miner.

Current native release workflow targets:

- `x86_64-unknown-linux-gnu`;
- `x86_64-pc-windows-msvc`;
- `x86_64-apple-darwin`.

Each archive must have:

- a matching `.sha256` file;
- a matching `.json` build manifest;
- GitHub build-provenance attestation;
- a successful native unpack-and-smoke verification.

If an official `pulsedag-wallet` binary is added to the v2.4.0 publication set, it must have its own checksum/provenance/install and restore/sign/broadcast verification evidence. Do not infer wallet release readiness from node/miner assets.

## Network and RPC note

The final public-testnet identity, bootnodes, endpoints and configuration digests must come from the approved launch record. Do not reuse private burn-in identities as public launch configuration unless the final release process explicitly records them.

Operator/admin RPC must remain on loopback or a private management interface. A public-safe listener exposes only the explicitly allowed public surface, including the canonical signed-transaction relay where enabled. Wallet private keys, mnemonics, seeds and passwords must never be sent to the node.

## Rollback

Retain the previously known-good release and its checksums before upgrade. A failed health, identity, storage, synchronization or release verification must stop the rollout and follow the approved rollback procedure. Persistent P2P identity and RocksDB state must not be deleted merely to make a binary rollback appear successful.

## Troubleshooting `release-binaries.yml`

- **Cargo.lock mismatch**: if `cargo metadata --locked --format-version 1` fails, regenerate/update and commit `Cargo.lock` with the manifest version change before rerunning.
- **Missing manifest**: every packaged archive must have a matching `.json` manifest.
- **Checksum failure**: discard the affected archive; never bypass verification.
- **Duplicate asset filename**: matrix artifacts must not collide across targets.
- **Smoke failure**: do not publish; inspect the unpacked binary, manifest and native-runner evidence.
- **GitHub release permission failure**: verify workflow permissions before retrying; do not substitute an untracked manual artifact.

## Guardrails

Installing or documenting v2.4.0 does not authorize public-testnet launch. Until the separate launch decision and actual public launch are recorded, `public_testnet_ready=false`, `thirty_day_public_testnet_clock_started=false`, and `contracts_enabled=false` remain unchanged.
