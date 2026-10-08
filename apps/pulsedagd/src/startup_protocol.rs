use anyhow::{bail, Result};
use pulsedag_core::{
    contracts_compile_identity, contracts_compile_time_executable,
    finality_v2::GHOSTDAG_V1_FINALITY_POLICY_VERSION, genesis::init_chain_state,
    genesis_v2::init_chain_state_v2, genesis_v3::init_chain_state_v3, ConsensusMode,
    ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_p2p::messages::{ProtocolCapabilitiesV1, P2P_PROTOCOL_CAPABILITIES_VERSION};

pub const STARTUP_PROTOCOL_MODE_ENV: &str = "PULSEDAG_PROTOCOL_CONSENSUS_MODE";
pub const PRODUCTION_V3_GENESIS_TIMESTAMP_ENV: &str = "PULSEDAG_V3_GENESIS_TIMESTAMP";
pub const CONTRACTS_ENABLED_ENV: &str = "PULSEDAG_CONTRACTS_ENABLED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupProtocolMode {
    Legacy,
    GhostdagV1,
    MonetaryV3,
}

impl StartupProtocolMode {
    fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" => Ok(Self::Legacy),
            "ghostdag_v1" => Ok(Self::GhostdagV1),
            "monetary_v3" => Ok(Self::MonetaryV3),
            other => bail!(
                "invalid {STARTUP_PROTOCOL_MODE_ENV} value '{other}'. Supported values: legacy, ghostdag_v1, monetary_v3"
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

fn parse_production_v3_genesis_timestamp(raw: &str) -> Result<u64> {
    let timestamp = raw.trim().parse::<u64>().map_err(|error| {
        anyhow::anyhow!(
            "invalid {PRODUCTION_V3_GENESIS_TIMESTAMP_ENV} value '{}': {error}",
            raw.trim()
        )
    })?;
    if timestamp == 0 {
        bail!("{PRODUCTION_V3_GENESIS_TIMESTAMP_ENV} must be greater than zero");
    }
    Ok(timestamp)
}

fn production_v3_genesis_timestamp_from_env() -> Result<u64> {
    let raw = std::env::var(PRODUCTION_V3_GENESIS_TIMESTAMP_ENV).map_err(|_| {
        anyhow::anyhow!(
            "{STARTUP_PROTOCOL_MODE_ENV}=monetary_v3 requires {PRODUCTION_V3_GENESIS_TIMESTAMP_ENV}"
        )
    })?;
    parse_production_v3_genesis_timestamp(&raw)
}

fn env_flag_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Task31 fail-closed: runtime enablement cannot bypass the compile gate (#1131).
pub fn enforce_inactive_contracts_for_task31() -> Result<()> {
    let env_enabled = env_flag_truthy(CONTRACTS_ENABLED_ENV);
    if env_enabled && !contracts_compile_time_executable() {
        bail!(
            "{CONTRACTS_ENABLED_ENV}=true is rejected: this binary was built without the executable-contracts Cargo feature ({})",
            contracts_compile_identity()
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct StartupProtocolSelection {
    pub mode: StartupProtocolMode,
    pub restore_identity: Option<ProtocolActivationIdentity>,
    pub local_capabilities: Option<ProtocolCapabilitiesV1>,
    pub production_v3_genesis_timestamp: Option<u64>,
}

impl StartupProtocolSelection {
    pub fn activated_v2(&self) -> bool {
        matches!(
            self.mode,
            StartupProtocolMode::GhostdagV1 | StartupProtocolMode::MonetaryV3
        )
    }

    pub fn production_v3(&self) -> bool {
        self.mode == StartupProtocolMode::MonetaryV3
    }

    /// Generic v2 FastSync imports validate only the v2 authoritative state
    /// contract. Production monetary-v3 must stay off that path until a
    /// monetary-v3-specific verifier/importer exists.
    pub fn generic_v2_fast_sync_allowed(&self) -> bool {
        self.mode == StartupProtocolMode::GhostdagV1
    }
}

pub fn select_startup_protocol(
    chain_id: &str,
    runtime_consensus_mode: ConsensusMode,
) -> Result<StartupProtocolSelection> {
    enforce_inactive_contracts_for_task31()?;
    let mode = StartupProtocolMode::from_env()?;
    let production_v3_genesis_timestamp = if mode == StartupProtocolMode::MonetaryV3 {
        if env_flag_truthy(CONTRACTS_ENABLED_ENV) {
            bail!(
                "{STARTUP_PROTOCOL_MODE_ENV}=monetary_v3 requires {CONTRACTS_ENABLED_ENV}=false"
            );
        }
        Some(production_v3_genesis_timestamp_from_env()?)
    } else {
        None
    };
    select_startup_protocol_for_mode(
        chain_id,
        runtime_consensus_mode,
        mode,
        production_v3_genesis_timestamp,
    )
}

fn select_startup_protocol_for_mode(
    chain_id: &str,
    runtime_consensus_mode: ConsensusMode,
    mode: StartupProtocolMode,
    production_v3_genesis_timestamp: Option<u64>,
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
                production_v3_genesis_timestamp: None,
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
            capabilities.validate_shape().map_err(|error| {
                anyhow::anyhow!("invalid activated-v2 startup capabilities: {error:?}")
            })?;
            Ok(StartupProtocolSelection {
                mode,
                restore_identity: Some(identity),
                local_capabilities: Some(capabilities),
                production_v3_genesis_timestamp: None,
            })
        }
        StartupProtocolMode::MonetaryV3 => {
            if runtime_consensus_mode != ConsensusMode::Legacy {
                bail!(
                    "{STARTUP_PROTOCOL_MODE_ENV}=monetary_v3 requires PULSEDAG_CONSENSUS_MODE=legacy"
                );
            }
            let frozen_timestamp = production_v3_genesis_timestamp.ok_or_else(|| {
                anyhow::anyhow!(
                    "{STARTUP_PROTOCOL_MODE_ENV}=monetary_v3 requires a frozen v3 genesis timestamp"
                )
            })?;
            let state = init_chain_state_v3(chain_id.to_string(), frozen_timestamp)?;
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
                high_cadence_allowed: true,
            };
            capabilities.validate_shape().map_err(|error| {
                anyhow::anyhow!("invalid production-v3 startup capabilities: {error:?}")
            })?;
            Ok(StartupProtocolSelection {
                mode,
                restore_identity: Some(identity),
                local_capabilities: Some(capabilities),
                production_v3_genesis_timestamp: Some(frozen_timestamp),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2, TRANSACTION_VERSION_V2};

    #[test]
    fn legacy_selection_preserves_current_restore_behavior() {
        let selected = select_startup_protocol_for_mode(
            "pulsedag-testnet",
            ConsensusMode::Legacy,
            StartupProtocolMode::Legacy,
            None,
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
            None,
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
            None,
        )
        .unwrap();
        assert!(selected.activated_v2());
        assert!(selected.generic_v2_fast_sync_allowed());
        let identity = selected.restore_identity.as_ref().unwrap();
        let capabilities = selected.local_capabilities.as_ref().unwrap();

        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::GhostdagV1);
        assert_eq!(
            identity.transaction_protocol_version,
            TRANSACTION_VERSION_V2
        );
        assert_eq!(
            identity.block_header_protocol_version,
            BLOCK_HEADER_VERSION_V2
        );
        assert_eq!(capabilities.protocol_identity, *identity);
        assert!(capabilities.supports_dag_frontier);
        assert!(capabilities.supports_consensus_metadata);
        assert!(!capabilities.high_cadence_allowed);
    }

    #[test]
    fn monetary_v3_selection_is_chain_timestamp_bound_and_high_cadence_enabled() {
        let timestamp = 1_800_000_456;
        let selected = select_startup_protocol_for_mode(
            "pulsedag-v3-production-candidate",
            ConsensusMode::Legacy,
            StartupProtocolMode::MonetaryV3,
            Some(timestamp),
        )
        .unwrap();
        assert!(selected.activated_v2());
        assert!(selected.production_v3());
        assert!(!selected.generic_v2_fast_sync_allowed());
        assert_eq!(selected.production_v3_genesis_timestamp, Some(timestamp));

        let identity = selected.restore_identity.as_ref().unwrap();
        let capabilities = selected.local_capabilities.as_ref().unwrap();
        let state =
            init_chain_state_v3("pulsedag-v3-production-candidate".to_string(), timestamp).unwrap();

        assert_eq!(identity.chain_id, state.chain_id);
        assert_eq!(identity.genesis_hash, state.dag.genesis_hash);
        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::GhostdagV1);
        assert_eq!(capabilities.protocol_identity, *identity);
        assert!(capabilities.high_cadence_allowed);
    }

    #[test]
    fn monetary_v3_requires_timestamp_and_legacy_runtime() {
        assert!(select_startup_protocol_for_mode(
            "pulsedag-v3-production-candidate",
            ConsensusMode::Legacy,
            StartupProtocolMode::MonetaryV3,
            None,
        )
        .is_err());
        assert!(select_startup_protocol_for_mode(
            "pulsedag-v3-production-candidate",
            ConsensusMode::GhostdagDev,
            StartupProtocolMode::MonetaryV3,
            Some(1_800_000_456),
        )
        .is_err());
        assert!(parse_production_v3_genesis_timestamp("0").is_err());
        assert_eq!(
            parse_production_v3_genesis_timestamp("1800000456").unwrap(),
            1_800_000_456
        );
    }

    #[test]
    fn ghostdag_v1_rejects_ghostdag_dev_runtime() {
        assert!(select_startup_protocol_for_mode(
            "pulsedag-private-v2.4.0",
            ConsensusMode::GhostdagDev,
            StartupProtocolMode::GhostdagV1,
            None,
        )
        .is_err());
    }

    #[test]
    fn startup_mode_parser_is_fail_closed() {
        assert_eq!(
            StartupProtocolMode::parse("legacy").unwrap(),
            StartupProtocolMode::Legacy
        );
        assert_eq!(
            StartupProtocolMode::parse("ghostdag_v1").unwrap(),
            StartupProtocolMode::GhostdagV1
        );
        assert_eq!(
            StartupProtocolMode::parse("monetary_v3").unwrap(),
            StartupProtocolMode::MonetaryV3
        );
        assert!(StartupProtocolMode::parse("ghostdag-v1").is_err());
        assert!(StartupProtocolMode::parse("").is_err());
    }

    #[test]
    fn contracts_env_cannot_bypass_compile_gate() {
        std::env::set_var(CONTRACTS_ENABLED_ENV, "true");
        let result = enforce_inactive_contracts_for_task31();
        if contracts_compile_time_executable() {
            result.expect("feature-on builds may accept the env flag");
        } else {
            let err = result.expect_err("feature-off builds must reject the env flag");
            assert!(err.to_string().contains("executable-contracts"));
        }
        std::env::remove_var(CONTRACTS_ENABLED_ENV);
        enforce_inactive_contracts_for_task31().unwrap();
    }
}
