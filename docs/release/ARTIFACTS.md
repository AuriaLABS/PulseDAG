# Release artifacts and checksums (v2.2)

## Scope guardrails
This guide is limited to release engineering and operator packaging workflow.

- No consensus behavior changes.
- Miner remains external and standalone.
- No pool logic is introduced.

## Asset naming convention
The `release-binaries` workflow publishes archives named:

- `pulsedagd-<tag>-<target>.tar.gz` (Linux/macOS)
- `pulsedagd-<tag>-<target>.zip` (Windows)

Examples:
- `pulsedagd-v2.2.2-x86_64-unknown-linux-gnu.tar.gz`
- `pulsedagd-v2.2.2-x86_64-pc-windows-msvc.zip`

Each archive contains a single top-level folder matching the archive stem, with the `pulsedagd` binary inside.

## Checksum outputs
For every archive the workflow emits:

- Per-asset checksum sidecar: `<archive>.sha256`
- Per-asset manifest metadata: `<archive>.json`
- Consolidated checksum list across all archives: `SHA256SUMS.txt`

## Operator verification before upgrade
From a release download directory:

```bash
sha256sum -c pulsedagd-v2.2.2-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

Then unpack and stage:

```bash
tar -xzf pulsedagd-v2.2.2-x86_64-unknown-linux-gnu.tar.gz
./pulsedagd-v2.2.2-x86_64-unknown-linux-gnu/pulsedagd --version
```

## Rollback packaging guidance
Keep the previously known-good archive and its `.sha256` file in the same artifact store used for staging evidence.
If rollback is required, verify checksum again before redeploying the old binary.
