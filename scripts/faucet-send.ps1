param([string]$From,[string]$To,[string]$PrivateKey,[int]$Amount=10,[int]$Fee=1)
$body = @{ from=$From; to=$To; amount=$Amount; fee=$Fee; private_key=$PrivateKey } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8080/wallet/transfer -ContentType "application/json" -Body $body
