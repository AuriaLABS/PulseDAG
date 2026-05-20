# Ejemplo de prueba manual con dos nodos
# Nodo A
# cargo run -p pulsedagd
# Nodo B
# cargo run -p pulsedagd -- --rpc-bind 127.0.0.1:8081

Invoke-RestMethod http://127.0.0.1:8080/p2p/runtime
Invoke-RestMethod http://127.0.0.1:8080/sync/lag
Invoke-RestMethod http://127.0.0.1:8081/p2p/runtime
Invoke-RestMethod http://127.0.0.1:8081/sync/lag
