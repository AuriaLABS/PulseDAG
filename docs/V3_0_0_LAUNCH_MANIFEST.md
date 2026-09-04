# PulseDAG v3.0.0 launch manifest

This control-plane manifest is intentionally **PRE_FREEZE** while the exact
candidate and network ceremonies are incomplete. `TBD` values are launch
blockers, not defaults.

Validate it with:

```bash
python3 scripts/validate_v3_0_0_network_freeze.py
```

```json
{
  "format": "pulsedag-v3-launch-manifest",
  "manifest_version": 1,
  "launch_state": "PRE_FREEZE",
  "decision": "DELAY_V3_DUAL_LAUNCH",
  "exact_candidate": {
    "release": "v3.0.0",
    "source_sha": "TBD",
    "tree_sha": "TBD",
    "monetary_policy_digest": "TBD",
    "config_digest": "TBD"
  },
  "mainnet": {
    "chain_id": "TBD",
    "genesis_hash": "TBD",
    "signing_domain": "TBD",
    "bootnode_identity_digest": "TBD"
  },
  "parallel_testnet": {
    "chain_id": "TBD",
    "genesis_hash": "TBD",
    "signing_domain": "TBD",
    "bootnode_identity_digest": "TBD"
  },
  "assertions": {
    "network_identity_separation": "PENDING",
    "genesis_reproducibility": "PENDING",
    "cross_network_mismatch_fails_closed": "PENDING",
    "artifacts_and_evidence_exact_candidate": "PENDING"
  }
}
```

The validator reports `launch_ready=false` until the state is `FROZEN`, the
decision is `GO_V3_DUAL_LAUNCH`, all assertions are `PASS`, and no launch
identity contains `TBD`.
