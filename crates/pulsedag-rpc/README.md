# pulsedag-rpc

JSON-RPC and REST API handlers for PulseDAG node.

## Purpose

This crate provides:
- **Axum web framework** integration for HTTP/REST endpoints
- **JSON-RPC 2.0** request/response handling
- **Block** and **transaction** query handlers
- **Async streaming** for real-time updates
- **Error handling** and response normalization
- **Input validation** and limit enforcement

## Dependencies

- `pulsedag-core`, `pulsedag-crypto`, `pulsedag-wallet`, `pulsedag-storage`, `pulsedag-p2p`, `pulsedag-api` — all crates (high coupling)
- `axum` — web framework
- `tokio` — async runtime (full feature set)
- `serde`, `serde_json` — request/response serialization
- `async-stream`, `futures-core` — streaming utilities
- `tracing` — structured logging
- `sha3`, `hex`, `ed25519-dalek` — hashing and signing for validation

## Key Modules

- `handlers/blocks.rs` — Block queries and listing with pagination
- `handlers/transactions.rs` — Transaction submission and queries
- `handlers/state.rs` — Node state endpoints (best tip, height, etc.)
- `server.rs` — Server startup and middleware configuration
- `errors.rs` — HTTP error response definitions
- `validation.rs` — Input validation (limits, pagination caps)

## Usage Example

```rust
use pulsedag_rpc::create_router;
use axum::Router;

let router: Router = create_router(db, p2p, wallet)?;
// Mount on Axum server
```

## Configuration

- **Block list limit:** Capped at 1000 (configurable via `BLOCK_LIST_LIMIT` env var)
- **Transaction limit:** Capped at 10000 (configurable via `TX_LIST_LIMIT` env var)
- **Page size default:** 100 (min 1, max configured limit)

## Tests

Run with:
```bash
cargo test -p pulsedag-rpc
```

Limit normalization invariant test:
```bash
cargo test -p pulsedag-rpc handlers::blocks::tests::limit_normalization
```

## Warnings

- **Coupling:** This crate depends on all other crates. Circular import risks are managed through clear dependency direction. Do not introduce reverse dependencies (e.g., `pulsedag-core` should never depend on `pulsedag-rpc`).
- **Resource limits:** Input validation prevents abuse; enforce pagination and query limits to avoid memory exhaustion.
- **Async context:** All handlers must be cancellation-safe; do not hide task failures in logs.
- **CORS:** Configure `tower-http::cors` middleware per deployment requirements.
- **Authentication:** Not built-in; add JWT/OAuth/IP-based auth at reverse proxy or middleware layer.
