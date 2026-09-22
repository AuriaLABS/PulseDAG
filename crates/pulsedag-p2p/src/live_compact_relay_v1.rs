use super::*;
use crate::live_protocol_sync_v1::protocol_sync_peer_is_authorized;
use crate::messages::compact_relay_carrier_v1::{
    attach_compact_relay_carrier_v1, decode_network_message_with_compact_relay_for_peer_v1,
    CompactRelayCarrierV1, CompactRelayWireV1, COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
};

fn saturating_bytes(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl CompactRelayTransportTelemetryV1 {
    fn note_outbound(&mut self, wire: &CompactRelayWireV1, encoded_bytes: usize) {
        self.outbound_carriers_encoded_total =
            self.outbound_carriers_encoded_total.saturating_add(1);
        self.outbound_encoded_bytes_total = self
            .outbound_encoded_bytes_total
            .saturating_add(saturating_bytes(encoded_bytes));
        match wire {
            CompactRelayWireV1::CapabilityProbe | CompactRelayWireV1::Capabilities(_) => {
                self.outbound_capability_messages_total =
                    self.outbound_capability_messages_total.saturating_add(1);
            }
            CompactRelayWireV1::Announce(_) => {
                self.outbound_announcements_total =
                    self.outbound_announcements_total.saturating_add(1);
            }
            CompactRelayWireV1::GetTransactions(_) => {
                self.outbound_body_requests_total =
                    self.outbound_body_requests_total.saturating_add(1);
            }
            CompactRelayWireV1::Transactions(_) => {
                self.outbound_body_responses_total =
                    self.outbound_body_responses_total.saturating_add(1);
            }
        }
    }

    fn note_inbound(&mut self, wire: &CompactRelayWireV1, encoded_bytes: usize) {
        self.inbound_carriers_accepted_total =
            self.inbound_carriers_accepted_total.saturating_add(1);
        self.inbound_accepted_bytes_total = self
            .inbound_accepted_bytes_total
            .saturating_add(saturating_bytes(encoded_bytes));
        match wire {
            CompactRelayWireV1::CapabilityProbe | CompactRelayWireV1::Capabilities(_) => {
                self.inbound_capability_messages_total =
                    self.inbound_capability_messages_total.saturating_add(1);
            }
            CompactRelayWireV1::Announce(_) => {
                self.inbound_announcements_total =
                    self.inbound_announcements_total.saturating_add(1);
            }
            CompactRelayWireV1::GetTransactions(_) => {
                self.inbound_body_requests_total =
                    self.inbound_body_requests_total.saturating_add(1);
            }
            CompactRelayWireV1::Transactions(_) => {
                self.inbound_body_responses_total =
                    self.inbound_body_responses_total.saturating_add(1);
            }
        }
    }

    fn note_decode_failure(&mut self) {
        self.decode_failures_total = self.decode_failures_total.saturating_add(1);
    }
}

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
    if let Ok(mut guard) = inner.lock() {
        guard
            .compact_relay_transport
            .note_outbound(wire, encoded.len());
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
        if guard.compact_relay_runtime.local_capabilities().is_none() {
            return Ok(None);
        }
        guard.peer_id.clone()
    };

    let decoded = match decode_network_message_with_compact_relay_for_peer_v1(bytes, &local_peer_id)
    {
        Ok(decoded) => decoded,
        Err(error) => {
            if let Ok(mut guard) = inner.lock() {
                guard.compact_relay_transport.note_decode_failure();
            }
            return Err(format!("compact-relay carrier decode failed: {error:?}"));
        }
    };
    let Some(carrier) = decoded.compact_relay else {
        return Ok(None);
    };

    {
        let mut guard = inner.lock().map_err(|_| "p2p lock poisoned".to_string())?;
        if !protocol_sync_peer_is_authorized(&guard, peer_id) {
            return Ok(None);
        }
        if let Err(error) = guard
            .compact_relay_runtime
            .note_inbound(peer_id, &carrier.wire)
        {
            guard.compact_relay_transport.note_decode_failure();
            return Err(format!(
                "compact-relay inbound session validation failed: {error:?}"
            ));
        }
        guard
            .compact_relay_transport
            .note_inbound(&carrier.wire, bytes.len());
    }
    Ok(Some((peer_id.to_string(), carrier.wire)))
}

pub(super) fn dispatch_reconstructed_compact_block_v1(
    chain_id: &str,
    peer_id: &str,
    block: Block,
    inner: &Arc<Mutex<InnerState>>,
    inbound_tx: &mpsc::UnboundedSender<InboundEvent>,
) -> Result<(), PulseError> {
    {
        let guard = inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        if guard.chain_id != chain_id {
            return Err(PulseError::Internal(format!(
                "compact-relay reconstructed block chain mismatch: state={} submit={chain_id}",
                guard.chain_id
            )));
        }
        if !protocol_sync_peer_is_authorized(&guard, peer_id)
            || !guard.compact_relay_runtime.peer_session_authorized(peer_id)
        {
            return Err(PulseError::Internal(format!(
                "compact-relay reconstructed block submission is not authorized for peer {peer_id}"
            )));
        }
    }

    let encoded = serde_json::to_vec(&NetworkMessage::Block {
        chain_id: chain_id.to_string(),
        block,
    })
    .map_err(|error| {
        PulseError::Internal(format!(
            "compact-relay reconstructed block serialization failed: {error}"
        ))
    })?;
    dispatch_network_message(chain_id, &encoded, Some(peer_id), inner, inbound_tx);
    Ok(())
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
        types::{compute_merkle_root, BlockHeader, TxOutput},
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
                &CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(CHAIN_ID)),
            )
            .unwrap();
        state
    }

    fn block() -> Block {
        let transactions = vec![Transaction {
            txid: "tx-live-compact".into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }];
        Block {
            hash: "compact-live-block".into(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent-a".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: compute_merkle_root(&transactions),
                state_root: "state".into(),
                blue_score: 1,
                height: 1,
            },
            transactions,
        }
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
                &CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(CHAIN_ID)),
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
    fn protocol_route_revocation_clears_compact_session() {
        let inner = Arc::new(Mutex::new(authorized_state()));
        assert!(inner
            .lock()
            .unwrap()
            .compact_relay_runtime
            .peer_session_authorized(REMOTE_PEER));

        let legacy_tips = serde_json::to_vec(&tips_message()).unwrap();
        decode_network_message_for_transport(
            &legacy_tips,
            Some(REMOTE_PEER),
            CHAIN_ID,
            &inner,
        )
        .unwrap();

        let guard = inner.lock().unwrap();
        assert!(!protocol_sync_peer_is_authorized(&guard, REMOTE_PEER));
        assert!(!guard
            .compact_relay_runtime
            .peer_session_authorized(REMOTE_PEER));
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
    fn transport_telemetry_counts_actual_encoded_bytes_and_message_classes() {
        let inner = Arc::new(Mutex::new(authorized_state()));
        let request = CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "telemetry-block".into(),
            txids: vec!["tx-a".into()],
        });
        let encoded =
            encode_compact_relay_for_transport(&inner, CHAIN_ID, REMOTE_PEER, &request).unwrap();
        {
            let guard = inner.lock().unwrap();
            let telemetry = &guard.compact_relay_transport;
            assert_eq!(telemetry.outbound_carriers_encoded_total, 1);
            assert_eq!(telemetry.outbound_body_requests_total, 1);
            assert_eq!(telemetry.outbound_encoded_bytes_total, encoded.len() as u64);
            assert_eq!(telemetry.inbound_carriers_accepted_total, 0);
        }

        let mut remote = ProtocolCapabilityTransportV1::default();
        remote
            .configure_local_capabilities(CHAIN_ID, protocol_capabilities())
            .unwrap();
        let base = remote.encode_tip_message(&tips_message()).unwrap();
        let inbound = attach_compact_relay_carrier_v1(
            &base,
            &CompactRelayCarrierV1 {
                target_peer_id: LOCAL_PEER.to_string(),
                chain_id: CHAIN_ID.to_string(),
                wire: CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(
                    CHAIN_ID,
                )),
            },
        )
        .unwrap();
        authorized_compact_relay_from_tip(&inbound, Some(REMOTE_PEER), &inner)
            .unwrap()
            .unwrap();

        let guard = inner.lock().unwrap();
        let telemetry = &guard.compact_relay_transport;
        assert_eq!(telemetry.inbound_carriers_accepted_total, 1);
        assert_eq!(telemetry.inbound_capability_messages_total, 1);
        assert_eq!(telemetry.inbound_accepted_bytes_total, inbound.len() as u64);
    }

    #[test]
    fn reconstructed_block_reenters_the_canonical_network_block_dispatch() {
        let inner = Arc::new(Mutex::new(authorized_state()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let expected = block();

        dispatch_reconstructed_compact_block_v1(
            CHAIN_ID,
            REMOTE_PEER,
            expected.clone(),
            &inner,
            &inbound_tx,
        )
        .unwrap();

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Block(observed)) if observed == expected
        ));
        let guard = inner.lock().unwrap();
        assert_eq!(guard.blocks_received, 1);
        assert_eq!(guard.invalid_blocks_received, 0);
    }

    #[test]
    fn reconstructed_and_full_blocks_share_invalid_merkle_rejection() {
        let compact_inner = Arc::new(Mutex::new(authorized_state()));
        let direct_inner = Arc::new(Mutex::new(InnerState {
            chain_id: CHAIN_ID.to_string(),
            ..InnerState::default()
        }));
        let (compact_tx, mut compact_rx) = mpsc::unbounded_channel();
        let (direct_tx, mut direct_rx) = mpsc::unbounded_channel();

        let mut invalid = block();
        invalid.header.merkle_root = "00".repeat(32);

        dispatch_reconstructed_compact_block_v1(
            CHAIN_ID,
            REMOTE_PEER,
            invalid.clone(),
            &compact_inner,
            &compact_tx,
        )
        .unwrap();

        let direct = serde_json::to_vec(&NetworkMessage::Block {
            chain_id: CHAIN_ID.to_string(),
            block: invalid,
        })
        .unwrap();
        dispatch_network_message(
            CHAIN_ID,
            &direct,
            Some(REMOTE_PEER),
            &direct_inner,
            &direct_tx,
        );

        assert!(compact_rx.try_recv().is_err());
        assert!(direct_rx.try_recv().is_err());

        let compact = compact_inner.lock().unwrap();
        let direct = direct_inner.lock().unwrap();
        assert_eq!(compact.invalid_blocks_received, 1);
        assert_eq!(direct.invalid_blocks_received, 1);
        assert_eq!(compact.last_drop_reason, direct.last_drop_reason);
        assert_eq!(
            compact.last_drop_reason.as_deref(),
            Some("invalid_block_merkle_root_mismatch")
        );
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
                wire: CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(
                    CHAIN_ID,
                )),
            },
        )
        .unwrap();

        let decoded = authorized_compact_relay_from_tip(&encoded, Some(REMOTE_PEER), &inner)
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
