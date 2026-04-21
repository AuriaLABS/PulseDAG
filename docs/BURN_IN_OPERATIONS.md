# Burn-in operations (30 days)

Before contracts, the node/testnet must run well for 30 days.

## Daily checks
- `/health`
- `/runtime`
- `/runtime/events?limit=50`
- `/runtime/events/summary?limit=200`
- `/dag/consistency`
- `/sync/status`
- `/orphans`
- `/mempool`

## Watch for
- sustained orphan growth
- repeated startup rebuilds
- repeated consistency issues
- mempool growth without confirmation
- height stalls beyond expected block interval windows

## Weekly actions
- run manual mempool sanitize
- verify snapshot freshness
- inspect runtime event summary
