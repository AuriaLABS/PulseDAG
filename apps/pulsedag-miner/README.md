# pulsedag-miner v2.4.0

`pulsedag-miner` is the standalone external miner for PulseDAG v2.4.0. Mining remains outside `pulsedagd`; pool logic, shares, payouts and accounting are not node responsibilities.

## Build

```bash
cargo build --locked --release -p pulsedag-miner
```

## Run

```bash
target/release/pulsedag-miner \
  --node http://127.0.0.1:8080 \
  --miner-address YOUR_ADDRESS \
  --threads 4 \
  --loop \
  --sleep-ms 1500 \
  --max-tries 50000
```

Use a node endpoint/profile that explicitly permits mining operations. The public-safe listener is not an operator/mining control plane.

The miner distinguishes ordinary nonce-search exhaustion from canonical backend verification failure and treats unknown submit finality as non-final pending bounded reconciliation. Do not count unresolved finality as a definitive node rejection and do not blindly resubmit stale work.

## Release artifacts

The canonical installation and checksum instructions are in [`docs/INSTALL_BINARIES_V2_4_0.md`](../../docs/INSTALL_BINARIES_V2_4_0.md).

Expected release archive names include the version and target triple, for example:

- `pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz`;
- `pulsedag-miner-v2.4.0-x86_64-pc-windows-msvc.zip`;
- `pulsedag-miner-v2.4.0-x86_64-apple-darwin.tar.gz`.

Every published archive must have the matching SHA-256 file and build manifest/provenance required by the release workflow.

## Release boundary

The v2.4.0 tag and GitHub Release are authorized once the exact software-release candidate passes the required release gates. Publication does not authorize public-testnet launch, Day 0, the 30-day public-testnet clock, contracts, or production/mainnet custody.
