use super::*;
use crate::live_protocol_sync_v1::protocol_sync_peer_is_authorized;
use crate::messages::compact_relay_carrier_v1::{
    attach_compact_relay_carrier_v1, decode_network_message_with_compact_relay_for_peer_v1,
    CompactRelayCarrierV1, CompactRelayWireV1, COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
};

fn encode_compact_relay_for_state(
    state: &InnerState,
    peer_id: &str,
    wire: &CompactRelayWireV1,
) -> Result<Vec<u8>, PulseError> {
    let protocol_route_authorized = protocol_sync_peer_is_authorized(state, peer_id);
    state
        .compact_relay_runtime
        .validate_outbound(peer_id, protocol_route_authorized, wire)
        .map_err(|error| {
            PulseError::Internal(format!(
                "compact-relay live transport authorization failed: {error:?}"
            ))
        })?;

    let inventory = current_tip_inventory(state, &state.chain_id);
    let tips = inventory
        .as_ref()
        .and_then(|inventory| inventory.selected_tip.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let base = state
        .protocol_capability_transport
        .encode_tip_message(&NetworkMessage::Tips {
            chain_id: state.chain_id.clone(),
            tips,
            inventory,
        })
        .map_err(|error| {
            PulseError::Internal(format!(
                "compact-relay capability carrier encode failed: {error:?}"
            ))
        })?;
    let encoded = attach_compact_relay_carrier_v1(
        &base,
        &CompactRelayCarrierV1 {
            target_peer_id: peer_id.to_string(),
            chain_id: state.chain_id.clone(),
            wire: wire.clone(),
        },
    )
    .map_err(|error| {
        PulseError::Internal(format!("compact-relay carrier encode failed: {error:?}"))
    })?;
    if encoded.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 {
        return Err(PulseError::Internal(format!(
            "compact-relay carrier exceeds live transport byte budget: encoded={} maximum={}",
            encoded.len(),
            COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1
        )));
    }
    Ok(encoded)
}

pub(super) fn validate_compact_relay_send(
    state: &InnerState,
    peer_id: &str,
    wire: &CompactRelayWireV1,
) -> Result<(), PulseError> {
    encode_compact_relay_for_state(state, peer_id, wire).map(|_| ())
}

pub(super) fn encode_compact_relay_for_transport(
    inner: &Arc<Mutex<InnerState>>,
    chain_id: &str,
    peer_id: &str,
    wire: &CompactRelayWireV1,
) -> Result<Vec<u8>, serde_json::Error> {
    {
        let guard = inner
            .lock()
            .map_err(|_| <serde_json::Error as serde::ser::Error>::custom("p2p lock poisoned"))?;
        if guard.chain_id != chain_id {
            return Err(<serde_json::Error as serde::ser::Error>::custom(format!(
                "compact-relay runtime chain mismatch: state={} transport={chain_id}",
                guard.chain_id
            )));
        }
        validate_compact_relay_send(&guard, peer_id, wire).map_err(|error| {
            <serde_json::Error as serde::ser::Error>::custom(format!(
                "compact-relay authorization changed before publish: {error:?}"
            ))
        })?;
    }

    let inventory = inner.lock().ok().and_then(|mut guard| {
        current_tip_inventory_for_send(&mut guard, chain_id, "CompactRelayV1")
    });
    let tips = inventory
        .as_ref()
        .and_then(|inventory| inventory.selected_tip.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let base = encode_network_message_for_transport(
        inner,
        &NetworkMessage::Tips {
            chain_id: chain_id.to_string(),
            tips,
            inventory,
        },
    )?;
    let encoded = attach_compact_relay_carrier_v1(
        &base,
        &CompactRelayCarrierV1 {
            target_peer_id: peer_id.to_string(),
            chain_id: chain_id.to_string(),
            wire: wire.clone(),
        },
    )
    .map_err(|error| {
        <serde_json::Error as serde::ser::Error>::custom(format!(
            "compact-relay carrier encode failed: {error:?}"
        ))
    })?;
    if encoded.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 {
        return Err(<serde_json::Error as serde::ser::Error>::custom(format!(
            "compact-relay carrier exceeds live transport byte budget after queueing: encoded={} maximum={}",
            encoded.len(),
            COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1
        )));
    }
    Ok(encoded)
}

pub(super) fn authorized_compact_relay_from_tip(
    bytes: &[u8],
    source_peer: Option<&str>,
    inner: &Arc<Mutex<InnerState>>,
) -> Result<Option<(String, CompactRelayWireV1)>, String> {
    let Some(peer_id) = source_peer else {
        return Ok(None);
    };
    let local_peer_id = {
        let guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        if guard
            .compact_relay_runtime
            .local_capabilities()
            .is_none()
        {
            return Ok(None);
        }
        guard.peer_id.clone()
    };

    let decoded = decode_network_message_with_compact_relay_for_peer_v1(bytes, &local_peer_id)
        .map_err(|error| format!("compact-relay carrier decode failed: {error:?}"))?;
    let Some(carrier) = decoded.compact_relay else {
        return Ok(None);
    };

    {
        let mut guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        guard
            .compact_relay_runtime
            .note_inbound(peer_id, &carrier.wire)
            .map_err(|error| {
                format!("compact-relay inbound session validation failed: {error:?}")
            })?;
    }
    Ok(Some((peer_id.to_string(), carrier.wire)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::compact_relay_carrier_v1::{
        CompactRelayCapabilitiesV1, CompactRelayCarrierV1,
    };
    use crate::messages::{CompactTransactionRequestV1, COMPACT_DAG_RELAY_VERSION_V1};
    use crate::messages::{ProtocolCapabilitiesV1, P2P_PROTOCOL_CAPABILITIES_VERSION};
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "compact-relay-live-io-testnet";
    const LOCAL_PEER: &str = "compact-relay-local-peer";
    const REMOTE_PEER: &str = "compact-relay-remote-peer";

    fn protocol_capabilities() -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: ProtocolActivationIdentity::activated_v2(
                CHAIN_ID.to_string(),
                "11".repeat(32),
                GHOSTDAG_V1_ORDERING_VERSION.to_string(),
            ),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    fn tips_message() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: vec!["tip".to_string()],
            inventory: None,
        }
    }

    fn route_state() -> InnerState {
        let mut state = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        state.active_connections.insert(REMOTE_PEER.to_string(), 1);
        state.connected_peers.push(REMOTE_PEER.to_string());
        state
            .protocol_capability_transport
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();

        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();
        let remote_caps = remote.encode_tip_message(&tips_message()).unwrap();
        state
            .protocol_capability_transport
            .decode_from_peer(REMOTE_PEER, &remote_caps)
            .unwrap();
        state
            .compact_relay_runtime
            .configure_local(CHAIN_ID, CompactRelayCapabilitiesV1::canonical(CHAIN_ID))
            .unwrap();
        state
    }

    fn authorized_state() -> InnerState {
        let mut state = route_state();
        state
            .compact_relay_runtime
            .note_inbound(
                REMOTE_PEER,
                &CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(
                    CHAIN_ID,
                )),
            )
            .unwrap();
        state
    }

    #[test]
    fn outbound_data_requires_protocol_route_and_compact_session() {
        let mut route_only = route_state();
        validate_compact_relay_send(
            &route_only,
            REMOTE_PEER,
            &CompactRelayWireV1::CapabilityProbe,
        )
        .unwrap();

        let data = CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "block-hash".to_string(),
            txids: vec!["tx-a".to_string()],
        });
        assert!(validate_compact_relay_send(&route_only, REMOTE_PEER, &data).is_err());

        route_only
            .compact_relay_runtime
            .note_inbound(
                REMOTE_PEER,
                &CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(
                    CHAIN_ID,
                )),
            )
            .unwrap();
        validate_compact_relay_send(&route_only, REMOTE_PEER, &data).unwrap();

        let unauthorized = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        assert!(validate_compact_relay_send(
            &unauthorized,
            REMOTE_PEER,
            &CompactRelayWireV1::CapabilityProbe
        )
        .is_err());
    }

    #[test]
    fn outbound_carrier_remains_legacy_tips_decodable() {
        let inner = Arc::new(Mutex::new(authorized_state()));
        let encoded = encode_compact_relay_for_transport(
            &inner,
            CHAIN_ID,
            REMOTE_PEER,
            &CompactRelayWireV1::CapabilityProbe,
        )
        .unwrap();
        assert!(encoded.len() <= COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);
        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "Tips");
        assert_eq!(legacy.chain_id(), CHAIN_ID);
    }

    #[test]
    fn inbound_capabilities_authorize_followup_compact_data() {
        let inner = Arc::new(Mutex::new(route_state()));
        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();
        let base = remote.encode_tip_message(&tips_message()).unwrap();
        let encoded = attach_compact_relay_carrier_v1(
            &base,
            &CompactRelayCarrierV1 {
                target_peer_id: LOCAL_PEER.to_string(),
                chain_id: CHAIN_ID.to_string(),
                wire: CompactRelayWireV1::Capabilities(
                    CompactRelayCapabilitiesV1::canonical(CHAIN_ID),
                ),
            },
        )
        .unwrap();

        let decoded =
            authorized_compact_relay_from_tip(&encoded, Some(REMOTE_PEER), &inner)
                .unwrap()
                .expect("addressed compact relay");
        assert_eq!(decoded.0, REMOTE_PEER);
        assert!(matches!(decoded.1, CompactRelayWireV1::Capabilities(_)));
        assert!(inner
            .lock()
            .unwrap()
            .compact_relay_runtime
            .peer_session_authorized(REMOTE_PEER));
    }
}
