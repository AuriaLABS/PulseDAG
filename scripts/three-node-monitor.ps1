Invoke-RestMethod http://127.0.0.1:8080/health | Out-Host
Invoke-RestMethod http://127.0.0.1:8081/health | Out-Host
Invoke-RestMethod http://127.0.0.1:8082/health | Out-Host
try { Invoke-RestMethod http://127.0.0.1:8080/p2p/runtime | Out-Host } catch {}
try { Invoke-RestMethod http://127.0.0.1:8081/p2p/runtime | Out-Host } catch {}
try { Invoke-RestMethod http://127.0.0.1:8082/p2p/runtime | Out-Host } catch {}
