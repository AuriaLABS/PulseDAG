use super::*;
use crate::messages::{
    attach_protocol_sync_carrier_v1, decode_network_message_with_protocol_sync_for_peer_v1,
    ProtocolMessageClassV1, ProtocolPeerRouteActionV1, ProtocolSyncCarrierV1,
};

pub(super) fn protocol_sync_peer_is_authorized(state: &InnerState, peer_id: &str) -> bool {
    let connected = state.active_connections.get(peer_id).copied().unwrap_or(0) > 0
        || state.connected_peers.iter().any(|peer| peer == peer_id);
    connected
        && state
            .protocol_capability_transport
            .route(peer_id, ProtocolMessageClassV1::ProtocolV2Sync)
            .action
            == ProtocolPeerRouteActionV1::SendProtocolV2
}

pub(super) fn validate_protocol_sync_send(
    state: &InnerState,
    peer_id: &str,
    wire: &ProtocolSyncWireV1,
) -> Result<(), PulseError> {
    wire.validate_for_chain(&state.chain_id).map_err(|error| {
        PulseError::Internal(format!(
            "invalid protocol-v2 sync payload for live transport: {error:?}"
        ))
    })?;
    if !protocol_sync_peer_is_authorized(state, peer_id) {
        return Err(PulseError::Internal(format!(
            "peer {peer_id} is not authorized for exact-compatible protocol-v2 sync"
        )));
    }
    Ok(())
}

pub(super) fn encode_protocol_sync_for_transport(
    inner: &Arc<Mutex<InnerState>>,
    chain_id: &str,
    peer_id: &str,
    wire: &ProtocolSyncWireV1,
) -> Result<Vec<u8>, serde_json::Error> {
    {
        let guard = inner
            .lock()
            .map_err(|_| <serde_json::Error as serde::ser::Error>::custom("p2p lock poisoned"))?;
        if guard.chain_id != chain_id {
            return Err(<serde_json::Error as serde::ser::Error>::custom(format!(
                "protocol-v2 sync runtime chain mismatch: state={} transport={chain_id}",
                guard.chain_id
            )));
        }
        validate_protocol_sync_send(&guard, peer_id, wire).map_err(|error| {
            <serde_json::Error as serde::ser::Error>::custom(format!(
                "protocol-v2 sync authorization changed before publish: {error:?}"
            ))
        })?;
    }

    let inventory = inner.lock().ok().and_then(|mut guard| {
        current_tip_inventory_for_send(&mut guard, chain_id, "ProtocolSyncV1")
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
    attach_protocol_sync_carrier_v1(
        &base,
        &ProtocolSyncCarrierV1 {
            target_peer_id: peer_id.to_string(),
            wire: wire.clone(),
        },
    )
    .map_err(|error| {
        <serde_json::Error as serde::ser::Error>::custom(format!(
            "protocol-v2 sync carrier encode failed: {error:?}"
        ))
    })
}

pub(super) fn authorized_protocol_sync_from_tip(
    bytes: &[u8],
    source_peer: Option<&str>,
    inner: &Arc<Mutex<InnerState>>,
) -> Result<Option<(String, ProtocolSyncWireV1)>, String> {
    let Some(peer_id) = source_peer else {
        return Ok(None);
    };
    let local_peer_id = {
        let guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        guard.peer_id.clone()
    };
    let decoded = decode_network_message_with_protocol_sync_for_peer_v1(bytes, &local_peer_id)
        .map_err(|error| format!("protocol-v2 sync carrier decode failed: {error:?}"))?;
    Ok(decoded
        .protocol_sync
        .map(|carrier| (peer_id.to_string(), carrier.wire)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{
        ProtocolCapabilitiesV1, SelectedChainLocatorV1, P2P_DAG_SYNC_CONTRACT_VERSION,
        P2P_PROTOCOL_CAPABILITIES_VERSION,
    };
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task27-live-protocol-sync-io";
    const LOCAL_PEER: &str = "local-peer-v2";
    const REMOTE_PEER: &str = "remote-peer-v2";

    fn capabilities() -> ProtocolCapabilitiesV1 {
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

    fn locator_wire() -> ProtocolSyncWireV1 {
        ProtocolSyncWireV1::SelectedChainLocator(SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: capabilities().protocol_identity,
            selected_tip: "tip".to_string(),
            locator: vec!["tip".to_string(), "ancestor".to_string()],
        })
    }

    fn exact_session_inner() -> Arc<Mutex<InnerState>> {
        let mut state = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        state.active_connections.insert(REMOTE_PEER.to_string(), 1);
        state.connected_peers.push(REMOTE_PEER.to_string());
        state
            .protocol_capability_transport
            .configure_local_capabilities(CHAIN_ID, capabilities())
            .unwrap();
        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, capabilities())
            .unwrap();
        let remote_caps = remote.encode_tip_message(&tips_message()).unwrap();
        state
            .protocol_capability_transport
            .decode_from_peer(REMOTE_PEER, &remote_caps)
            .unwrap();
        Arc::new(Mutex::new(state))
    }

    fn combined_remote_wire(target: &str, wire: ProtocolSyncWireV1) -> Vec<u8> {
        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, capabilities())
            .unwrap();
        let base = remote.encode_tip_message(&tips_message()).unwrap();
        attach_protocol_sync_carrier_v1(
            &base,
            &ProtocolSyncCarrierV1 {
                target_peer_id: target.to_string(),
                wire,
            },
        )
        .unwrap()
    }

    #[test]
    fn outbound_protocol_sync_requires_exact_compatible_live_session() {
        let mut state = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        state.active_connections.insert(REMOTE_PEER.to_string(), 1);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = Libp2pHandle {
            inner: Arc::new(Mutex::new(state)),
            outbound_tx: tx,
        };
        assert!(handle
            .send_protocol_sync_v1(REMOTE_PEER, &locator_wire())
            .is_err());
        assert!(rx.try_recv().is_err());

        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = Libp2pHandle {
            inner: exact_session_inner(),
            outbound_tx: tx,
        };
        handle
            .send_protocol_sync_v1(REMOTE_PEER, &locator_wire())
            .unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(OutboundMessage::ProtocolSync { peer_id, .. }) if peer_id == REMOTE_PEER
        ));
    }

    #[test]
    fn queued_protocol_sync_is_rejected_after_session_revocation() {
        let inner = exact_session_inner();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = Libp2pHandle {
            inner: inner.clone(),
            outbound_tx: tx,
        };
        handle
            .send_protocol_sync_v1(REMOTE_PEER, &locator_wire())
            .unwrap();
        let queued = rx.try_recv().unwrap();

        {
            let mut guard = inner.lock().unwrap();
            guard
                .protocol_capability_transport
                .peer_disconnected(REMOTE_PEER);
        }

        let OutboundMessage::ProtocolSync { peer_id, wire } = queued else {
            panic!("expected queued protocol sync message");
        };
        assert!(encode_protocol_sync_for_transport(&inner, CHAIN_ID, &peer_id, &wire).is_err());
    }

    #[test]
    fn protocol_sync_uses_standard_non_block_queue_lane() {
        let inner = exact_session_inner();
        let mut queue = OutboundPriorityQueue::default();
        {
            let mut guard = inner.lock().unwrap();
            guard.queued_messages = 1;
            guard.queued_non_block_messages = 1;
        }
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::ProtocolSync {
                peer_id: REMOTE_PEER.to_string(),
                wire: locator_wire(),
            },
        );
        assert!(queue.blocks.is_empty());
        assert!(queue.priority_txs.is_empty());
        assert_eq!(queue.standard_txs.len(), 1);

        assert!(matches!(
            pop_outbound_message(&inner, &mut queue),
            Some(OutboundMessage::ProtocolSync { peer_id, .. }) if peer_id == REMOTE_PEER
        ));
        let guard = inner.lock().unwrap();
        assert_eq!(guard.queued_messages, 0);
        assert_eq!(guard.queued_non_block_messages, 0);
        assert_eq!(guard.dequeued_non_block_messages, 1);
    }

    #[test]
    fn outbound_carrier_remains_legacy_tips_decodable() {
        let inner = exact_session_inner();
        let encoded =
            encode_protocol_sync_for_transport(&inner, CHAIN_ID, REMOTE_PEER, &locator_wire())
                .unwrap();
        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "Tips");
        let decoded =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, REMOTE_PEER).unwrap();
        assert_eq!(decoded.protocol_sync.unwrap().wire, locator_wire());
    }

    #[test]
    fn authorized_targeted_inbound_emits_tips_then_protocol_sync() {
        let inner = exact_session_inner();
        let encoded = combined_remote_wire(LOCAL_PEER, locator_wire());
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();

        dispatch_network_message(CHAIN_ID, &encoded, Some(REMOTE_PEER), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::ProtocolSync { peer_id, wire })
                if peer_id == REMOTE_PEER && wire == locator_wire()
        ));
    }

    #[test]
    fn unknown_session_ignores_hidden_sync_but_keeps_legacy_tips() {
        let mut state = InnerState {
            chain_id: CHAIN_ID.to_string(),
            peer_id: LOCAL_PEER.to_string(),
            ..InnerState::default()
        };
        state.active_connections.insert(REMOTE_PEER.to_string(), 1);
        state
            .protocol_capability_transport
            .configure_local_capabilities(CHAIN_ID, capabilities())
            .unwrap();
        let inner = Arc::new(Mutex::new(state));
        let base = serde_json::to_vec(&tips_message()).unwrap();
        let encoded = attach_protocol_sync_carrier_v1(
            &base,
            &ProtocolSyncCarrierV1 {
                target_peer_id: LOCAL_PEER.to_string(),
                wire: locator_wire(),
            },
        )
        .unwrap();
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();

        dispatch_network_message(CHAIN_ID, &encoded, Some(REMOTE_PEER), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(inbound_rx.try_recv().is_err());
    }

    #[test]
    fn other_target_is_ignored_without_protocol_sync_event() {
        let inner = exact_session_inner();
        let encoded = combined_remote_wire("other-peer", locator_wire());
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();

        dispatch_network_message(CHAIN_ID, &encoded, Some(REMOTE_PEER), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(inbound_rx.try_recv().is_err());
    }

    #[test]
    fn malformed_local_target_from_authorized_peer_is_penalized_after_tips() {
        let inner = exact_session_inner();
        let encoded = combined_remote_wire(LOCAL_PEER, locator_wire());
        let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        value[crate::messages::PROTOCOL_SYNC_EXTENSION_FIELD_V1]["wire"] =
            serde_json::Value::String("malformed-wire".to_string());
        let malformed = serde_json::to_vec(&value).unwrap();
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();

        dispatch_network_message(CHAIN_ID, &malformed, Some(REMOTE_PEER), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { .. })
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_decode_failed, 1);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("protocol_sync_decode_failed")
        );
    }
}
