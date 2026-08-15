# Release artifacts and checksums (v2.4.0)

## Scope guardrails

This guide is limited to release engineering and operator packaging workflow.

- No consensus behavior is changed by packaging.
- Miner remains external and standalone.
- Node release builds are keyless and do not provide raw-private-key wallet RPC custody.
- No pool logic is introduced.
- Packaging/publication does not authorize public-testnet launch.

## Cargo.lock policy for CI and release builds

`Cargo.lock` is a committed release input and must be synchronized with workspace manifests. Release workflows fail fast with `cargo metadata --locked` and build with `--locked` so dependency resolution cannot drift silently.

For the v2.4.0 version bump, update only the local PulseDAG workspace package versions required by the workspace version change. Do not combine the mechanical version bump with unrelated dependency upgrades.

### Intentional dependency change procedure

If dependency resolution must change:

1. update manifests intentionally;
2. regenerate/update the lockfile;
3. commit the lockfile diff with the dependency change;
4. rerun the full dependency/RustSec and exact-SHA release validation matrix.

## Asset naming convention

The `release-binaries` workflow publishes two standalone binary families per target:

- Node: `pulsedagd-<tag>-<target>.tar.gz` (Linux/macOS) or `.zip` (Windows).
- External miner: `pulsedag-miner-<tag>-<target>.tar.gz` (Linux/macOS) or `.zip` (Windows).

v2.4.0 examples:

- `pulsedagd-v2.4.0-x86_64-unknown-linux-gnu.tar.gz`;
- `pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz`;
- `pulsedagd-v2.4.0-x86_64-pc-windows-msvc.zip`;
- `pulsedag-miner-v2.4.0-x86_64-pc-windows-msvc.zip`;
- `pulsedagd-v2.4.0-x86_64-apple-darwin.tar.gz`;
- `pulsedag-miner-v2.4.0-x86_64-apple-darwin.tar.gz`.

Each archive contains a single top-level folder matching the archive stem, with the expected binary inside. `pulsedag-miner` remains external and standalone.

If an official `pulsedag-wallet` executable is later added to the v2.4.0 publication set, extend the release workflow and allowlist explicitly; do not smuggle it into an existing node/miner archive.

## Checksum and provenance outputs

For every archive the workflow emits:

- `<archive>.sha256`;
- `<archive>.json` build manifest;
- GitHub build-provenance attestation;
- consolidated `SHA256SUMS.txt`;
- consolidated `release-provenance.json`;
- generated `INSTALL-VERIFY.md`.

Per-archive manifests include archive digest/size plus repository, commit and workflow-run provenance. The publish job re-verifies the downloaded artifact set before release creation/upload.

## CI end-to-end verification flow

### Build job

- validate `Cargo.lock` in locked mode;
- verify standalone miner crate build surface;
- build node and miner release binaries;
- package each binary with the exact requested tag/target identity;
- verify checksum and manifest metadata;
- unpack and smoke-test the standalone miner;
- unpack and smoke-test the node/miner asset set;
- attest release archives;
- upload the native artifact bundle.

### Publish job

- download and flatten matrix artifacts while rejecting duplicate filenames;
- verify every archive/checksum/manifest pair;
- build and verify `SHA256SUMS.txt` and `release-provenance.json`;
- generate `INSTALL-VERIFY.md`;
- verify the final allowlisted bundle structure;
- create/upload the GitHub Release only after all validation steps pass.

## Operator verification before upgrade

From a release download directory:

```bash
sha256sum -c pulsedagd-v2.4.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

Optional provenance spot-check:

```bash
jq '.artifacts[] | {archive, archive_sha256, provenance}' release-provenance.json
```

Then follow `INSTALL-VERIFY.md` and [`../INSTALL_BINARIES_V2_4_0.md`](../INSTALL_BINARIES_V2_4_0.md). Do not deploy an archive with a checksum/provenance mismatch.

## Repeatable standalone operator smoke flow

For a practical local node + external-miner smoke:

```bash
scripts/release/standalone_operator_smoke.sh --miner-address YOUR_ADDRESS
```

This confirms the standalone binary surfaces, starts a local node, waits for status/health, and runs a bounded external-miner template/search/submit probe. It does not introduce pool behavior or public launch authorization.

## Rollback packaging guidance

Keep the previously known-good archive, checksum and provenance material in the staging/evidence store. If rollback is required, verify the old asset again before redeploying it. Preserve persistent identity and storage according to the approved rollback/recovery procedure.

## Release boundary

The v2.4.0 tag and GitHub Release remain blocked until the exact versioned candidate is approved. Publication still does not set `GO_PUBLIC_TESTNET`, Day 0, the 30-day clock, contracts, or production/mainnet custody state.
