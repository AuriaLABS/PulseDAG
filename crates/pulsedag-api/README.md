# pulsedag-api

Public API types and contracts for PulseDAG RPC/HTTP endpoints.

## Purpose

This crate provides:
- **Request/Response types** for RPC endpoints
- **Data structures** for API serialization
- **Validation types** for input constraints
- **Error response** formats
- **OpenAPI** compatibility (planned)

## Dependencies

- `pulsedag-core` — core data structures
- `serde` — serialization framework

## Key Modules

- `requests` — RPC request types (GetBlock, GetTransaction, etc.)
- `responses` — RPC response envelopes
- `transactions` — Transaction API types
- `blocks` — Block API types
- `errors` — API error response definitions

## Usage Example

```rust
use pulsedag_api::{GetBlockRequest, BlockResponse};

let req = GetBlockRequest { hash: "...".into() };
let resp: BlockResponse = rpc_call(req).await?;
```

## Testing

Run with:
```bash
cargo test -p pulsedag-api
```

JSON roundtrip tests confirm serialization stability.

## Warnings

- **API versioning:** Changes to request/response structures should be backward-compatible or versioned.
- **Validation:** Input validation rules are enforced by `pulsedag-rpc` handlers; this crate defines contracts only.
- **No business logic:** This crate should remain type-only; business logic belongs in handlers.
