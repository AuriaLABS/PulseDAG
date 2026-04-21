$ErrorActionPreference = "Stop"

Write-Host "[1/3] snapshot"
Invoke-RestMethod http://127.0.0.1:8080/snapshot | Out-Host

Write-Host "[2/3] replay plan"
Invoke-RestMethod http://127.0.0.1:8080/sync/replay-plan | Out-Host

Write-Host "[3/3] rebuild preview"
Invoke-RestMethod http://127.0.0.1:8080/sync/rebuild-preview | Out-Host
