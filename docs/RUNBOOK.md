# Runbook

## Start node
Load env file and run:
```powershell
cargo run -p pulsedagd
```

## Start miner
```powershell
cargo run -p pulsedag-miner -- --node http://127.0.0.1:8080 --miner-address YOUR_ADDRESS --threads 4 --loop --sleep-ms 1500 --max-tries 50000
```

## Check health
- `/health`
- `/status`
- `/runtime`
- `/p2p/runtime`
- `/sync/lag`
- `/readiness`

## If node falls behind
- inspect `/sync/status`
- inspect `/sync/verify`
- inspect `/orphans`
- if needed run rebuild
- then run mempool sanitize

## If runtime alerts grow
- inspect `/runtime/events?limit=50`
- inspect `/runtime/events/summary?limit=500`
- verify peers, lag, orphan count, mempool size
