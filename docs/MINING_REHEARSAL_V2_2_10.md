# Mining Rehearsal v2.2.10 (External `pulsedag-miner`)

This runbook validates the final v2.2.10 PoW flow end-to-end.

## 1) Start node

```bash
scripts/v2_2_9_start_node_a.sh
```

## 2) Verify node health

```bash
curl -fsS http://127.0.0.1:18080/health
curl -fsS http://127.0.0.1:18080/status
```

## 3) Read active PoW metadata

```bash
curl -fsS http://127.0.0.1:18080/pow
```

Confirm algorithm identity is `kHeavyHash` and metadata is coherent with current node build.

## 4) Request mining template

```bash
curl -fsS -X POST http://127.0.0.1:18080/mining/template \
  -H 'content-type: application/json' \
  -d '{"miner_address":"rehearsal-node-a-miner"}'
```

## 5) Run miner with real CLI flags

Script wrapper (recommended):

```bash
scripts/v2_2_9_start_miner_node_a.sh
```

Equivalent direct command:

```bash
cargo run -p pulsedag-miner -- \
  --node http://127.0.0.1:18080 \
  --miner-address rehearsal-node-a-miner \
  --threads 4 \
  --max-tries 50000 \
  --sleep-ms 1500 \
  --refresh-before-expiry-ms 1000 \
  --loop
```

## 6) Submit solved block

The miner submits automatically to `POST /mining/submit`.

Manual submit shape (for debugging):

```bash
curl -fsS -X POST http://127.0.0.1:18080/mining/submit \
  -H 'content-type: application/json' \
  -d '{"template_id":"...","block":{...}}'
```

## 7) Verify acceptance and chain movement

```bash
curl -fsS http://127.0.0.1:18080/status
curl -fsS http://127.0.0.1:18080/tips
```

## 8) Troubleshooting

- `invalid_pow`: check miner/node version alignment and canonical header bytes.
- `stale_template`: request fresh template; ensure fast submit.
- `duplicate`: same solved block/template already accepted.
- target mismatch: confirm difficulty/target decode and 256-bit compare semantics.
- mutated header: ensure no field rewrite between template and submit except nonce update.
- miner/node version mismatch: rebuild both from same v2.2.10 branch/commit.

## 9) Scope reminders

- Miner is external.
- No pool logic in miner.
- No smart contracts in this milestone.
