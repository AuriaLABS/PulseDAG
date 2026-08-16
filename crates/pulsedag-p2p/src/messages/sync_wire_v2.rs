use serde::{Deserialize, Serialize};

use super::protocol_v2::ProtocolPeerSessionV1;
use super::{
    DagFrontierResponseV1, DagSyncContractError, ProtocolCapabilityHandshakeV1,
    ProtocolCompatibilityError, ProtocolMessageClassV1, ProtocolPeerCompatibilityV1,
    ProtocolPeerRouteActionV1, ProtocolPeerRouterV1, SelectedChainLocatorV1,
};

/// Canonical v2.4 protocol-sync payload carried by the live P2P transport.
///
/// This envelope deliberately contains only consensus-relevant negotiation and
/// DAG-sync contracts. It does not activate GHOSTDAG, high cadence, or any
/// release profile by itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "sync_type", content = "payload", rename_all = "snake_case")]
pub enum ProtocolSyncWireV1 {
    CapabilityHandshake(ProtocolCapabilityHandshakeV1),
    SelectedChainLocator(SelectedChainLocatorV1),
    DagFrontier(DagFrontierResponseV1),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolSyncWireError {
    EmptyChainId,
    ChainIdMismatch {
        expected: String,
        observed: String,
    },
    HandshakeProtocolIdentityMismatch {
        chain_id: String,
        identity_chain_id: String,
    },
    ProtocolCapability(ProtocolCompatibilityError),
    DagSync(DagSyncContractError),
}

impl ProtocolSyncWireV1 {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CapabilityHandshake(_) => "CapabilityHandshake",
            Self::SelectedChainLocator(_) => "SelectedChainLocator",
            Self::DagFrontier(_) => "DagFrontier",
        }
    }

    pub fn chain_id(&self) -> &str {
        match self {
            Self::CapabilityHandshake(ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                chain_id,
            })
            | Self::CapabilityHandshake(ProtocolCapabilityHandshakeV1::ProtocolCapabilities {
                chain_id,
                ..
            }) => chain_id,
            Self::SelectedChainLocator(locator) => &locator.protocol_identity.chain_id,
            Self::DagFrontier(frontier) => &frontier.protocol_identity.chain_id,
        }
    }

    /// Validate the payload without trusting peer/runtime state.
    pub fn validate_shape(&self) -> Result<(), ProtocolSyncWireError> {
        match self {
            Self::CapabilityHandshake(ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                chain_id,
            }) => {
                if chain_id.is_empty() {
                    return Err(ProtocolSyncWireError::EmptyChainId);
                }
            }
            Self::CapabilityHandshake(ProtocolCapabilityHandshakeV1::ProtocolCapabilities {
                chain_id,
                capabilities,
            }) => {
                if chain_id.is_empty() {
                    return Err(ProtocolSyncWireError::EmptyChainId);
                }
                capabilities
                    .validate_shape()
                    .map_err(ProtocolSyncWireError::ProtocolCapability)?;
                if capabilities.protocol_identity.chain_id != *chain_id {
                    return Err(ProtocolSyncWireError::HandshakeProtocolIdentityMismatch {
                        chain_id: chain_id.clone(),
                        identity_chain_id: capabilities.protocol_identity.chain_id.clone(),
                    });
                }
            }
            Self::SelectedChainLocator(locator) => locator
                .validate_shape()
                .map_err(ProtocolSyncWireError::DagSync)?,
            Self::DagFrontier(frontier) => frontier
                .validate_shape()
                .map_err(ProtocolSyncWireError::DagSync)?,
        }
        Ok(())
    }

    /// Fail closed before a protocol-sync message is admitted to peer/sync
    /// state for a different runtime chain.
    pub fn validate_for_chain(&self, expected_chain_id: &str) -> Result<(), ProtocolSyncWireError> {
        self.validate_shape()?;
        let observed = self.chain_id();
        if observed != expected_chain_id {
            return Err(ProtocolSyncWireError::ChainIdMismatch {
                expected: expected_chain_id.to_string(),
                observed: observed.to_string(),
            });
        }
        Ok(())
    }
}

/// Side-effect-free dispatch result consumed by the eventual live transport.
///
/// `AdvertiseCapabilitiesViaLegacyCarrier` is deliberately distinct from
/// `SendProtocolV2`: an unknown peer must not receive a new undecodable wire
/// variant merely to discover whether it supports that variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolSyncDispatchActionV1 {
    SendProtocolV2,
    AdvertiseCapabilitiesViaLegacyCarrier,
    HoldForCapabilities,
    UseLegacyFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolSyncDispatchPlanV1 {
    pub peer_id: String,
    pub wire_kind: String,
    pub action: ProtocolSyncDispatchActionV1,
    pub penalize_peer: bool,
}

/// Inbound authorization is deliberately separate from payload validation.
/// A syntactically valid locator/frontier is not authoritative unless the
/// authenticated peer already negotiated exact Task 27 capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolSyncInboundActionV1 {
    AcceptProtocolV2,
    HoldUntilCapabilitiesKnown,
    UseLegacyFallback,
    UseLegacyCapabilityCarrier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolSyncInboundDecisionV1 {
    pub peer_id: String,
    pub wire_kind: String,
    pub action: ProtocolSyncInboundActionV1,
    pub penalize_peer: bool,
}

/// Plan one outbound protocol-sync payload without performing network I/O.
///
/// The payload is shape/chain validated first. Capability negotiation for an
/// unknown peer is required to use a backward-compatible carrier; locator and
/// frontier payloads are held until exact capability compatibility is known.
/// Incompatible peers remain eligible for legacy-safe fallback and are never
/// penalized merely for being a different supported version.
pub fn plan_protocol_sync_dispatch_v1(
    router: &ProtocolPeerRouterV1,
    peer_id: &str,
    wire: &ProtocolSyncWireV1,
    expected_chain_id: &str,
) -> Result<ProtocolSyncDispatchPlanV1, ProtocolSyncWireError> {
    wire.validate_for_chain(expected_chain_id)?;

    let action = if matches!(wire, ProtocolSyncWireV1::CapabilityHandshake(_)) {
        match router.compatibility(peer_id) {
            ProtocolPeerCompatibilityV1::Compatible => ProtocolSyncDispatchActionV1::SendProtocolV2,
            ProtocolPeerCompatibilityV1::Unknown => {
                ProtocolSyncDispatchActionV1::AdvertiseCapabilitiesViaLegacyCarrier
            }
            ProtocolPeerCompatibilityV1::Incompatible(_) => {
                ProtocolSyncDispatchActionV1::UseLegacyFallback
            }
        }
    } else {
        match router
            .route(peer_id, ProtocolMessageClassV1::ProtocolV2Sync)
            .action
        {
            ProtocolPeerRouteActionV1::SendProtocolV2 => {
                ProtocolSyncDispatchActionV1::SendProtocolV2
            }
            ProtocolPeerRouteActionV1::HoldForCapabilities => {
                ProtocolSyncDispatchActionV1::HoldForCapabilities
            }
            ProtocolPeerRouteActionV1::UseLegacyFallback
            | ProtocolPeerRouteActionV1::SendLegacySafe => {
                ProtocolSyncDispatchActionV1::UseLegacyFallback
            }
        }
    };

    Ok(ProtocolSyncDispatchPlanV1 {
        peer_id: peer_id.to_string(),
        wire_kind: wire.kind().to_string(),
        action,
        penalize_peer: false,
    })
}

/// Gate one decoded inbound Task 27 sync payload against both its chain identity
/// and the negotiated state of the authenticated peer.
///
/// Chain/payload validation always runs first. Capability handshakes themselves
/// are not accepted as direct v2 bootstrap traffic: mixed-version bootstrap uses
/// the backward-compatible GetTips/Tips carrier. Unknown and incompatible peers
/// are routed away from authoritative v2 sync without a false reputation penalty.
pub fn gate_protocol_sync_inbound_v1(
    session: &ProtocolPeerSessionV1,
    peer_id: &str,
    wire: &ProtocolSyncWireV1,
    expected_chain_id: &str,
) -> Result<ProtocolSyncInboundDecisionV1, ProtocolSyncWireError> {
    wire.validate_for_chain(expected_chain_id)?;

    let action = match wire {
        ProtocolSyncWireV1::CapabilityHandshake(_) => {
            ProtocolSyncInboundActionV1::UseLegacyCapabilityCarrier
        }
        ProtocolSyncWireV1::SelectedChainLocator(_) | ProtocolSyncWireV1::DagFrontier(_) => {
            match session.compatibility(peer_id) {
                ProtocolPeerCompatibilityV1::Compatible => {
                    ProtocolSyncInboundActionV1::AcceptProtocolV2
                }
                ProtocolPeerCompatibilityV1::Unknown => {
                    ProtocolSyncInboundActionV1::HoldUntilCapabilitiesKnown
                }
                ProtocolPeerCompatibilityV1::Incompatible(_) => {
                    ProtocolSyncInboundActionV1::UseLegacyFallback
                }
            }
        }
    };

    Ok(ProtocolSyncInboundDecisionV1 {
        peer_id: peer_id.to_string(),
        wire_kind: wire.kind().to_string(),
        action,
        penalize_peer: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{
        DagFrontierEntryV1, ProtocolCapabilitiesV1, P2P_DAG_SYNC_CONTRACT_VERSION,
        P2P_PROTOCOL_CAPABILITIES_VERSION,
    };
    use pulsedag_core::{
        BlockConsensusMetadataV1, ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "pulsedag-testnet";

    fn identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn capabilities(chain_id: &str) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: identity(chain_id),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    fn compatible_session() -> ProtocolPeerSessionV1 {
        let local = capabilities(CHAIN_ID);
        let mut session = ProtocolPeerSessionV1::default();
        session
            .configure_local_capabilities(CHAIN_ID, local.clone())
            .unwrap();
        session
            .observe_remote_capabilities("peer-v2", Some(local))
            .unwrap();
        session
    }

    fn incompatible_session() -> ProtocolPeerSessionV1 {
        let local = capabilities(CHAIN_ID);
        let mut remote = local.clone();
        remote.supports_dag_frontier = false;
        let mut session = ProtocolPeerSessionV1::default();
        session
            .configure_local_capabilities(CHAIN_ID, local)
            .unwrap();
        session
            .observe_remote_capabilities("peer-old", Some(remote))
            .unwrap();
        session
    }

    fn locator(chain_id: &str) -> SelectedChainLocatorV1 {
        SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity(chain_id),
            selected_tip: "tip".to_string(),
            locator: vec!["tip".to_string(), "ancestor".to_string()],
        }
    }

    fn frontier(chain_id: &str) -> DagFrontierResponseV1 {
        DagFrontierResponseV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity(chain_id),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            ordering_version: GHOSTDAG_V1_ORDERING_VERSION.to_string(),
            common_ancestor: "ancestor".to_string(),
            selected_tip: "tip".to_string(),
            selected_chain_suffix: vec!["ancestor".to_string(), "tip".to_string()],
            required_context: Vec::new(),
            frontier: vec![DagFrontierEntryV1 {
                hash: "frontier".to_string(),
                parents: vec!["tip".to_string()],
                consensus: BlockConsensusMetadataV1 {
                    selected_parent: Some("tip".to_string()),
                    blue_score: 2,
                    blue_work_decimal: "200".to_string(),
                    merge_set_blues: Vec::new(),
                    merge_set_reds: Vec::new(),
                },
            }],
        }
    }

    #[test]
    fn all_protocol_sync_wire_variants_round_trip_and_validate() {
        let messages = [
            ProtocolSyncWireV1::CapabilityHandshake(
                ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                    chain_id: CHAIN_ID.to_string(),
                },
            ),
            ProtocolSyncWireV1::CapabilityHandshake(
                ProtocolCapabilityHandshakeV1::ProtocolCapabilities {
                    chain_id: CHAIN_ID.to_string(),
                    capabilities: capabilities(CHAIN_ID),
                },
            ),
            ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID)),
            ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID)),
        ];

        for message in messages {
            assert_eq!(message.validate_for_chain(CHAIN_ID), Ok(()));
            let encoded = serde_json::to_vec(&message).expect("protocol sync wire serializes");
            let decoded: ProtocolSyncWireV1 =
                serde_json::from_slice(&encoded).expect("protocol sync wire deserializes");
            assert_eq!(decoded, message);
            assert_eq!(decoded.chain_id(), CHAIN_ID);
        }
    }

    #[test]
    fn runtime_chain_mismatch_fails_closed_for_every_sync_payload() {
        let messages = [
            ProtocolSyncWireV1::CapabilityHandshake(
                ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                    chain_id: CHAIN_ID.to_string(),
                },
            ),
            ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID)),
            ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID)),
        ];

        for message in messages {
            assert!(matches!(
                message.validate_for_chain("different-chain"),
                Err(ProtocolSyncWireError::ChainIdMismatch { .. })
            ));
        }
    }

    #[test]
    fn capability_chain_id_must_match_activated_protocol_identity() {
        let message = ProtocolSyncWireV1::CapabilityHandshake(
            ProtocolCapabilityHandshakeV1::ProtocolCapabilities {
                chain_id: CHAIN_ID.to_string(),
                capabilities: capabilities("different-chain"),
            },
        );

        assert!(matches!(
            message.validate_shape(),
            Err(ProtocolSyncWireError::HandshakeProtocolIdentityMismatch { .. })
        ));
    }

    #[test]
    fn malformed_locator_and_frontier_are_rejected_before_dispatch() {
        let mut malformed_locator = locator(CHAIN_ID);
        malformed_locator.locator.clear();
        assert!(matches!(
            ProtocolSyncWireV1::SelectedChainLocator(malformed_locator).validate_shape(),
            Err(ProtocolSyncWireError::DagSync(
                DagSyncContractError::LocatorEmpty
            ))
        ));

        let mut malformed_frontier = frontier(CHAIN_ID);
        malformed_frontier.ordering_version = "wrong-ordering".to_string();
        assert!(matches!(
            ProtocolSyncWireV1::DagFrontier(malformed_frontier).validate_shape(),
            Err(ProtocolSyncWireError::DagSync(
                DagSyncContractError::OrderingVersionMismatch { .. }
            ))
        ));
    }

    #[test]
    fn unknown_peer_uses_legacy_carrier_for_capability_probe() {
        let router = ProtocolPeerRouterV1::default();
        let wire = ProtocolSyncWireV1::CapabilityHandshake(
            ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                chain_id: CHAIN_ID.to_string(),
            },
        );

        assert_eq!(
            plan_protocol_sync_dispatch_v1(&router, "peer-old", &wire, CHAIN_ID),
            Ok(ProtocolSyncDispatchPlanV1 {
                peer_id: "peer-old".to_string(),
                wire_kind: "CapabilityHandshake".to_string(),
                action: ProtocolSyncDispatchActionV1::AdvertiseCapabilitiesViaLegacyCarrier,
                penalize_peer: false,
            })
        );
    }

    #[test]
    fn unknown_peer_never_receives_locator_or_frontier_v2_bytes() {
        let router = ProtocolPeerRouterV1::default();
        for wire in [
            ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID)),
            ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID)),
        ] {
            let plan = plan_protocol_sync_dispatch_v1(&router, "peer-old", &wire, CHAIN_ID)
                .expect("valid wire plans");
            assert_eq!(
                plan.action,
                ProtocolSyncDispatchActionV1::HoldForCapabilities
            );
            assert!(!plan.penalize_peer);
        }
    }

    #[test]
    fn compatible_peer_can_receive_locator_and_frontier_v2() {
        let local = capabilities(CHAIN_ID);
        let mut router = ProtocolPeerRouterV1::default();
        router.observe_remote_capabilities("peer-v2", &local, local.clone());

        for wire in [
            ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID)),
            ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID)),
        ] {
            let plan = plan_protocol_sync_dispatch_v1(&router, "peer-v2", &wire, CHAIN_ID)
                .expect("valid wire plans");
            assert_eq!(plan.action, ProtocolSyncDispatchActionV1::SendProtocolV2);
            assert!(!plan.penalize_peer);
        }
    }

    #[test]
    fn incompatible_peer_falls_back_without_protocol_penalty() {
        let local = capabilities(CHAIN_ID);
        let mut remote = local.clone();
        remote.supports_dag_frontier = false;
        let mut router = ProtocolPeerRouterV1::default();
        router.observe_remote_capabilities("peer-old", &local, remote);

        let wire = ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID));
        let plan = plan_protocol_sync_dispatch_v1(&router, "peer-old", &wire, CHAIN_ID)
            .expect("valid wire plans");
        assert_eq!(plan.action, ProtocolSyncDispatchActionV1::UseLegacyFallback);
        assert!(!plan.penalize_peer);
    }

    #[test]
    fn dispatch_rejects_wrong_chain_before_peer_routing() {
        let router = ProtocolPeerRouterV1::default();
        let wire = ProtocolSyncWireV1::SelectedChainLocator(locator("different-chain"));

        assert!(matches!(
            plan_protocol_sync_dispatch_v1(&router, "peer", &wire, CHAIN_ID),
            Err(ProtocolSyncWireError::ChainIdMismatch { .. })
        ));
    }

    #[test]
    fn inbound_locator_and_frontier_require_exact_compatible_peer() {
        let session = compatible_session();
        for wire in [
            ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID)),
            ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID)),
        ] {
            assert_eq!(
                gate_protocol_sync_inbound_v1(&session, "peer-v2", &wire, CHAIN_ID),
                Ok(ProtocolSyncInboundDecisionV1 {
                    peer_id: "peer-v2".to_string(),
                    wire_kind: wire.kind().to_string(),
                    action: ProtocolSyncInboundActionV1::AcceptProtocolV2,
                    penalize_peer: false,
                })
            );
        }
    }

    #[test]
    fn inbound_unknown_peer_is_held_without_false_penalty() {
        let session = ProtocolPeerSessionV1::default();
        let wire = ProtocolSyncWireV1::SelectedChainLocator(locator(CHAIN_ID));
        assert_eq!(
            gate_protocol_sync_inbound_v1(&session, "peer-unknown", &wire, CHAIN_ID),
            Ok(ProtocolSyncInboundDecisionV1 {
                peer_id: "peer-unknown".to_string(),
                wire_kind: "SelectedChainLocator".to_string(),
                action: ProtocolSyncInboundActionV1::HoldUntilCapabilitiesKnown,
                penalize_peer: false,
            })
        );
    }

    #[test]
    fn inbound_incompatible_peer_falls_back_without_false_penalty() {
        let session = incompatible_session();
        let wire = ProtocolSyncWireV1::DagFrontier(frontier(CHAIN_ID));
        assert_eq!(
            gate_protocol_sync_inbound_v1(&session, "peer-old", &wire, CHAIN_ID),
            Ok(ProtocolSyncInboundDecisionV1 {
                peer_id: "peer-old".to_string(),
                wire_kind: "DagFrontier".to_string(),
                action: ProtocolSyncInboundActionV1::UseLegacyFallback,
                penalize_peer: false,
            })
        );
    }

    #[test]
    fn inbound_direct_capability_handshake_uses_legacy_carrier_boundary() {
        let session = ProtocolPeerSessionV1::default();
        let wire = ProtocolSyncWireV1::CapabilityHandshake(
            ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                chain_id: CHAIN_ID.to_string(),
            },
        );
        assert_eq!(
            gate_protocol_sync_inbound_v1(&session, "peer", &wire, CHAIN_ID),
            Ok(ProtocolSyncInboundDecisionV1 {
                peer_id: "peer".to_string(),
                wire_kind: "CapabilityHandshake".to_string(),
                action: ProtocolSyncInboundActionV1::UseLegacyCapabilityCarrier,
                penalize_peer: false,
            })
        );
    }

    #[test]
    fn inbound_wrong_chain_fails_before_peer_authorization() {
        let session = compatible_session();
        let wire = ProtocolSyncWireV1::SelectedChainLocator(locator("different-chain"));
        assert!(matches!(
            gate_protocol_sync_inbound_v1(&session, "peer-v2", &wire, CHAIN_ID),
            Err(ProtocolSyncWireError::ChainIdMismatch { .. })
        ));
    }
}
