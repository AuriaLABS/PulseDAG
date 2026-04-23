# Runtime Event Stream (v2.2)

PulseDAG v2.2 adds a Server-Sent Events (SSE) stream for key runtime and network signals.

## Endpoint

- `GET /runtime/events/stream`

## Model

- Transport: SSE (`text/event-stream`)
- Event name: `runtime_event`
- Payload JSON shape:

```json
{
  "sequence": 42,
  "dropped_count": 0,
  "event": {
    "timestamp_unix": 1710000000,
    "level": "info",
    "kind": "sync_phase_change",
    "message": "sync pipeline moved to header discovery"
  }
}
```

## Tunables

Query params:

- `poll_interval_ms` (default `500`, min `100`, max `5000`)
- `scan_limit` (default `200`, min `20`, max `1000`)
- `emit_limit` (default `32`, min `1`, max `200`)
- `heartbeat_secs` (default `15`, min `5`, max `60`)

## Safety and Backpressure

- The stream polls a bounded recent event window (`scan_limit`) and deduplicates in-memory.
- Each poll emits at most `emit_limit` events to avoid unbounded response pressure.
- If more than `emit_limit` unseen events arrive in one poll, oldest unseen items for that poll are dropped and `dropped_count` is set on emitted envelopes.
- The server uses periodic keepalive frames for idle connections.
- Client disconnects are handled by Axum's SSE response lifecycle and do not alter node runtime state.

## Operational Notes

- This stream is incremental and focused on operator visibility.
- Typical high-value `kind` values include reconnect/recovery, sync phase changes, snapshot/rebuild lifecycle, and mining accept/reject signals, when those events are appended to runtime events.
- Existing polling endpoints remain available:
  - `GET /runtime/events`
  - `GET /runtime/events/summary`
