# Security policy

## Supported versions

PulseDAG is currently in private/public-testnet development. Security fixes are applied to the active release line and to the current default branch when applicable.

| Version / branch | Supported |
| --- | --- |
| `release/2.4.0` while active | Yes |
| `main` | Yes |
| Older development and release lines | No, unless a maintainer explicitly reopens support |

This policy does not declare mainnet readiness or production custody readiness.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability, leaked credential, private key, seed phrase, authentication bypass, consensus exploit or remotely triggerable denial of service.

Use GitHub's private vulnerability-reporting or Security Advisory flow for this repository when available:

1. open the repository **Security** tab;
2. choose **Report a vulnerability** or create a private security advisory;
3. include a clear impact statement, affected commit/version, reproduction steps and any proof of concept;
4. remove real private keys, wallet seeds, bearer tokens, unrestricted endpoints and personally identifying data.

If the private reporting control is unavailable, contact a repository maintainer privately and ask for a confidential reporting channel. Do not publish exploit details while coordination is in progress.

## What to include

A useful report contains:

- affected source SHA, release and platform;
- whether the issue affects consensus, storage/replay, P2P/sync, mining, RPC/API, wallet/key handling, release artifacts or operator safety;
- prerequisites and network exposure;
- deterministic reproduction steps;
- logs or evidence with secrets removed;
- expected and observed behavior;
- suggested mitigation when known.

## Severity guidance

### Critical / Sev-1

Examples include:

- remotely exploitable consensus or state divergence;
- accepted invalid block or transaction;
- loss or silent corruption of accepted chain state;
- private-key or seed disclosure;
- authentication bypass exposing privileged administration;
- remotely triggerable persistent node takeover;
- reproducible supply-chain compromise of official artifacts.

### High / Sev-2

Examples include:

- remotely triggerable node crash or sustained deadlock;
- snapshot/restore defect that can silently produce incorrect state;
- mining-submit ambiguity that can materially corrupt accounting or operational evidence;
- P2P behavior that prevents convergence across honest nodes;
- serious rate-limit or public-RPC exposure failure.

### Medium / Sev-3

Examples include:

- bounded denial of service requiring substantial resources;
- incorrect or missing security-relevant telemetry;
- operator misconfiguration that should fail closed but does not;
- dependency vulnerability without a demonstrated PulseDAG exploit path.

### Low / Sev-4

Examples include documentation defects, hardening opportunities and non-sensitive information disclosure with limited operational impact.

Final severity is assigned by maintainers after reproducing and assessing the report.

## Disclosure and remediation

Maintainers should:

1. acknowledge the report privately;
2. reproduce and classify it;
3. preserve evidence and identify affected SHAs/releases;
4. develop and validate a fix on a private branch or advisory fork when needed;
5. invalidate any affected release candidate or burn-in clock;
6. publish a coordinated advisory after a fix or mitigation is available.

No public-testnet GO decision may be recorded with an unresolved Sev-1 security issue.

## Testnet and custody boundaries

- Testnet coins have no monetary value and must not be marketed as investments or redeemable assets.
- PulseDAG wallet/key handling is not yet approved for production custody.
- Operators must use disposable testnet keys and must never reuse mainnet, exchange or personal high-value keys.
- Private keys, seeds, credentials and bearer tokens must never be committed, attached to issues or included in evidence bundles.
- Smart contracts, high-cadence consensus and mainnet claims remain outside the v2.4.0 public-testnet scope.

## Safe-harbor intent

Good-faith research that avoids privacy violations, data destruction, fund theft, persistence on third-party systems and unnecessary service disruption will be handled constructively. This statement is an intent to coordinate responsibly, not a waiver of applicable law or third-party terms.
