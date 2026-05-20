# PulseDAG v2.2.9 Smoke Test

1. `cargo fmt --check`
2. `cargo test --workspace`
3. `cargo build --workspace`
4. Start node A.
5. Start node B.
6. Optionally start node C.
7. Check endpoints if available:
   - `/health`
   - `/status`
   - `/release`
   - `/pow`
   - `/tips`
   - `/runtime`
   - `/metrics`
   - `/p2p/status`
8. Run external miner against node A.
9. Verify accepted block (or document mining attempt evidence if not accepted in-window).
10. Check propagation/catch-up behavior across peers.
11. Stop and restart node B.
12. Confirm catch-up behavior (or document limitation).
