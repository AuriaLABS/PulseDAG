# Release artifacts and checksums (v2.2.4)

## Scope guardrails
This guide is limited to release engineering and operator packaging workflow.

- No consensus behavior changes.
- Miner remains external and standalone.
- No pool logic is introduced.

## Cargo.lock policy for CI and release builds
Release engineering now uses an explicit lockfile policy:

- `Cargo.lock` is a committed release input and must be up to date with workspace manifests.
- CI/release workflows run an early fail-fast lock validation (`cargo metadata --locked`) before build/test packaging work.
- Release builds run with `cargo build --locked` to prevent silent lockfile mutation.

This keeps release dependency resolution deterministic and reproducible across reruns and platforms.

### Intentional dependency change procedure
If dependency resolution must change, do it as a deliberate source change:

1. Update manifests as needed.
2. Regenerate/update lockfile (`cargo generate-lockfile` or targeted `cargo update`).
3. Commit the `Cargo.lock` diff in the same PR.
4. Let CI validate the updated lockfile in locked mode.

Follow-up policy decision (if needed): whether to also enforce `--locked` in all non-release build/test jobs. Current policy enforces fail-fast lock drift checks broadly and strict locked mode in release builds.

## Asset naming convention
The `release-binaries` workflow publishes two standalone binary families per target:

- Node: `pulsedagd-<tag>-<target>.tar.gz` (Linux/macOS) or `.zip` (Windows)
- External miner: `pulsedag-miner-<tag>-<target>.tar.gz` (Linux/macOS) or `.zip` (Windows)

Examples:
- `pulsedagd-v2.2.4-x86_64-unknown-linux-gnu.tar.gz`
- `pulsedag-miner-v2.2.4-x86_64-unknown-linux-gnu.tar.gz`
- `pulsedagd-v2.2.4-x86_64-pc-windows-msvc.zip`
- `pulsedag-miner-v2.2.4-x86_64-pc-windows-msvc.zip`

Each archive contains a single top-level folder matching the archive stem, with exactly one binary inside (`pulsedagd` or `pulsedag-miner`).

`pulsedag-miner` remains external and standalone; release packaging does not introduce any pool behavior or pool-facing interfaces.

## Checksum outputs
For every archive the workflow emits:

- Per-asset checksum sidecar: `<archive>.sha256`
- Per-asset manifest metadata: `<archive>.json`
- Consolidated checksum list across all archives: `SHA256SUMS.txt`
- Consolidated provenance summary: `release-provenance.json`

In addition, each platform archive is attested in GitHub artifact attestations using the release workflow identity (OIDC-backed provenance).
The workflow now performs end-to-end verification in both jobs: it validates every archive, checksum sidecar, and manifest; unpacks each archive; and runs a basic smoke command on the unpacked `pulsedagd` and `pulsedag-miner` binaries before publish.

Per-archive JSON manifests now include:
- `archive_sha256`
- `archive_size_bytes`
- `provenance.repository`
- `provenance.commit`
- `provenance.github_run_id`
- `provenance.github_run_attempt`

## CI end-to-end verification flow
`release-binaries` validates packaged assets twice:

1. **Build job (`dist/`)**
   - Verifies `<archive>.sha256` matches archive bytes.
   - Verifies `<archive>.json` metadata (`archive`, digest, size, tag, provenance).
   - Unpacks each archive and checks the expected single-binary layout.
   - Runs a binary smoke check:
     - `pulsedagd --version`
     - `pulsedag-miner --help`

2. **Publish job (`final/`)**
   - Re-verifies per-archive checksums and manifests after artifact download/flattening.
   - Builds and validates `SHA256SUMS.txt`.
   - Builds and validates `release-provenance.json` against all per-archive manifests.
   - Repeats unpack + smoke checks for node and miner release assets.

## Operator verification before upgrade
From a release download directory:

```bash
sha256sum -c pulsedagd-v2.2.4-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c pulsedag-miner-v2.2.4-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c SHA256SUMS.txt --ignore-missing
```

Optional provenance spot-check:

```bash
jq '.artifacts[] | {archive, archive_sha256, provenance}' release-provenance.json
```

Then unpack and stage:

```bash
tar -xzf pulsedagd-v2.2.4-x86_64-unknown-linux-gnu.tar.gz
./pulsedagd-v2.2.4-x86_64-unknown-linux-gnu/pulsedagd --version

tar -xzf pulsedag-miner-v2.2.4-x86_64-unknown-linux-gnu.tar.gz
./pulsedag-miner-v2.2.4-x86_64-unknown-linux-gnu/pulsedag-miner --help
```

## Rollback packaging guidance
Keep the previously known-good archive and its `.sha256` file in the same artifact store used for staging evidence.
If rollback is required, verify checksum again before redeploying the old binary.
