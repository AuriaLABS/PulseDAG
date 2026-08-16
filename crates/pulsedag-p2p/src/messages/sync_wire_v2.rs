use serde::{Deserialize, Serialize};

use super::{
    DagFrontierResponseV1, DagSyncContractError, ProtocolCapabilityHandshakeV1,
    ProtocolCompatibilityError, SelectedChainLocatorV1,
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
}
