use crate::{
    pow::{dev_difficulty_snapshot, pow_target_u64, DevDifficultyPolicy},
    state::ChainState,
};

pub const CONSENSUS_TARGET_BLOCK_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsensusDifficultySnapshot {
    pub best_height: u64,
    pub next_height: u64,
    pub expected_difficulty: u32,
    pub expected_target_u64: u64,
    pub target_block_interval_secs: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub retarget_min_bps: u64,
    pub retarget_max_bps: u64,
    pub retarget_was_clamped: bool,
    pub retarget_rationale: String,
    pub retarget_signal_quality: String,
    pub policy: DevDifficultyPolicy,
}

pub fn consensus_difficulty_snapshot(state: &ChainState) -> ConsensusDifficultySnapshot {
    let snapshot = dev_difficulty_snapshot(state);
    let expected_difficulty = u32::try_from(snapshot.suggested_difficulty).unwrap_or(u32::MAX);
    ConsensusDifficultySnapshot {
        best_height: state.dag.best_height,
        next_height: state.dag.best_height.saturating_add(1),
        expected_difficulty,
        expected_target_u64: pow_target_u64(u64::from(expected_difficulty)),
        target_block_interval_secs: CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        retarget_min_bps: snapshot.retarget_min_bps,
        retarget_max_bps: snapshot.retarget_max_bps,
        retarget_was_clamped: snapshot.retarget_was_clamped,
        retarget_rationale: snapshot.retarget_rationale,
        retarget_signal_quality: snapshot.retarget_signal_quality,
        policy: snapshot.policy,
    }
}

pub fn expected_difficulty(state: &ChainState) -> u32 {
    consensus_difficulty_snapshot(state).expected_difficulty
}

pub fn expected_target_u64(state: &ChainState) -> u64 {
    consensus_difficulty_snapshot(state).expected_target_u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::init_chain_state;

    #[test]
    fn consensus_snapshot_uses_sixty_second_target() {
        let state = init_chain_state("test".to_string());
        let snapshot = consensus_difficulty_snapshot(&state);
        assert_eq!(snapshot.target_block_interval_secs, 60);
        assert_eq!(snapshot.expected_difficulty, 1);
        assert_eq!(snapshot.expected_target_u64, pow_target_u64(1));
    }

    #[test]
    fn consensus_target_conversion_uses_expected_difficulty_bits() {
        let state = init_chain_state("test".to_string());
        let expected = expected_difficulty(&state);
        let target = expected_target_u64(&state);
        assert_eq!(target, crate::target_from_compact(expected));
    }
}
