# PulseDAG first-network release evidence policy

## Current candidate

Software version: v3.0.0. Final source SHA and native network identity are not frozen by this document. Decision remains `PENDING_EXACT_CANDIDATE_EVIDENCE`. Public testnet is the first live native monetary network; mainnet follows independently. The sole launch authority is [#781](https://github.com/AuriaLABS/PulseDAG/issues/781).

## Evidence classes

Bind each acceptance record to the exact source/tree SHA, network identity, genesis and configuration digests, artifact digests, workflow run and UTC timestamp.

1. Locked Cargo metadata, formatting, compilation, workspace tests and Clippy.
2. Native consensus and monetary policy: deterministic parent/order/work, PoW/retarget, signed UTXO admission, fees, rewards, supply, state roots, atomic persistence and fail-closed invalid-input handling.
3. Chain/signing domains, externally pinned genesis, network-specific storage and wallet identity, restart and authenticated checkpoint/replay safety.
4. Applicable local P2P, RPC, resource limits, dependency and keyless-node security regressions.
5. Native node and standalone CPU miner package builds and smoke checks on every approved target; wallet tooling and its explicit custody boundary.
6. Per-archive manifests, SHA-256 checksums, provenance, install verification and independent review.
7. Approved operator roles, bootnodes, DNS/TLS, credentials, observability, incident response, backup, rollback and reset policy.

Apply the [T0/T1/M0 sequence](runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md): minimum safe native implementation and local evidence before testnet GO; genuine public testnet validation after authorization; at least 30 consecutive meaningful monitored days and separate exact-candidate mainnet acceptance thereafter. No private live-network prerequisite. Bounded contract core READY is required before mainnet, but contracts remain INACTIVE throughout v3.0.0.

## Artifact and decision rules

- Node and external miner are separate assets. Verify the [packaging and checksum contract](release/ARTIFACTS.md) and [installation instructions](INSTALL_BINARIES.md).
- Do not advertise production wallet custody or GPU hardware readiness without specific evidence.
- Preserve evidence independently of GitHub Release publication. Record owner, UTC decision, rationale, open risks, security dispositions, rollback conditions and artifact identities.
- Any relevant change to code, dependency, configuration, consensus, storage, signing domain, package or validation workflow invalidates affected evidence. Never aggregate incompatible candidates into a single final PASS.
- A failed build, digest mismatch, unresolved integrity/security failure or unsafe recovery blocks the affected decision.
- No tag or publication without separate maintainer authorization; neither authorizes a network launch.
- PRE-GO remains `public_testnet_ready=false`, `thirty_day_public_testnet_clock_started=false`, `contracts_enabled=false`. No mining activation, Day 0, high cadence or mainnet GO by inference.

## Historical provenance

The superseded prototype [release decision](https://github.com/AuriaLABS/PulseDAG/blob/49f2c05deabec5fad2473fdf631682e1386c391f/docs/release/V2_3_0_RELEASE_DECISION.md), [approval record](https://github.com/AuriaLABS/PulseDAG/blob/49f2c05deabec5fad2473fdf631682e1386c391f/docs/release/V2_3_0_RELEASE_APPROVAL_RECORD.md) and [closeout checklist](https://github.com/AuriaLABS/PulseDAG/blob/49f2c05deabec5fad2473fdf631682e1386c391f/docs/checklists/V2_3_0_PRIVATE_TESTNET_RELEASE_CLOSEOUT.md) remain available in immutable Git history. They are not current authorization, migration obligations or first-network acceptance. Preserve applicable regression coverage, not historical release gates.
