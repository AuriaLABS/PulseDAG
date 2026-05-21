$ErrorActionPreference = 'Stop'

param(
  [Parameter(Mandatory = $true)]
  [string]$EvidenceRoot
)

if (!(Test-Path $EvidenceRoot)) { throw "Evidence root not found: $EvidenceRoot" }
$statePath = Join-Path $EvidenceRoot 'processes.json'
if (!(Test-Path $statePath)) { throw "Missing process state file: $statePath" }

$apiDir = Join-Path $EvidenceRoot 'api'
New-Item -ItemType Directory -Path $apiDir -Force | Out-Null

$targets = @('/health','/status','/release','/readiness','/p2p/status','/sync/status','/pow/metrics')
$nodes = Get-Content $statePath | ConvertFrom-Json | Where-Object { $_.role -eq 'node' }

foreach ($node in $nodes) {
  foreach ($path in $targets) {
    $safeName = $path.TrimStart('/').Replace('/','_')
    $outFile = Join-Path $apiDir ("{0}_{1}.json" -f $node.name, $safeName)
    $url = "$($node.rpc)$path"
    try {
      $resp = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec 5
      $resp | ConvertTo-Json -Depth 20 | Set-Content -Path $outFile -Encoding UTF8
    }
    catch {
      if ($path -eq '/pow/metrics') {
        Set-Content -Path $outFile -Value '{"skipped":"endpoint unavailable"}' -Encoding UTF8
      }
      else {
        Set-Content -Path $outFile -Value ("{`"error`":`"{0}`"}" -f $_.Exception.Message.Replace('"','\"')) -Encoding UTF8
      }
    }
  }
}

$summaryPath = Join-Path $EvidenceRoot 'summary.md'
$time = Get-Date -Format o
$processes = Get-Content $statePath | ConvertFrom-Json

$lines = @(
  '# Windows/WSL Local Rehearsal Evidence (v2.2.18)',
  '',
  "- collected_at: $time",
  "- evidence_root: $EvidenceRoot",
  '- scenario: build workspace + 3 local nodes + 1 external CPU miner',
  '- defaults: localhost RPC, no admin exposure, CPU miner default, GPU optional',
  '',
  '## Processes',
  ''
)
foreach ($p in $processes) {
  $lines += "- $($p.role): $($p.name) pid=$($p.pid) rpc=$($p.rpc) log=$($p.log)"
}
$lines += ''
$lines += '## API captures'
$lines += ''
$lines += '- Endpoints captured per node: /health, /status, /release, /readiness, /p2p/status, /sync/status, /pow/metrics (if available).'
$lines += "- Stored under: $apiDir"
$lines += ''
$lines += '## Logs'
$lines += ''
$lines += "- Stored under: $(Join-Path $EvidenceRoot 'logs')"

$lines | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "Evidence collected at: $EvidenceRoot"
Write-Host "Summary: $summaryPath"
