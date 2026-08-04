# v2.4.0 public-safe RPC contract

## Scope

This document defines the read-only RPC edge contract required before a PulseDAG public testnet may be exposed. It does not activate a listener, allocate DNS, issue TLS certificates, start Day 0 or authorize public launch.

The public listener and the operator/admin listener are separate security boundaries. A public process must use `PULSEDAG_API_PROFILE=public_safe`, keep admin disabled and omit any operator token from that listener. Operator/admin RPC remains bound to loopback or a private management network.

## Public route boundary

The `public_safe` router exposes only documented read-only and liveness endpoints, including version, health, readiness, release, metrics, chain/DAG, blocks, addresses, transaction lookup, mempool inventory and read-only P2P/sync status.

The public listener does not expose:

- transaction construction or submission;
- mining templates, submission, jobs or worker control;
- wallet creation, signing or transfer;
- snapshot creation, pruning or rebuild/reconcile operations;
- diagnostics or operator query packs;
- admin routes or compatibility aliases for those operations.

An unmatched public route returns HTTP 404 with the stable API error code `public_route_forbidden`. Oversized requests return HTTP 413 with `request_too_large`. Quota rejection returns HTTP 429 with `rate_limited`. A denied browser origin returns HTTP 403 with `cors_origin_denied`.

The repository test contract fixes representative forbidden paths from every write, mining, wallet, maintenance, diagnostics and admin family. This inventory is compiled only for tests and does not alter or expand the runtime route table. It also fixes the liveness bypass inventory and the stable error-code names above. The complete RPC and release matrix remains authoritative for handler behavior on the exact candidate SHA.

## Request limits

The default v2.4.0 public-safe policy is:

- request body limit: 131072 bytes;
- request quota: 30 requests per 60-second window;
- identity: transport peer IP when Axum `ConnectInfo` is present;
- fallback identity: one global bucket when transport client identity is unavailable;
- maximum tracked identities: 4096;
- stale windows expire after the configured window;
- when the map is full, the oldest window is evicted deterministically.

The node enforces the body limit while consuming the actual request body, not only from a caller-supplied `Content-Length` header. Requests without that header and streamed bodies are subject to the same bound.

`/health`, `/status`, `/readiness`, `/release`, `/metrics`, P2P/sync status and their versioned equivalents are not rejected by the quota. They remain protected by the bounded liveness-handler timeout. Their quota exemption does not exempt them from the request-body bound; an oversized request to a liveness path is rejected with the same HTTP 413 / `request_too_large` contract.

The metrics surface exports:

- `rpc_rate_limit_rejected_total`;
- `rpc_rate_limit_evictions_total`;
- `rpc_rate_limit_tracked_keys`.

Growth in rejections or evictions must be included in launch-rehearsal evidence and alerting.

## CORS

Browser cross-origin access is denied unless an exact origin is listed in `PULSEDAG_RPC_CORS_ALLOWLIST`.

- Empty allowlist means deny all browser origins.
- `*` entries are discarded and never become credentialed wildcard access.
- Allowed methods are `GET`, `HEAD` and `OPTIONS`.
- Allowed request headers are `Accept` and `Content-Type`.
- The response varies on `Origin`.
- CORS is not authentication and does not replace firewall or proxy controls.

## TLS and reverse-proxy boundary

The public launch record must name the owner of TLS termination, reverse proxy, firewall rules and edge rate limiting. No default is implied by this repository.

Recommended deployment:

1. bind the PulseDAG public-safe RPC to loopback or a private service network;
2. terminate public TLS at a maintained reverse proxy;
3. expose only the public-safe listener through the firewall;
4. apply an additional edge quota and connection limit;
5. keep operator/admin RPC unreachable from the public proxy.

The node does not trust arbitrary `Forwarded`, `X-Forwarded-For` or similar headers. When a reverse proxy is used, the in-process limiter sees the proxy transport address and may collapse traffic into the global/proxy bucket. Per-client public enforcement therefore belongs at the trusted edge unless a separately reviewed trusted-proxy integration is implemented.

## Launch evidence

Before public-testnet GO, preserve:

- sanitized public-safe configuration and digest;
- exact node SHA, binary/image digest and network identity;
- public route negative-test results;
- TLS certificate and termination owner;
- firewall and reverse-proxy configuration owner;
- effective body, connection and request limits;
- CORS allowlist;
- proof that admin/operator routes are unavailable from the public listener;
- limiter rejection, eviction and tracked-key telemetry during rehearsal.

Any public listener, proxy or limit change after candidate freeze requires renewed validation. The 30-day public-testnet clock starts only after #781 records GO and the first accepted public-testnet block is observed.
