use pulsedag_core::{
    ActivatedV2P2pDriveResult, ActivatedV2P2pRuntimeOutcome, BlockAcceptanceResult, ChainState,
    Hash, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_p2p::messages::ProtocolCapabilitiesV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundP2pBlockProtocol {
    Legacy,
    ActivatedV2(ProtocolActivationIdentity),
}

/// Select the inbound P2P block validation path from the locally configured
/// protocol capability identity.
///
/// No local capabilities means the exact historical v1 path. Once capabilities
/// are configured, the selector is intentionally strict: malformed capabilities
/// or any identity other than the canonical activated-v2 identity fail closed
/// instead of silently falling back to legacy validation.
pub fn resolve_inbound_p2p_block_protocol(
    capabilities: Option<&ProtocolCapabilitiesV1>,
    state: &ChainState,
) -> Result<InboundP2pBlockProtocol, String> {
    let Some(capabilities) = capabilities else {
        return Ok(InboundP2pBlockProtocol::Legacy);
    };

    capabilities
        .validate_shape()
        .map_err(|error| format!("invalid local protocol capabilities: {error:?}"))?;

    let expected = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    if capabilities.protocol_identity != expected {
        return Err(format!(
            "local protocol capability identity mismatch: expected={} actual={}",
            expected
                .fingerprint()
                .unwrap_or_else(|_| "invalid-expected-identity".to_string()),
            capabilities
                .protocol_identity
                .fingerprint()
                .unwrap_or_else(|_| "invalid-configured-identity".to_string())
        ));
    }

    Ok(InboundP2pBlockProtocol::ActivatedV2(expected))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivatedV2InboundSummary {
    pub accepted_hashes: Vec<Hash>,
    pub staged_hashes: Vec<Hash>,
    pub duplicate_hashes: Vec<Hash>,
    pub missing_parents: Vec<Hash>,
    pub rejected: Vec<(Hash, BlockAcceptanceResult)>,
}

impl ActivatedV2InboundSummary {
    pub fn authoritative_progress(&self) -> bool {
        !self.accepted_hashes.is_empty()
    }
}

fn record_runtime_outcome(
    outcome: &ActivatedV2P2pRuntimeOutcome,
    summary: &mut ActivatedV2InboundSummary,
) {
    match outcome {
        ActivatedV2P2pRuntimeOutcome::Accepted { block_hash, .. } => {
            summary.accepted_hashes.push(block_hash.clone());
        }
        ActivatedV2P2pRuntimeOutcome::Staged { block_hash, .. } => {
            summary.staged_hashes.push(block_hash.clone());
        }
        ActivatedV2P2pRuntimeOutcome::Promoted {
            promoted_hashes, ..
        } => {
            summary
                .accepted_hashes
                .extend(promoted_hashes.iter().cloned());
        }
        ActivatedV2P2pRuntimeOutcome::MissingParents {
            missing_parents, ..
        } => {
            summary
                .missing_parents
                .extend(missing_parents.iter().cloned());
        }
        ActivatedV2P2pRuntimeOutcome::Duplicate { block_hash } => {
            summary.duplicate_hashes.push(block_hash.clone());
        }
        ActivatedV2P2pRuntimeOutcome::Rejected { block_hash, result } => {
            summary.rejected.push((block_hash.clone(), result.clone()));
        }
    }
}

pub fn summarize_activated_v2_drive(
    drive: &ActivatedV2P2pDriveResult,
) -> ActivatedV2InboundSummary {
    let mut summary = ActivatedV2InboundSummary::default();
    record_runtime_outcome(&drive.primary, &mut summary);
    for outcome in &drive.retried {
        record_runtime_outcome(outcome, &mut summary);
    }
    for hashes in [
        &mut summary.accepted_hashes,
        &mut summary.staged_hashes,
        &mut summary.duplicate_hashes,
        &mut summary.missing_parents,
    ] {
        hashes.sort();
        hashes.dedup();
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        finality_v2::GHOSTDAG_V1_FINALITY_POLICY_VERSION, CONSENSUS_METADATA_SCHEMA_VERSION,
    };
    use pulsedag_p2p::messages::P2P_PROTOCOL_CAPABILITIES_VERSION;

    fn state() -> ChainState {
        pulsedag_core::genesis::init_chain_state("task28-daemon-selector".to_string())
    }

    fn activated_capabilities(state: &ChainState) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: ProtocolActivationIdentity::activated_v2(
                state.chain_id.clone(),
                state.dag.genesis_hash.clone(),
                GHOSTDAG_V1_ORDERING_VERSION,
            ),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    #[test]
    fn absent_capabilities_preserve_exact_legacy_path() {
        let state = state();
        assert_eq!(
            resolve_inbound_p2p_block_protocol(None, &state),
            Ok(InboundP2pBlockProtocol::Legacy)
        );
    }

    #[test]
    fn exact_activated_identity_selects_v2_path() {
        let state = state();
        let capabilities = activated_capabilities(&state);
        assert_eq!(
            resolve_inbound_p2p_block_protocol(Some(&capabilities), &state),
            Ok(InboundP2pBlockProtocol::ActivatedV2(
                capabilities.protocol_identity
            ))
        );
    }

    #[test]
    fn mismatched_identity_fails_closed() {
        let state = state();
        let mut capabilities = activated_capabilities(&state);
        capabilities.protocol_identity.chain_id = "other-chain".to_string();

        let error = resolve_inbound_p2p_block_protocol(Some(&capabilities), &state)
            .expect_err("mismatched identity must not fall back to legacy");
        assert!(error.contains("identity mismatch"));
    }

    #[test]
    fn malformed_capabilities_fail_closed() {
        let state = state();
        let mut capabilities = activated_capabilities(&state);
        capabilities.capabilities_version = P2P_PROTOCOL_CAPABILITIES_VERSION + 1;

        let error = resolve_inbound_p2p_block_protocol(Some(&capabilities), &state)
            .expect_err("malformed capabilities must fail closed");
        assert!(error.contains("invalid local protocol capabilities"));
    }

    #[test]
    fn drive_summary_separates_authoritative_staged_missing_and_rejected_outcomes() {
        let drive = ActivatedV2P2pDriveResult {
            primary: ActivatedV2P2pRuntimeOutcome::MissingParents {
                block_hash: "child".to_string(),
                missing_parents: vec!["p2".to_string(), "p1".to_string()],
                pending_count: 1,
            },
            retried: vec![
                ActivatedV2P2pRuntimeOutcome::Accepted {
                    block_hash: "accepted".to_string(),
                    generation: 2,
                },
                ActivatedV2P2pRuntimeOutcome::Staged {
                    block_hash: "staged".to_string(),
                    staged_count: 1,
                },
                ActivatedV2P2pRuntimeOutcome::Promoted {
                    anchor_hash: "anchor".to_string(),
                    promoted_hashes: vec!["side".to_string(), "anchor".to_string()],
                    generation: 3,
                },
                ActivatedV2P2pRuntimeOutcome::Duplicate {
                    block_hash: "duplicate".to_string(),
                },
                ActivatedV2P2pRuntimeOutcome::Rejected {
                    block_hash: "bad".to_string(),
                    result: BlockAcceptanceResult::Rejected("bad-v2".to_string()),
                },
            ],
            pending_count: 1,
            staged_count: 1,
        };

        let summary = summarize_activated_v2_drive(&drive);
        assert_eq!(
            summary.accepted_hashes,
            vec![
                "accepted".to_string(),
                "anchor".to_string(),
                "side".to_string()
            ]
        );
        assert_eq!(summary.staged_hashes, vec!["staged".to_string()]);
        assert_eq!(summary.duplicate_hashes, vec!["duplicate".to_string()]);
        assert_eq!(
            summary.missing_parents,
            vec!["p1".to_string(), "p2".to_string()]
        );
        assert_eq!(summary.rejected.len(), 1);
        assert!(summary.authoritative_progress());
    }
}
