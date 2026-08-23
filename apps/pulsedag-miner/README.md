# pulsedag-miner v2.4.0

`pulsedag-miner` is the official external standalone miner for PulseDAG.

Canonical references:

- [`docs/POW_SPEC_FINAL.md`](../../docs/POW_SPEC_FINAL.md)
- [`docs/POW_CURRENT_PATH.md`](../../docs/POW_CURRENT_PATH.md)
- [`docs/DIFFICULTY_RETARGET_V2_4_0.md`](../../docs/DIFFICULTY_RETARGET_V2_4_0.md)

The final v2.4.0 packaged-binary installation guide and artifact digests are frozen only after Task31 exact-candidate validation.

## Scope

The miner performs three operations:

1. request a mining template from a node;
2. solve PoW outside the node;
3. submit the solved block to the node.

It does not implement pool logic, shares, payouts, accounting, or server-side pool coordination.

## Run from source

```bash
cargo run --locked -p pulsedag-miner -- \
  --node http://127.0.0.1:8080 \
  --miner-address YOUR_ADDRESS \
  --threads 4 \
  --max-tries 500000 \
  --loop \
  --sleep-ms 1000 \
  --refresh-before-expiry-ms 1000
```

## Run a release binary

Final v2.4.0 archives will use the pattern:

`pulsedag-miner-v2.4.0-<target>.*`

Example after a candidate is explicitly approved and packaged:

```bash
tar -xzf pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu.tar.gz
./pulsedag-miner-v2.4.0-x86_64-unknown-linux-gnu/pulsedag-miner --help
```

Each approved archive requires a `.sha256` checksum and provenance manifest. The release binary runs independently of the source tree and does not require Cargo.

## Supported flags

- `--node`
- `--miner-address`
- `--max-tries`
- `--loop`
- `--sleep-ms`
- `--threads`
- `--refresh-before-expiry-ms`
- `--backend`
- `--gpu-device`
- `--heartbeat` / `--no-heartbeat`
- `--worker-id`

## CPU and GPU backends

The CPU backend is the default and the canonical operational reference.

The optional GPU backend is feature-gated:

```bash
cargo build --locked -p pulsedag-miner --release --features gpu
./target/release/pulsedag-miner \
  --node http://127.0.0.1:8080 \
  --miner-address YOUR_ADDRESS \
  --backend gpu \
  --loop
```

Every nonce produced by a GPU backend must be verified through the canonical CPU/core PoW path before submission. GPU support remains optional and does not change consensus or add pool functionality. See [`GPU.md`](GPU.md).

## Deterministic multi-thread search

Worker `t` searches the strided nonce sequence:

`nonce = t, t + T, t + 2T, ...`

where `T` is the effective thread count. This limits obvious overlap and preserves a reproducible search schedule.

## Operator smoke

```bash
scripts/release/standalone_operator_smoke.sh --miner-address YOUR_ADDRESS
```

This validates packaged standalone artifacts and a short node-plus-external-miner flow when run against the exact candidate package set.

## Release boundary

The repository version is `v2.4.0`, but Task31 remains `PENDING_EXACT_CANDIDATE_EVIDENCE`. Tag creation and GitHub Release publication are not authorized, and this guide does not authorize a public-testnet launch, Day 0, default high cadence or smart contracts.
