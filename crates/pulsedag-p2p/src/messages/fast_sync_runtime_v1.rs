use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::ProtocolActivationIdentity;

use super::fast_sync_carrier_v1::{
    attach_fast_sync_carrier_v1, decode_network_message_with_fast_sync_for_peer_v1,
    FastSyncCapabilitiesV1, FastSyncCarrierErrorV1, FastSyncCarrierV1, FastSyncWireErrorV1,
    FastSyncWireV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncRuntimeSessionErrorV1 {
    Wire(FastSyncWireErrorV1),
    LocalCapabilitiesMissing,
    LocalCapabilitySurfaceMismatch(String),
    PeerCapabilitySessionMissing { peer_id: String },
    ProtocolRouteUnauthorized { peer_id: String },
    EmptyPeerId,
}

impl From<FastSyncWireErrorV1> for FastSyncRuntimeSessionErrorV1 {
    fn from(value: FastSyncWireErrorV1) -> Self {
        Self::Wire(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncRuntimeTransportErrorV1 {
    Session(FastSyncRuntimeSessionErrorV1),
    Carrier(FastSyncCarrierErrorV1),
}

impl From<FastSyncRuntimeSessionErrorV1> for FastSyncRuntimeTransportErrorV1 {
    fn from(value: FastSyncRuntimeSessionErrorV1) -> Self {
        Self::Session(value)
    }
}

impl From<FastSyncCarrierErrorV1> for FastSyncRuntimeTransportErrorV1 {
    fn from(value: FastSyncCarrierErrorV1) -> Self {
        Self::Carrier(value)
    }
}

fn capability_surface_matches(
    local: &FastSyncCapabilitiesV1,
    remote: &FastSyncCapabilitiesV1,
) -> bool {
    local.chain_id == remote.chain_id
        && local.genesis_hash == remote.genesis_hash
        && local.protocol_fingerprint == remote.protocol_fingerprint
        && local.manifest_version == remote.manifest_version
        && local.protocol_snapshot_bundle_format_version
            == remote.protocol_snapshot_bundle_format_version
        && local.storage_schema_version == remote.storage_schema_version
        && local.payload_encoding == remote.payload_encoding
}

fn require_capability_surface(
    local: &FastSyncCapabilitiesV1,
    remote: &FastSyncCapabilitiesV1,
) -> Result<(), FastSyncRuntimeSessionErrorV1> {
    if capability_surface_matches(local, remote) {
        Ok(())
    } else {
        Err(FastSyncRuntimeSessionErrorV1::LocalCapabilitySurfaceMismatch(
            "peer fast-sync manifest/bundle/storage-schema/payload-encoding surface differs from the local candidate"
                .to_string(),
        ))
    }
}

fn require_peer_id(peer_id: &str) -> Result<(), FastSyncRuntimeSessionErrorV1> {
    if peer_id.trim().is_empty() {
        Err(FastSyncRuntimeSessionErrorV1::EmptyPeerId)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FastSyncRuntimeSessionBookV1 {
    local_capabilities: Option<FastSyncCapabilitiesV1>,
    remote_capabilities: BTreeMap<String, FastSyncCapabilitiesV1>,
    capability_probe_seen: BTreeSet<String>,
}

impl FastSyncRuntimeSessionBookV1 {
    pub fn configure_local(
        &mut self,
        expected: &ProtocolActivationIdentity,
        capabilities: FastSyncCapabilitiesV1,
    ) -> Result<(), FastSyncRuntimeSessionErrorV1> {
        capabilities.validate_for_expected(expected)?;
        self.local_capabilities = Some(capabilities);
        self.remote_capabilities.clear();
        self.capability_probe_seen.clear();
        Ok(())
    }

    pub fn reset_local(&mut self) {
        self.local_capabilities = None;
        self.remote_capabilities.clear();
        self.capability_probe_seen.clear();
    }

    pub fn local_capabilities(&self) -> Option<&FastSyncCapabilitiesV1> {
        self.local_capabilities.as_ref()
    }

    pub fn remote_capabilities(&self, peer_id: &str) -> Option<&FastSyncCapabilitiesV1> {
        self.remote_capabilities.get(peer_id)
    }

    pub fn peer_session_authorized(&self, peer_id: &str) -> bool {
        self.local_capabilities.is_some()
            && (self.capability_probe_seen.contains(peer_id)
                || self.remote_capabilities.contains_key(peer_id))
    }

    pub fn peer_disconnected(&mut self, peer_id: &str) {
        self.remote_capabilities.remove(peer_id);
        self.capability_probe_seen.remove(peer_id);
    }

    pub fn note_inbound(
        &mut self,
        expected: &ProtocolActivationIdentity,
        peer_id: &str,
        wire: &FastSyncWireV1,
    ) -> Result<(), FastSyncRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        wire.validate_for_chain(&expected.chain_id)?;
        let local = self
            .local_capabilities
            .as_ref()
            .ok_or(FastSyncRuntimeSessionErrorV1::LocalCapabilitiesMissing)?;
        local.validate_for_expected(expected)?;

        match wire {
            FastSyncWireV1::CapabilityProbe { .. } => {
                self.capability_probe_seen.insert(peer_id.to_string());
                Ok(())
            }
            FastSyncWireV1::Capabilities(remote) => {
                remote.validate_for_expected(expected)?;
                require_capability_surface(local, remote)?;
                self.remote_capabilities
                    .insert(peer_id.to_string(), remote.clone());
                Ok(())
            }
            _ if self.peer_session_authorized(peer_id) => Ok(()),
            _ => Err(
                FastSyncRuntimeSessionErrorV1::PeerCapabilitySessionMissing {
                    peer_id: peer_id.to_string(),
                },
            ),
        }
    }

    pub fn validate_outbound(
        &self,
        expected: &ProtocolActivationIdentity,
        peer_id: &str,
        protocol_route_authorized: bool,
        wire: &FastSyncWireV1,
    ) -> Result<(), FastSyncRuntimeSessionErrorV1> {
        require_peer_id(peer_id)?;
        if !protocol_route_authorized {
            return Err(FastSyncRuntimeSessionErrorV1::ProtocolRouteUnauthorized {
                peer_id: peer_id.to_string(),
            });
        }
        wire.validate_for_chain(&expected.chain_id)?;
        let local = self
            .local_capabilities
            .as_ref()
            .ok_or(FastSyncRuntimeSessionErrorV1::LocalCapabilitiesMissing)?;
        local.validate_for_expected(expected)?;

        match wire {
            FastSyncWireV1::CapabilityProbe { .. } => Ok(()),
            FastSyncWireV1::Capabilities(outbound) if outbound == local => Ok(()),
            FastSyncWireV1::Capabilities(_) => Err(
                FastSyncRuntimeSessionErrorV1::LocalCapabilitySurfaceMismatch(
                    "outbound fast-sync capabilities do not equal the configured local candidate"
                        .to_string(),
                ),
            ),
            _ if self.peer_session_authorized(peer_id) => Ok(()),
            _ => Err(
                FastSyncRuntimeSessionErrorV1::PeerCapabilitySessionMissing {
                    peer_id: peer_id.to_string(),
                },
            ),
        }
    }
}

pub fn encode_authorized_fast_sync_tip_v1(
    encoded_tips: &[u8],
    expected: &ProtocolActivationIdentity,
    sessions: &FastSyncRuntimeSessionBookV1,
    peer_id: &str,
    protocol_route_authorized: bool,
    wire: &FastSyncWireV1,
) -> Result<Vec<u8>, FastSyncRuntimeTransportErrorV1> {
    sessions.validate_outbound(expected, peer_id, protocol_route_authorized, wire)?;
    Ok(attach_fast_sync_carrier_v1(
        encoded_tips,
        &FastSyncCarrierV1 {
            target_peer_id: peer_id.to_string(),
            wire: wire.clone(),
        },
    )?)
}

pub fn decode_authorized_fast_sync_tip_v1(
    bytes: &[u8],
    source_peer: Option<&str>,
    local_peer_id: &str,
    expected: &ProtocolActivationIdentity,
    sessions: &mut FastSyncRuntimeSessionBookV1,
    protocol_route_authorized: bool,
) -> Result<Option<(String, FastSyncWireV1)>, FastSyncRuntimeTransportErrorV1> {
    let Some(peer_id) = source_peer else {
        return Ok(None);
    };
    if !protocol_route_authorized {
        return Ok(None);
    }
    let decoded = decode_network_message_with_fast_sync_for_peer_v1(bytes, local_peer_id)?;
    let Some(carrier) = decoded.fast_sync else {
        return Ok(None);
    };
    sessions.note_inbound(expected, peer_id, &carrier.wire)?;
    Ok(Some((peer_id.to_string(), carrier.wire)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::NetworkMessage;
    use pulsedag_core::GHOSTDAG_V1_ORDERING_VERSION;

    const CHAIN_ID: &str = "fast-sync-runtime-session-testnet";
    const LOCAL_PEER: &str = "peer-fast-sync-local";
    const PEER: &str = "peer-fast-sync-runtime";

    fn identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            CHAIN_ID.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn capabilities() -> FastSyncCapabilitiesV1 {
        let expected = identity();
        FastSyncCapabilitiesV1 {
            contract_version: super::super::fast_sync_carrier_v1::P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            genesis_hash: expected.genesis_hash.clone(),
            protocol_fingerprint: expected.fingerprint().unwrap(),
            manifest_version: 1,
            protocol_snapshot_bundle_format_version: 2,
            storage_schema_version: 1,
            payload_encoding: "bincode-1.3-fast-sync-bundle-v1".to_string(),
            max_chunk_bytes: 24 * 1024,
            max_commitments_per_page: 256,
        }
    }

    fn tips_bytes() -> Vec<u8> {
        serde_json::to_vec(&NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: Vec::new(),
            inventory: None,
        })
        .unwrap()
    }

    fn summary_request() -> FastSyncWireV1 {
        FastSyncWireV1::GetTransferSummary {
            chain_id: CHAIN_ID.to_string(),
        }
    }

    #[test]
    fn remote_schema_mismatch_fails_before_transfer_messages_are_authorized() {
        let expected = identity();
        let mut book = FastSyncRuntimeSessionBookV1::default();
        book.configure_local(&expected, capabilities()).unwrap();
        let mut remote = capabilities();
        remote.storage_schema_version += 1;

        let error = book
            .note_inbound(&expected, PEER, &FastSyncWireV1::Capabilities(remote))
            .unwrap_err();
        assert!(matches!(
            error,
            FastSyncRuntimeSessionErrorV1::LocalCapabilitySurfaceMismatch(_)
        ));
        assert!(!book.peer_session_authorized(PEER));
        assert!(book
            .validate_outbound(&expected, PEER, true, &summary_request())
            .is_err());
    }

    #[test]
    fn compatible_remote_capabilities_authorize_transfer_and_disconnect_revokes_it() {
        let expected = identity();
        let mut book = FastSyncRuntimeSessionBookV1::default();
        book.configure_local(&expected, capabilities()).unwrap();
        book.note_inbound(
            &expected,
            PEER,
            &FastSyncWireV1::Capabilities(capabilities()),
        )
        .unwrap();

        assert!(book.peer_session_authorized(PEER));
        book.validate_outbound(&expected, PEER, true, &summary_request())
            .unwrap();
        book.peer_disconnected(PEER);
        assert!(!book.peer_session_authorized(PEER));
        assert!(book
            .validate_outbound(&expected, PEER, true, &summary_request())
            .is_err());
    }

    #[test]
    fn inbound_probe_opens_server_side_session_but_protocol_route_is_still_required() {
        let expected = identity();
        let mut book = FastSyncRuntimeSessionBookV1::default();
        book.configure_local(&expected, capabilities()).unwrap();
        let probe = FastSyncWireV1::CapabilityProbe {
            chain_id: CHAIN_ID.to_string(),
        };
        book.note_inbound(&expected, PEER, &probe).unwrap();

        assert!(book.peer_session_authorized(PEER));
        assert!(book
            .validate_outbound(&expected, PEER, false, &summary_request())
            .is_err());
        book.validate_outbound(&expected, PEER, true, &summary_request())
            .unwrap();
    }

    #[test]
    fn outbound_capabilities_must_equal_the_configured_local_candidate() {
        let expected = identity();
        let mut book = FastSyncRuntimeSessionBookV1::default();
        let local = capabilities();
        book.configure_local(&expected, local.clone()).unwrap();

        book.validate_outbound(
            &expected,
            PEER,
            true,
            &FastSyncWireV1::Capabilities(local.clone()),
        )
        .unwrap();

        let mut forged = local;
        forged.payload_encoding = "other-encoding".to_string();
        assert!(book
            .validate_outbound(&expected, PEER, true, &FastSyncWireV1::Capabilities(forged),)
            .is_err());
    }

    #[test]
    fn outbound_live_carrier_requires_protocol_route_before_encoding() {
        let expected = identity();
        let mut book = FastSyncRuntimeSessionBookV1::default();
        book.configure_local(&expected, capabilities()).unwrap();
        let probe = FastSyncWireV1::CapabilityProbe {
            chain_id: CHAIN_ID.to_string(),
        };

        assert!(encode_authorized_fast_sync_tip_v1(
            &tips_bytes(),
            &expected,
            &book,
            PEER,
            false,
            &probe,
        )
        .is_err());
        let encoded =
            encode_authorized_fast_sync_tip_v1(&tips_bytes(), &expected, &book, PEER, true, &probe)
                .unwrap();
        let decoded = decode_network_message_with_fast_sync_for_peer_v1(&encoded, PEER).unwrap();
        assert_eq!(decoded.fast_sync.unwrap().wire, probe);
    }

    #[test]
    fn inbound_live_capability_surface_is_checked_before_session_admission() {
        let expected = identity();
        let mut receiver = FastSyncRuntimeSessionBookV1::default();
        receiver.configure_local(&expected, capabilities()).unwrap();
        let mut incompatible = capabilities();
        incompatible.manifest_version += 1;
        let encoded = attach_fast_sync_carrier_v1(
            &tips_bytes(),
            &FastSyncCarrierV1 {
                target_peer_id: LOCAL_PEER.to_string(),
                wire: FastSyncWireV1::Capabilities(incompatible),
            },
        )
        .unwrap();

        assert!(decode_authorized_fast_sync_tip_v1(
            &encoded,
            Some(PEER),
            LOCAL_PEER,
            &expected,
            &mut receiver,
            true,
        )
        .is_err());
        assert!(!receiver.peer_session_authorized(PEER));
    }

    #[test]
    fn inbound_other_target_is_ignored_without_mutating_session_state() {
        let expected = identity();
        let mut receiver = FastSyncRuntimeSessionBookV1::default();
        receiver.configure_local(&expected, capabilities()).unwrap();
        let encoded = attach_fast_sync_carrier_v1(
            &tips_bytes(),
            &FastSyncCarrierV1 {
                target_peer_id: "another-peer".to_string(),
                wire: FastSyncWireV1::CapabilityProbe {
                    chain_id: CHAIN_ID.to_string(),
                },
            },
        )
        .unwrap();

        assert!(decode_authorized_fast_sync_tip_v1(
            &encoded,
            Some(PEER),
            LOCAL_PEER,
            &expected,
            &mut receiver,
            true,
        )
        .unwrap()
        .is_none());
        assert!(!receiver.peer_session_authorized(PEER));
    }
}
