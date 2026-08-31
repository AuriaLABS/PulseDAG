# Security Policy

## Supported scope

PulseDAG v2.4.x remains an active validation/development line. The definitive public-launch target is **v3.0.0**, with coordinated **mainnet + parallel public testnet** launch planned for **Q4 2026** after the v3 launch gates complete.

Security reports are accepted for the node, external miner, wallet/custody surfaces, RPC/P2P surfaces, storage/replay logic, release tooling and GitHub Actions workflows.

The current repository state does **not** imply v3.0.0 launch GO, mainnet readiness, production custody, smart-contract activation or default high-cadence authorization.

## Report a vulnerability privately

Do **not** open a public GitHub issue for a suspected vulnerability.

Use GitHub private vulnerability reporting for this repository:

https://github.com/AuriaLABS/PulseDAG/security/advisories/new

Include the affected commit or release, operating system, configuration/profile, reproduction steps, expected/observed behavior, impact, logs with secrets removed, and whether the issue appears remotely reachable.

If the private-reporting UI is unavailable, do not publish exploit details. Contact a repository maintainer privately first and request a private reporting channel.

## Severity guide

- **Critical / Sev-1:** consensus safety failure, remote code execution, unauthorized key/custody compromise, chain/state corruption, remotely exploitable arbitrary write, or a vulnerability that can split or permanently halt the network.
- **High / Sev-2:** remotely triggerable denial of service, authentication/authorization bypass, exploitable storage/replay corruption with recovery, or material P2P/RPC isolation failure.
- **Medium / Sev-3:** bounded resource exhaustion, information disclosure, local privilege boundary weakness, or operator-safety defect with realistic misuse.
- **Low / Sev-4:** defense-in-depth issue with limited impact or prerequisites.

Do not include real private keys, wallet seeds, credentials, access tokens or production secrets in reports.

## Handling and disclosure

Maintainers should acknowledge valid reports, reproduce against an exact SHA, classify reachability, track remediation on a private advisory while exploit details remain sensitive, and publish coordinated disclosure only after a fix or explicit risk disposition is available.

A green CI/security workflow is evidence for one exact candidate only. Known RustSec dispositions remain fail-closed and do not become mainnet/public-launch approval merely because their expected warning set is stable.

## v3.0.0 launch security boundary

Issues #781, #794, #803 and #819 are the active launch/security/custody controls. Before `GO_V3_DUAL_LAUNCH`:

- no unresolved reachable security issue may remain without an explicit reviewed mainnet/public-exposure disposition;
- exact-candidate dependency/reachability and secret-scanning evidence must pass;
- wallet/custody claims must match the exact packaged v3.0.0 artifacts;
- independent mainnet/testnet chain identities and public endpoints must be frozen and reviewed;
- admin/operator control planes must remain private and public-safe RPC must remain fail-closed.

The old `GO_PUBLIC_TESTNET` decision and a 30-day public-testnet clock are **not** v3.0.0 mainnet prerequisites. Mainnet and the parallel testnet are launched in one coordinated release window after the v3 GO decision.

## Legacy v2.4.x compatibility boundary

Historical v2.4.x tooling and evidence may still assert the following legacy fail-closed state while those validation surfaces remain in the repository:

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- `contracts_enabled=false`

These legacy fields preserve old validation semantics only. They do not authorize or schedule the v3.0.0 launch and must not be interpreted as the current launch-control model.
