# Security Policy

## Supported scope

PulseDAG v2.4.0 is under active validation. Security reports are accepted for the node, external miner, RPC/P2P surfaces, storage/replay logic, release tooling and GitHub Actions workflows.

The current repository state does **not** imply public-testnet GO, mainnet readiness, production custody, smart-contract activation or default high-cadence authorization.

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

A green CI/security workflow is evidence for one exact candidate only. Known RustSec dispositions remain fail-closed and do not become public-GO approval merely because their expected warning set is stable.

## v2.4.0 public-testnet boundary

Public launch remains controlled by issues #781, #794 and #803. Until an explicit `GO_PUBLIC_TESTNET` is recorded:

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- `contracts_enabled=false`
- no official public bootnode/RPC endpoint should be advertised as launched.
