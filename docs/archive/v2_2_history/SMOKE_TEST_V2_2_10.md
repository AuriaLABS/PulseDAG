# PulseDAG v2.2.10 Smoke Test

1. `cargo fmt --check`
2. `cargo test --workspace`
3. `cargo build --workspace --release`
4. Start node.
5. `curl /health`
6. `curl /status`
7. `curl /release`
8. `curl /pow`
9. `curl /mining/template`
10. Run external miner.
11. Submit valid work.
12. Submit/attempt invalid work.
13. Check `/tips` and `/status`.
