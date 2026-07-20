# Install binaries v2.3.0

> Candidate documentation. Use only for an explicitly approved and published `v2.3.0` release whose asset checksums and manifests match the downloaded files.

## Verify checksums

### Linux (bash)

```bash
sha256sum -c pulsedagd-v2.3.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c pulsedag-miner-v2.3.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

### macOS

```bash
shasum -a 256 -c pulsedagd-v2.3.0-x86_64-apple-darwin.tar.gz.sha256
shasum -a 256 -c pulsedag-miner-v2.3.0-x86_64-apple-darwin.tar.gz.sha256
```

### Windows (PowerShell)

```powershell
Get-FileHash .\pulsedagd-v2.3.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Get-FileHash .\pulsedag-miner-v2.3.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Compare the displayed Windows hashes with the matching `.sha256` files or `SHA256SUMS.txt`. Do not run an archive whose checksum does not match exactly.

## Verify install from archive

```bash
scripts/release/verify_install_from_archive.sh \
  --archive pulsedagd-v2.3.0-x86_64-unknown-linux-gnu.tar.gz \
  --timeout-secs 10
scripts/release/verify_install_from_archive.sh \
  --archive pulsedag-miner-v2.3.0-x86_64-unknown-linux-gnu.tar.gz \
  --timeout-secs 10
```

## Included release assets

Each supported target publishes separate archives for:

- `pulsedagd` — PulseDAG node;
- `pulsedag-miner` — standalone external miner.

Supported release workflow targets:

- `x86_64-unknown-linux-gnu`;
- `x86_64-pc-windows-msvc`;
- `x86_64-apple-darwin`.

Each archive must have:

- a matching `.sha256` file;
- a matching `.json` build manifest;
- GitHub build-provenance attestation;
- a successful unpack-and-smoke verification from the release workflow.

## Private-testnet configuration note

v2.3.0 private-testnet ordinary nodes require complete libp2p bootnode addresses containing the seed peer ID:

```text
/ip4/<seed-address>/tcp/<port>/p2p/<seed-peer-id>
```

RPC remains loopback-only by default. Mining remains external to the node.

## Rollback

Use the Task 09 lifecycle controller and retain the prior release as the `previous` release before upgrade. A failed health check must restore the prior binary automatically. Persistent identity and RocksDB directories must not be deleted during a binary rollback.

## Troubleshooting `release-binaries.yml`

- **Cargo.lock mismatch**: if `cargo metadata --locked --format-version 1` fails, update and commit `Cargo.lock` before rerunning.
- **Missing manifest**: each packaged archive must have a matching `.json` manifest.
- **Checksum failure**: discard the affected archive; do not bypass verification.
- **Duplicate asset filename**: matrix artifacts must not produce colliding filenames across targets.
- **Smoke failure**: do not publish the release; inspect the unpacked binary and manifest evidence.
- **GitHub release permission failure**: verify workflow `contents: write` permission and release token scope.

## Guardrails

Installing or documenting v2.3.0 does not authorize public-testnet launch. `public_testnet_ready=false` and the unstarted 30-day public-testnet clock remain unchanged unless a later, separate launch decision explicitly changes them.
