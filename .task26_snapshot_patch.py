from pathlib import Path

# Patch core state replay with canonical snapshot materialization/verification.
path = Path("crates/pulsedag-core/src/state_replay_v2.rs")
text = path.read_text()
old_import = "    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2},\n"
new_import = "    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2, GHOSTDAG_V1_ORDERING_VERSION},\n"
if old_import not in text:
    raise SystemExit("state_replay import anchor not found")
text = text.replace(old_import, new_import, 1)

anchor = "\n#[cfg(test)]\nmod tests {\n"
helpers = r'''
/// Materialize a canonical, self-consistent v2.4 snapshot state without
/// mutating the caller's live runtime state.
///
/// The authoritative UTXO is rebuilt from the frozen total DAG order, then the
/// snapshot-only ordering/state-root fields are populated from that same replay.
/// Runtime consensus mode and other operational state remain unchanged.
pub fn materialize_authoritative_state_v2(state: &ChainState) -> Result<ChainState, PulseError> {
    let replay = rebuild_authoritative_state_v2(state)?;
    let mut materialized = state.clone();
    materialized.utxo = replay.utxo.clone();
    materialized.dag.ordered_dag = replay.ordered_dag.blocks.clone();
    materialized.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    materialized.dag.ordered_dag_tip = replay.diagnostics.ordered_dag_tip.clone();
    materialized.dag.ordered_dag_state_root = Some(replay.diagnostics.state_root.clone());
    materialized.dag.ordered_dag_conflict_diagnostics =
        replay.diagnostics.conflict_diagnostics.clone();
    Ok(materialized)
}

/// Verify that a persisted/restored v2.4 snapshot is already materialized from
/// the same authoritative ordering and transactional replay it claims.
///
/// This is deliberately stricter than merely recomputing a valid state: stale
/// legacy ordering fields or a stale UTXO payload are rejected rather than
/// silently normalized during restore.
pub fn verify_authoritative_state_snapshot_v2(
    state: &ChainState,
) -> Result<StateReplayV2Diagnostics, PulseError> {
    let replay = rebuild_authoritative_state_v2(state)?;
    let observed_state_root = state.utxo.compute_state_root()?;

    if state.dag.ordering_version != GHOSTDAG_V1_ORDERING_VERSION {
        return Err(PulseError::NonDeterministicState(format!(
            "v2.4 snapshot ordering version {} does not match {}",
            state.dag.ordering_version, GHOSTDAG_V1_ORDERING_VERSION
        )));
    }
    if state.dag.ordered_dag != replay.ordered_dag.blocks {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot ordered DAG does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_tip != replay.diagnostics.ordered_dag_tip {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot ordered DAG tip does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_state_root.as_deref()
        != Some(replay.diagnostics.state_root.as_str())
    {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot recorded state root does not match authoritative recomputation"
                .to_string(),
        ));
    }
    if observed_state_root != replay.diagnostics.state_root {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot UTXO state root does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_conflict_diagnostics != replay.diagnostics.conflict_diagnostics {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot conflict diagnostics do not match authoritative recomputation"
                .to_string(),
        ));
    }

    Ok(replay.diagnostics)
}
'''
if anchor not in text:
    raise SystemExit("state_replay test anchor not found")
text = text.replace(anchor, "\n" + helpers + anchor, 1)
path.write_text(text)

# Export the new core helpers.
path = Path("crates/pulsedag-core/src/lib.rs")
text = path.read_text()
old = '''pub use state_replay_v2::{
    rebuild_authoritative_state_v2, StateReplayV2, StateReplayV2Diagnostics,
};
'''
new = '''pub use state_replay_v2::{
    materialize_authoritative_state_v2, rebuild_authoritative_state_v2,
    verify_authoritative_state_snapshot_v2, StateReplayV2, StateReplayV2Diagnostics,
};
'''
if old not in text:
    raise SystemExit("lib state_replay export anchor not found")
path.write_text(text.replace(old, new, 1))

# Add semantic GhostdagV1 snapshot verification to the protocol envelope.
path = Path("crates/pulsedag-storage/src/protocol_bundle.rs")
text = path.read_text()
old_import = '''use pulsedag_core::{
    errors::PulseError, ProtocolActivationIdentity, ProtocolActivationRecordV1,
    ProtocolRestoreIdentityGate,
};
'''
new_import = '''use pulsedag_core::{
    derive_finality_boundary_v1, errors::PulseError, verify_authoritative_state_snapshot_v2,
    ProtocolActivationIdentity, ProtocolActivationRecordV1, ProtocolConsensusMode,
    ProtocolRestoreIdentityGate,
};
'''
if old_import not in text:
    raise SystemExit("protocol_bundle import anchor not found")
text = text.replace(old_import, new_import, 1)

old_verify = '''        if !report.restore_guarantees_explicit {
            return Err(verification_error(&report));
        }
        Ok(report)
'''
new_verify = '''        if !report.restore_guarantees_explicit {
            return Err(verification_error(&report));
        }

        if expected.consensus_mode == ProtocolConsensusMode::GhostdagV1 {
            let diagnostics =
                verify_authoritative_state_snapshot_v2(&bundle.legacy_bundle.snapshot).map_err(
                    |error| {
                        storage_error(format!(
                            "activated-v2 snapshot is not authoritatively materialized: {error:?}"
                        ))
                    },
                )?;
            let finality = derive_finality_boundary_v1(&bundle.legacy_bundle.snapshot).map_err(
                |error| {
                    storage_error(format!(
                        "activated-v2 snapshot finality boundary is not derivable: {error:?}"
                    ))
                },
            )?;
            if finality.protocol_identity != *expected {
                return Err(storage_error(
                    "activated-v2 snapshot finality identity does not match envelope identity",
                ));
            }
            if finality.ordered_dag_digest != diagnostics.ordered_dag_digest {
                return Err(storage_error(
                    "activated-v2 snapshot finality digest does not match authoritative ordering",
                ));
            }
        }

        Ok(report)
'''
if old_verify not in text:
    raise SystemExit("protocol_bundle verify anchor not found")
path.write_text(text.replace(old_verify, new_verify, 1))
