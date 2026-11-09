# Native-v3 checkpoint external pin / anti-rollback (OFFLINE TRIAL ONLY)

Status: **NO CHECKPOINT TRUST AUTHORITY; NO NETWORK GO**. Tracks #1367 and #781. This regression tests only a narrow precondition for restoration, **not** its production implementation.

## Threat boundary

A checksum *inside* a checkpoint, or a signature from a key supplied by that checkpoint, does not establish its history, freshness, economic finality or network identity. An old snapshot may be internally well-formed and correctly hashed. A future restore verifier must compare it against a trustworthy, **independently provisioned and authenticated** chain-bound anchor and a trusted monotonic minimum score. Neither a peer nor the snapshot may choose that minimum or supply its own authoritative anchor.

`scripts/native_v3_checkpoint_external_anchor_trial.py` is a Python-standard-library **local-only comparator** accepting three separate paths: a simulated external pin JSON, an untrusted checkpoint claim JSON, and raw candidate snapshot bytes. It requires:

- Exact versioned schemas and field sets; nonzero canonical 64-hex genesis/checkpoint/snapshot digests; positive bounded integer scores; a public-testnet-v3 chain namespace.
- A **separately supplied** pin containing `checkpoint_score`, `minimum_accepted_score`, network identity, genesis, checkpoint identity and SHA256 of **exact snapshot bytes**. A matching, but older, checkpoint claim is rejected; missing, cross-network, over-budget, duplicate-field, activation-enabled or tampered inputs fail closed.
- Only canonical booleans `false` for `contracts_enabled` and `monetary_runtime_enabled` in the claim. No input is treated as a permission or config renderer. The snapshot is capped at 8 MiB **for this diagnostic only** (NOT a consensus/pruning size constant).

**Important limitations:** the script CANNOT prove that its supposedly out-of-band pin came from a real trusted source, was signed by an authorized key, had finality, was updated monotonically across restart, or is resilient to simultaneous pin+snapshot rollback by an attacker. It does not parse or authenticate actual native DAG checkpoints, canonical GD-08 monetary order, rewards, retarget, pruning proof, state root, stored sidecar or peer identity. Its `PASS_ONLY` output explicitly denies source authentication, native consensus and GO. **Do not wire it to production startup or load snapshots based on its verdict.**

## Offline test procedure

```bash
python3 -m unittest discover -v -s scripts/tests -p 'test_native_v3_checkpoint_external_anchor_trial.py'
python3 scripts/native_v3_checkpoint_external_anchor_trial.py \
  /path/to/externally-pinned-TEST-ONLY.json \
  /path/to/untrusted-claim-TEST-ONLY.json \
  /path/to/raw-snapshot-TEST-ONLY.bin
```

The tests synthesize pins and snapshot bytes, then reject stale score, out-of-range score, cross-network/genesis identity, mismatched checkpoint hash, nonmatching snapshot bytes, malformed/duplicate JSON, oversize inputs, and claims that enable monetary execution/contracts. They do **not** prove any network checkpoint has ever been independently authenticated.

**Production work still required (#1367):** a separately frozen trust-anchor key/threshold and network scope, authenticated tip/finality policy, monotonic rollback-resistant freshness state, native GHOSTDAG selected parent/blue-red/retarget authority, full-DAG versus independently authenticated pruned-state equivalence for monetary UTXO/fees/rewards/supply, transactional restoration and reorg attacks. Only a separately approved `GO_PUBLIC_TESTNET` may start the first live monetary testnet; mainnet has its own later GO. Monetary mining OFF and smart contracts INACTIVE.
