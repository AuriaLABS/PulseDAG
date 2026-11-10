# Native-v3 signed checkpoint, externally pinned verification — DORMANT

Tracking: [#1367](https://github.com/AuriaLABS/PulseDAG/issues/1367)

This PR adds a **real Ed25519 signature check in Rust** rather than a textual checksum-only or self-proclaimed signed manifest. The separately provided public key and exact expected checkpoint, snapshot, selected tip, consensus digest, monetary digest, chain ID and genesis hash are compared to the candidate. The signed preimage uses an explicit domain/version, bounded length-prefixed chain identity, 32-byte field commitments and fixed-width little-endian counters.

## What this implements

- Verifies the certificate's signature **against an independently supplied key**, never against the untrusted certificate's own authority.
- Enforces separately pinned minimum height, blue score and monotonic sequence with a separate **previously committed accepted position**: no accepted-sequence replay, no lower height/score or identity substitution.
- Rejects forged signature, attacker signing key, changed payload digests, wrong network and malformed/zero commitments. Unit tests use deterministic local test signing keys only.
- Returns explicit negative trust/GO fields even after a genuine signature verifies.
- No P2P/daemon/snapshot restoration or monetary admission callsite; the dedicated CI explicitly checks that boundary.

## Still **not** trustworthy production checkpoints

The verification API cannot prove that the external pin/key and previously accepted position themselves were acquired or stored securely. It does **not** hash actual snapshot bytes, authenticate an entire DAG ancestry after pruning, derive approved selected parent/K/blue-red/GHOSTDAG authority, independently prove fees/rewards/UTXO/retarget parity, or establish checkpoint finality. A malicious *external* authority can still sign false state; a compromised pin can still authorise bad data. Do not enable v3 pruned restoration or treat the returned signature verdict as monetary authority.

Remaining tasks: independently authenticated pin/key provisioning, monotonic persistence and rotation/revocation, binding snapshot bytes and full/pruned same-corpus state roots, approved GD-08 and native-GHOSTDAG policy, adversarial restart/reorg vectors and production-level threat review.

**Testnet/mainnet NO-GO · Monetary mining OFF · Contracts INACTIVE.**
