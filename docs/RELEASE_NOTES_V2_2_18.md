# PulseDAG v2.2.18 release notes

## Scope
v2.2.18 is a **private-testnet RC preparation** release.

## What changed in v2.2.18
- Version metadata alignment: `VERSION=v2.2.18`, Cargo workspace version `2.2.18`.
- Added preflight and local evidence gates for operators.
- Added local 3-node / 1-miner smoke helper and evidence collector scripts.

## Non-goals / unchanged behavior
- No consensus changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- GPU is optional/scaffold only unless canonical kernel evidence exists.
- v2.3.0 remains a future readiness decision.
- No v3.0 readiness claim.
