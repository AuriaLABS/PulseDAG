# PulseDAG v3.0.0 exact-candidate evidence ledger

This document defines the machine-readable evidence identity contract used by #1048 and, ultimately, the final #781 launch decision.

It does **not** freeze a candidate, create final network identities, or authorize launch. The validator may be merged while the candidate is moving; the real ledger is populated only after candidate-changing work stops.

## Why this exists

PulseDAG has many independent validation streams: consensus/replay, P2P, sync/pruning, mempool, mining, GPU, programmability, wallet, security, packages, burn-in and operations. A green result is useful only if it is provably attached to the same source tree, artifacts and network/config identities as the release decision.

The v3 ledger therefore rejects:

- evidence produced from a different source or tree object;
- evidence that references an unknown packaged artifact or undeclared configuration digest;
- a network config digest absent from the frozen config registry;
- a `PASS` record that has been invalidated;
- duplicate gate identifiers;
- malformed or non-canonical SHA-256 digests;
- malformed UTC evidence timestamps;
- mainnet and parallel-testnet identity collisions;
- a final decision on an unfrozen candidate;
- `GO_V3_DUAL_LAUNCH` while any recorded gate is not `PASS`.

## Validator

Run the dependency-free validator self-test:

```bash
python3 scripts/release/validate_v3_evidence_ledger.py --self-test
```

Validate a populated ledger:

```bash
python3 scripts/release/validate_v3_evidence_ledger.py docs/release/v3-evidence-ledger.json
```

The repository intentionally does not commit a fake final `v3-evidence-ledger.json` while the candidate remains unfrozen.

## Format v1

Top-level fields are strict; unknown fields fail closed.

- `format`: exactly `pulsedag-v3-evidence-ledger`.
- `format_version`: JSON integer exactly `1`; booleans are rejected.
- `candidate_frozen`: boolean.
- `candidate`: exact release/source/tree/protocol identity.
- `networks`: independent `mainnet` and `parallel_testnet` identities.
- `configs`: frozen registry of every network/evidence configuration digest referenced by the ledger.
- `artifacts`: exact packaged artifact, SBOM and provenance digests.
- `evidence`: one record per gate/evidence unit.
- `decision`: one #781 decision value.

### Candidate

The final ledger requires:

- `release_version = v3.0.0`;
- `source_sha`: canonical Git object ID;
- `tree_sha`: canonical Git tree object ID;
- `version_file = v3.0.0`;
- `cargo_workspace_version = 3.0.0`;
- protocol identities for P2P, transaction, mining, contract, VM, proof and storage;
- the frozen monetary-policy digest.

The validator accepts 40- or 64-hex Git object IDs so the format does not assume a particular Git object hash transition.

### Network identities

Both network objects require:

- network profile;
- chain ID;
- genesis hash;
- config digest;
- bootnode-identity digest;
- signing domain;
- application domain.

Every one of those values must differ between mainnet and the parallel public testnet. This is intentionally stricter than checking only `chain_id`.

Each network's `config_digest` must also exist in the top-level frozen `configs` registry. This prevents a syntactically valid but old/unrelated config digest from being attached to PASS evidence.

### Frozen config registry

Each `configs` entry contains:

- unique config name;
- role/purpose;
- canonical SHA-256 digest.

Evidence may reference only digests declared in this registry. Additional exact-candidate rehearsal/adversarial configs may be registered alongside the two production network configs, but undeclared digests fail closed.

### Artifacts

Each artifact record contains:

- unique artifact name;
- platform identity;
- artifact SHA-256;
- SBOM SHA-256;
- provenance SHA-256.

SHA-256 values use canonical `sha256:<64 lowercase hex>` form.

### Evidence records

Every evidence record contains:

- unique `gate_id`;
- `PASS`, `FAIL` or `PENDING` status;
- exact `source_sha` and `tree_sha`, which must equal the candidate;
- referenced artifact SHA-256 values, all of which must exist in `artifacts`;
- referenced config SHA-256 values, all of which must exist in `configs`;
- strict UTC RFC3339 start/end timestamps in `YYYY-MM-DDTHH:MM:SS[.fraction]Z` form;
- evidence bundle/content digest;
- explicit invalidation state.

An invalidated record cannot remain `PASS`.

Long-running evidence such as the 168-hour integrated burn-in and the 30 accepted programmability days must record their real UTC boundaries. A candidate-changing fix requires the affected evidence to be invalidated/rebaselined rather than silently carried forward.

## Launch decision semantics

Allowed values are:

- `PENDING_V3_DUAL_LAUNCH`;
- `GO_V3_DUAL_LAUNCH`;
- `DELAY_V3_DUAL_LAUNCH`;
- `NO_GO_V3_DUAL_LAUNCH`.

An unfrozen candidate may only use `PENDING_V3_DUAL_LAUNCH`. The validator self-tests both the accepted unfrozen/pending state and rejection of an unfrozen GO.

`GO_V3_DUAL_LAUNCH` additionally requires `candidate_frozen=true` and every ledger evidence record to be `PASS`. Passing this validator is necessary evidence-integrity hygiene, not sufficient launch authorization: #781 remains the sole human/control-plane authority for the final decision.

## Relationship to historical evidence

Historical #789, #803, #819 and v2.x release evidence can be referenced as provenance or test design input, but cannot be copied into the final ledger as exact-candidate PASS evidence unless it is rerun or independently proven invariant and represented under the exact frozen v3 identity.
