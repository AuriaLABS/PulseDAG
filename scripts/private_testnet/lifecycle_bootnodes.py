#!/usr/bin/env python3
"""Resolve complete libp2p bootnode multiaddrs for private-testnet lifecycle operations."""

from __future__ import annotations

import re
import socket

from lifecycle_core import LifecycleError

DNS_BOOTNODE = re.compile(r"^/(dns4|dns6)/([^/]+)/tcp/([0-9]+)/p2p/([^/]+)$")
IP_BOOTNODE = re.compile(r"^/(ip4|ip6)/([^/]+)/tcp/([0-9]+)/p2p/([^/]+)$")


def resolve_bootnodes(values: dict[str, str], allow_unresolved: bool) -> list[dict[str, object]]:
    """Resolve bootnodes while retaining the peer ID required by libp2p dialing."""

    bootstrap = values.get("PULSEDAG_P2P_BOOTSTRAP", "").strip()
    results: list[dict[str, object]] = []
    for raw in (entry.strip() for entry in bootstrap.split(",")):
        if not raw:
            continue

        dns_match = DNS_BOOTNODE.fullmatch(raw)
        ip_match = IP_BOOTNODE.fullmatch(raw)
        if dns_match:
            family_name, host, port_raw, peer_id = dns_match.groups()
            family = socket.AF_INET if family_name == "dns4" else socket.AF_INET6
            try:
                addresses = sorted(
                    {
                        entry[4][0]
                        for entry in socket.getaddrinfo(
                            host,
                            int(port_raw),
                            family=family,
                            type=socket.SOCK_STREAM,
                        )
                    }
                )
            except socket.gaierror as exc:
                if not allow_unresolved:
                    raise LifecycleError(f"bootnode DNS resolution failed for {raw}: {exc}") from exc
                addresses = []
            results.append(
                {
                    "multiaddr": raw,
                    "peer_id": peer_id,
                    "resolved_addresses": addresses,
                }
            )
        elif ip_match:
            _family_name, host, _port_raw, peer_id = ip_match.groups()
            results.append(
                {
                    "multiaddr": raw,
                    "peer_id": peer_id,
                    "resolved_addresses": [host],
                }
            )
        else:
            raise LifecycleError(
                "unsupported bootnode multiaddr; expected "
                f"/ip4|ip6|dns4|dns6/<host>/tcp/<port>/p2p/<peer-id>: {raw}"
            )
    return results
