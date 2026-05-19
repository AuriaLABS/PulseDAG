# RPC/API Endpoint Inventory (v2.2.17)

This document inventories all HTTP endpoints registered in `crates/pulsedag-rpc/src/routes.rs` for release **v2.2.17** and assigns an exposure policy for operations closeout.

## Scope and routing notes

- Public routes are mounted under both:
  - `/api/v1/*` (stable prefix)
  - `/*` (compatibility prefix)
- Admin routes are mounted under both:
  - `/admin/*`
  - `/*` (admin compatibility prefix)
- Therefore, for v2.2.17, admin-capable handlers are reachable without `/admin` when admin routing is enabled.
- **Default recommendation:** bind RPC to localhost and only expose through a secured gateway with explicit allow-lists.

## Exposure classes

- `public_safe`: read-only status/query endpoints safe for broad client consumption.
- `operator_local`: operational endpoints intended for trusted node/miner operators on local or tightly controlled networks.
- `admin_dangerous`: privileged endpoints that can change node state, wallet state, sync behavior, or operational data.
- `dev_only`: diagnostic/testing helpers unsuitable for production internet exposure.
- `deprecated_or_internal`: compatibility/internal surface that should be phased out from external use.

## Endpoint inventory

> Legend:
> - `Mutates`: `yes` = writes/changes node/wallet/runtime state; `no` = read-only.
> - `Internet-exposed`: recommended internet exposure policy for v2.2.17.

| Method | Path | Purpose | Expected caller | Exposure level | Mutates | Internet-exposed | Auth requirement | Rate-limit recommendation | Evidence needed for v2.2.17 closeout |
|---|---|---|---|---|---|---|---|---|---|
| GET | `/` and `/api/v1/` | API version metadata | explorer client, operator tooling | public_safe | no | yes (read-only) | none | 60 rpm/IP | curl output proving version/stage only |
| GET | `/version` and `/api/v1/version` | API version metadata alias | explorer client, operator tooling | deprecated_or_internal | no | no (prefer canonical `/api/v1/`) | none | 30 rpm/IP | alias behavior captured in release evidence |
| GET | `/health` and `/api/v1/health` | liveness health | load balancer, monitor | public_safe | no | yes (minimal) | none | 120 rpm/IP | screenshot/log of healthy response during soak |
| GET | `/readiness` and `/api/v1/readiness` | readiness/serving state | orchestrator, SRE monitor | operator_local | no | no unless behind auth gateway | mTLS or gateway token | 60 rpm/IP | readiness transitions recorded during restart drill |
| GET | `/status` and `/api/v1/status` | node/runtime status summary | operator, dashboards | operator_local | no | no unless redacted and proxied | gateway token | 30 rpm/IP | status snapshots from burn-in |
| GET | `/release` and `/api/v1/release` | release/build metadata | operator, support | operator_local | no | no (metadata leakage risk) | gateway token | 20 rpm/IP | release metadata captured and matched to tag |
| GET | `/bootstrap` and `/api/v1/bootstrap` | bootstrap progress | operator | operator_local | no | no | gateway token | 30 rpm/IP | bootstrap evidence from fresh-node rehearsal |
| GET | `/genesis` and `/api/v1/genesis` | genesis block/config view | explorer client | public_safe | no | yes | none | 60 rpm/IP | genesis hash matches expected network config |
| GET | `/dag` and `/api/v1/dag` | DAG query snapshot | explorer, operator | operator_local | no | no (heavy query surface) | gateway token | 20 rpm/IP | latency sample under baseline load |
| GET | `/tips` and `/api/v1/tips` | current tips | explorer/miner | public_safe | no | yes | none | 90 rpm/IP | tips parity across nodes in rehearsal |
| GET | `/blocks` and `/api/v1/blocks` | block list/query | explorer | public_safe | no | yes | none | 60 rpm/IP | query response sanity sample |
| POST | `/blocks/validate` and `/api/v1/blocks/validate` | validate block payload | operator/dev tooling | dev_only | no | no | gateway token | 15 rpm/IP | validation test vectors replay output |
| GET | `/blocks/latest` and `/api/v1/blocks/latest` | latest block | explorer | public_safe | no | yes | none | 120 rpm/IP | latest block checks in smoke output |
| GET | `/blocks/recent` and `/api/v1/blocks/recent` | recent blocks | explorer | public_safe | no | yes | none | 60 rpm/IP | recent block query evidence |
| GET | `/blocks/page` and `/api/v1/blocks/page` | paginated block list | explorer | public_safe | no | yes | none | 60 rpm/IP | pagination sample across pages |
| GET | `/blocks/:hash/overview` and `/api/v1/blocks/:hash/overview` | block overview | explorer | public_safe | no | yes | none | 60 rpm/IP | known-hash lookup output |
| GET | `/blocks/:hash/transactions` and `/api/v1/blocks/:hash/transactions` | block tx list | explorer | public_safe | no | yes | none | 60 rpm/IP | tx listing consistency check |
| GET | `/blocks/:hash` and `/api/v1/blocks/:hash` | full block by hash | explorer | public_safe | no | yes | none | 60 rpm/IP | block retrieval for sample hashes |
| GET | `/utxos` and `/api/v1/utxos` | UTXO query | wallet/explorer | operator_local | no | no unless hardened | gateway token | 40 rpm/IP | UTXO query traces in functional smoke |
| GET | `/address/:address` and `/api/v1/address/:address` | address detail | wallet/explorer | public_safe | no | yes | none | 60 rpm/IP | sample addresses queried |
| GET | `/address/:address/summary` and `/api/v1/address/:address/summary` | summarized address stats | wallet/explorer | public_safe | no | yes | none | 60 rpm/IP | summary output sample |
| GET | `/address/:address/activity` and `/api/v1/address/:address/activity` | address activity feed | wallet/explorer | public_safe | no | yes | none | 60 rpm/IP | activity sample output |
| GET | `/address/:address/utxos` and `/api/v1/address/:address/utxos` | address UTXOs | wallet/explorer | operator_local | no | no unless hardened | gateway token | 40 rpm/IP | address UTXO checks in wallet rehearsal |
| GET | `/txs` and `/api/v1/txs` | tx list | explorer | public_safe | no | yes | none | 60 rpm/IP | tx list response sample |
| GET | `/txs/recent` and `/api/v1/txs/recent` | recent tx list | explorer | public_safe | no | yes | none | 90 rpm/IP | recent tx queries captured |
| GET | `/txs/page` and `/api/v1/txs/page` | paginated txs | explorer | public_safe | no | yes | none | 60 rpm/IP | pagination sample |
| GET | `/txs/activity` and `/api/v1/txs/activity` | tx activity feed | explorer | public_safe | no | yes | none | 60 rpm/IP | activity feed output sample |
| GET | `/txs/:txid/lookup` and `/api/v1/txs/:txid/lookup` | tx lookup helper | explorer | public_safe | no | yes | none | 60 rpm/IP | lookup of known txid |
| GET | `/transactions` and `/api/v1/transactions` | confirmed tx dataset | explorer/analytics | operator_local | no | no (volume/cost) | gateway token | 30 rpm/IP | payload-size and latency evidence |
| GET | `/mempool` and `/api/v1/mempool` | mempool contents/status | operator, miner | operator_local | no | no | gateway token | 20 rpm/IP | mempool snapshots during load test |
| GET | `/txs/:txid` and `/api/v1/txs/:txid` | tx detail by id | explorer | public_safe | no | yes | none | 60 rpm/IP | tx detail sample |
| POST | `/tx/build` and `/api/v1/tx/build` | build unsigned tx | wallet frontend/backend | operator_local | no | no (abuse surface) | authenticated client token | 20 rpm/IP | wallet integration test evidence |
| POST | `/tx/submit` and `/api/v1/tx/submit` | submit transaction | wallet backend | admin_dangerous | yes | no | strong auth + origin allow-list | 10 rpm/IP + burst 2 | controlled submit test logs |
| POST | `/mine` and `/api/v1/mine` | mining work helper | miner | operator_local | no | no | miner token or mTLS | 30 rpm/IP | miner-node contract test artifact |
| POST | `/mine/preview` and `/api/v1/mine/preview` | mine preview/debug | operator/dev | dev_only | no | no | operator token | 10 rpm/IP | preview output retained in rehearsal |
| POST | `/mining/template` and `/api/v1/mining/template` | mining template creation | miner | operator_local | no | no | miner token/mTLS | 60 rpm/IP | template call traces from miner run |
| POST | `/mining/submit` and `/api/v1/mining/submit` | submit mined solution | miner | admin_dangerous | yes | no | miner auth (mTLS/token) | 30 rpm/IP | share acceptance/rejection logs |
| POST | `/mining/workers/heartbeat` and `/api/v1/mining/workers/heartbeat` | worker heartbeat updates | miner workers | operator_local | yes | no | worker token | 120 rpm/IP | heartbeat stream evidence |
| GET | `/mining/workers/stats` and `/api/v1/mining/workers/stats` | worker stats | operator | operator_local | no | no | operator token | 30 rpm/IP | worker stats scrape evidence |
| POST | `/mining/jobs/claim` and `/api/v1/mining/jobs/claim` | claim mining job | miner | operator_local | yes | no | miner token | 60 rpm/IP | claim lifecycle evidence |
| POST | `/mining/jobs/submit` and `/api/v1/mining/jobs/submit` | submit mining job result | miner | admin_dangerous | yes | no | miner token | 30 rpm/IP | job submit acceptance evidence |
| GET | `/p2p/status` and `/api/v1/p2p/status` | p2p status summary | operator | operator_local | no | no | operator token | 20 rpm/IP | p2p status during multi-node soak |
| GET | `/p2p/peers` and `/api/v1/p2p/peers` | peer list/details | operator | operator_local | no | no | operator token | 20 rpm/IP | peer table capture during rehearsal |
| GET | `/p2p/propagation` and `/api/v1/p2p/propagation` | propagation diagnostics | operator | operator_local | no | no | operator token | 15 rpm/IP | propagation SLO evidence |
| GET | `/p2p/topics` and `/api/v1/p2p/topics` | topic subscriptions/health | operator | operator_local | no | no | operator token | 15 rpm/IP | topics output sample |
| GET | `/p2p/topology` and `/api/v1/p2p/topology` | topology graph data | operator | operator_local | no | no | operator token | 10 rpm/IP | topology captures from 3-5 node tests |
| GET | `/search/:query` and `/api/v1/search/:query` | generic search lookup | explorer | public_safe | no | yes (with WAF constraints) | none | 30 rpm/IP | query abuse/sanitization check |
| GET | `/metrics` and `/api/v1/metrics` | prometheus metrics | monitoring system | operator_local | no | no public internet | metrics-scrape auth or private network | scrape every 15s from allow-listed source | successful scrape in observability package |
| GET | `/orphans` and `/api/v1/orphans` | orphan block status | operator | operator_local | no | no | operator token | 20 rpm/IP | orphan metrics in chaos evidence |
| GET | `/dashboard` and `/api/v1/dashboard` | dashboard aggregate payload | operator UI | operator_local | no | no | operator token | 20 rpm/IP | dashboard API response in ops rehearsal |
| GET | `/errors` and `/api/v1/errors` | error catalog | developer/operator | public_safe | no | yes | none | 60 rpm/IP | catalog retrieval sample |
| GET | `/checks` and `/api/v1/checks` | node checks summary | operator | operator_local | no | no | operator token | 30 rpm/IP | checks output in maintenance runbook |
| GET | `/policy` and `/api/v1/policy` | node policy config summary | operator | operator_local | no | no | operator token | 20 rpm/IP | policy dump evidence |
| GET | `/pow` and `/api/v1/pow` | PoW info | miner/operator | operator_local | no | no | operator token | 30 rpm/IP | pow info retrieval in benchmark run |
| POST | `/pow/validate-header` and `/api/v1/pow/validate-header` | validate PoW header | dev/miner tooling | dev_only | no | no | operator token | 20 rpm/IP | vector validation outputs |
| POST | `/pow/hash-header` and `/api/v1/pow/hash-header` | hash PoW header | dev/miner tooling | dev_only | no | no | operator token | 20 rpm/IP | deterministic hash evidence |
| POST | `/pow/check-header` and `/api/v1/pow/check-header` | policy check on header | dev/miner tooling | dev_only | no | no | operator token | 20 rpm/IP | policy check logs |
| POST | `/pow/mine-header` and `/api/v1/pow/mine-header` | CPU mining helper | dev/miner tooling | dev_only | no | no | operator token | 10 rpm/IP | mine-header smoke evidence |
| GET | `/pow/policy` and `/api/v1/pow/policy` | PoW policy view | miner/operator | operator_local | no | no | operator token | 30 rpm/IP | policy response in miner rehearsal |
| GET | `/pow/metrics` and `/api/v1/pow/metrics` | PoW metrics view | operator | operator_local | no | no | operator token | 30 rpm/IP | metrics sample in bench report |
| GET | `/pow/metrics/history` and `/api/v1/pow/metrics/history` | PoW metrics history | operator | operator_local | no | no | operator token | 20 rpm/IP | history retrieval evidence |
| GET | `/pow/metrics/summary` and `/api/v1/pow/metrics/summary` | PoW metrics summary | operator | operator_local | no | no | operator token | 30 rpm/IP | summary output saved |
| GET | `/pow/health` and `/api/v1/pow/health` | PoW subsystem health | operator | operator_local | no | no | operator token | 30 rpm/IP | health endpoint during miner run |
| GET | `/pow/export` and `/api/v1/pow/export` | PoW export payload | operator/dev | operator_local | no | no | operator token | 10 rpm/IP | export artifact checksums |
| GET | `/pow/dashboard` and `/api/v1/pow/dashboard` | PoW dashboard aggregate | operator UI | operator_local | no | no | operator token | 20 rpm/IP | dashboard endpoint sample |
| GET | `/sync/status` and `/api/v1/sync/status` | sync status | operator | operator_local | no | no | operator token | 30 rpm/IP | sync status through recovery drill |
| GET | `/sync/missing` and `/api/v1/sync/missing` | missing blocks overview | operator | operator_local | no | no | operator token | 15 rpm/IP | missing-set sample output |
| GET | `/sync/blocks` and `/api/v1/sync/blocks` | sync block details | operator | operator_local | no | no | operator token | 15 rpm/IP | sync block query logs |
| GET | `/sync/verify` and `/api/v1/sync/verify` | sync verification summary | operator | operator_local | no | no | operator token | 10 rpm/IP | verify output from sync tests |
| GET | `/snapshot` and `/api/v1/snapshot` | snapshot metadata | operator | operator_local | no | no | operator token | 10 rpm/IP | snapshot metadata capture |
| GET | `/admin/dag/consistency` and `/dag/consistency` | DAG consistency checks | operator/SRE | admin_dangerous | no | no | strong admin auth + mTLS | 5 rpm/IP | consistency report in closeout bundle |
| POST | `/admin/wallet/new` and `/wallet/new` | create wallet material | privileged operator | admin_dangerous | yes | no | strong admin auth + HSM policy | 2 rpm/IP | wallet control test evidence |
| POST | `/admin/wallet/sign` and `/wallet/sign` | sign transaction payload | privileged operator | admin_dangerous | yes | no | strong admin auth | 5 rpm/IP | signing audit trail |
| POST | `/admin/wallet/transfer` and `/wallet/transfer` | transfer funds | privileged operator | admin_dangerous | yes | no | strong admin auth + dual control | 2 rpm/IP | controlled transfer rehearsal evidence |
| POST | `/admin/mining/jobs/cleanup` and `/mining/jobs/cleanup` | cleanup mining jobs | operator | admin_dangerous | yes | no | admin auth | 5 rpm/IP | cleanup action log |
| GET | `/admin/runtime` and `/runtime` | runtime status internals | operator | admin_dangerous | no | no | admin auth | 10 rpm/IP | runtime dump attached to support pack |
| GET | `/admin/runtime/events` and `/runtime/events` | runtime event list | operator | admin_dangerous | no | no | admin auth | 10 rpm/IP | event stream capture |
| GET | `/admin/runtime/events/stream` and `/runtime/events/stream` | streaming events | operator tooling | admin_dangerous | no | no | admin auth | max 2 concurrent streams/source | stream stability evidence |
| GET | `/admin/runtime/events/summary` and `/runtime/events/summary` | event summary | operator | admin_dangerous | no | no | admin auth | 10 rpm/IP | summary output retained |
| GET | `/admin/diagnostics` and `/diagnostics` | diagnostics bundle | operator/SRE | admin_dangerous | no | no | admin auth | 5 rpm/IP | diagnostics package artifact |
| GET | `/admin/operator/query-pack` and `/operator/query-pack` | operator query package | operator/SRE | admin_dangerous | no | no | admin auth | 5 rpm/IP | query-pack output archived |
| GET | `/admin/maintenance/report` and `/maintenance/report` | maintenance report | operator | admin_dangerous | no | no | admin auth | 5 rpm/IP | report attached to maintenance check |
| POST | `/admin/pow/metrics/capture` and `/pow/metrics/capture` | force metrics capture | operator | admin_dangerous | yes | no | admin auth | 5 rpm/IP | capture invocation log |
| POST | `/admin/pow/metrics/prune` and `/pow/metrics/prune` | prune metrics history | operator | admin_dangerous | yes | no | admin auth | 2 rpm/IP | prune before/after evidence |
| POST | `/admin/pow/mine-and-capture` and `/pow/mine-and-capture` | mine+capture workflow | operator/dev | admin_dangerous | yes | no | admin auth | 2 rpm/IP | workflow evidence with bounded run |
| POST | `/admin/pow/auto/run` and `/pow/auto/run` | automatic PoW run orchestration | operator/dev | admin_dangerous | yes | no | admin auth | 2 rpm/IP | run log and rollback evidence |
| GET | `/admin/sync/replay-plan` and `/sync/replay-plan` | sync replay planning | operator | admin_dangerous | no | no | admin auth | 5 rpm/IP | replay plan output stored |
| GET | `/admin/sync/incremental-plan` and `/sync/incremental-plan` | incremental sync plan | operator | admin_dangerous | no | no | admin auth | 5 rpm/IP | incremental plan evidence |
| POST | `/admin/snapshot/create` and `/snapshot/create` | create snapshot | operator | admin_dangerous | yes | no | admin auth | 1 rpm/IP | snapshot creation log + checksum |
| POST | `/admin/prune` and `/prune` | prune chain/state | operator | admin_dangerous | yes | no | admin auth + change-control ticket | 1 rpm/IP | prune safety checklist + output |
| POST | `/admin/sync/rebuild` and `/sync/rebuild` | trigger rebuild | operator | admin_dangerous | yes | no | admin auth + change-control ticket | 1 rpm/IP | rebuild initiation and completion evidence |
| POST | `/admin/sync/reconcile-mempool` and `/sync/reconcile-mempool` | mempool reconcile action | operator | admin_dangerous | yes | no | admin auth | 2 rpm/IP | reconcile action logs |
| GET | `/admin/sync/rebuild-preview` and `/sync/rebuild-preview` | rebuild preview/planning | operator | admin_dangerous | no | no | admin auth | 5 rpm/IP | preview output attached |

## Closeout policy assertions for v2.2.17

1. **No admin endpoint is classified as `public_safe`.**
2. **All mutating endpoints are non-internet-exposed by default.**
3. **`/metrics` is operator-local and should be scraped only from trusted network paths.**
4. **`/p2p/*`, `/sync/*`, `/mining/*`, `/snapshot*`, `/prune*`, `/rebuild*`, and wallet/admin endpoints are explicitly restricted to operator/admin exposure profiles.**
5. **Admin compatibility routes at root (`/*`) are treated as privileged equivalents of `/admin/*` and must follow the same controls.**

## Source evidence

Endpoint inventory is derived from router registrations in:
- `crates/pulsedag-rpc/src/routes.rs`

Release reference:
- `docs/RELEASE_NOTES_V2_2_17.md`
