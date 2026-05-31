# Codex task: restore main build before v2.3.0 work

Priority: P0 / blocking.

Goal: repair the current main branch after the orphan recovery merge so the workspace builds and tests pass before any v2.3.0 work.

Required checks:

```bash
cargo check --workspace --locked
cargo test --workspace
cargo fmt --all -- --check
bash scripts/v2_2_19_preflight_check.sh
```

Scope:

- fix orphan recovery API compile errors;
- fix pulsedag-core exports used by pulsedagd and RPC;
- repair orphan queue/adoption return types;
- add or repair orphan unit tests;
- keep VERSION and Cargo workspace at v2.2.19.

Guardrails:

- no consensus rule changes;
- no supply, reward, PoW, or difficulty changes;
- do not enable smart contracts;
- do not set public_testnet_ready true;
- keep miner external.
