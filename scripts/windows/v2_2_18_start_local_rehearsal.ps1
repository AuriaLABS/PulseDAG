$ErrorActionPreference = 'Stop'

param(
  [string]$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path,
  [string]$EvidenceRoot = '',
  [switch]$UseGpuMiner
)

$timestamp = Get-Date -Format 'yyyyMMdd_HHmmss'
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
  $EvidenceRoot = Join-Path $WorkspaceRoot "evidence\windows_wsl_rehearsal_v2_2_18_$timestamp"
}
New-Item -ItemType Directory -Path $EvidenceRoot -Force | Out-Null
$logsDir = Join-Path $EvidenceRoot 'logs'
New-Item -ItemType Directory -Path $logsDir -Force | Out-Null

Push-Location $WorkspaceRoot
try {
  Write-Host '[1/4] Build workspace'
  cargo build --workspace

  $nodeBin = Join-Path $WorkspaceRoot 'target\debug\pulsedagd.exe'
  $minerBin = Join-Path $WorkspaceRoot 'target\debug\pulsedag-miner.exe'
  if (!(Test-Path $nodeBin)) { throw "Missing node binary: $nodeBin" }
  if (!(Test-Path $minerBin)) { throw "Missing miner binary: $minerBin" }

  Write-Host '[2/4] Start 3 local nodes (localhost RPC, no admin exposure)'
  $nodes = @(
    @{ Name='node-a'; Rpc=18080; P2p=19080 },
    @{ Name='node-b'; Rpc=18081; P2p=19081 },
    @{ Name='node-c'; Rpc=18082; P2p=19082 }
  )

  $meta = @()
  foreach ($n in $nodes) {
    $dataDir = Join-Path $EvidenceRoot ("data_{0}" -f $n.Name)
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    $logPath = Join-Path $logsDir ("{0}.log" -f $n.Name)

    $args = @(
      '--rpc-bind', ("127.0.0.1:{0}" -f $n.Rpc),
      '--p2p-bind', ("127.0.0.1:{0}" -f $n.P2p),
      '--data-dir', $dataDir,
      '--no-admin'
    )

    $proc = Start-Process -FilePath $nodeBin -ArgumentList $args -RedirectStandardOutput $logPath -RedirectStandardError $logPath -PassThru
    $meta += [pscustomobject]@{ role='node'; name=$n.Name; pid=$proc.Id; rpc=("http://127.0.0.1:{0}" -f $n.Rpc); p2p=("127.0.0.1:{0}" -f $n.P2p); log=$logPath }
  }

  Write-Host '[3/4] Start 1 external CPU miner (GPU optional and disabled by default)'
  $minerLog = Join-Path $logsDir 'miner.log'
  $minerArgs = @('--rpc', 'http://127.0.0.1:18080', '--mode', 'cpu', '--threads', '1')
  if ($UseGpuMiner) {
    Write-Host 'GPU flag requested; miner will receive --mode gpu.'
    $minerArgs = @('--rpc', 'http://127.0.0.1:18080', '--mode', 'gpu')
  }
  $minerProc = Start-Process -FilePath $minerBin -ArgumentList $minerArgs -RedirectStandardOutput $minerLog -RedirectStandardError $minerLog -PassThru
  $meta += [pscustomobject]@{ role='miner'; name='external-miner'; pid=$minerProc.Id; rpc='http://127.0.0.1:18080'; log=$minerLog }

  Write-Host '[4/4] Persist rehearsal state'
  $statePath = Join-Path $EvidenceRoot 'processes.json'
  $meta | ConvertTo-Json -Depth 5 | Set-Content -Path $statePath -Encoding UTF8
  Set-Content -Path (Join-Path $EvidenceRoot 'WORKSPACE_ROOT.txt') -Value $WorkspaceRoot -Encoding UTF8

  Write-Host "Rehearsal started. Evidence dir: $EvidenceRoot"
  Write-Host "Next: run scripts/windows/v2_2_18_collect_local_evidence.ps1 -EvidenceRoot `"$EvidenceRoot`""
}
finally {
  Pop-Location
}
