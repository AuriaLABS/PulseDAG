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
}
