use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ordering::DAG_ORDERING_VERSION,
    state::{ChainState, ConsensusMode},
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
};

pub const BLOCK_HEADER_VERSION_V1: u32 = 1;
pub const BLOCK_HEADER_VERSION_V2: u32 = 2;
pub const PROTOCOL_ACTIVATION_IDENTITY_FINGERPRINT_DOMAIN: &[u8] =
    b"PulseDAG:protocol-activation-identity:v1";

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

fn encode_len_prefixed(out: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("protocol identity field exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value);
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

    /// Canonical byte representation used only to compare/persist protocol
    /// activation identity. This is separate from transaction/header consensus
    /// serialization and does not activate any protocol mode.
    pub fn canonical_fingerprint_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;

        let mut out = Vec::with_capacity(192);
        encode_len_prefixed(&mut out, PROTOCOL_ACTIVATION_IDENTITY_FINGERPRINT_DOMAIN);
        encode_len_prefixed(&mut out, self.chain_id.as_bytes());
        encode_len_prefixed(&mut out, self.genesis_hash.as_bytes());
        out.extend_from_slice(&self.transaction_protocol_version.to_le_bytes());
        out.extend_from_slice(&self.block_header_protocol_version.to_le_bytes());
        encode_len_prefixed(&mut out, self.consensus_mode.as_str().as_bytes());
        encode_len_prefixed(&mut out, self.dag_ordering_version.as_bytes());
        Ok(out)
    }

    /// Stable SHA-256 compatibility fingerprint for storage/snapshot/P2P gates.
    pub fn fingerprint(&self) -> Result<String, String> {
        Ok(hex::encode(Sha256::digest(
            self.canonical_fingerprint_bytes()?,
        )))
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
        assert_eq!(
            identity.transaction_protocol_version,
            TRANSACTION_VERSION_V1
        );
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

        assert_eq!(
            identity.transaction_protocol_version,
            TRANSACTION_VERSION_V2
        );
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
        assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    }

    #[test]
    fn activated_v2_fingerprint_has_a_frozen_golden_vector() {
        let identity = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            "ghostdag-order-v1",
        );

        let bytes = identity.canonical_fingerprint_bytes().unwrap();
        assert_eq!(bytes.len(), 125);
        assert_eq!(
            identity.fingerprint().unwrap(),
            "793cc7ba13c579514ef08f33a79160906e9a2cedd610ea958344d3d8e2c26209"
        );
    }

    #[test]
    fn every_protocol_identity_component_changes_the_fingerprint() {
        let base = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            "ghostdag-order-v1",
        );
        let base_fingerprint = base.fingerprint().unwrap();

        let mut variants = Vec::new();

        let mut value = base.clone();
        value.chain_id.push_str("-other");
        variants.push(value);

        let mut value = base.clone();
        value.genesis_hash.push_str("-other");
        variants.push(value);

        let mut value = base.clone();
        value.transaction_protocol_version += 1;
        variants.push(value);

        let mut value = base.clone();
        value.block_header_protocol_version += 1;
        variants.push(value);

        let mut value = base.clone();
        value.consensus_mode = ProtocolConsensusMode::Legacy;
        variants.push(value);

        let mut value = base.clone();
        value.dag_ordering_version.push_str("-other");
        variants.push(value);

        for variant in variants {
            assert_ne!(variant.fingerprint().unwrap(), base_fingerprint);
        }
    }

    #[test]
    fn invalid_identity_cannot_be_fingerprinted() {
        let mut identity = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            "ghostdag-order-v1",
        );
        identity.chain_id.clear();

        assert!(identity.canonical_fingerprint_bytes().is_err());
        assert!(identity.fingerprint().is_err());
    }
}
