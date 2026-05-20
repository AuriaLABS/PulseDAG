Invoke-RestMethod http://127.0.0.1:8080/health | Out-Host
Invoke-RestMethod http://127.0.0.1:8080/runtime | Out-Host
Invoke-RestMethod http://127.0.0.1:8080/readiness | Out-Host
Invoke-RestMethod http://127.0.0.1:8080/runtime/events/summary?limit=500 | Out-Host
