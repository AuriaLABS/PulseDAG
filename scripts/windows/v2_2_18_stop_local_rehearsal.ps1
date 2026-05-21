$ErrorActionPreference = 'Stop'

param(
  [Parameter(Mandatory = $true)]
  [string]$EvidenceRoot
)

$statePath = Join-Path $EvidenceRoot 'processes.json'
if (!(Test-Path $statePath)) { throw "Missing process state file: $statePath" }

$procs = Get-Content $statePath | ConvertFrom-Json
foreach ($p in $procs) {
  try {
    $proc = Get-Process -Id $p.pid -ErrorAction Stop
    Stop-Process -Id $proc.Id -Force
    Write-Host "Stopped $($p.role) $($p.name) pid=$($p.pid)"
  }
  catch {
    Write-Host "Process already exited or unavailable: $($p.name) pid=$($p.pid)"
  }
}
