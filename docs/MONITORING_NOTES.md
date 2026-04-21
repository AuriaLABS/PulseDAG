# Monitorización básica

## Endpoints a vigilar
- `/health`
- `/runtime`
- `/p2p/runtime`
- `/sync/status`
- `/sync/lag`
- `/orphans`
- `/runtime/events?limit=50`

## Señales de problema
- `active_alert_count > 0` sostenido
- `sync_lag_blocks` creciendo
- `orphan_count` creciendo de forma sostenida
- `last_self_audit_ok = false`
- `startup_recovery_mode` inesperado en reinicios frecuentes
