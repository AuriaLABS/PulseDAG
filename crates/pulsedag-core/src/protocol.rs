use serde::{Deserialize, Serialize};

use crate::{
    ordering::DAG_ORDERING_VERSION,
    state::{ChainState, ConsensusMode},
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
};

pub const BLOCK_HEADER_VERSION_V1: u32 = 1;
pub const BLOCK_HEADER_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolConsensusMode {
    Legacy,
    GhostdagDev,
    GhostdagV1,
}

impl ProtocolConsensusMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::GhostdagDev => "ghostdag_dev",
            Self::GhostdagV1 => "ghostdag_v1",
        }
    }

    pub fn is_release_activated(self) -> bool {
        matches!(self, Self::GhostdagV1)
    }
}

impl From<ConsensusMode> for ProtocolConsensusMode {
    fn from(value: ConsensusMode) -> Self {
        match value {
            ConsensusMode::Legacy => Self::Legacy,
            ConsensusMode::GhostdagDev => Self::GhostdagDev,
        }
    }
}

impl std::fmt::Display for ProtocolConsensusMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Consensus-relevant protocol identity shared by storage, snapshots, P2P,
/// mining, wallet, replay, and release evidence surfaces.
///
/// The structure itself does not activate consensus. `ghostdag_v1` remains a
/// reserved release identity until the downstream v2.4.0 activation gates are
/// implemented and explicitly selected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolActivationIdentity {
    pub chain_id: String,
    pub genesis_hash: String,
    pub transaction_protocol_version: u32,
    pub block_header_protocol_version: u32,
    pub consensus_mode: ProtocolConsensusMode,
    pub dag_ordering_version: String,
}

impl ProtocolActivationIdentity {
    /// Identity for current historical/runtime v1 semantics. This preserves
    /// the configured runtime consensus mode but always records v1 transaction
    /// and block-header protocol versions.
    pub fn legacy_from_state(state: &ChainState) -> Self {
        Self {
            chain_id: state.chain_id.clone(),
            genesis_hash: state.dag.genesis_hash.clone(),
            transaction_protocol_version: TRANSACTION_VERSION_V1,
            block_header_protocol_version: BLOCK_HEADER_VERSION_V1,
            consensus_mode: state.dag.consensus_mode.into(),
            dag_ordering_version: state.dag.ordering_version.clone(),
        }
    }

    /// Reserved identity constructor for the final clean-chain v2.4.0
    /// `ghostdag_v1` protocol. Constructing this value does not make the mode
    /// selectable or release-ready.
    pub fn activated_v2(
        chain_id: impl Into<String>,
        genesis_hash: impl Into<String>,
        dag_ordering_version: impl Into<String>,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            genesis_hash: genesis_hash.into(),
            transaction_protocol_version: TRANSACTION_VERSION_V2,
            block_header_protocol_version: BLOCK_HEADER_VERSION_V2,
            consensus_mode: ProtocolConsensusMode::GhostdagV1,
            dag_ordering_version: dag_ordering_version.into(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.chain_id.is_empty() {
            return Err("protocol identity chain_id must not be empty".into());
        }
        if self.genesis_hash.is_empty() {
            return Err("protocol identity genesis_hash must not be empty".into());
        }
        if self.transaction_protocol_version == 0 {
            return Err("transaction protocol version must be non-zero".into());
        }
        if self.block_header_protocol_version == 0 {
            return Err("block-header protocol version must be non-zero".into());
        }
        if self.dag_ordering_version.is_empty() {
            return Err("DAG ordering version must not be empty".into());
        }
        Ok(())
    }

    pub fn legacy_default_for_chain(
        chain_id: impl Into<String>,
        genesis_hash: impl Into<String>,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            genesis_hash: genesis_hash.into(),
            transaction_protocol_version: TRANSACTION_VERSION_V1,
            block_header_protocol_version: BLOCK_HEADER_VERSION_V1,
            consensus_mode: ProtocolConsensusMode::Legacy,
            dag_ordering_version: DAG_ORDERING_VERSION.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::init_chain_state;

    #[test]
    fn legacy_identity_preserves_current_v1_runtime_semantics() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(identity.chain_id, state.chain_id);
        assert_eq!(identity.genesis_hash, state.dag.genesis_hash);
        assert_eq!(identity.transaction_protocol_version, TRANSACTION_VERSION_V1);
        assert_eq!(
            identity.block_header_protocol_version,
            BLOCK_HEADER_VERSION_V1
        );
        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::Legacy);
        assert_eq!(identity.dag_ordering_version, state.dag.ordering_version);
        assert!(identity.validate().is_ok());
    }

    #[test]
    fn activated_v2_identity_is_explicit_and_release_distinct() {
        let identity = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            "ghostdag-order-v1",
        );

        assert_eq!(identity.transaction_protocol_version, TRANSACTION_VERSION_V2);
        assert_eq!(
            identity.block_header_protocol_version,
            BLOCK_HEADER_VERSION_V2
        );
        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::GhostdagV1);
        assert!(identity.consensus_mode.is_release_activated());
        assert!(identity.validate().is_ok());
    }

    #[test]
    fn chain_identity_changes_are_detectable() {
        let a = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-a",
            "genesis",
            "ghostdag-order-v1",
        );
        let b = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-b",
            "genesis",
            "ghostdag-order-v1",
        );

        assert_ne!(a, b);
    }
}
