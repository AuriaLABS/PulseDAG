use std::collections::BTreeMap;

use pulsedag_core::ProtocolActivationIdentity;
use serde::{Deserialize, Serialize};

pub const P2P_PROTOCOL_CAPABILITIES_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolCapabilitiesV1 {
    pub capabilities_version: u32,
    pub protocol_identity: ProtocolActivationIdentity,
    pub consensus_metadata_schema_version: u32,
    pub finality_policy_version: String,
    pub supports_dag_frontier: bool,
    pub supports_consensus_metadata: bool,
    pub high_cadence_allowed: bool,
}

impl ProtocolCapabilitiesV1 {
    pub fn validate_shape(&self) -> Result<(), ProtocolCompatibilityError> {
        if self.capabilities_version != P2P_PROTOCOL_CAPABILITIES_VERSION {
            return Err(ProtocolCompatibilityError::CapabilitiesVersionMismatch {
                local: P2P_PROTOCOL_CAPABILITIES_VERSION,
                remote: self.capabilities_version,
            });
        }
        self.protocol_identity
            .validate()
            .map_err(|detail| ProtocolCompatibilityError::InvalidProtocolIdentity { detail })?;
        if self.consensus_metadata_schema_version == 0 {
            return Err(ProtocolCompatibilityError::InvalidConsensusMetadataSchemaVersion);
        }
        if self.finality_policy_version.is_empty() {
            return Err(ProtocolCompatibilityError::EmptyFinalityPolicyVersion);
        }
        Ok(())
    }
}

/// Reserved v2.4 capability-handshake wire envelope.
///
/// This type is intentionally independent of the current live `NetworkMessage`
/// dispatcher. Task 27 can validate and freeze the compatibility contract before
/// the later slice wires request/response handling into peer admission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ProtocolCapabilityHandshakeV1 {
    GetProtocolCapabilities {
        chain_id: String,
    },
    ProtocolCapabilities {
        chain_id: String,
        capabilities: ProtocolCapabilitiesV1,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum ProtocolCompatibilityError {
    CapabilitiesVersionMismatch { local: u32, remote: u32 },
    InvalidProtocolIdentity { detail: String },
    InvalidConsensusMetadataSchemaVersion,
    EmptyFinalityPolicyVersion,
    ProtocolIdentityMismatch,
    ConsensusMetadataSchemaMismatch { local: u32, remote: u32 },
    FinalityPolicyMismatch { local: String, remote: String },
    DagFrontierCapabilityMissing,
    ConsensusMetadataCapabilityMissing,
    HighCadencePolicyMismatch { local: bool, remote: bool },
}

/// Compare all consensus-affecting P2P capabilities before a node treats peer
/// data as authoritative v2.4 sync input.
///
/// This function is intentionally strict: there is no downgrade or field-wise
/// negotiation for an activated identity. Legacy compatibility remains a
/// separate runtime path until the release activation gate is wired.
pub fn require_protocol_compatibility_v1(
    local: &ProtocolCapabilitiesV1,
    remote: &ProtocolCapabilitiesV1,
) -> Result<(), ProtocolCompatibilityError> {
    local.validate_shape()?;
    remote.validate_shape()?;

    if local.protocol_identity != remote.protocol_identity {
        return Err(ProtocolCompatibilityError::ProtocolIdentityMismatch);
    }
    if local.consensus_metadata_schema_version != remote.consensus_metadata_schema_version {
        return Err(
            ProtocolCompatibilityError::ConsensusMetadataSchemaMismatch {
                local: local.consensus_metadata_schema_version,
                remote: remote.consensus_metadata_schema_version,
            },
        );
    }
    if local.finality_policy_version != remote.finality_policy_version {
        return Err(ProtocolCompatibilityError::FinalityPolicyMismatch {
            local: local.finality_policy_version.clone(),
            remote: remote.finality_policy_version.clone(),
        });
    }
    if !local.supports_dag_frontier || !remote.supports_dag_frontier {
        return Err(ProtocolCompatibilityError::DagFrontierCapabilityMissing);
    }
    if !local.supports_consensus_metadata || !remote.supports_consensus_metadata {
        return Err(ProtocolCompatibilityError::ConsensusMetadataCapabilityMissing);
    }
    if local.high_cadence_allowed != remote.high_cadence_allowed {
        return Err(ProtocolCompatibilityError::HighCadencePolicyMismatch {
            local: local.high_cadence_allowed,
            remote: remote.high_cadence_allowed,
        });
    }
    Ok(())
}

/// Per-peer capability state used to decide whether remote data is eligible for
/// authoritative v2 sync. Incompatibility is a routing decision, not a peer ban:
/// callers may continue using legacy/safe capabilities where Task 22 permits it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProtocolPeerCompatibilityV1 {
    #[default]
    Unknown,
    Compatible,
    Incompatible(ProtocolCompatibilityError),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolPeerStateV1 {
    remote_capabilities: Option<ProtocolCapabilitiesV1>,
    compatibility: ProtocolPeerCompatibilityV1,
}

impl ProtocolPeerStateV1 {
    pub fn remote_capabilities(&self) -> Option<&ProtocolCapabilitiesV1> {
        self.remote_capabilities.as_ref()
    }

    pub fn compatibility(&self) -> &ProtocolPeerCompatibilityV1 {
        &self.compatibility
    }

    pub fn can_use_v2_sync(&self) -> bool {
        matches!(self.compatibility, ProtocolPeerCompatibilityV1::Compatible)
    }

    /// Record one remote capability observation. A syntactically valid but
    /// incompatible peer remains represented explicitly and is merely excluded
    /// from authoritative v2 sync by `can_use_v2_sync`.
    pub fn observe_remote_capabilities(
        &mut self,
        local: &ProtocolCapabilitiesV1,
        remote: ProtocolCapabilitiesV1,
    ) -> &ProtocolPeerCompatibilityV1 {
        self.compatibility = match require_protocol_compatibility_v1(local, &remote) {
            Ok(()) => ProtocolPeerCompatibilityV1::Compatible,
            Err(error) => ProtocolPeerCompatibilityV1::Incompatible(error),
        };
        self.remote_capabilities = Some(remote);
        &self.compatibility
    }

    /// Return the peer to the legacy/unknown state after disconnect, capability
    /// expiry, or a mixed-version fallback that has not negotiated v2.
    pub fn mark_unknown(&mut self) {
        self.remote_capabilities = None;
        self.compatibility = ProtocolPeerCompatibilityV1::Unknown;
    }
}

/// Message classes used by the runtime to avoid sending undecodable v2 sync
/// traffic to legacy or not-yet-negotiated peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolMessageClassV1 {
    LegacySafe,
    ProtocolV2Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolPeerRouteActionV1 {
    SendLegacySafe,
    SendProtocolV2,
    HoldForCapabilities,
    UseLegacyFallback,
}

/// Routing decision deliberately separates compatibility from peer reputation.
/// A version/capability mismatch is not, by itself, evidence of peer misconduct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolPeerRouteDecisionV1 {
    pub action: ProtocolPeerRouteActionV1,
    pub penalize_peer: bool,
}

impl ProtocolPeerRouteDecisionV1 {
    fn without_penalty(action: ProtocolPeerRouteActionV1) -> Self {
        Self {
            action,
            penalize_peer: false,
        }
    }
}

/// Deterministic per-peer routing registry for Task 27 mixed-version behavior.
///
/// The registry does not own connections and does not send messages. It gives
/// the live dispatcher a single fail-closed policy boundary:
///
/// - legacy-safe traffic remains available to unknown/incompatible peers;
/// - v2 sync traffic is emitted only to exact-compatible peers;
/// - unknown peers hold v2 traffic until capability negotiation completes;
/// - incompatible peers fall back to legacy-safe capabilities without penalty;
/// - eligible-v2 peer ordering is deterministic by peer id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolPeerRouterV1 {
    peers: BTreeMap<String, ProtocolPeerStateV1>,
}

impl ProtocolPeerRouterV1 {
    pub fn observe_remote_capabilities(
        &mut self,
        peer_id: impl Into<String>,
        local: &ProtocolCapabilitiesV1,
        remote: ProtocolCapabilitiesV1,
    ) -> ProtocolPeerCompatibilityV1 {
        let state = self.peers.entry(peer_id.into()).or_default();
        state.observe_remote_capabilities(local, remote).clone()
    }

    pub fn compatibility(&self, peer_id: &str) -> ProtocolPeerCompatibilityV1 {
        self.peers
            .get(peer_id)
            .map(|state| state.compatibility().clone())
            .unwrap_or_default()
    }

    pub fn route(
        &self,
        peer_id: &str,
        message_class: ProtocolMessageClassV1,
    ) -> ProtocolPeerRouteDecisionV1 {
        if message_class == ProtocolMessageClassV1::LegacySafe {
            return ProtocolPeerRouteDecisionV1::without_penalty(
                ProtocolPeerRouteActionV1::SendLegacySafe,
            );
        }

        match self.compatibility(peer_id) {
            ProtocolPeerCompatibilityV1::Compatible => {
                ProtocolPeerRouteDecisionV1::without_penalty(
                    ProtocolPeerRouteActionV1::SendProtocolV2,
                )
            }
            ProtocolPeerCompatibilityV1::Unknown => ProtocolPeerRouteDecisionV1::without_penalty(
                ProtocolPeerRouteActionV1::HoldForCapabilities,
            ),
            ProtocolPeerCompatibilityV1::Incompatible(_) => {
                ProtocolPeerRouteDecisionV1::without_penalty(
                    ProtocolPeerRouteActionV1::UseLegacyFallback,
                )
            }
        }
    }

    pub fn eligible_v2_peers(&self) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, state)| state.can_use_v2_sync())
            .map(|(peer_id, _)| peer_id.clone())
            .collect()
    }

    pub fn mark_peer_unknown(&mut self, peer_id: &str) {
        self.peers
            .entry(peer_id.to_string())
            .or_default()
            .mark_unknown();
    }

    pub fn remove_peer(&mut self, peer_id: &str) -> bool {
        self.peers.remove(peer_id).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolPeerSessionErrorV1 {
    LocalCapabilities(ProtocolCompatibilityError),
    LocalChainIdMismatch { expected: String, observed: String },
    LocalCapabilitiesUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolPeerSessionObservationV1 {
    LegacyNoCapabilities,
    Compatible,
    Incompatible(ProtocolCompatibilityError),
}

/// Runtime-owned Task 27 negotiation state.
///
/// This type intentionally contains no transport or async behavior. The live
/// libp2p loop can keep one instance in its existing mutex-protected state and
/// feed it capability observations keyed by the authenticated peer id. Local
/// protocol identity must be configured explicitly from the node's real
/// activation state; it is never synthesized from `chain_id` alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolPeerSessionV1 {
    local_capabilities: Option<ProtocolCapabilitiesV1>,
    router: ProtocolPeerRouterV1,
}

impl ProtocolPeerSessionV1 {
    pub fn local_capabilities(&self) -> Option<&ProtocolCapabilitiesV1> {
        self.local_capabilities.as_ref()
    }

    pub fn configure_local_capabilities(
        &mut self,
        expected_chain_id: &str,
        capabilities: ProtocolCapabilitiesV1,
    ) -> Result<(), ProtocolPeerSessionErrorV1> {
        capabilities
            .validate_shape()
            .map_err(ProtocolPeerSessionErrorV1::LocalCapabilities)?;
        if capabilities.protocol_identity.chain_id != expected_chain_id {
            return Err(ProtocolPeerSessionErrorV1::LocalChainIdMismatch {
                expected: expected_chain_id.to_string(),
                observed: capabilities.protocol_identity.chain_id.clone(),
            });
        }
        self.local_capabilities = Some(capabilities);
        self.router = ProtocolPeerRouterV1::default();
        Ok(())
    }

    pub fn observe_remote_capabilities(
        &mut self,
        peer_id: &str,
        remote: Option<ProtocolCapabilitiesV1>,
    ) -> Result<ProtocolPeerSessionObservationV1, ProtocolPeerSessionErrorV1> {
        let Some(remote) = remote else {
            self.router.mark_peer_unknown(peer_id);
            return Ok(ProtocolPeerSessionObservationV1::LegacyNoCapabilities);
        };
        let local = self
            .local_capabilities
            .as_ref()
            .ok_or(ProtocolPeerSessionErrorV1::LocalCapabilitiesUnavailable)?;
        let compatibility = self
            .router
            .observe_remote_capabilities(peer_id.to_string(), local, remote);
        Ok(match compatibility {
            ProtocolPeerCompatibilityV1::Unknown => {
                unreachable!("concrete capability observation cannot remain unknown")
            }
            ProtocolPeerCompatibilityV1::Compatible => {
                ProtocolPeerSessionObservationV1::Compatible
            }
            ProtocolPeerCompatibilityV1::Incompatible(error) => {
                ProtocolPeerSessionObservationV1::Incompatible(error)
            }
        })
    }

    pub fn compatibility(&self, peer_id: &str) -> ProtocolPeerCompatibilityV1 {
        self.router.compatibility(peer_id)
    }

    pub fn route(
        &self,
        peer_id: &str,
        message_class: ProtocolMessageClassV1,
    ) -> ProtocolPeerRouteDecisionV1 {
        self.router.route(peer_id, message_class)
    }

    pub fn eligible_v2_peers(&self) -> Vec<String> {
        self.router.eligible_v2_peers()
    }

    pub fn peer_disconnected(&mut self, peer_id: &str) {
        self.router.remove_peer(peer_id);
    }

    pub fn reset_local_capabilities(&mut self) {
        self.local_capabilities = None;
        self.router = ProtocolPeerRouterV1::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn capabilities(chain_id: &str) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: ProtocolActivationIdentity::activated_v2(
                chain_id.to_string(),
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

    #[test]
    fn handshake_wire_round_trips_without_live_dispatcher_integration() {
        let messages = [
            ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                chain_id: "pulsedag-testnet".to_string(),
            },
            ProtocolCapabilityHandshakeV1::ProtocolCapabilities {
                chain_id: "pulsedag-testnet".to_string(),
                capabilities: capabilities("pulsedag-testnet"),
            },
        ];

        for message in messages {
            let encoded = serde_json::to_vec(&message).unwrap();
            let decoded: ProtocolCapabilityHandshakeV1 = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn exact_consensus_capabilities_are_compatible() {
        let local = capabilities("pulsedag-testnet");
        let remote = local.clone();
        assert_eq!(require_protocol_compatibility_v1(&local, &remote), Ok(()));
    }

    #[test]
    fn chain_identity_mismatch_fails_closed() {
        let local = capabilities("pulsedag-testnet");
        let remote = capabilities("pulsedag-private");
        assert_eq!(
            require_protocol_compatibility_v1(&local, &remote),
            Err(ProtocolCompatibilityError::ProtocolIdentityMismatch)
        );
    }

    #[test]
    fn ordering_identity_mismatch_fails_closed() {
        let local = capabilities("pulsedag-testnet");
        let mut remote = local.clone();
        remote.protocol_identity.dag_ordering_version = "different-ordering".to_string();
        assert_eq!(
            require_protocol_compatibility_v1(&local, &remote),
            Err(ProtocolCompatibilityError::ProtocolIdentityMismatch)
        );
    }

    #[test]
    fn missing_consensus_sync_capability_fails_closed() {
        let local = capabilities("pulsedag-testnet");
        let mut remote = local.clone();
        remote.supports_consensus_metadata = false;
        assert_eq!(
            require_protocol_compatibility_v1(&local, &remote),
            Err(ProtocolCompatibilityError::ConsensusMetadataCapabilityMissing)
        );
    }

    #[test]
    fn high_cadence_policy_must_match() {
        let local = capabilities("pulsedag-testnet");
        let mut remote = local.clone();
        remote.high_cadence_allowed = true;
        assert_eq!(
            require_protocol_compatibility_v1(&local, &remote),
            Err(ProtocolCompatibilityError::HighCadencePolicyMismatch {
                local: false,
                remote: true
            })
        );
    }

    #[test]
    fn peer_state_starts_unknown_and_does_not_authorize_v2_sync() {
        let state = ProtocolPeerStateV1::default();
        assert_eq!(state.compatibility(), &ProtocolPeerCompatibilityV1::Unknown);
        assert!(state.remote_capabilities().is_none());
        assert!(!state.can_use_v2_sync());
    }

    #[test]
    fn peer_state_authorizes_only_exact_compatible_v2_capabilities() {
        let local = capabilities("pulsedag-testnet");
        let mut state = ProtocolPeerStateV1::default();
        let observed = state.observe_remote_capabilities(&local, local.clone());

        assert_eq!(observed, &ProtocolPeerCompatibilityV1::Compatible);
        assert_eq!(state.remote_capabilities(), Some(&local));
        assert!(state.can_use_v2_sync());
    }

    #[test]
    fn incompatible_peer_is_retained_but_excluded_from_v2_sync() {
        let local = capabilities("pulsedag-testnet");
        let mut remote = local.clone();
        remote.protocol_identity.dag_ordering_version = "different-ordering".to_string();
        let mut state = ProtocolPeerStateV1::default();
        let observed = state.observe_remote_capabilities(&local, remote.clone());

        assert_eq!(
            observed,
            &ProtocolPeerCompatibilityV1::Incompatible(
                ProtocolCompatibilityError::ProtocolIdentityMismatch
            )
        );
        assert_eq!(state.remote_capabilities(), Some(&remote));
        assert!(!state.can_use_v2_sync());
    }

    #[test]
    fn peer_state_can_return_to_unknown_for_legacy_fallback() {
        let local = capabilities("pulsedag-testnet");
        let mut state = ProtocolPeerStateV1::default();
        state.observe_remote_capabilities(&local, local.clone());
        assert!(state.can_use_v2_sync());

        state.mark_unknown();
        assert_eq!(state.compatibility(), &ProtocolPeerCompatibilityV1::Unknown);
        assert!(state.remote_capabilities().is_none());
        assert!(!state.can_use_v2_sync());
    }

    #[test]
    fn unknown_peer_never_receives_protocol_v2_before_negotiation() {
        let router = ProtocolPeerRouterV1::default();

        assert_eq!(
            router.route("legacy-peer", ProtocolMessageClassV1::ProtocolV2Sync),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::HoldForCapabilities,
                penalize_peer: false,
            }
        );
        assert_eq!(
            router.route("legacy-peer", ProtocolMessageClassV1::LegacySafe),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::SendLegacySafe,
                penalize_peer: false,
            }
        );
    }

    #[test]
    fn exact_compatible_peer_is_the_only_v2_send_route() {
        let local = capabilities("pulsedag-testnet");
        let mut router = ProtocolPeerRouterV1::default();
        assert_eq!(
            router.observe_remote_capabilities("peer-v2", &local, local.clone()),
            ProtocolPeerCompatibilityV1::Compatible
        );

        assert_eq!(
            router.route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::SendProtocolV2,
                penalize_peer: false,
            }
        );
        assert_eq!(router.eligible_v2_peers(), vec!["peer-v2".to_string()]);
    }

    #[test]
    fn incompatible_peer_falls_back_without_false_penalty() {
        let local = capabilities("pulsedag-testnet");
        let mut remote = local.clone();
        remote.supports_dag_frontier = false;
        let mut router = ProtocolPeerRouterV1::default();

        assert_eq!(
            router.observe_remote_capabilities("peer-old", &local, remote),
            ProtocolPeerCompatibilityV1::Incompatible(
                ProtocolCompatibilityError::DagFrontierCapabilityMissing
            )
        );
        assert_eq!(
            router.route("peer-old", ProtocolMessageClassV1::ProtocolV2Sync),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::UseLegacyFallback,
                penalize_peer: false,
            }
        );
        assert_eq!(
            router.route("peer-old", ProtocolMessageClassV1::LegacySafe),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::SendLegacySafe,
                penalize_peer: false,
            }
        );
    }

    #[test]
    fn eligible_v2_peer_order_is_deterministic() {
        let local = capabilities("pulsedag-testnet");
        let mut router = ProtocolPeerRouterV1::default();
        for peer in ["peer-z", "peer-a", "peer-m"] {
            router.observe_remote_capabilities(peer, &local, local.clone());
        }

        assert_eq!(
            router.eligible_v2_peers(),
            vec![
                "peer-a".to_string(),
                "peer-m".to_string(),
                "peer-z".to_string(),
            ]
        );
    }

    #[test]
    fn disconnect_or_capability_expiry_returns_peer_to_hold_state() {
        let local = capabilities("pulsedag-testnet");
        let mut router = ProtocolPeerRouterV1::default();
        router.observe_remote_capabilities("peer-v2", &local, local.clone());
        assert_eq!(
            router
                .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::SendProtocolV2
        );

        router.mark_peer_unknown("peer-v2");
        assert_eq!(
            router.compatibility("peer-v2"),
            ProtocolPeerCompatibilityV1::Unknown
        );
        assert_eq!(
            router.route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync),
            ProtocolPeerRouteDecisionV1 {
                action: ProtocolPeerRouteActionV1::HoldForCapabilities,
                penalize_peer: false,
            }
        );
        assert!(router.eligible_v2_peers().is_empty());
        assert!(router.remove_peer("peer-v2"));
        assert!(!router.remove_peer("peer-v2"));
    }

    #[test]
    fn peer_session_requires_real_local_identity_before_observation() {
        let mut session = ProtocolPeerSessionV1::default();
        assert_eq!(
            session.observe_remote_capabilities(
                "peer-v2",
                Some(capabilities("pulsedag-testnet")),
            ),
            Err(ProtocolPeerSessionErrorV1::LocalCapabilitiesUnavailable)
        );
        assert_eq!(
            session.compatibility("peer-v2"),
            ProtocolPeerCompatibilityV1::Unknown
        );
    }

    #[test]
    fn peer_session_configures_valid_local_identity_and_routes_compatible_peer() {
        let local = capabilities("pulsedag-testnet");
        let mut session = ProtocolPeerSessionV1::default();
        assert_eq!(
            session.configure_local_capabilities("pulsedag-testnet", local.clone()),
            Ok(())
        );
        assert_eq!(session.local_capabilities(), Some(&local));
        assert_eq!(
            session.observe_remote_capabilities("peer-v2", Some(local)),
            Ok(ProtocolPeerSessionObservationV1::Compatible)
        );
        assert_eq!(
            session
                .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::SendProtocolV2
        );
        assert_eq!(session.eligible_v2_peers(), vec!["peer-v2".to_string()]);
    }

    #[test]
    fn peer_session_legacy_observation_and_disconnect_revoke_v2_authorization() {
        let local = capabilities("pulsedag-testnet");
        let mut session = ProtocolPeerSessionV1::default();
        session
            .configure_local_capabilities("pulsedag-testnet", local.clone())
            .unwrap();
        session
            .observe_remote_capabilities("peer-v2", Some(local))
            .unwrap();

        assert_eq!(
            session.observe_remote_capabilities("peer-v2", None),
            Ok(ProtocolPeerSessionObservationV1::LegacyNoCapabilities)
        );
        assert_eq!(
            session.compatibility("peer-v2"),
            ProtocolPeerCompatibilityV1::Unknown
        );
        assert!(!session
            .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
            .penalize_peer);

        session.peer_disconnected("peer-v2");
        assert_eq!(
            session.compatibility("peer-v2"),
            ProtocolPeerCompatibilityV1::Unknown
        );
    }

    #[test]
    fn peer_session_rejects_wrong_local_chain_and_falls_back_for_remote_identity_mismatch() {
        let local = capabilities("pulsedag-testnet");
        let mut session = ProtocolPeerSessionV1::default();
        assert_eq!(
            session.configure_local_capabilities("different-chain", local.clone()),
            Err(ProtocolPeerSessionErrorV1::LocalChainIdMismatch {
                expected: "different-chain".to_string(),
                observed: "pulsedag-testnet".to_string(),
            })
        );

        session
            .configure_local_capabilities("pulsedag-testnet", local.clone())
            .unwrap();
        let mut remote = local;
        remote.protocol_identity.genesis_hash = "22".repeat(32);
        assert_eq!(
            session.observe_remote_capabilities("peer-other", Some(remote)),
            Ok(ProtocolPeerSessionObservationV1::Incompatible(
                ProtocolCompatibilityError::ProtocolIdentityMismatch
            ))
        );
        let route = session.route("peer-other", ProtocolMessageClassV1::ProtocolV2Sync);
        assert_eq!(route.action, ProtocolPeerRouteActionV1::UseLegacyFallback);
        assert!(!route.penalize_peer);
    }

    #[test]
    fn peer_session_local_reset_clears_all_authorization() {
        let local = capabilities("pulsedag-testnet");
        let mut session = ProtocolPeerSessionV1::default();
        session
            .configure_local_capabilities("pulsedag-testnet", local.clone())
            .unwrap();
        session
            .observe_remote_capabilities("peer-v2", Some(local))
            .unwrap();
        session.reset_local_capabilities();
        assert!(session.local_capabilities().is_none());
        assert_eq!(
            session.compatibility("peer-v2"),
            ProtocolPeerCompatibilityV1::Unknown
        );
    }
}
