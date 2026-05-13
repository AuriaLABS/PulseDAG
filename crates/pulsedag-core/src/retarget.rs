use crate::{
    pow::{pow_target_u64, DevDifficultyPolicy},
    state::ChainState,
};

pub const CONSENSUS_TARGET_BLOCK_INTERVAL_SECS: u64 = 60;
const CONSENSUS_DIFFICULTY_WINDOW: usize = 20;
const CONSENSUS_DIFFICULTY_USE_MEDIAN: bool = false;
const CONSENSUS_MAX_FUTURE_DRIFT_SECS: u64 = CONSENSUS_TARGET_BLOCK_INTERVAL_SECS * 2;
const CONSENSUS_RETARGET_DEADBAND_BPS: u64 = 800;
const CONSENSUS_RETARGET_DAMPING_DIVISOR: u64 = 2;
const CONSENSUS_RETARGET_MIN_BPS: u64 = 8_000;
const CONSENSUS_RETARGET_MAX_BPS: u64 = 12_500;

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
    let policy = consensus_difficulty_policy();
    let observed_block_count = consensus_observed_block_count(state, policy.window_size);
    let interval = consensus_recent_block_interval_secs_with_mode(
        state,
        policy.window_size,
        policy.use_median,
    );
    let avg_block_interval_secs = if interval == 0 {
        policy.target_block_interval_secs
    } else {
        interval
    };
    let current_difficulty = consensus_current_difficulty_for_chain(state);
    let retarget_multiplier_bps = consensus_retarget_multiplier_bps(avg_block_interval_secs);
    let raw_multiplier_bps = policy
        .target_block_interval_secs
        .saturating_mul(10_000)
        .checked_div(avg_block_interval_secs.max(1))
        .unwrap_or(10_000);
    let suggested_difficulty =
        consensus_adjust_difficulty_for_interval(current_difficulty, avg_block_interval_secs);
    let expected_difficulty = u32::try_from(suggested_difficulty).unwrap_or(u32::MAX);
    let observed_intervals = observed_block_count.saturating_sub(1);
    let retarget_signal_quality = if observed_intervals < 2 {
        "low".to_string()
    } else {
        "normal".to_string()
    };
    let retarget_rationale = if observed_intervals < 2 {
        "insufficient_signal".to_string()
    } else if retarget_multiplier_bps == 10_000 {
        "within_deadband".to_string()
    } else if retarget_multiplier_bps == CONSENSUS_RETARGET_MIN_BPS {
        "clamped_to_min".to_string()
    } else if retarget_multiplier_bps == CONSENSUS_RETARGET_MAX_BPS {
        "clamped_to_max".to_string()
    } else if raw_multiplier_bps > 10_000 {
        "damped_increase".to_string()
    } else {
        "damped_decrease".to_string()
    };
    let retarget_was_clamped = retarget_multiplier_bps == CONSENSUS_RETARGET_MIN_BPS
        || retarget_multiplier_bps == CONSENSUS_RETARGET_MAX_BPS;

    ConsensusDifficultySnapshot {
        best_height: state.dag.best_height,
        next_height: state.dag.best_height.saturating_add(1),
        expected_difficulty,
        expected_target_u64: pow_target_u64(u64::from(expected_difficulty)),
        target_block_interval_secs: CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        observed_block_count,
        avg_block_interval_secs,
        retarget_multiplier_bps,
        retarget_min_bps: CONSENSUS_RETARGET_MIN_BPS,
        retarget_max_bps: CONSENSUS_RETARGET_MAX_BPS,
        retarget_was_clamped,
        retarget_rationale,
        retarget_signal_quality,
        policy,
    }
}

fn consensus_recent_blocks(state: &ChainState, window_size: usize) -> Vec<&crate::types::Block> {
    let mut blocks = state.dag.blocks.values().collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.header
            .height
            .cmp(&a.header.height)
            .then_with(|| b.header.timestamp.cmp(&a.header.timestamp))
    });
    blocks
        .into_iter()
        .take(window_size.max(2))
        .collect::<Vec<_>>()
}

fn consensus_recent_intervals_secs(state: &ChainState, window_size: usize) -> Vec<u64> {
    let window = consensus_recent_blocks(state, window_size);
    let mut intervals = Vec::new();
    for pair in window.windows(2) {
        let newer = pair[0].header.timestamp;
        let older = pair[1].header.timestamp;
        intervals.push(newer.saturating_sub(older));
    }
    intervals
}

fn consensus_median(values: &mut [u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[mid - 1].saturating_add(values[mid]) / 2
    } else {
        values[mid]
    }
}

fn consensus_recent_block_interval_secs_with_mode(
    state: &ChainState,
    window_size: usize,
    use_median: bool,
) -> u64 {
    let mut intervals = consensus_recent_intervals_secs(state, window_size);
    if intervals.is_empty() {
        return 0;
    }
    if use_median {
        consensus_median(&mut intervals)
    } else {
        intervals.iter().copied().sum::<u64>() / (intervals.len() as u64)
    }
}

fn consensus_current_difficulty_for_chain(state: &ChainState) -> u64 {
    state
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| u64::from(b.header.difficulty).max(1))
        .unwrap_or_else(|| match state.dag.best_height {
            0..=9 => 1,
            10..=49 => 2,
            50..=199 => 4,
            _ => 8,
        })
}

fn consensus_difficulty_policy() -> DevDifficultyPolicy {
    DevDifficultyPolicy {
        target_block_interval_secs: CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        window_size: CONSENSUS_DIFFICULTY_WINDOW,
        use_median: CONSENSUS_DIFFICULTY_USE_MEDIAN,
        max_future_drift_secs: CONSENSUS_MAX_FUTURE_DRIFT_SECS,
    }
}

fn consensus_observed_block_count(state: &ChainState, window_size: usize) -> usize {
    consensus_recent_blocks(state, window_size).len()
}

fn consensus_retarget_multiplier_bps(avg_block_interval_secs: u64) -> u64 {
    if avg_block_interval_secs == 0 {
        return 10_000;
    }

    let raw = CONSENSUS_TARGET_BLOCK_INTERVAL_SECS.saturating_mul(10_000)
        / avg_block_interval_secs.max(1);
    let lower_bound = 10_000u64.saturating_sub(CONSENSUS_RETARGET_DEADBAND_BPS);
    let upper_bound = 10_000u64.saturating_add(CONSENSUS_RETARGET_DEADBAND_BPS);
    if (lower_bound..=upper_bound).contains(&raw) {
        return 10_000;
    }

    let deviation = raw as i64 - 10_000;
    let damped = 10_000i64 + (deviation / CONSENSUS_RETARGET_DAMPING_DIVISOR as i64);
    (damped as u64).clamp(CONSENSUS_RETARGET_MIN_BPS, CONSENSUS_RETARGET_MAX_BPS)
}

fn consensus_adjust_difficulty_for_interval(current: u64, avg_block_interval_secs: u64) -> u64 {
    if avg_block_interval_secs == 0 {
        return current.max(1);
    }
    let multiplier_bps = consensus_retarget_multiplier_bps(avg_block_interval_secs);
    let adjusted = current
        .max(1)
        .saturating_mul(multiplier_bps)
        .saturating_add(5_000)
        / 10_000;
    adjusted.max(1)
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
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
    };

    fn append_header_only_block(
        state: &mut ChainState,
        height: u64,
        timestamp: u64,
        difficulty: u32,
    ) {
        let parent = if height == 1 {
            state.dag.genesis_hash.clone()
        } else {
            format!("retarget-parent-{}", height - 1)
        };
        let hash = format!("retarget-parent-{height}");
        state.dag.blocks.insert(
            hash.clone(),
            Block {
                hash: hash.clone(),
                header: BlockHeader {
                    version: 1,
                    parents: vec![parent],
                    timestamp,
                    difficulty,
                    nonce: 0,
                    merkle_root: format!("merkle-{height}"),
                    state_root: format!("state-{height}"),
                    blue_score: height,
                    height,
                },
                transactions: Vec::new(),
            },
        );
        state.dag.best_height = height;
        state.dag.tips.clear();
        state.dag.tips.insert(hash);
    }

    fn state_with_fixed_interval_tip(difficulty: u32) -> ChainState {
        let mut state = init_chain_state("test".to_string());
        let start = 1_700_000_000;
        for height in 1..=25 {
            append_header_only_block(&mut state, height, start + height * 60, difficulty);
        }
        state
    }

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

    #[test]
    fn consensus_snapshot_uses_fixed_chain_policy() {
        let state = state_with_fixed_interval_tip(4);
        let snapshot = consensus_difficulty_snapshot(&state);

        assert_eq!(snapshot.expected_difficulty, 4);
        assert_eq!(snapshot.expected_target_u64, pow_target_u64(4));
        assert_eq!(
            snapshot.avg_block_interval_secs,
            CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );
        assert_eq!(snapshot.retarget_multiplier_bps, 10_000);
        assert_eq!(snapshot.retarget_min_bps, CONSENSUS_RETARGET_MIN_BPS);
        assert_eq!(snapshot.retarget_max_bps, CONSENSUS_RETARGET_MAX_BPS);
        assert_eq!(snapshot.policy.window_size, CONSENSUS_DIFFICULTY_WINDOW);
        assert_eq!(snapshot.policy.use_median, CONSENSUS_DIFFICULTY_USE_MEDIAN);
        assert_eq!(
            snapshot.policy.target_block_interval_secs,
            CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );
    }
}
