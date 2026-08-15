$ErrorActionPreference = "Stop"

Write-Host "== PulseDAG dev smoke test (keyless node + local wallet signing) =="

$env:PULSEDAG_CHAIN_ID="pulsedag-devnet"
$env:PULSEDAG_RPC_BIND="127.0.0.1:8080"
$env:PULSEDAG_P2P_ENABLED="true"
$env:PULSEDAG_P2P_MODE="memory"
$env:PULSEDAG_ROCKSDB_PATH=".\data\rocksdb"
$env:PULSEDAG_ADMIN_ENABLED="true"
$env:RUST_LOG="info"

Remove-Item -Recurse -Force .\data\rocksdb -ErrorAction SilentlyContinue

Write-Host "Building keyless node and local wallet harness..."
& cargo build -p pulsedagd
if ($LASTEXITCODE -ne 0) { throw "pulsedagd build failed" }
& cargo build -p pulsedag-wallet --bin pulsedag-wallet-harness
if ($LASTEXITCODE -ne 0) { throw "wallet harness build failed" }

$nodeBin = ".\target\debug\pulsedagd.exe"
$walletBin = ".\target\debug\pulsedag-wallet-harness.exe"
$walletDir = Join-Path ([System.IO.Path]::GetTempPath()) ("pulsedag-dev-smoke-wallet-" + [Guid]::NewGuid().ToString("N"))
$keystorePath = Join-Path $walletDir "wallet.json"
$utxosPath = Join-Path $walletDir "sender-utxos.json"
[System.IO.Directory]::CreateDirectory($walletDir) | Out-Null
$walletPassword = [Convert]::ToHexString([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32)).ToLowerInvariant()

function Invoke-LocalWalletHarness {
  param([string[]]$Arguments)
  $json = $walletPassword | & $walletBin @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "local wallet harness failed"
  }
  return (($json | Out-String) | ConvertFrom-Json)
}

$init = Invoke-LocalWalletHarness @(
  "init",
  "--keystore", $keystorePath,
  "--network-profile", "local-dev",
  "--chain-id", $env:PULSEDAG_CHAIN_ID,
  "--receive-count", "2"
)
$sender = $init.receive[0].address
$receiver = $init.receive[1].address

$p = Start-Process $nodeBin -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 3

try {
  $health = Invoke-RestMethod http://127.0.0.1:8080/health

  $mineBody = @{ miner_address = $sender } | ConvertTo-Json
  $null = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/mine -ContentType "application/json" -Body $mineBody

  $utxos = Invoke-RestMethod "http://127.0.0.1:8080/address/$sender/utxos"
  $utxos | ConvertTo-Json -Depth 32 -Compress | Set-Content -Path $utxosPath -NoNewline

  $signed = Invoke-LocalWalletHarness @(
    "sign",
    "--keystore", $keystorePath,
    "--utxos-file", $utxosPath,
    "--network-profile", "local-dev",
    "--chain-id", $env:PULSEDAG_CHAIN_ID,
    "--to", $receiver,
    "--amount", "10",
    "--fee", "1",
    "--account", "0",
    "--branch", "receive",
    "--index", "0"
  )
  $submitBody = $signed | ConvertTo-Json -Depth 32 -Compress
  $transfer = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/api/v1/tx/submit -ContentType "application/json" -Body $submitBody

  $null = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/mine -ContentType "application/json" -Body $mineBody

  $a1 = Invoke-RestMethod "http://127.0.0.1:8080/address/$sender"
  $a2 = Invoke-RestMethod "http://127.0.0.1:8080/address/$receiver"

  [pscustomobject]@{
    health_ok = $health.ok
    transfer_ok = $transfer.ok
    sender_balance = $a1.data.balance
    receiver_balance = $a2.data.balance
  } | Format-List
}
finally {
  if ($p -and !$p.HasExited) {
    Stop-Process -Id $p.Id -Force
  }
  $walletPassword = $null
  Remove-Item -Recurse -Force $walletDir -ErrorAction SilentlyContinue
}
