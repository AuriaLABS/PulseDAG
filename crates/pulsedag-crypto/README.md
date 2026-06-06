# pulsedag-crypto

Cryptographic utilities and key management for PulseDAG.

## Purpose

This crate provides:
- **Key derivation** and private key handling
- **Ed25519 signing** and verification
- **Random key generation** (`rand`)
- **Hex encoding** for key serialization

## Dependencies

- `pulsedag-core` — core data structures (error types, hashing)
- `ed25519-dalek` — Ed25519 elliptic curve cryptography
- `rand` — cryptographically secure random generation
- `hex` — hex encoding/decoding for keys and signatures

## Key Modules

- `keys` — Private/public key management
- `signing` — Ed25519 signing operations
- `errors` — Error handling (delegates to `PulseError` from `pulsedag-core`)

## Usage Example

```rust
use pulsedag_crypto::{generate_keypair, sign_data};

let (private_key, public_key) = generate_keypair();
let signature = sign_data(&private_key, b"message")?;
```

## Tests

Run with:
```bash
cargo test -p pulsedag-crypto
```

## Warnings

- **Private key handling:** Keys are held in memory; ensure proper cleanup in production environments.
- **Random seed:** Uses `rand` crate for secure randomness; do not seed with predictable values.
