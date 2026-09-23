use std::collections::BTreeMap;

use pulsedag_core::{types::Hash, MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1};

use super::{
    attach_compact_relay_carrier_v1, complete_compact_block_reconstruction_for_chain_v1,
    decode_network_message_with_compact_relay_for_peer_v1, CompactBlockAnnouncementV1,
    CompactBlockReconstructionPlanV1, CompactBlockReconstructionRequestStateV1,
    CompactRelayCapabilitiesV1, CompactRelayCarrierErrorV1, CompactRelayCarrierV1,
    CompactRelayErrorV1, CompactRelayWireV1, CompactTransactionResponseV1,
    COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
};

pub const COMPACT_RELAY_MAX_INFLIGHT_PER_PEER_V1: usize = 64;
pub const COMPACT_RELAY_MAX_RETAINED_BYTES_PER_PEER_V1: u64 = 48 * 1_024 * 1_024;
pub const COMPACT_RELAY_MAX_RETAINED_BYTES_GLOBAL_V1: u64 = 192 * 1_024 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayRuntimeSessionErrorV1 {
    Carrier(CompactRelayCarrierErrorV1),
    Compact(CompactRelayErrorV1),
    LocalCapabilitiesMissing,
    LocalCapabilitySurfaceMismatch,
    PeerCapabilitySessionMissing {
        peer_id: String,
    },
    ProtocolRouteUnauthorized {
        peer_id: String,
    },
    EmptyPeerId,
    InFlightLimitExceeded {
        peer_id: String,
        maximum: usize,
    },
    InFlightRetainedBytesPerPeerLimitExceeded {
        peer_id: String,
        observed: u64,
        maximum: u64,
    },
    InFlightRetainedBytesGlobalLimitExceeded {
        observed: u64,
        maximum: u64,
    },
    InFlightAlreadyExists {
        peer_id: String,
        block_hash: Hash,
    },
    InFlightMissing {
        peer_id: String,
        block_hash: Hash,
    },
}

impl From<CompactRelayCarrierErrorV1> for CompactRelayRuntimeSessionErrorV1 {
    fn from(value: CompactRelayCarrierErrorV1) -> Self {
        Self::Carrier(value)
    }
}

impl From<CompactRelayErrorV1> for CompactRelayRuntimeSessionErrorV1 {
    fn from(value: CompactRelayErrorV1) -> Self {
        Self::Compact(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayRuntimeTransportErrorV1 {
    Session(CompactRelayRuntimeSessionErrorV1),
    Carrier(CompactRelayCarrierErrorV1),
}

impl From<CompactRelayRuntimeSessionErrorV1> for CompactRelayRuntimeTransportErrorV1 {
    fn from(value: CompactRelayRuntimeSessionErrorV1) -> Self {
        Self::Session(value)
    }
}

impl From<CompactRelayCarrierErrorV1> for CompactRelayRuntimeTransportErrorV1 {
    fn from(value: CompactRelayCarrierErrorV1) -> Self {
        Self::Carrier(value)
    }
}

fn require_peer_id(peer_id: &str) -> Result<(), CompactRelayRuntimeSessionErrorV1> {
    if peer_id.trim().is_empty() {
        Err(CompactRelayRuntimeSessionErrorV1::EmptyPeerId)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompactRelayRuntimeSessionBookV1 {
    local_capabilities: Option<CompactRelayCapabilitiesV1>,
    remote_capabilities: BTreeMap<String, CompactRelayCapabilitiesV1>,
    in_flight: BTreeMap<(String, Hash), CompactBlockReconstructionRequestStateV1>,
}

impl CompactRelayRuntimeSessionBookV1 {
    fn retained_state_upper_bound_bytes(state: &CompactBlockReconstructionRequestStateV1) -> u64 {
        let transaction_bytes = u64::try_from(state.retained_known_transaction_count())
            .unwrap_or(u64::MAX)
            .saturating_mul(MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1);
        transaction_bytes
            .saturating_add(u64::try_from(COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1).unwrap_or(u64::MAX))
    }

    fn retained_bytes_for_peer(&self, peer_id: &str) -> u64 {
        self.in_flight
            .iter()
            .filter(|((owner, _), _)| owner == peer_id)
            .fold(0_u64, |total, (_, state)| {
                total.saturating_add(Self::retained_state_upper_bound_bytes(state))
            })
    }

    fn retained_bytes_global(&self) -> u64 {
        self.in_flight.values().fold(0_u64, |total, state| {
            total.saturating_add(Self::retained_state_upper_bound_bytes(state))
        })
    }

    pub fn configure_local(
        &mut self,
        expected_chain_id: &str,
        capabilities: CompactRelayCapabilitiesV1,
    ) -> Result<(), CompactRelayRuntimeSessionErrorV1> {
        capabilities.validate_for_chain(expected_chain_id)?;
        self.local_capabilities = Some(capabilities);
        self.remote_capabilities.clear();
        self.in_flight.clear();
        Ok(())
    }

    pub fn reset_local(&mut self) {
        self.local_capabilities = None;
        self.remote_capabilities.clear();
        self.in_flight.clear();
    }

    pub fn local_capabilities(&self) -> Option<&CompactRelayCapabilitiesV1> {
        self.local_capabilities.as_ref()
    }

    pub fn remote_capabilities(&self, peer_id: &str) -> Option<&CompactRelayCapabilitiesV1> {
        self.remote_capabilities.get(peer_id)
    }

    pub fn peer_session_authorized(&self, peer_id: &str) -> bool {
        self.local_capabilities.is_some() && self.remote_capabilities.contains_key(peer_id)
    }

    pub fn peer_disconnected(&mut self, peer_id: &str) {
        self.remote_capabilities.remove(peer_id);
        self.in_flight.retain(|(owner, _), _| owner != peer_id);
    }

    pub fn note_inbound(
        &mut self,
        peer_id: &str,
        wire: &CompactRelayWireV1,
    ) -> Result<(), CompactRelayRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        let local = self
            .local_capabilities
            .as_ref()
            .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?;

        match wire {
            CompactRelayWireV1::CapabilityProbe => Ok(()),
            CompactRelayWireV1::Capabilities(remote) => {
                remote.validate_for_chain(&local.chain_id)?;
                if remote != local {
                    return Err(CompactRelayRuntimeSessionErrorV1::LocalCapabilitySurfaceMismatch);
                }
                self.remote_capabilities
                    .insert(peer_id.to_string(), remote.clone());
                Ok(())
            }
            _ if self.peer_session_authorized(peer_id) => Ok(()),
            _ => Err(
                CompactRelayRuntimeSessionErrorV1::PeerCapabilitySessionMissing {
                    peer_id: peer_id.to_string(),
                },
            ),
        }
    }

    pub fn validate_outbound(
        &self,
        peer_id: &str,
        protocol_route_authorized: bool,
        wire: &CompactRelayWireV1,
    ) -> Result<(), CompactRelayRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        if !protocol_route_authorized {
            return Err(
                CompactRelayRuntimeSessionErrorV1::ProtocolRouteUnauthorized {
                    peer_id: peer_id.to_string(),
                },
            );
        }
        let local = self
            .local_capabilities
            .as_ref()
            .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?;

        match wire {
            CompactRelayWireV1::CapabilityProbe => Ok(()),
            CompactRelayWireV1::Capabilities(outbound) if outbound == local => Ok(()),
            CompactRelayWireV1::Capabilities(_) => {
                Err(CompactRelayRuntimeSessionErrorV1::LocalCapabilitySurfaceMismatch)
            }
            _ if self.peer_session_authorized(peer_id) => Ok(()),
            _ => Err(
                CompactRelayRuntimeSessionErrorV1::PeerCapabilitySessionMissing {
                    peer_id: peer_id.to_string(),
                },
            ),
        }
    }

    pub fn register_in_flight(
        &mut self,
        peer_id: &str,
        state: CompactBlockReconstructionRequestStateV1,
    ) -> Result<(), CompactRelayRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        if !self.peer_session_authorized(peer_id) {
            return Err(
                CompactRelayRuntimeSessionErrorV1::PeerCapabilitySessionMissing {
                    peer_id: peer_id.to_string(),
                },
            );
        }

        let block_hash = state.request.block_hash.clone();
        let key = (peer_id.to_string(), block_hash.clone());
        if self.in_flight.contains_key(&key) {
            return Err(CompactRelayRuntimeSessionErrorV1::InFlightAlreadyExists {
                peer_id: peer_id.to_string(),
                block_hash,
            });
        }

        let count = self
            .in_flight
            .keys()
            .filter(|(owner, _)| owner == peer_id)
            .count();
        if count >= COMPACT_RELAY_MAX_INFLIGHT_PER_PEER_V1 {
            return Err(CompactRelayRuntimeSessionErrorV1::InFlightLimitExceeded {
                peer_id: peer_id.to_string(),
                maximum: COMPACT_RELAY_MAX_INFLIGHT_PER_PEER_V1,
            });
        }

        let candidate_bytes = Self::retained_state_upper_bound_bytes(&state);
        let peer_retained = self
            .retained_bytes_for_peer(peer_id)
            .saturating_add(candidate_bytes);
        if peer_retained > COMPACT_RELAY_MAX_RETAINED_BYTES_PER_PEER_V1 {
            return Err(
                CompactRelayRuntimeSessionErrorV1::InFlightRetainedBytesPerPeerLimitExceeded {
                    peer_id: peer_id.to_string(),
                    observed: peer_retained,
                    maximum: COMPACT_RELAY_MAX_RETAINED_BYTES_PER_PEER_V1,
                },
            );
        }

        let global_retained = self.retained_bytes_global().saturating_add(candidate_bytes);
        if global_retained > COMPACT_RELAY_MAX_RETAINED_BYTES_GLOBAL_V1 {
            return Err(
                CompactRelayRuntimeSessionErrorV1::InFlightRetainedBytesGlobalLimitExceeded {
                    observed: global_retained,
                    maximum: COMPACT_RELAY_MAX_RETAINED_BYTES_GLOBAL_V1,
                },
            );
        }

        self.in_flight.insert(key, state);
        Ok(())
    }

    pub fn in_flight_count(&self, peer_id: &str) -> usize {
        self.in_flight
            .keys()
            .filter(|(owner, _)| owner == peer_id)
            .count()
    }

    pub fn abandon_in_flight(&mut self, peer_id: &str, block_hash: &str) -> bool {
        self.in_flight
            .remove(&(peer_id.to_string(), block_hash.to_string()))
            .is_some()
    }

    pub fn complete_in_flight(
        &mut self,
        peer_id: &str,
        announcement: &CompactBlockAnnouncementV1,
        response: &CompactTransactionResponseV1,
    ) -> Result<CompactBlockReconstructionPlanV1, CompactRelayRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        let block_hash = announcement.block_hash.clone();
        let key = (peer_id.to_string(), block_hash.clone());
        let state = self.in_flight.get(&key).ok_or_else(|| {
            CompactRelayRuntimeSessionErrorV1::InFlightMissing {
                peer_id: peer_id.to_string(),
                block_hash,
            }
        })?;

        let chain_id = self
            .local_capabilities
            .as_ref()
            .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?
            .chain_id
            .clone();
        let completed = complete_compact_block_reconstruction_for_chain_v1(
            announcement,
            state,
            response,
            &chain_id,
        )?;
        self.in_flight.remove(&key);
        Ok(completed)
    }
}

pub fn encode_authorized_compact_relay_tip_v1(
    encoded_tips: &[u8],
    sessions: &CompactRelayRuntimeSessionBookV1,
    peer_id: &str,
    protocol_route_authorized: bool,
    wire: &CompactRelayWireV1,
) -> Result<Vec<u8>, CompactRelayRuntimeTransportErrorV1> {
    sessions.validate_outbound(peer_id, protocol_route_authorized, wire)?;
    let local = sessions
        .local_capabilities()
        .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?;

    Ok(attach_compact_relay_carrier_v1(
        encoded_tips,
        &CompactRelayCarrierV1 {
            target_peer_id: peer_id.to_string(),
            chain_id: local.chain_id.clone(),
            wire: wire.clone(),
        },
    )?)
}

pub fn decode_authorized_compact_relay_tip_v1(
    bytes: &[u8],
    source_peer: Option<&str>,
    local_peer_id: &str,
    sessions: &mut CompactRelayRuntimeSessionBookV1,
    protocol_route_authorized: bool,
) -> Result<Option<(String, CompactRelayWireV1)>, CompactRelayRuntimeTransportErrorV1> {
    let Some(peer_id) = source_peer else {
        return Ok(None);
    };
    if !protocol_route_authorized {
        return Ok(None);
    }

    let decoded = decode_network_message_with_compact_relay_for_peer_v1(bytes, local_peer_id)?;
    let Some(carrier) = decoded.compact_relay else {
        return Ok(None);
    };
    let expected_chain = sessions
        .local_capabilities()
        .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?
        .chain_id
        .clone();
    if carrier.chain_id != expected_chain {
        return Err(CompactRelayRuntimeSessionErrorV1::Carrier(
            CompactRelayCarrierErrorV1::ChainIdMismatch {
                expected: expected_chain,
                observed: carrier.chain_id,
            },
        )
        .into());
    }

    sessions.note_inbound(peer_id, &carrier.wire)?;
    Ok(Some((peer_id.to_string(), carrier.wire)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::NetworkMessage;
    use crate::messages::{
        build_compact_block_announcement_v1, plan_compact_block_reconstruction_v1,
        COMPACT_DAG_RELAY_VERSION_V1, P2P_WIRE_MAX_INVENTORY_ITEMS_V1,
    };
    use pulsedag_core::types::{
        compute_block_hash, compute_merkle_root, Block, BlockHeader, Transaction, TxOutput,
    };

    const CHAIN_ID: &str = "compact-relay-runtime-testnet";
    const LOCAL_PEER: &str = "peer-compact-runtime-local";
    const PEER: &str = "peer-compact-runtime-remote";

    fn capabilities() -> CompactRelayCapabilitiesV1 {
        CompactRelayCapabilitiesV1::canonical(CHAIN_ID)
    }

    fn configured() -> CompactRelayRuntimeSessionBookV1 {
        let mut sessions = CompactRelayRuntimeSessionBookV1::default();
        sessions.configure_local(CHAIN_ID, capabilities()).unwrap();
        sessions
    }

    fn authorize(sessions: &mut CompactRelayRuntimeSessionBookV1, peer_id: &str) {
        sessions
            .note_inbound(peer_id, &CompactRelayWireV1::Capabilities(capabilities()))
            .unwrap();
    }

    fn transaction(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn block(hash: &str) -> Block {
        let transactions = vec![
            transaction("coinbase"),
            transaction(&format!("{hash}-a")),
            transaction(&format!("{hash}-b")),
        ];
        let header = BlockHeader {
            version: 1,
            parents: vec!["parent-a".into(), "parent-b".into()],
            timestamp: 1,
            difficulty: 1,
            nonce: 1,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "state".into(),
            blue_score: 2,
            height: 2,
        };
        Block {
            hash: compute_block_hash(&header),
            header,
            transactions,
        }
    }

    fn request_state(block: &Block) -> CompactBlockReconstructionRequestStateV1 {
        let announcement = build_compact_block_announcement_v1(block).unwrap();
        let known = [(
            block.transactions[0].txid.clone(),
            block.transactions[0].clone(),
        )]
        .into_iter()
        .collect();
        match plan_compact_block_reconstruction_v1(&announcement, &known).unwrap() {
            CompactBlockReconstructionPlanV1::RequestTransactions(state) => state,
            other => panic!("unexpected reconstruction plan: {other:?}"),
        }
    }

    fn tips_bytes() -> Vec<u8> {
        serde_json::to_vec(&NetworkMessage::Tips {
            chain_id: CHAIN_ID.into(),
            tips: Vec::new(),
            inventory: None,
        })
        .unwrap()
    }

    #[test]
    fn capability_surface_authorizes_data_and_disconnect_revokes_session() {
        let mut sessions = configured();
        assert!(!sessions.peer_session_authorized(PEER));
        authorize(&mut sessions, PEER);
        assert!(sessions.peer_session_authorized(PEER));

        let announcement = build_compact_block_announcement_v1(&block("block-a")).unwrap();
        sessions
            .validate_outbound(PEER, true, &CompactRelayWireV1::Announce(announcement))
            .unwrap();

        sessions.peer_disconnected(PEER);
        assert!(!sessions.peer_session_authorized(PEER));
    }

    #[test]
    fn data_is_fail_closed_before_capability_negotiation() {
        let sessions = configured();
        let announcement = build_compact_block_announcement_v1(&block("block-b")).unwrap();
        assert!(matches!(
            sessions.validate_outbound(PEER, true, &CompactRelayWireV1::Announce(announcement)),
            Err(CompactRelayRuntimeSessionErrorV1::PeerCapabilitySessionMissing { .. })
        ));
    }

    #[test]
    fn protocol_route_is_required_even_for_capability_probe() {
        let sessions = configured();
        assert!(matches!(
            sessions.validate_outbound(PEER, false, &CompactRelayWireV1::CapabilityProbe),
            Err(CompactRelayRuntimeSessionErrorV1::ProtocolRouteUnauthorized { .. })
        ));
    }

    #[test]
    fn in_flight_state_is_bounded_per_peer_and_duplicate_block_is_rejected() {
        let mut sessions = configured();
        authorize(&mut sessions, PEER);

        let first = block("block-0");
        let state = request_state(&first);
        sessions.register_in_flight(PEER, state.clone()).unwrap();
        assert!(matches!(
            sessions.register_in_flight(PEER, state),
            Err(CompactRelayRuntimeSessionErrorV1::InFlightAlreadyExists { .. })
        ));

        for index in 1..COMPACT_RELAY_MAX_INFLIGHT_PER_PEER_V1 {
            let candidate = block(&format!("block-{index}"));
            sessions
                .register_in_flight(PEER, request_state(&candidate))
                .unwrap();
        }
        assert_eq!(
            sessions.in_flight_count(PEER),
            COMPACT_RELAY_MAX_INFLIGHT_PER_PEER_V1
        );

        let overflow = block("block-overflow");
        assert!(matches!(
            sessions.register_in_flight(PEER, request_state(&overflow)),
            Err(CompactRelayRuntimeSessionErrorV1::InFlightLimitExceeded { .. })
        ));
    }

    fn high_retention_request_state(hash: &str) -> CompactBlockReconstructionRequestStateV1 {
        let transactions = (0..P2P_WIRE_MAX_INVENTORY_ITEMS_V1)
            .map(|index| transaction(&format!("{hash}-tx-{index}")))
            .collect::<Vec<_>>();
        let header = BlockHeader {
            version: 1,
            parents: vec!["parent-a".into(), "parent-b".into()],
            timestamp: 1,
            difficulty: 1,
            nonce: 1,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "state".into(),
            blue_score: 2,
            height: 2,
        };
        let candidate = Block {
            hash: compute_block_hash(&header),
            header,
            transactions,
        };
        let announcement = build_compact_block_announcement_v1(&candidate).unwrap();
        let known = candidate
            .transactions
            .iter()
            .take(P2P_WIRE_MAX_INVENTORY_ITEMS_V1 - 1)
            .cloned()
            .map(|transaction| (transaction.txid.clone(), transaction))
            .collect();
        match plan_compact_block_reconstruction_v1(&announcement, &known).unwrap() {
            CompactBlockReconstructionPlanV1::RequestTransactions(state) => state,
            other => panic!("unexpected reconstruction plan: {other:?}"),
        }
    }

    #[test]
    fn retained_reconstruction_bytes_are_bounded_per_peer() {
        let mut sessions = configured();
        authorize(&mut sessions, PEER);

        sessions
            .register_in_flight(PEER, high_retention_request_state("heavy-a"))
            .unwrap();
        assert!(matches!(
            sessions.register_in_flight(PEER, high_retention_request_state("heavy-b")),
            Err(
                CompactRelayRuntimeSessionErrorV1::InFlightRetainedBytesPerPeerLimitExceeded { .. }
            )
        ));
        assert_eq!(sessions.in_flight_count(PEER), 1);
    }

    #[test]
    fn retained_reconstruction_bytes_are_bounded_globally() {
        let mut sessions = configured();
        for index in 0..7 {
            let peer = format!("peer-heavy-{index}");
            authorize(&mut sessions, &peer);
            let result = sessions.register_in_flight(
                &peer,
                high_retention_request_state(&format!("heavy-global-{index}")),
            );
            if index < 6 {
                result.unwrap();
            } else {
                assert!(matches!(
                    result,
                    Err(
                        CompactRelayRuntimeSessionErrorV1::InFlightRetainedBytesGlobalLimitExceeded {
                            ..
                        }
                    )
                ));
            }
        }
    }

    #[test]
    fn disconnect_clears_peer_in_flight_state() {
        let mut sessions = configured();
        authorize(&mut sessions, PEER);
        let candidate = block("block-disconnect");
        sessions
            .register_in_flight(PEER, request_state(&candidate))
            .unwrap();
        assert_eq!(sessions.in_flight_count(PEER), 1);

        sessions.peer_disconnected(PEER);
        assert_eq!(sessions.in_flight_count(PEER), 0);
    }

    #[test]
    fn response_completion_consumes_exact_registered_state() {
        let mut sessions = configured();
        authorize(&mut sessions, PEER);
        let candidate = block("block-complete");
        let announcement = build_compact_block_announcement_v1(&candidate).unwrap();
        let state = request_state(&candidate);
        let request = state.request.clone();
        sessions.register_in_flight(PEER, state).unwrap();

        let requested = request
            .txids
            .iter()
            .map(|txid| {
                candidate
                    .transactions
                    .iter()
                    .find(|transaction| &transaction.txid == txid)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>();
        let response = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: candidate.hash.clone(),
            transactions: requested,
        };

        assert!(matches!(
            sessions
                .complete_in_flight(PEER, &announcement, &response)
                .unwrap(),
            CompactBlockReconstructionPlanV1::Complete(_)
        ));
        assert_eq!(sessions.in_flight_count(PEER), 0);
    }

    #[test]
    fn invalid_response_preserves_in_flight_state_for_valid_retry() {
        let mut sessions = configured();
        authorize(&mut sessions, PEER);
        let candidate = block("block-retry");
        let announcement = build_compact_block_announcement_v1(&candidate).unwrap();
        let state = request_state(&candidate);
        let request = state.request.clone();
        sessions.register_in_flight(PEER, state).unwrap();

        let invalid = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: candidate.hash.clone(),
            transactions: vec![transaction("wrong")],
        };
        assert!(matches!(
            sessions.complete_in_flight(PEER, &announcement, &invalid),
            Err(CompactRelayRuntimeSessionErrorV1::Compact(
                CompactRelayErrorV1::ResponseCountMismatch { .. }
                    | CompactRelayErrorV1::ResponseTransactionMismatch { .. }
            ))
        ));
        assert_eq!(sessions.in_flight_count(PEER), 1);

        let requested = request
            .txids
            .iter()
            .map(|txid| {
                candidate
                    .transactions
                    .iter()
                    .find(|transaction| &transaction.txid == txid)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>();
        let valid = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: candidate.hash.clone(),
            transactions: requested,
        };
        assert!(matches!(
            sessions
                .complete_in_flight(PEER, &announcement, &valid)
                .unwrap(),
            CompactBlockReconstructionPlanV1::Complete(_)
        ));
        assert_eq!(sessions.in_flight_count(PEER), 0);
    }

    #[test]
    fn transport_roundtrip_is_authorized_only_after_capabilities() {
        let mut sender = configured();
        let mut receiver = configured();
        authorize(&mut sender, PEER);
        authorize(&mut receiver, PEER);

        let encoded = encode_authorized_compact_relay_tip_v1(
            &tips_bytes(),
            &sender,
            PEER,
            true,
            &CompactRelayWireV1::CapabilityProbe,
        )
        .unwrap();

        let decoded =
            decode_authorized_compact_relay_tip_v1(&encoded, Some(PEER), PEER, &mut receiver, true)
                .unwrap()
                .expect("targeted compact relay carrier");
        assert_eq!(decoded.0, PEER);
        assert!(matches!(decoded.1, CompactRelayWireV1::CapabilityProbe));
    }
}
