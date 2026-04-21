# Runtime counters for mempool sanitize

Add these runtime fields:

- `mempool_sanitize_runs`
- `mempool_sanitize_removed_total`
- `mempool_sanitize_removed_last_run`
- `last_mempool_sanitize_unix`
- `last_mempool_sanitize_ok`

## Journal event kinds
- `mempool_sanitize_started`
- `mempool_sanitize_completed`
- `mempool_sanitize_failed`
