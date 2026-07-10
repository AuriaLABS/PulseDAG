# RPC liveness and status lock ordering

`/health` is a listener/process liveness endpoint. It must never acquire the
`ChainState`, P2P, runtime, or storage locks synchronously. The handler reads the
latest cached `NodeRpcSnapshot` only and reports stale cached data as
`degraded` instead of waiting for fresh data.

For status-like endpoints that need fresh state, avoid nested locks where
possible. When a fresh multi-subsystem snapshot is captured, use the single
non-blocking order below and fall back to cached degraded data if any step is
busy:

1. P2P status through the bounded RPC status snapshot helper.
2. `ChainState::try_read()`.
3. `NodeRuntimeStats::try_read()`.
4. Store/update the cached RPC snapshot.

Do not hold a chain read guard while waiting on unbounded P2P/runtime work. Do
not run DAG consistency traversal, RocksDB scans, or reconciliation from
liveness handlers. DAG consistency checks belong in explicit diagnostics or
background maintenance/self-audit paths, with the latest result copied into the
cached snapshot.
