# Task: implement the v2.3.0 five-node mempool/transaction-relay runtime gate

## Context

Candidate run `29296544878` passed workspace validation and the staged `5N/1M -> 5N/2M -> 5N/4M` network gate on commit `a4439259d08c0b8f98add607e5d0290e69b6ee90`.

The mempool gate failed only because the required executable is absent:

```text
scripts/v2_3_0_mempool_tx_relay_evidence.sh
```

Unit/core/P2P/RPC tests and relay-test discovery already pass. Do not weaken `.github/workflows/v2_3_0_mempool_tx_relay_gate.yml`.

## Deliverables

1. Add an executable `scripts/v2_3_0_mempool_tx_relay_evidence.sh`.
2. Extract only genuinely reusable node lifecycle/topology/capture helpers from the existing 5-node rehearsal into a small shell library under `scripts/lib/` if that reduces duplication. Keep existing rehearsals behavior-compatible.
3. Add shell regression tests under `scripts/tests/` for manifest validation, cleanup, failure propagation and deterministic comparison.
4. Remove this task file in the implementation commit.

## Runtime drill

The script must be self-contained on an Ubuntu GitHub Actions runner and accept:

```text
OUT_DIR=<absolute output directory>
```

Optional bounded overrides may include base ports, startup timeout, convergence timeout and transaction count. Defaults must be suitable for CI.

The drill must:

- build or require the exact checked-out `pulsedagd` and `pulsedag-miner` release binaries;
- start five `libp2p-real` private-profile nodes with unique RPC/P2P ports and data directories;
- establish stable 5-node topology with four compatible peers per node;
- create real spendable UTXOs using the repository's supported mining/wallet/RPC flows;
- submit at least one valid transaction through node `n1` using the public transaction API;
- prove the same txid reaches all five mempools through P2P relay without submitting it independently to each node;
- resubmit the same transaction through a different node and prove duplicate suppression: one txid per mempool, no relay storm and duplicate/rejection counters or logs attributable to the duplicate;
- exercise a bounded rejection condition with a stable taxonomy. Prefer the real configured capacity path. If a CI-only reduced capacity is introduced, it must be an explicit private/rehearsal-only override, preserve the production default of 4096 and have unit tests proving the default is unchanged;
- mine/confirm at least one relayed transaction and prove it is removed from all five mempools;
- verify the final mempool txid sets are sorted and identical on all five nodes;
- capture `/api/v1/mempool`, `/api/v1/p2p/status`, `/api/v1/sync/status`, `/api/v1/checks` or compatibility equivalents for every node;
- terminate all miners/nodes and wait for ports to close on every exit path.

Do not synthesize endpoint data or manually copy txids between node state directories.

## Required runtime manifest

Write `${OUT_DIR}/evidence_manifest.json` with at least:

```json
{
  "result": "PASS",
  "evidence_kind": "runtime",
  "candidate_commit": "<full sha>",
  "node_count": 5,
  "relay_converged": true,
  "duplicate_suppression": true,
  "capacity_rejection_taxonomy": true,
  "confirmation_cleanup": true,
  "deterministic_final_mempool_sets": true,
  "submitted_txids": [],
  "confirmed_txids": [],
  "final_mempool_digest": "...",
  "public_testnet_ready": false
}
```

Also include per-node counts/digests, topology status, relay counters, duplicate evidence, rejection code/reason, confirmation block and timestamps. On failure write `result=FAIL`, a non-empty `failure_reasons` array and return non-zero.

## Evidence files

Store under `OUT_DIR`:

- node and miner logs;
- endpoint captures before submit, after relay, after duplicate, after confirmation and final;
- submitted transaction payloads/responses;
- sorted txid sets and digest per node;
- command log and timing summary;
- `SHA256SUMS` covering all evidence files except itself.

## Tests and validation

Run and pass:

```bash
bash -n scripts/v2_3_0_mempool_tx_relay_evidence.sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedag-core mempool --locked -- --nocapture
cargo test -p pulsedag-p2p --locked -- --nocapture
cargo test -p pulsedag-rpc mempool --locked -- --nocapture
bash scripts/tests/<new-driver-tests>.sh
```

Then run the actual workflow in `runtime-closeout` mode and require the mempool job and its final enforcement step to pass.

## Guardrails

- Keep `VERSION=v2.2.20` and workspace version `2.2.20`.
- Keep `public_testnet_ready=false`.
- Do not change consensus, PoW or the 30-day testnet clock.
- Do not replace runtime evidence with unit tests or fabricated JSON.
- Do not relax any jq condition in the existing gate.