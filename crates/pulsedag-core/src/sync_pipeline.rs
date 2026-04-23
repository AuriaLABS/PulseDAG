use serde::{Deserialize, Serialize};

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

    pub fn fallback_after_failure(&mut self, message: impl Into<String>, now_unix: u64) {
        self.counters.phase_failures = self.counters.phase_failures.saturating_add(1);
        self.last_error = Some(message.into());
        self.transition_to(SyncPhase::PeerSelection, now_unix);
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
    use super::{SyncPhase, SyncPipelineStatus};

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
}
