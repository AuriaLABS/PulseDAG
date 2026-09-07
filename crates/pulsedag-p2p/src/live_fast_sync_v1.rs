use super::*;
use crate::live_protocol_sync_v1::protocol_sync_peer_is_authorized;
use crate::messages::fast_sync_carrier_v1::{
    attach_fast_sync_carrier_v1, decode_network_message_with_fast_sync_for_peer_v1,
    FastSyncCarrierV1, FastSyncWireV1, P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1,
};

fn encode_fast_sync_for_state(
    state: &InnerState,
    peer_id: &str,
    wire: &FastSyncWireV1,
) -> Result<Vec<u8>, PulseError> {
    let protocol_route_authorized = protocol_sync_peer_is_authorized(state, peer_id);
    state
        .protocol_capability_transport
        .validate_fast_sync_outbound(peer_id, protocol_route_authorized, wire)
        .map_err(|error| {
            PulseError::Internal(format!(
                "fast-sync live transport authorization failed: {error:?}"
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
                "fast-sync capability carrier encode failed: {error:?}"
            ))
        })?;
    let encoded = attach_fast_sync_carrier_v1(
        &base,
        &FastSyncCarrierV1 {
            target_peer_id: peer_id.to_string(),
            wire: wire.clone(),
        },
    )
    .map_err(|error| PulseError::Internal(format!("fast-sync carrier encode failed: {error:?}")))?;
    if encoded.len() > P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1 {
        return Err(PulseError::Internal(format!(
            "fast-sync carrier exceeds live transport byte budget: encoded={} maximum={}",
            encoded.len(),
            P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1
        )));
    }
    Ok(encoded)
}

pub(super) fn validate_fast_sync_send(
    state: &InnerState,
    peer_id: &str,
    wire: &FastSyncWireV1,
) -> Result<(), PulseError> {
    encode_fast_sync_for_state(state, peer_id, wire).map(|_| ())
}

pub(super) fn encode_fast_sync_for_transport(
    inner: &Arc<Mutex<InnerState>>,
    chain_id: &str,
    peer_id: &str,
    wire: &FastSyncWireV1,
) -> Result<Vec<u8>, serde_json::Error> {
    {
        let guard = inner
            .lock()
            .map_err(|_| <serde_json::Error as serde::ser::Error>::custom("p2p lock poisoned"))?;
        if guard.chain_id != chain_id {
            return Err(<serde_json::Error as serde::ser::Error>::custom(format!(
                "fast-sync runtime chain mismatch: state={} transport={chain_id}",
                guard.chain_id
            )));
        }
        validate_fast_sync_send(&guard, peer_id, wire).map_err(|error| {
            <serde_json::Error as serde::ser::Error>::custom(format!(
                "fast-sync authorization changed before publish: {error:?}"
            ))
        })?;
    }

    let inventory = inner
        .lock()
        .ok()
        .and_then(|mut guard| current_tip_inventory_for_send(&mut guard, chain_id, "FastSyncV1"));
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
    let encoded = attach_fast_sync_carrier_v1(
        &base,
        &FastSyncCarrierV1 {
            target_peer_id: peer_id.to_string(),
            wire: wire.clone(),
        },
    )
    .map_err(|error| {
        <serde_json::Error as serde::ser::Error>::custom(format!(
            "fast-sync carrier encode failed: {error:?}"
        ))
    })?;
    if encoded.len() > P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1 {
        return Err(<serde_json::Error as serde::ser::Error>::custom(format!(
            "fast-sync carrier exceeds live transport byte budget after queueing: encoded={} maximum={}",
            encoded.len(),
            P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1
        )));
    }
    Ok(encoded)
}

pub(super) fn authorized_fast_sync_from_tip(
    bytes: &[u8],
    source_peer: Option<&str>,
    inner: &Arc<Mutex<InnerState>>,
) -> Result<Option<(String, FastSyncWireV1)>, String> {
    let Some(peer_id) = source_peer else {
        return Ok(None);
    };
    let local_peer_id = {
        let guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        if guard
            .protocol_capability_transport
            .fast_sync_session_book()
            .local_capabilities()
            .is_none()
        {
            return Ok(None);
        }
        guard.peer_id.clone()
    };

    let decoded = decode_network_message_with_fast_sync_for_peer_v1(bytes, &local_peer_id)
        .map_err(|error| format!("fast-sync carrier decode failed: {error:?}"))?;
    let Some(carrier) = decoded.fast_sync else {
        return Ok(None);
    };

    {
        let mut guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        guard
            .protocol_capability_transport
            .note_fast_sync_inbound(peer_id, &carrier.wire)
            .map_err(|error| format!("fast-sync inbound session validation failed: {error:?}"))?;
    }
    Ok(Some((peer_id.to_string(), carrier.wire)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::fast_sync_carrier_v1::{
        FastSyncCapabilitiesV1, P2P_FAST_SYNC_CONTRACT_VERSION,
    };
    use crate::messages::{ProtocolCapabilitiesV1, P2P_PROTOCOL_CAPABILITIES_VERSION};
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "fast-sync-live-io-testnet";
    const LOCAL_PEER: &str = "fast-sync-local-peer";
    const REMOTE_PEER: &str = "fast-sync-remote-peer";

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

    fn fast_sync_capabilities() -> FastSyncCapabilitiesV1 {
        let protocol = protocol_capabilities();
        FastSyncCapabilitiesV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            genesis_hash: protocol.protocol_identity.genesis_hash.clone(),
            protocol_fingerprint: protocol.protocol_identity.fingerprint().unwrap(),
            manifest_version: 1,
            protocol_snapshot_bundle_format_version: 2,
            storage_schema_version: 1,
            payload_encoding: "bincode-1.3-fast-sync-bundle-v1".to_string(),
            max_chunk_bytes: 24 * 1024,
            max_commitments_per_page: 256,
        }
    }

    fn tips_message() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: vec!["tip".to_string()],
            inventory: None,
        }
    }

    fn probe_wire() -> FastSyncWireV1 {
        FastSyncWireV1::CapabilityProbe {
            chain_id: CHAIN_ID.to_string(),
        }
    }

    fn transfer_request_wire() -> FastSyncWireV1 {
        FastSyncWireV1::GetTransferSummary {
            chain_id: CHAIN_ID.to_string(),
        }
    }

    fn exact_protocol_route_state_without_fast_sync() -> InnerState {
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
    }

    fn exact_protocol_route_state() -> InnerState {
        let mut state = exact_protocol_route_state_without_fast_sync();
        state
            .protocol_capability_transport
            .configure_fast_sync_capabilities(fast_sync_capabilities())
            .unwrap();
        state
    }

    fn exact_fast_sync_inner() -> Arc<Mutex<InnerState>> {
        let mut state = exact_protocol_route_state();
        state
            .protocol_capability_transport
            .note_fast_sync_inbound(
                REMOTE_PEER,
                &FastSyncWireV1::Capabilities(fast_sync_capabilities()),
            )
            .unwrap();
        Arc::new(Mutex::new(state))
    }

    fn combined_remote_wire(target: &str, wire: FastSyncWireV1) -> Vec<u8> {
        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();
        let base = remote.encode_tip_message(&tips_message()).unwrap();
        attach_fast_sync_carrier_v1(
            &base,
            &FastSyncCarrierV1 {
                target_peer_id: target.to_string(),
                wire,
            },
        )
        .unwrap()
    }

    #[test]
    fn outbound_transfer_requires_task27_route_and_fast_sync_session() {
        let mut disconnected = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        disconnected
            .protocol_capability_transport
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();
        disconnected
            .protocol_capability_transport
            .configure_fast_sync_capabilities(fast_sync_capabilities())
            .unwrap();
        assert!(validate_fast_sync_send(&disconnected, REMOTE_PEER, &probe_wire()).is_err());

        let route_only = exact_protocol_route_state();
        validate_fast_sync_send(&route_only, REMOTE_PEER, &probe_wire()).unwrap();
        assert!(
            validate_fast_sync_send(&route_only, REMOTE_PEER, &transfer_request_wire()).is_err()
        );

        let inner = exact_fast_sync_inner();
        let guard = inner.lock().unwrap();
        validate_fast_sync_send(&guard, REMOTE_PEER, &transfer_request_wire()).unwrap();
    }

    #[test]
    fn outbound_carrier_remains_legacy_tips_decodable() {
        let inner = exact_fast_sync_inner();
        let encoded =
            encode_fast_sync_for_transport(&inner, CHAIN_ID, REMOTE_PEER, &transfer_request_wire())
                .unwrap();
        assert!(encoded.len() <= P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1);
        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "Tips");
        let decoded =
            decode_network_message_with_fast_sync_for_peer_v1(&encoded, REMOTE_PEER).unwrap();
        assert_eq!(decoded.fast_sync.unwrap().wire, transfer_request_wire());
    }

    #[test]
    fn queued_fast_sync_is_rejected_after_session_revocation() {
        let inner = exact_fast_sync_inner();
        {
            let guard = inner.lock().unwrap();
            validate_fast_sync_send(&guard, REMOTE_PEER, &transfer_request_wire()).unwrap();
        }
        {
            let mut guard = inner.lock().unwrap();
            guard
                .protocol_capability_transport
                .peer_disconnected(REMOTE_PEER);
        }
        assert!(encode_fast_sync_for_transport(
            &inner,
            CHAIN_ID,
            REMOTE_PEER,
            &transfer_request_wire(),
        )
        .is_err());
    }

    #[test]
    fn targeted_capabilities_establish_fast_sync_session() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state()));
        assert!(!inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
        let encoded = combined_remote_wire(
            LOCAL_PEER,
            FastSyncWireV1::Capabilities(fast_sync_capabilities()),
        );
        let decoded = authorized_fast_sync_from_tip(&encoded, Some(REMOTE_PEER), &inner)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.0, REMOTE_PEER);
        assert!(inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
    }

    #[test]
    fn disabled_local_fast_sync_ignores_targeted_extension() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state_without_fast_sync()));
        let encoded = combined_remote_wire(
            LOCAL_PEER,
            FastSyncWireV1::Capabilities(fast_sync_capabilities()),
        );
        assert!(
            authorized_fast_sync_from_tip(&encoded, Some(REMOTE_PEER), &inner)
                .unwrap()
                .is_none()
        );
        let guard = inner.lock().unwrap();
        assert!(guard
            .protocol_capability_transport
            .fast_sync_session_book()
            .local_capabilities()
            .is_none());
        assert!(!guard
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
    }

    #[test]
    fn other_target_is_ignored_without_fast_sync_session_mutation() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state()));
        let encoded = combined_remote_wire(
            "other-peer",
            FastSyncWireV1::Capabilities(fast_sync_capabilities()),
        );
        assert!(
            authorized_fast_sync_from_tip(&encoded, Some(REMOTE_PEER), &inner)
                .unwrap()
                .is_none()
        );
        assert!(!inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
    }

    #[test]
    fn transfer_before_fast_sync_session_fails_closed() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state()));
        let encoded = combined_remote_wire(LOCAL_PEER, transfer_request_wire());
        assert!(authorized_fast_sync_from_tip(&encoded, Some(REMOTE_PEER), &inner).is_err());
        assert!(!inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
    }

    #[test]
    fn libp2p_handle_queues_transfer_only_after_exact_fast_sync_capabilities() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state()));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = Libp2pHandle {
            inner: inner.clone(),
            outbound_tx: tx,
        };
        assert!(handle
            .send_fast_sync_v1(REMOTE_PEER, &transfer_request_wire())
            .is_err());
        assert!(rx.try_recv().is_err());

        inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .note_fast_sync_inbound(
                REMOTE_PEER,
                &FastSyncWireV1::Capabilities(fast_sync_capabilities()),
            )
            .unwrap();
        handle
            .send_fast_sync_v1(REMOTE_PEER, &transfer_request_wire())
            .unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(OutboundMessage::FastSync { peer_id, wire })
                if peer_id == REMOTE_PEER && wire == transfer_request_wire()
        ));
    }

    #[test]
    fn fast_sync_uses_standard_non_block_queue_lane() {
        let inner = exact_fast_sync_inner();
        let mut queue = OutboundPriorityQueue::default();
        {
            let mut guard = inner.lock().unwrap();
            guard.queued_messages = 1;
            guard.queued_non_block_messages = 1;
        }
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::FastSync {
                peer_id: REMOTE_PEER.to_string(),
                wire: transfer_request_wire(),
            },
        );
        assert!(queue.blocks.is_empty());
        assert!(queue.priority_txs.is_empty());
        assert_eq!(queue.standard_txs.len(), 1);
        assert!(matches!(
            pop_outbound_message(&inner, &mut queue),
            Some(OutboundMessage::FastSync { peer_id, .. }) if peer_id == REMOTE_PEER
        ));
        let guard = inner.lock().unwrap();
        assert_eq!(guard.queued_messages, 0);
        assert_eq!(guard.queued_non_block_messages, 0);
        assert_eq!(guard.dequeued_non_block_messages, 1);
    }

    #[test]
    fn targeted_capabilities_dispatch_tips_then_fast_sync_event() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state()));
        let encoded = combined_remote_wire(
            LOCAL_PEER,
            FastSyncWireV1::Capabilities(fast_sync_capabilities()),
        );
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        dispatch_network_message(CHAIN_ID, &encoded, Some(REMOTE_PEER), &inner, &inbound_tx);
        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::FastSync { peer_id, wire })
                if peer_id == REMOTE_PEER
                    && wire == FastSyncWireV1::Capabilities(fast_sync_capabilities())
        ));
        assert!(inner
            .lock()
            .unwrap()
            .protocol_capability_transport
            .fast_sync_peer_authorized(REMOTE_PEER));
    }

    #[test]
    fn disabled_local_fast_sync_dispatch_ignores_extension_without_penalty() {
        let inner = Arc::new(Mutex::new(exact_protocol_route_state_without_fast_sync()));
        let encoded = combined_remote_wire(
            LOCAL_PEER,
            FastSyncWireV1::Capabilities(fast_sync_capabilities()),
        );
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        dispatch_network_message(CHAIN_ID, &encoded, Some(REMOTE_PEER), &inner, &inbound_tx);
        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_decode_failed, 0);
        assert_ne!(
            guard.last_drop_reason.as_deref(),
            Some("fast_sync_decode_failed")
        );
    }
}
