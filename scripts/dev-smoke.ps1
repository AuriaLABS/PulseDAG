$ErrorActionPreference = "Stop"

Write-Host "== PulseDAG v0.8.1 smoke test =="

$env:PULSEDAG_CHAIN_ID="pulsedag-devnet"
$env:PULSEDAG_RPC_BIND="127.0.0.1:8080"
$env:PULSEDAG_P2P_ENABLED="true"
$env:PULSEDAG_P2P_MODE="memory"
$env:PULSEDAG_ROCKSDB_PATH=".\data\rocksdb"
$env:RUST_LOG="info"

Remove-Item -Recurse -Force .\data\rocksdb -ErrorAction SilentlyContinue

$p = Start-Process cargo -ArgumentList "run -p pulsedagd" -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 3

try {
  $health = Invoke-RestMethod http://127.0.0.1:8080/health
  $w1 = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/wallet/new -ContentType "application/json" -Body "{}"
  $w2 = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/wallet/new -ContentType "application/json" -Body "{}"

  $mineBody = @{ miner_address = $w1.data.address } | ConvertTo-Json
  $null = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/mine -ContentType "application/json" -Body $mineBody

  $txBody = @{
    from = $w1.data.address
    to = $w2.data.address
    amount = 10
    fee = 1
    private_key = $w1.data.private_key
  } | ConvertTo-Json

  $transfer = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/wallet/transfer -ContentType "application/json" -Body $txBody
  $null = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/mine -ContentType "application/json" -Body $mineBody

  $a1 = Invoke-RestMethod "http://127.0.0.1:8080/address/$($w1.data.address)"
  $a2 = Invoke-RestMethod "http://127.0.0.1:8080/address/$($w2.data.address)"

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
}
