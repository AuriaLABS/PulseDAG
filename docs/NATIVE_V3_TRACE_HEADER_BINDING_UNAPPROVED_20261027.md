# Native-v3 first-seen trace: bounded full-PoW header binding (UNAPPROVED)

**2026-10-27 — Test-only tooling.** [PR #1341](https://github.com/AuriaLABS/PulseDAG/pull/1341) remains DRAFT, unmerged. The trial K, parent cap, monetary consensus, native-v3 P2P admission, mining and mainnet remain **unapproved/inactive**. Smart contracts remain **INACTIVE** for v3.0.

## What the two read-only tools establish

- `scripts/native_v3_propagation_evidence.py` validates manifest format, independent exact-candidate SHA/network/genesis pins, distinct claimed hosts/active miners, calibration field ranges, 120+ declared mined block IDs, 60+ seconds, recorded peer receive times, right-censored missingness and distribution summaries. Its declared identities, event clocks, miners and locations are **not authenticated** merely by parsing.
- `scripts/native_v3_trace_header_binding.py` is a stricter separate **offline pilot** tool. It runs the above validator and requires a second complete, genesis-rooted set of canonical native-v3 header records. Against independently supplied **chain ID, genesis SHA256 and genesis timestamp**, it independently checks each claimed block ID against SHA256 of the full chain-domain-separated canonical `BlockHeaderV3` bytes, every parent hash, strictly causal height/timestamp, canonical compact target and actual **full 256-bit Python KHeavyHash PoW**. No header can be silently pruned. The claimed `blue_score` is **not authoritative**: it is merely covered by the header hash and is **not** verified against independently approved native GHOSTDAG policy. Native tx bodies, fees, UTXO and rewards are not checked.

Both read inputs only and produce clearly non-authoritative JSON outputs. A self-authored trace and self-authored valid-PoW headers are **still not independent peer/clock/host evidence**. The digest of input bytes is not a digital signature.

## Input schema and explicit independent pins

The first-seen manifest and events use the `NATIVE_V3_FIRST_SEEN_V1` format from the first script. The **third** file is UTF-8 JSONL, exactly one object per line, with:

```json
{"block_hash":"<64 lowercase hex characters>","header":{"version":3,"parents":["<canonical parent SHA256>"],"timestamp_ns":1800000000500000000,"difficulty":545259519,"nonce":0,"merkle_root":"<64 lowercase hex characters>","state_root":"<64 lowercase hex characters>","blue_score":1,"height":1}}
```

This record is a **schema illustration**, not a real mined or PoW-accepted block. Valid bytes and nonce must be supplied by a genuine external capture. Include one real pinned genesis header, followed by exactly one full-PoW header for **every** mined block in the first-seen event file; all required ancestors must be present. The order of records does not matter. Pilot budget: 2–256 total headers, 1 MiB complete header JSONL, 4 KiB per line. If a real run requires thousands of ancestors, use a separately reviewed proof-backed sliding/snapshot protocol; do **not** pretend a truncated graph proves full ancestry. The current parser budgets for first-seen events remain 200,000 rows, 64 MiB total, 4 KiB per line and 1,000,000 potential block/peer pairs.

```bash
python3 scripts/native_v3_trace_header_binding.py \
  --manifest /secure-external-capture/manifest.json \
  --events-jsonl /secure-external-capture/first-seen.jsonl \
  --headers-jsonl /secure-external-capture/native-v3-headers.jsonl \
  --expected-sha '<independently approved 40-hex candidate HEAD>' \
  --expected-network '<independently pinned rehearsal network identifier>' \
  --expected-chain-id '<independently pinned native chain ID>' \
  --expected-genesis '<independently frozen 64-hex genesis header SHA256>' \
  --expected-genesis-ns '<independently frozen nanosecond timestamp>' \
  --output /secure-external-capture/header-validation-result.json
```

Never derive these independent pins solely from the supplied manifest or header file; obtain them from separately approved operator configuration. A valid result is **not** mainnet permission, GHOSTDAG K/blue-work freeze, independently signed network proof or permission to activate mining.

## Nonnegotiable evidence still missing

A genuine 25-node / 16-active-miner trial requires independently authenticated host ownership, tamper-evident collector logs and versioned collector binaries, calibration and uncertainty proof for each physical clock, network topology/packet loss, actual native-v3 ingress measurements, block body provenance, real anticone/false-red rates and the full monetary/native-v3 replay and fail-closed pipeline. The synthetic self-test only mines a tiny easy-target test chain, checks nonce-forgery rejection, incomplete ancestry and malformed/duplicate records, with Python and `-O`; those results do **not** constitute a 25-node performance experiment.

Mainnet #781: **NO-GO**. Consensus-policy issue #1309: **OPEN**, `K=32` and eight-parent maximum **UNAPPROVED**. Native monetary mining: **OFF**. Smart contracts: **INACTIVE**.
