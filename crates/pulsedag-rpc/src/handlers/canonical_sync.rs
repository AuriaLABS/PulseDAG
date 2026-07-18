use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::api::NodeRuntimeStats;
use pulsedag_core::{state::ChainState, SyncPhase};
use pulsedag_p2p::P2pStatus;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CanonicalSyncState {
    pub sync_state: String,
    pub catchup_stage: String,
    pub lag_blocks: u64,
    pub lag_band: String,
    pub network_selected_height_gap: u64,
    pub network_selected_tip_mismatch: bool,
    pub storage_replay_gap: u64,
    pub storage_memory_mismatch: bool,
    pub local_selected_height: u64,
    pub best_remote_selected_height: Option<u64>,
    pub best_remote_selected_tip: Option<String>,
    pub catchup_progress_bps: u64,
    pub catchup_summary: String,
    pub recovery_reason: Option<String>,
    pub selected_sync_peer: Option<String>,
    pub canonical_sync_state_generation: u64,
    pub canonical_remote_tip_generation: u64,
    pub canonical_remote_tip_peer: Option<String>,
    pub live_sync_error_active: u64,
    pub live_sync_error_cleared_total: u64,
    pub stale_sync_error_suppressed_total: u64,
    pub sync_live_error: Option<String>,
    pub sync_last_historical_error: Option<String>,
    pub sync_last_error_resolved_at: Option<u64>,
    pub sync_last_error_resolution_reason: Option<String>,
    pub synced_with_recovering_stage_total: u64,
    pub aligned_with_active_recovery_reason_total: u64,
    pub catchup_recovery_started_total: u64,
    pub catchup_recovery_completed_total: u64,
    pub catchup_recovery_last_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteSelectedTipEvidence {
    pub peer_id: String,
    pub selected_height: u64,
    pub selected_tip: Option<String>,
    pub chain_id: String,
    pub observed_at_unix: u64,
    pub direct_request_capable: bool,
    pub connected: bool,
    pub from_tip_inventory: bool,
}

pub const REMOTE_SELECTED_TIP_MAX_AGE_SECS: u64 = 30;

pub fn lag_band(lag_blocks: u64) -> &'static str {
    match lag_blocks {
        0 => "aligned",
        1..=2 => "near_tip",
        3..=10 => "catching_up",
        11..=100 => "lagging",
        _ => "severely_lagging",
    }
}

fn counters_coherent(runtime: &NodeRuntimeStats) -> bool {
    runtime.sync_pipeline.counters.blocks_applied <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested
}

fn active_selected_segment_gap(runtime: &NodeRuntimeStats) -> u64 {
    let selected_recovery_active = runtime.active_session_id.is_some()
        || matches!(
            runtime.sync_state.as_str(),
            "locating_common_ancestor"
                | "selected_chain_locator_sync"
                | "requesting_selected_headers"
                | "requesting_selected_blocks"
                | "applying_selected_segment"
        );
    if selected_recovery_active {
        runtime.selected_segment_gap_blocks
    } else {
        0
    }
}

fn local_selected_height(chain: &ChainState) -> u64 {
    chain
        .dag
        .selected_chain
        .last()
        .and_then(|tip| chain.dag.blocks.get(tip))
        .map(|block| block.header.height)
        .unwrap_or(chain.dag.best_height)
}

pub fn select_remote_sync_evidence(
    chain_id: &str,
    now_unix: u64,
    evidence: &[RemoteSelectedTipEvidence],
) -> Option<RemoteSelectedTipEvidence> {
    evidence
        .iter()
        .filter(|ev| ev.chain_id == chain_id)
        .filter(|ev| ev.connected && ev.direct_request_capable && ev.from_tip_inventory)
        .filter(|ev| {
            now_unix.saturating_sub(ev.observed_at_unix) <= REMOTE_SELECTED_TIP_MAX_AGE_SECS
        })
        .max_by(|a, b| {
            a.selected_height
                .cmp(&b.selected_height)
                .then_with(|| a.observed_at_unix.cmp(&b.observed_at_unix))
                .then_with(|| b.peer_id.cmp(&a.peer_id))
        })
        .cloned()
}

pub fn remote_sync_evidence_from_p2p_status(
    status: Option<&P2pStatus>,
    _now_unix: u64,
) -> Vec<RemoteSelectedTipEvidence> {
    let Some(status) = status else {
        return Vec::new();
    };

    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|ev| ev.connected)
        .filter(|ev| {
            status
                .connected_peers
                .iter()
                .any(|connected| connected == &ev.peer_id)
        })
        .filter(|ev| ev.direct_request_capable)
        .map(|ev| RemoteSelectedTipEvidence {
            peer_id: ev.peer_id.clone(),
            selected_height: ev.selected_height,
            selected_tip: ev.selected_tip.clone(),
            chain_id: ev.chain_id.clone(),
            observed_at_unix: ev.observed_at_unix,
            direct_request_capable: ev.direct_request_capable,
            connected: ev.connected,
            from_tip_inventory: true,
        })
        .collect()
}

pub fn build_canonical_sync_state_with_remote_evidence(
    chain: &ChainState,
    runtime: &NodeRuntimeStats,
    persisted_block_count: usize,
    now_unix: u64,
    fallback_selected_sync_peer: Option<String>,
    remote_evidence: &[RemoteSelectedTipEvidence],
) -> CanonicalSyncState {
    let local_selected_height = local_selected_height(chain);
    let local_selected_tip = chain.dag.selected_chain.last().cloned();
    let best_remote = select_remote_sync_evidence(&chain.chain_id, now_unix, remote_evidence);
    let best_remote_selected_height = best_remote.as_ref().map(|ev| ev.selected_height);
    let best_remote_selected_tip = best_remote.as_ref().and_then(|ev| ev.selected_tip.clone());
    let instantaneous_network_selected_height_gap = best_remote_selected_height
        .unwrap_or(local_selected_height)
        .saturating_sub(local_selected_height);
    let network_selected_height_gap =
        instantaneous_network_selected_height_gap.max(active_selected_segment_gap(runtime));
    let network_selected_tip_mismatch = best_remote_selected_tip.is_some()
        && local_selected_tip.is_some()
        && best_remote_selected_tip != local_selected_tip;
    let storage_replay_gap =
        (persisted_block_count as u64).saturating_sub(chain.dag.blocks.len() as u64);
    let storage_memory_mismatch =
        storage_replay_gap > 0 || chain.dag.blocks.len() > persisted_block_count;
    let lag_blocks = network_selected_height_gap;
    let lag_band = lag_band(lag_blocks).to_string();
    let catchup_progress_bps = best_remote_selected_height
        .map(|remote| {
            if remote == 0 {
                10_000
            } else {
                local_selected_height
                    .saturating_mul(10_000)
                    .saturating_div(remote)
                    .min(10_000)
            }
        })
        .unwrap_or(10_000);
    let pending_missing_parents = pulsedag_core::pending_missing_parent_count(chain);
    let active_terminal_missing_parents =
        pulsedag_core::terminal_missing_parent_active_blocking_count(chain);
    let has_orphan_work = !chain.orphan_blocks.is_empty() || active_terminal_missing_parents > 0;
    let coherent = counters_coherent(runtime);
    let historical_error = runtime.sync_pipeline.last_error.clone();
    let live_error_active = historical_error.is_some()
        && (pending_missing_parents > 0
            || active_terminal_missing_parents > 0
            || runtime.pending_block_requests > 0
            || runtime.inflight_block_requests > 0);
    let sync_live_error = live_error_active
        .then(|| historical_error.clone())
        .flatten();
    let stale_sync_error_suppressed_total =
        u64::from(historical_error.is_some() && !live_error_active);
    let no_blockers = lag_blocks == 0
        && !network_selected_tip_mismatch
        && !has_orphan_work
        && pending_missing_parents == 0
        && runtime.pending_block_requests == 0
        && runtime.inflight_block_requests == 0;
    let stalled = runtime.sync_pipeline.phase != SyncPhase::Idle
        && lag_band != "aligned"
        && (lag_blocks > 0
            || runtime.sync_pipeline.counters.blocks_requested
                > runtime.sync_pipeline.counters.blocks_applied)
        && runtime
            .sync_pipeline
            .last_transition_unix
            .map(|ts| now_unix.saturating_sub(ts) > 120)
            .unwrap_or(false);

    let (sync_state, catchup_stage, recovery_reason) = if no_blockers
        && coherent
        && !live_error_active
    {
        ("synced".to_string(), "steady".to_string(), None)
    } else if live_error_active || !coherent {
        (
            runtime.sync_state.clone(),
            "degraded".to_string(),
            Some(
                sync_live_error
                    .clone()
                    .map(|err| format!("sync live error: {err}"))
                    .unwrap_or_else(|| {
                        "sync counter incoherence detected; verify sync pipeline accounting"
                            .to_string()
                    }),
            ),
        )
    } else if lag_blocks > 2 || network_selected_tip_mismatch {
        ("locating_common_ancestor".to_string(), "discovering".to_string(), Some(format!("peer selected tip ahead: local_height={local_selected_height} remote_height={} network_gap={lag_blocks} tip_mismatch={network_selected_tip_mismatch}", best_remote_selected_height.unwrap_or(local_selected_height))))
    } else if stalled {
        (runtime.sync_state.clone(), "recovering".to_string(), Some(format!("no-progress escalation: sync stalled in {:?} with lag_band={lag_band}; bounded remediation active (fallbacks={}, timeouts={}, restarts={})", runtime.sync_pipeline.phase, runtime.sync_pipeline.fallback_count, runtime.sync_pipeline.timeout_fallback_count, runtime.sync_pipeline.restart_count)))
    } else if pending_missing_parents > 0 {
        (
            runtime.sync_state.clone(),
            "recovering".to_string(),
            Some(format!(
                "orphan recovery pending: {} missing parent(s) still queued for reprocess",
                pending_missing_parents
            )),
        )
    } else if has_orphan_work {
        (
            runtime.sync_state.clone(),
            "recovering".to_string(),
            Some(
                "orphan recovery active: queued orphan or terminal missing-parent blocker remains"
                    .to_string(),
            ),
        )
    } else {
        let stage = match runtime.sync_pipeline.phase {
            SyncPhase::Idle if lag_blocks == 0 => "steady",
            SyncPhase::Idle => "requesting_selected_headers",
            SyncPhase::PeerSelection | SyncPhase::HeaderDiscovery => "discovering",
            SyncPhase::BlockAcquisition => "recovering",
            SyncPhase::ValidationApplication => "validating",
            SyncPhase::CatchUpCompletion => "steady",
        };
        let reason = (stage != "steady" || lag_blocks > 0).then(|| format!("catch-up in progress: stage={stage}, lag_band={lag_band}, network_gap={lag_blocks}, storage_replay_gap={storage_replay_gap}"));
        (runtime.sync_state.clone(), stage.to_string(), reason)
    };

    let selected_sync_peer = if sync_state == "synced" && catchup_stage == "steady" {
        None
    } else {
        best_remote
            .as_ref()
            .map(|ev| ev.peer_id.clone())
            .or_else(|| runtime.sync_pipeline.selected_peer.clone())
            .or(fallback_selected_sync_peer)
    };
    let catchup_summary = format!("stage={catchup_stage} lag_blocks={lag_blocks} lag_band={lag_band} network_selected_height_gap={network_selected_height_gap} storage_replay_gap={storage_replay_gap}");
    let synced_with_recovering_stage_total =
        u64::from(sync_state == "synced" && catchup_stage == "recovering");
    let aligned_with_active_recovery_reason_total =
        u64::from(lag_band == "aligned" && recovery_reason.is_some() && catchup_stage != "steady");
    let catchup_recovery_started_total = runtime.sync_pipeline.restart_count
        + runtime.sync_pipeline.fallback_count
        + runtime.sync_pipeline.timeout_fallback_count;
    let catchup_recovery_completed_total =
        u64::from(sync_state == "synced" && catchup_stage == "steady");
    let catchup_recovery_last_reason = recovery_reason.clone();
    let sync_last_error_resolved_at =
        (historical_error.is_some() && !live_error_active).then_some(now_unix);
    let sync_last_error_resolution_reason = (historical_error.is_some() && !live_error_active)
        .then_some(
            "historical sync error has no active missing-parent, orphan, or request blocker"
                .to_string(),
        );
    let mut remote_hasher = DefaultHasher::new();
    best_remote.hash(&mut remote_hasher);
    let canonical_remote_tip_generation = remote_hasher.finish();
    let canonical_remote_tip_peer = best_remote.as_ref().map(|ev| ev.peer_id.clone());
    let mut hasher = DefaultHasher::new();
    sync_state.hash(&mut hasher);
    catchup_stage.hash(&mut hasher);
    lag_blocks.hash(&mut hasher);
    lag_band.hash(&mut hasher);
    recovery_reason.hash(&mut hasher);
    selected_sync_peer.hash(&mut hasher);
    storage_replay_gap.hash(&mut hasher);
    network_selected_tip_mismatch.hash(&mut hasher);
    canonical_remote_tip_generation.hash(&mut hasher);
    let canonical_sync_state_generation = hasher.finish();

    CanonicalSyncState {
        sync_state,
        catchup_stage,
        lag_blocks,
        lag_band,
        network_selected_height_gap,
        network_selected_tip_mismatch,
        storage_replay_gap,
        storage_memory_mismatch,
        local_selected_height,
        best_remote_selected_height,
        best_remote_selected_tip,
        catchup_progress_bps,
        catchup_summary,
        recovery_reason,
        selected_sync_peer,
        canonical_sync_state_generation,
        canonical_remote_tip_generation,
        canonical_remote_tip_peer,
        live_sync_error_active: u64::from(live_error_active),
        live_sync_error_cleared_total: u64::from(historical_error.is_some() && !live_error_active),
        stale_sync_error_suppressed_total,
        sync_live_error,
        sync_last_historical_error: historical_error,
        sync_last_error_resolved_at,
        sync_last_error_resolution_reason,
        synced_with_recovering_stage_total,
        aligned_with_active_recovery_reason_total,
        catchup_recovery_started_total,
        catchup_recovery_completed_total,
        catchup_recovery_last_reason,
    }
}

pub fn build_canonical_sync_state(
    chain: &ChainState,
    runtime: &NodeRuntimeStats,
    persisted_block_count: usize,
    now_unix: u64,
    fallback_selected_sync_peer: Option<String>,
) -> CanonicalSyncState {
    build_canonical_sync_state_with_remote_evidence(
        chain,
        runtime,
        persisted_block_count,
        now_unix,
        fallback_selected_sync_peer,
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::NodeRuntimeStats;
    use pulsedag_core::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
    };

    fn block(hash: &str, parent: &str, height: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec![parent.to_string()],
                timestamp: height,
                difficulty: 1,
                nonce: 0,
                merkle_root: "root".into(),
                state_root: "state".into(),
                blue_score: height,
                height,
            },
            transactions: Vec::new(),
        }
    }

    fn chain_at_selected_height(height: u64) -> ChainState {
        let mut chain = init_chain_state("testnet-dev".to_string());
        let mut parent = chain.dag.genesis_hash.clone();
        for h in 1..=height {
            let hash = format!("b{h}");
            let b = block(&hash, &parent, h);
            chain.dag.blocks.insert(hash.clone(), b);
            chain
                .dag
                .selected_parents
                .insert(hash.clone(), Some(parent.clone()));
            chain.dag.selected_chain.push(hash.clone());
            parent = hash;
        }
        chain.dag.best_height = height;
        chain.dag.tips.clear();
        chain.dag.tips.insert(parent);
        chain
    }

    fn fresh_remote(peer_id: &str, height: u64) -> RemoteSelectedTipEvidence {
        RemoteSelectedTipEvidence {
            peer_id: peer_id.to_string(),
            selected_height: height,
            selected_tip: Some(format!("remote-{height}")),
            chain_id: "testnet-dev".to_string(),
            observed_at_unix: 1_000,
            direct_request_capable: true,
            connected: true,
            from_tip_inventory: true,
        }
    }

    #[test]
    fn network_gap_is_not_storage_replay_gap() {
        let mut chain = chain_at_selected_height(631);
        let parent = chain.dag.genesis_hash.clone();
        for i in chain.dag.blocks.len()..878 {
            let hash = format!("side{i}");
            chain
                .dag
                .blocks
                .insert(hash.clone(), block(&hash, &parent, 1));
        }
        let runtime = NodeRuntimeStats::default();
        let state = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &runtime,
            879,
            1_000,
            None,
            &[fresh_remote("peer-a", 690)],
        );
        assert_eq!(state.network_selected_height_gap, 59);
        assert_eq!(state.lag_blocks, 59);
        assert_eq!(state.storage_replay_gap, 1);
        assert_eq!(state.best_remote_selected_height, Some(690));
        assert_ne!(state.lag_band, "near_tip");
        assert_ne!(state.sync_state, "synced");
    }

    #[test]
    fn disconnected_stale_or_wrong_chain_peer_evidence_is_not_used() {
        let stale = RemoteSelectedTipEvidence {
            observed_at_unix: 900,
            ..fresh_remote("stale", 700)
        };
        let disconnected = RemoteSelectedTipEvidence {
            connected: false,
            ..fresh_remote("disconnected", 710)
        };
        let wrong_chain = RemoteSelectedTipEvidence {
            chain_id: "other".into(),
            ..fresh_remote("wrong", 720)
        };
        assert!(select_remote_sync_evidence(
            "testnet-dev",
            1_000,
            &[stale, disconnected, wrong_chain]
        )
        .is_none());
    }

    #[test]
    fn historical_error_without_active_blocker_is_suppressed() {
        let chain = chain_at_selected_height(3);
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.last_error = Some("missing parent old".into());
        let state = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            None,
            &[],
        );
        assert_eq!(state.catchup_stage, "steady");
        assert_eq!(state.live_sync_error_active, 0);
        assert_eq!(state.stale_sync_error_suppressed_total, 1);
        assert!(state.sync_live_error.is_none());
        assert!(state.sync_last_historical_error.is_some());
        assert!(state.sync_last_error_resolved_at.is_some());
    }

    #[test]
    fn active_unresolved_missing_parent_still_degrades() {
        let mut chain = chain_at_selected_height(3);
        chain
            .orphan_parent_index
            .insert("missing".into(), ["orphan".into()].into_iter().collect());
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.last_error = Some("missing parent active".into());
        let state = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            None,
            &[],
        );
        assert_eq!(state.catchup_stage, "degraded");
        assert_eq!(state.live_sync_error_active, 1);
        assert!(state.sync_live_error.is_some());
    }
    #[test]
    fn active_selected_segment_preserves_initial_gap_until_frontier() {
        let chain = chain_at_selected_height(72);
        let evidence = vec![fresh_remote("peer-a", 120)];
        let mut runtime = NodeRuntimeStats {
            sync_state: "requesting_selected_blocks".into(),
            selected_segment_gap_blocks: 112,
            active_session_id: Some(7),
            ..NodeRuntimeStats::default()
        };
        let active = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            None,
            &evidence,
        );
        assert_eq!(active.network_selected_height_gap, 112);

        runtime.active_session_id = None;
        runtime.sync_state = "dag_frontier_tips_sync".into();
        let frontier = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            None,
            &evidence,
        );
        assert_eq!(frontier.network_selected_height_gap, 48);
    }

    #[test]
    fn p2p_status_remote_inventory_produces_n5_style_gap() {
        let status = P2pStatus {
            chain_id: "testnet-dev".into(),
            connected_peers: vec!["n1".into()],
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "n1".into(),
                connection_generation: 7,
                chain_id: "testnet-dev".into(),
                selected_tip: Some("remote-741".into()),
                selected_height: 741,
                selected_blue_score: Some(741),
                ordered_dag_tip: Some("ordered-741".into()),
                state_root_digest: Some("state-root-741".into()),
                observed_at_unix: 1_000,
                inventory_generation: 11,
                age_secs: 0,
                direct_request_capable: true,
                connected: true,
            }],
            ..P2pStatus::default()
        };
        let chain = chain_at_selected_height(717);
        let evidence = remote_sync_evidence_from_p2p_status(Some(&status), 1_000);
        let state = build_canonical_sync_state_with_remote_evidence(
            &chain,
            &NodeRuntimeStats::default(),
            chain.dag.blocks.len(),
            1_000,
            None,
            &evidence,
        );

        assert_eq!(state.best_remote_selected_height, Some(741));
        assert_eq!(state.network_selected_height_gap, 24);
        assert_eq!(state.sync_state, "locating_common_ancestor");
        assert_eq!(state.catchup_stage, "discovering");
    }
}
