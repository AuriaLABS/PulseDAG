# Testnet public checklist

## Mandatory
- [ ] final PoW dry-run completed via `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`
- [ ] dry-run topology executed (>=5 nodes, >=4 external miners, no pool logic)
- [ ] restart, churn, and recovery checks passed in dry-run evidence
- [ ] explicit dry-run go/no-go record (`artifacts/release-evidence/<run_id>/dry-run/go-no-go.md`)
- [ ] `cargo build` clean enough for release
- [ ] node starts with documented env only
- [ ] 3 nodes connect and sync
- [ ] external miner mines against at least 1 node
- [ ] blocks propagate across 3 nodes
- [ ] transactions propagate across 3 nodes
- [ ] `sync_lag_blocks` returns to 0 after catch-up
- [ ] snapshot auto works
- [ ] prune auto/manual works
- [ ] mempool bounded policy active
- [ ] orphan queue bounded and TTL-pruned
- [ ] rebuild after restart works
- [ ] runtime journal persists events
- [ ] seed nodes defined
- [ ] faucet or equivalent testnet funding path defined
- [ ] runbook available

## Strongly recommended
- [ ] 24h stable with 3 nodes
- [ ] 72h stable with 3-5 nodes
- [ ] 7-day stable pre-burn-in
- [ ] binary releases for Windows and Linux

## Hard gate before contracts
- [ ] public testnet stable
- [ ] **30 days burn-in completed**
- [ ] no persistent desync
- [ ] no repeated rebuild loops
- [ ] no pathological orphan/mempool growth
