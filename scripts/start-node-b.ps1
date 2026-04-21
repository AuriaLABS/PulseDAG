Get-Content .\config\testnet\node-b.env | ForEach-Object { if ($_ -match "=") { $k,$v = $_ -split "=",2; [System.Environment]::SetEnvironmentVariable($k,$v,"Process") } }
Get-Content .\.env.example | ForEach-Object { if ($_ -match "=") { $k,$v = $_ -split "=",2; if (-not [string]::IsNullOrWhiteSpace($k) -and -not [System.Environment]::GetEnvironmentVariable($k,"Process")) { [System.Environment]::SetEnvironmentVariable($k,$v,"Process") } } }
cargo run -p pulsedagd
