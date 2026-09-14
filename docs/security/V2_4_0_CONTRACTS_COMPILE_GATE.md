# v2.4.0 contracts compile gate

Status: active for the moving Task31 candidate. Does **not** authorize contract activation.

## Rule

Default `pulsedagd` / `pulsedag-core` builds compile **without** the `executable-contracts` Cargo feature.

- `contracts_enabled=false` remains the only authorized runtime state for Task31.
- `PULSEDAG_CONTRACTS_ENABLED=true` is rejected at startup unless the binary was built with `--features executable-contracts`.
- Even with the feature, public-testnet / Task31 authorization is still `NO-GO` until a separate contracts-scope decision is recorded (`#1041`, `#781`, `#794`).

## What stays in `main`

Inactive foundation modules may remain in tree:

- `contract_v3.rs`
- `covenant_v1.rs` / `covenant_utxo_v1.rs`
- `tx_v3.rs` / `mempool_v3.rs`

Those modules are **not** the Task31 protocol identity (tx v2 + header v2 + `ghostdag_v1`).

## Operator check

Startup must log or fail closed on:

- `contracts_compile_identity`
- `PULSEDAG_CONTRACTS_ENABLED`
- `startup_protocol_mode`
- `consensus_mode`
- `public_testnet_ready`

See `#1131`.
