Invoke-RestMethod http://127.0.0.1:8080/mempool
Invoke-RestMethod -Method Post http://127.0.0.1:8080/mempool/sanitize
Invoke-RestMethod http://127.0.0.1:8080/runtime
Invoke-RestMethod "http://127.0.0.1:8080/runtime/events?limit=20"
