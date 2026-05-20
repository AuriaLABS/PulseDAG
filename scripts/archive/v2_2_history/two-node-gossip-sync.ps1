
# Smoke manual 2 nodos
# 1. Arrancar nodo A
# 2. Arrancar nodo B con bootstrap a A
# 3. Consultar:
#    Invoke-RestMethod http://127.0.0.1:8080/p2p/runtime
#    Invoke-RestMethod http://127.0.0.1:8080/sync/lag
# 4. Minar en A
# 5. Confirmar que B reduce lag
