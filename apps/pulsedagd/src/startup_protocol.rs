use anyhow::{bail, Result};
use pulsedag_core::{
    finality_v2::GHOSTDAG_V1_FINALITY_POLICY_VERSION,
    genesis::init_chain_state,
    genesis_v2::init_chain_state_v2,
    ConsensusMode, ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
    GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_p2p::messages::{
    ProtocolCapabilitiesV1, P2P_PROTOCOL_CAPABILITIES_VERSION,
};

pub const STARTUP_PROTOCOL_MODE_ENV: &str = "PULSEDAG_PROTOCOL_CONSENSUS_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupProtocolMode {
    Legacy,
    GhostdagV1,
}

impl StartupProtocolMode {
    fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" => Ok(Self::Legacy),
            "ghostdag_v1" => Ok(Self::GhostdagV1),
            other => bail!(
                "invalid {STARTUP_PROTOCOL_MODE_ENV} value '{other}'. Supported values: legacy, ghostdag_v1"
            ),
        }
    }

    pub fn from_env() -> Result<Self> {
        std::env::var(STARTUP_PROTOCOL_MODE_ENV)
            .ok()
            .map(|raw| Self::parse(&raw))
            .transpose()
            .map(|mode| mode.unwrap_or(Self::Legacy))
    }
}

#[derive(Debug, Clone)]
pub struct StartupProtocolSelection {
    pub mode: StartupProtocolMode,
    pub restore_identity: Option<ProtocolActivationIdentity>,
    pub local_capabilities: Option<ProtocolCapabilitiesV1>,
}

impl StartupProtocolSelection {
    pub fn activated_v2(&self) -> bool {
        self.mode == StartupProtocolMode::GhostdagV1
    }
}

pub fn select_startup_protocol(
    chain_id: &str,
    runtime_consensus_mode: ConsensusMode,
) -> Result<StartupProtocolSelection> {
    select_startup_protocol_for_mode(
        chain_id,
        runtime_consensus_mode,
        StartupProtocolMode::from_env()?,
    )
}

fn select_startup_protocol_for_mode(
    chain_id: &str,
    runtime_consensus_mode: ConsensusMode,
    mode: StartupProtocolMode,
) -> Result<StartupProtocolSelection> {
    match mode {
        StartupProtocolMode::Legacy => {
            let restore_identity = if runtime_consensus_mode == ConsensusMode::Legacy {
                let state = init_chain_state(chain_id.to_string());
                Some(ProtocolActivationIdentity::legacy_from_state(&state))
            } else {
                None
            };
            Ok(StartupProtocolSelection {
                mode,
                restore_identity,
                local_capabilities: None,
            })
        }
        StartupProtocolMode::GhostdagV1 => {
            if runtime_consensus_mode != ConsensusMode::Legacy {
                bail!(
                    "{STARTUP_PROTOCOL_MODE_ENV}=ghostdag_v1 requires PULSEDAG_CONSENSUS_MODE=legacy; ghostdag_dev is a separate historical/dev runtime"
                );
            }
            let state = init_chain_state_v2(chain_id.to_string())?;
            let identity = ProtocolActivationIdentity::activated_v2(
                state.chain_id.clone(),
                state.dag.genesis_hash.clone(),
                GHOSTDAG_V1_ORDERING_VERSION,
            );
            let capabilities = ProtocolCapabilitiesV1 {
                capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
                protocol_identity: identity.clone(),
                consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
                finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
                supports_dag_frontier: true,
                supports_consensus_metadata: true,
                high_cadence_allowed: false,
            };
            capabilities
                .validate_shape()
                .map_err(|error| anyhow::anyhow!("invalid activated-v2 startup capabilities: {error:?}"))?;
            Ok(StartupProtocolSelection {
                mode,
                restore_identity: Some(identity),
                local_capabilities: Some(capabilities),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2, TRANSACTION_VERSION_V2,
    };

    #[test]
    fn legacy_selection_preserves_current_restore_behavior() {
        let selected = select_startup_protocol_for_mode(
            "pulsedag-testnet",
            ConsensusMode::Legacy,
            StartupProtocolMode::Legacy,
        )
        .unwrap();
        let identity = selected.restore_identity.unwrap();
        assert_eq!(identity.chain_id, "pulsedag-testnet");
        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::Legacy);
        assert!(selected.local_capabilities.is_none());

        let ghostdag_dev = select_startup_protocol_for_mode(
            "pulsedag-testnet",
            ConsensusMode::GhostdagDev,
            StartupProtocolMode::Legacy,
        )
        .unwrap();
        assert!(ghostdag_dev.restore_identity.is_none());
        assert!(ghostdag_dev.local_capabilities.is_none());
    }

    #[test]
    fn ghostdag_v1_selection_is_chain_bound_and_high_cadence_off() {
        let selected = select_startup_protocol_for_mode(
            "pulsedag-private-v2.4.0",
            ConsensusMode::Legacy,
            StartupProtocolMode::GhostdagV1,
        )
        .unwrap();
        assert!(selected.activated_v2());
        let identity = selected.restore_identity.as_ref().unwrap();
        let capabilities = selected.local_capabilities.as_ref().unwrap();

        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::GhostdagV1);
        assert_eq!(identity.transaction_protocol_version, TRANSACTION_VERSION_V2);
        assert_eq!(identity.block_header_protocol_version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(capabilities.protocol_identity, *identity);
        assert!(capabilities.supports_dag_frontier);
        assert!(capabilities.supports_consensus_metadata);
        assert!(!capabilities.high_cadence_allowed);
    }

    #[test]
    fn ghostdag_v1_rejects_ghostdag_dev_runtime() {
        assert!(select_startup_protocol_for_mode(
            "pulsedag-private-v2.4.0",
            ConsensusMode::GhostdagDev,
            StartupProtocolMode::GhostdagV1,
        )
        .is_err());
    }

    #[test]
    fn startup_mode_parser_is_fail_closed() {
        assert_eq!(StartupProtocolMode::parse("legacy").unwrap(), StartupProtocolMode::Legacy);
        assert_eq!(
            StartupProtocolMode::parse("ghostdag_v1").unwrap(),
            StartupProtocolMode::GhostdagV1
        );
        assert!(StartupProtocolMode::parse("ghostdag-v1").is_err());
        assert!(StartupProtocolMode::parse("").is_err());
    }
}
