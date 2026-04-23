use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPeerCandidate {
    pub peer_id: String,
    pub score: i32,
    pub fail_streak: u32,
    pub connected: bool,
    pub next_retry_unix: u64,
    pub suppressed_until_unix: u64,
    pub recovery_success_count: u64,
    pub recent_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankedSyncPeer {
    pub peer_id: String,
    pub rank_score: i64,
    pub excluded_until_unix: Option<u64>,
}

pub fn rank_sync_candidates(
    candidates: &[SyncPeerCandidate],
    now_unix: u64,
) -> Vec<RankedSyncPeer> {
    let mut ranked = candidates
        .iter()
        .map(|candidate| {
            let retry_penalty = if candidate.next_retry_unix > now_unix {
                (candidate.next_retry_unix.saturating_sub(now_unix) as i64) * 4
            } else {
                0
            };
            let flap_penalty = if candidate.suppressed_until_unix > now_unix {
                (candidate.suppressed_until_unix.saturating_sub(now_unix) as i64) * 4
            } else {
                0
            };
            let recent_failure_penalty = (candidate.recent_failures as i64) * 12;
            let fail_streak_penalty = (candidate.fail_streak as i64) * 40;
            let recovery_bonus = (candidate.recovery_success_count as i64) * 8;
            let connected_bonus = if candidate.connected { 25 } else { 0 };
            let rank_score = candidate.score as i64 + connected_bonus + recovery_bonus
                - fail_streak_penalty
                - recent_failure_penalty
                - retry_penalty
                - flap_penalty;
            let excluded_until_unix = if candidate.suppressed_until_unix > now_unix {
                Some(candidate.suppressed_until_unix)
            } else if candidate.next_retry_unix > now_unix {
                Some(candidate.next_retry_unix)
            } else {
                None
            };
            RankedSyncPeer {
                peer_id: candidate.peer_id.clone(),
                rank_score,
                excluded_until_unix,
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.rank_score
            .cmp(&a.rank_score)
            .then_with(|| a.peer_id.cmp(&b.peer_id))
    });
    ranked
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    #[default]
    Idle,
    PeerSelection,
    HeaderDiscovery,
    BlockAcquisition,
    ValidationApplication,
    CatchUpCompletion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SyncProgressCounters {
    pub peer_candidates_considered: u64,
    pub headers_discovered: u64,
    pub blocks_requested: u64,
    pub blocks_acquired: u64,
    pub blocks_validated: u64,
    pub blocks_applied: u64,
    pub phase_failures: u64,
    pub restart_resumes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SyncPipelineStatus {
    pub phase: SyncPhase,
    pub last_transition_unix: Option<u64>,
    pub completed_cycles: u64,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub selected_peer: Option<String>,
    pub selection_version: u64,
    pub fallback_count: u64,
    pub timeout_fallback_count: u64,
    pub last_fallback_reason: Option<String>,
    pub last_fallback_peer: Option<String>,
    pub counters: SyncProgressCounters,
}

impl SyncPipelineStatus {
    pub fn begin_cycle(&mut self, now_unix: u64) {
        if self.phase == SyncPhase::Idle || self.phase == SyncPhase::CatchUpCompletion {
            self.transition_to(SyncPhase::PeerSelection, now_unix);
        }
    }

    pub fn observe_peer_candidate(&mut self, now_unix: u64) {
        self.counters.peer_candidates_considered =
            self.counters.peer_candidates_considered.saturating_add(1);
        self.transition_to(SyncPhase::PeerSelection, now_unix);
    }

    pub fn observe_headers(&mut self, discovered: u64, now_unix: u64) {
        self.counters.headers_discovered =
            self.counters.headers_discovered.saturating_add(discovered);
        self.transition_to(SyncPhase::HeaderDiscovery, now_unix);
    }

    pub fn request_blocks(&mut self, requested: u64, now_unix: u64) {
        self.counters.blocks_requested = self.counters.blocks_requested.saturating_add(requested);
        self.transition_to(SyncPhase::BlockAcquisition, now_unix);
    }

    pub fn acquire_blocks(&mut self, acquired: u64) {
        self.counters.blocks_acquired = self.counters.blocks_acquired.saturating_add(acquired);
    }

    pub fn validate_and_apply_blocks(&mut self, count: u64, now_unix: u64) {
        self.counters.blocks_validated = self.counters.blocks_validated.saturating_add(count);
        self.counters.blocks_applied = self.counters.blocks_applied.saturating_add(count);
        self.transition_to(SyncPhase::ValidationApplication, now_unix);
    }

    pub fn complete_cycle(&mut self, now_unix: u64) {
        self.transition_to(SyncPhase::CatchUpCompletion, now_unix);
        self.completed_cycles = self.completed_cycles.saturating_add(1);
        self.transition_to(SyncPhase::Idle, now_unix);
        self.last_error = None;
    }

    pub fn observe_selected_peer(&mut self, peer_id: impl Into<String>, now_unix: u64) {
        self.selection_version = self.selection_version.saturating_add(1);
        self.selected_peer = Some(peer_id.into());
        self.transition_to(SyncPhase::PeerSelection, now_unix);
    }

    pub fn fallback_after_failure(&mut self, message: impl Into<String>, now_unix: u64) {
        self.fallback_count = self.fallback_count.saturating_add(1);
        self.counters.phase_failures = self.counters.phase_failures.saturating_add(1);
        let message = message.into();
        self.last_error = Some(message.clone());
        self.last_fallback_reason = Some(message);
        self.transition_to(SyncPhase::PeerSelection, now_unix);
    }

    pub fn timeout_fallback(
        &mut self,
        peer_id: impl Into<String>,
        timeout_secs: u64,
        now_unix: u64,
    ) {
        let peer_id = peer_id.into();
        self.timeout_fallback_count = self.timeout_fallback_count.saturating_add(1);
        self.last_fallback_peer = Some(peer_id.clone());
        self.fallback_after_failure(
            format!("sync peer {} timed out after {}s", peer_id, timeout_secs),
            now_unix,
        );
    }

    pub fn resume_after_restart(&mut self, now_unix: u64) {
        self.restart_count = self.restart_count.saturating_add(1);
        if self.phase != SyncPhase::Idle {
            self.counters.restart_resumes = self.counters.restart_resumes.saturating_add(1);
            self.transition_to(SyncPhase::PeerSelection, now_unix);
        }
    }

    fn transition_to(&mut self, next: SyncPhase, now_unix: u64) {
        if self.phase == next {
            self.last_transition_unix = Some(now_unix);
            return;
        }
        if !is_transition_allowed(self.phase, next) {
            return;
        }
        self.phase = next;
        self.last_transition_unix = Some(now_unix);
    }
}

fn is_transition_allowed(current: SyncPhase, next: SyncPhase) -> bool {
    use SyncPhase::*;
    match (current, next) {
        (Idle, PeerSelection)
        | (PeerSelection, HeaderDiscovery)
        | (HeaderDiscovery, BlockAcquisition)
        | (BlockAcquisition, ValidationApplication)
        | (ValidationApplication, CatchUpCompletion)
        | (CatchUpCompletion, Idle) => true,
        (_, PeerSelection) => true,
        (same, same_again) if same == same_again => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{rank_sync_candidates, SyncPeerCandidate, SyncPhase, SyncPipelineStatus};

    #[test]
    fn sync_phase_transitions_follow_expected_order() {
        let mut status = SyncPipelineStatus::default();
        status.begin_cycle(10);
        status.observe_headers(3, 11);
        status.request_blocks(2, 12);
        status.acquire_blocks(2);
        status.validate_and_apply_blocks(2, 13);
        status.complete_cycle(14);

        assert_eq!(status.phase, SyncPhase::Idle);
        assert_eq!(status.completed_cycles, 1);
        assert_eq!(status.counters.blocks_applied, 2);
    }

    #[test]
    fn restart_resume_reconstructs_sync_state_safely() {
        let mut status = SyncPipelineStatus::default();
        status.begin_cycle(20);
        status.observe_headers(1, 21);
        status.resume_after_restart(22);

        assert_eq!(status.phase, SyncPhase::PeerSelection);
        assert_eq!(status.restart_count, 1);
        assert_eq!(status.counters.restart_resumes, 1);
    }

    #[test]
    fn failure_falls_back_to_peer_selection_safely() {
        let mut status = SyncPipelineStatus::default();
        status.begin_cycle(30);
        status.observe_headers(1, 31);
        status.request_blocks(1, 32);
        status.fallback_after_failure("block fetch timeout", 33);

        assert_eq!(status.phase, SyncPhase::PeerSelection);
        assert_eq!(status.counters.phase_failures, 1);
        assert_eq!(status.last_error.as_deref(), Some("block fetch timeout"));
    }

    #[test]
    fn slow_peer_is_deprioritized_for_sync() {
        let ranked = rank_sync_candidates(
            &[
                SyncPeerCandidate {
                    peer_id: "peer-fast".into(),
                    score: 120,
                    fail_streak: 0,
                    connected: true,
                    next_retry_unix: 100,
                    suppressed_until_unix: 0,
                    recovery_success_count: 2,
                    recent_failures: 0,
                },
                SyncPeerCandidate {
                    peer_id: "peer-slow".into(),
                    score: 130,
                    fail_streak: 2,
                    connected: true,
                    next_retry_unix: 150,
                    suppressed_until_unix: 0,
                    recovery_success_count: 0,
                    recent_failures: 3,
                },
            ],
            100,
        );
        assert_eq!(ranked[0].peer_id, "peer-fast");
    }

    #[test]
    fn fallback_occurs_when_selected_peer_times_out() {
        let mut status = SyncPipelineStatus::default();
        status.begin_cycle(10);
        status.observe_selected_peer("peer-a", 10);
        status.timeout_fallback("peer-a", 12, 22);
        assert_eq!(status.phase, SyncPhase::PeerSelection);
        assert_eq!(status.timeout_fallback_count, 1);
        assert_eq!(status.last_fallback_peer.as_deref(), Some("peer-a"));
    }

    #[test]
    fn alternate_peer_can_be_chosen_successfully() {
        let mut status = SyncPipelineStatus::default();
        status.begin_cycle(1);
        status.observe_selected_peer("peer-a", 1);
        status.timeout_fallback("peer-a", 5, 6);
        status.observe_selected_peer("peer-b", 7);
        assert_eq!(status.selected_peer.as_deref(), Some("peer-b"));
        assert_eq!(status.selection_version, 2);
    }

    #[test]
    fn multiple_degraded_peers_do_not_cause_sync_starvation() {
        let ranked = rank_sync_candidates(
            &[
                SyncPeerCandidate {
                    peer_id: "peer-a".into(),
                    score: 60,
                    fail_streak: 4,
                    connected: true,
                    next_retry_unix: 205,
                    suppressed_until_unix: 0,
                    recovery_success_count: 0,
                    recent_failures: 4,
                },
                SyncPeerCandidate {
                    peer_id: "peer-b".into(),
                    score: 58,
                    fail_streak: 3,
                    connected: true,
                    next_retry_unix: 203,
                    suppressed_until_unix: 0,
                    recovery_success_count: 1,
                    recent_failures: 3,
                },
                SyncPeerCandidate {
                    peer_id: "peer-c".into(),
                    score: 57,
                    fail_streak: 2,
                    connected: true,
                    next_retry_unix: 200,
                    suppressed_until_unix: 0,
                    recovery_success_count: 2,
                    recent_failures: 2,
                },
            ],
            200,
        );
        assert_eq!(ranked.len(), 3);
        assert!(ranked[0].rank_score >= ranked[2].rank_score);
    }
}
