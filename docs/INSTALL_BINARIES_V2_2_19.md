# Install binaries v2.2.19

## Verify checksums

### Linux (bash)
```bash
sha256sum -c pulsedagd-v2.2.19-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c pulsedag-miner-v2.2.19-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

### macOS
```bash
shasum -a 256 -c pulsedagd-v2.2.19-x86_64-apple-darwin.tar.gz.sha256
shasum -a 256 -c pulsedag-miner-v2.2.19-x86_64-apple-darwin.tar.gz.sha256
```

### Windows (PowerShell)
```powershell
Get-FileHash .\pulsedagd-v2.2.19-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Get-FileHash .\pulsedag-miner-v2.2.19-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

## Verify install from archive
```bash
scripts/release/verify_install_from_archive.sh --archive pulsedagd-v2.2.19-x86_64-unknown-linux-gnu.tar.gz --timeout-secs 10
scripts/release/verify_install_from_archive.sh --archive pulsedag-miner-v2.2.19-x86_64-unknown-linux-gnu.tar.gz --timeout-secs 10
```

## Included release assets
Release bundles include both binaries:
- `pulsedagd` (node)
- `pulsedag-miner` (standalone miner)

## Troubleshooting release-binaries.yml
- **Cargo.lock mismatch**: if `cargo metadata --locked --format-version 1` fails, update and commit `Cargo.lock` before rerunning.
- **Missing manifest**: each packaged archive must have a matching `.json` manifest; rerun packaging and ensure manifests are uploaded.
- **Duplicate asset filename**: matrix artifacts must not produce colliding filenames across targets; fix naming collisions before publish.
- **GitHub release permission failure**: verify workflow `contents: write` permission and that `GH_TOKEN` can create/upload release assets.
