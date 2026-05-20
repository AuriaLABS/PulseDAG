Write-Host "1) Parar nodo B manualmente"
Write-Host "2) Arrancarlo otra vez con scripts/start-node-b.ps1"
Write-Host "3) Verificar recovery y lag"
Invoke-RestMethod http://127.0.0.1:8081/health
Invoke-RestMethod http://127.0.0.1:8081/runtime
Invoke-RestMethod http://127.0.0.1:8081/sync/status
Invoke-RestMethod http://127.0.0.1:8081/runtime/events?limit=20
