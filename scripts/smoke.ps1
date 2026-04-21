$ErrorActionPreference = "Stop"

Write-Host "[1/6] health"
Invoke-RestMethod http://127.0.0.1:8080/health | Out-Host

Write-Host "[2/6] status"
Invoke-RestMethod http://127.0.0.1:8080/status | Out-Host

Write-Host "[3/6] checks"
Invoke-RestMethod http://127.0.0.1:8080/checks | Out-Host

Write-Host "[4/6] dashboard"
Invoke-RestMethod http://127.0.0.1:8080/dashboard | Out-Host

Write-Host "[5/6] snapshot"
Invoke-RestMethod http://127.0.0.1:8080/snapshot | Out-Host

Write-Host "[6/6] replay plan"
Invoke-RestMethod http://127.0.0.1:8080/sync/replay-plan | Out-Host
