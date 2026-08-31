# Security Policy

## Supported scope

PulseDAG v2.4.x remains the active repository/development baseline. The definitive public-launch target is **v3.0.0**, with coordinated **mainnet + parallel public testnet** launch planned for **Q4 2026** after the integrated launch gates complete.

The v3.0.0 acceptance scope incorporates:

- the v2.5 scale/resilience/P2P/GPU-mining workstream;
- the v2.6 programmability/smart-contract/verifiable-application workstream;
- production monetary policy, reward/fee/supply accounting and deterministic genesis;
- production wallet/custody;
- release, infrastructure and dual-network launch controls.

Security reports are accepted for the node, CPU/NVIDIA/AMD miner surfaces, wallet/custody, RPC/P2P, storage/replay/sync, monetary/reward/coinbase logic, genesis generator/ceremony, network configuration/bootstrap, contract/compiler/VM/proof surfaces, asset/application logic, release tooling and GitHub Actions workflows.

The current repository state does **not** itself imply v3.0.0 launch GO, mainnet readiness or production contract/wallet activation. Those claims require the exact integrated v3 candidate and final acceptance evidence.

## Report a vulnerability privately

Do **not** open a public GitHub issue for a suspected vulnerability.

Use GitHub private vulnerability reporting for this repository:

https://github.com/AuriaLABS/PulseDAG/security/advisories/new

Include the affected commit or release, operating system, configuration/profile, reproduction steps, expected/observed behavior, impact, logs with secrets removed, and whether the issue appears remotely reachable.

For contract/programming issues, also include the transaction/script/VM/proof version and a minimal deterministic reproducer where possible. For GPU-mining issues, include GPU family/runtime/driver and whether CPU-reference PoW disagrees. For monetary/genesis issues, include the policy/genesis/network digest, reward index and whether the defect changes issuance, spendability or network identity.

If the private-reporting UI is unavailable, do not publish exploit details. Contact a repository maintainer privately first and request a private reporting channel.

## Severity guide

- **Critical / Sev-1:** consensus safety failure, remote code execution, unauthorized key/custody compromise, chain/state or contract/application-state corruption, deterministic execution failure that can split the network, remotely exploitable arbitrary write, proof-validation bypass, hidden/unauthorized issuance, genesis-allocation mismatch, cross-network replay/identity collision, or a vulnerability that can permanently halt/split the network.
- **High / Sev-2:** remotely triggerable denial of service, authentication/authorization bypass, exploitable storage/replay corruption with recovery, material P2P/RPC isolation failure, bounded contract/proof validation bypass, incorrect coinbase maturity/reward accounting with bounded impact, or remotely triggerable GPU/miner control-path failure with network impact.
- **Medium / Sev-3:** bounded resource exhaustion, information disclosure, local privilege boundary weakness, application/indexing inconsistency without consensus impact, or operator-safety defect with realistic misuse.
- **Low / Sev-4:** defense-in-depth issue with limited impact or substantial prerequisites.

Do not include real private keys, wallet seeds, credentials, access tokens or production secrets in reports.

## Handling and disclosure

Maintainers should acknowledge valid reports, reproduce against an exact SHA, classify reachability, track remediation on a private advisory while exploit details remain sensitive, and publish coordinated disclosure only after a fix or explicit risk disposition is available.

A green CI/security workflow is evidence for one exact candidate only. Known RustSec dispositions remain fail-closed and do not become mainnet/public-launch approval merely because an expected warning set is stable.

Any security-affecting change to consensus, P2P, storage, GPU mining, wallet signing, contracts, VM, compiler, proof verification, assets, monetary/reward/fee policy, genesis construction, network identity or activation semantics after candidate freeze invalidates affected evidence and requires explicit rebaseline. A changed frozen genesis input creates a new network identity.

## Integrated v3.0.0 launch security boundary

Issues #781, #794, #803 and #819 are the active launch/security/custody controls. Before `GO_V3_DUAL_LAUNCH`:

- no unresolved reachable security issue may remain without an explicit reviewed mainnet/public-exposure disposition;
- exact-candidate dependency/reachability and secret-scanning evidence must pass;
- dependency review must cover node, miner/GPU, wallet, contract runtime/compiler, proof-system and genesis/release tooling targets included in v3;
- CPU/NVIDIA/AMD PoW implementations must remain canonically equivalent to the CPU reference path;
- P2P peer/resource/eclipsing controls must pass the incorporated v2.5 adversarial gates;
- contract/VM/proof execution must be deterministic, domain separated and resource bounded;
- contract/application state and economic/issuance semantics must replay deterministically;
- monetary policy must be explicit, content-addressed and bound to the exact candidate; no development/test constant or allocation may become production policy implicitly;
- production genesis must be deterministic and independently reproducible from an immutable input manifest and exact timestamp;
- production genesis must contain no placeholder/undocumented allocation such as `genesis-treasury`;
- canonical DAG reward-index, subsidy, coinbase-maturity, fee/burn and supply-accounting rules must be deterministic and covered by independent vectors;
- mainnet/testnet chain IDs, signing/network domains and genesis identities must be distinct and cross-network node/miner/wallet/application use must fail closed;
- `docs/V3_0_0_LAUNCH_MANIFEST.md` must be `FROZEN` with no launch-required `TBD`, and `scripts/validate_v3_0_0_network_freeze.py` must report `launch_ready=true`;
- wallet/custody claims must match the exact packaged v3.0.0 artifacts and frozen transaction/contract/network semantics;
- independent mainnet/testnet chain, signing and application/proof domains must be frozen and reviewed;
- admin/operator control planes must remain private and public-safe RPC/event/contract query surfaces must remain bounded and fail closed;
- the >=1,000,000-block DAG replay, >=1,000,000 programmable-operation replay and required burn-in/rehearsal evidence must be tied to the exact integrated candidate.

## Genesis ceremony security

The production genesis ceremony is a release-security event. It requires:

- immutable, content-addressed mainnet and testnet input manifests;
- reviewed genesis-generator source/build identity;
- independent clean-environment reproduction with byte-identical outputs;
- no undocumented environment input or runtime timestamp;
- no private keys, seed phrases, TLS private keys, cloud credentials or operator secrets embedded in genesis/config artifacts;
- exact allocation/supply reconciliation against the approved monetary policy;
- independent verification of genesis hashes, transaction IDs, state roots and config digests;
- hard stop on any mismatch, hidden allocation, identity collision or unreproducible output.

## Superseded public-testnet security sequencing

The old `GO_PUBLIC_TESTNET` decision and a pre-mainnet 30-day public-testnet clock are **not** v3.0.0 mainnet prerequisites.

The v2.5 standalone public-testnet canary/30-day acceptance gate and the v2.6 dependency on that clock are superseded as launch sequencing. Their technical security, determinism, GPU, contract and burn-in requirements are incorporated into the pre-launch v3 evidence program.

Mainnet and the parallel testnet are launched in one coordinated release window after the v3 GO decision.

## Legacy v2.4.x compatibility boundary

Historical v2.4.x tooling and evidence may still assert the following legacy fail-closed state while those validation surfaces remain in the repository:

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- `contracts_enabled=false`

These legacy fields preserve old validation semantics only. They do not define the integrated v3.0.0 scope and must not be interpreted as the current launch-control model.
