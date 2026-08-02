use crate::{
    pow::{bits_from_target, pow_target_u64, target_from_bits, target_hex, PowTarget},
    state::ChainState,
    types::{Block, Hash},
};

pub const CONSENSUS_TARGET_BLOCK_INTERVAL_SECS: u64 = 60;
pub const CONSENSUS_POW_LIMIT_BITS: u32 = 0x207f_ffff;
pub const CONSENSUS_MIN_TARGET_BITS: u32 = 0x0101_0000;
const CONSENSUS_DIFFICULTY_WINDOW: usize = 20;
const CONSENSUS_DIFFICULTY_USE_MEDIAN: bool = false;
const CONSENSUS_MAX_FUTURE_DRIFT_SECS: u64 = CONSENSUS_TARGET_BLOCK_INTERVAL_SECS * 2;
const CONSENSUS_RETARGET_DEADBAND_BPS: u64 = 800;
const CONSENSUS_RETARGET_DAMPING_DIVISOR: u64 = 2;
const CONSENSUS_RETARGET_MIN_BPS: u64 = 8_000;
const CONSENSUS_RETARGET_MAX_BPS: u64 = 12_500;
const BPS_DENOMINATOR: u64 = 10_000;
const BPS_RECIPROCAL_NUMERATOR: u64 = BPS_DENOMINATOR * BPS_DENOMINATOR;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsensusDifficultyPolicy {
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub use_median: bool,
    pub max_future_drift_secs: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsensusDifficultySnapshot {
    pub best_height: u64,
    pub next_height: u64,
    /// Compatibility field: this carries canonical compact target bits.
    pub expected_difficulty: u32,
    pub expected_bits: u32,
    pub current_bits: u32,
    pub expected_target_u64: u64,
    pub expected_target_hex: String,
    pub current_target_hex: String,
    pub pow_limit_bits: u32,
    pub pow_limit_target_hex: String,
    pub target_block_interval_secs: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    /// Work/difficulty multiplier. Values above 10_000 harden work.
    pub retarget_multiplier_bps: u64,
    /// Target multiplier. Values below 10_000 harden work.
    pub target_multiplier_bps: u64,
    pub retarget_min_bps: u64,
    pub retarget_max_bps: u64,
    pub retarget_was_clamped: bool,
    pub target_was_clamped_to_pow_limit: bool,
    pub retarget_rationale: String,
    pub retarget_signal_quality: String,
    pub policy: ConsensusDifficultyPolicy,
}

pub fn consensus_difficulty_snapshot(state: &ChainState) -> ConsensusDifficultySnapshot {
    let policy = consensus_difficulty_policy();
    let recent_blocks = consensus_recent_blocks(state, policy.window_size);
    let observed_block_count = recent_blocks.len();
    let avg_block_interval_secs = consensus_average_block_interval_secs(&recent_blocks, &policy);
    let observed_intervals = observed_block_count.saturating_sub(1);
    let current_bits = consensus_current_bits_from_blocks(&recent_blocks);
    let current_target = target_from_bits(current_bits);
    let retarget_multiplier_bps = consensus_retarget_multiplier_bps(avg_block_interval_secs);
    let target_multiplier_bps =
        consensus_target_multiplier_bps_from_work_multiplier(retarget_multiplier_bps);
    let (expected_bits, expected_target, target_was_clamped_to_pow_limit) =
        consensus_adjust_target_for_interval(current_bits, avg_block_interval_secs);
    let raw_multiplier_bps = policy
        .target_block_interval_secs
        .saturating_mul(BPS_DENOMINATOR)
        .checked_div(avg_block_interval_secs.max(1))
        .unwrap_or(BPS_DENOMINATOR);
    let retarget_signal_quality = if observed_intervals < 2 {
        "low".to_string()
    } else {
        "normal".to_string()
    };
    let retarget_rationale = if observed_intervals < 2 {
        "insufficient_signal".to_string()
    } else if retarget_multiplier_bps == BPS_DENOMINATOR {
        "within_deadband".to_string()
    } else if retarget_multiplier_bps == CONSENSUS_RETARGET_MIN_BPS {
        "clamped_to_min".to_string()
    } else if retarget_multiplier_bps == CONSENSUS_RETARGET_MAX_BPS {
        "clamped_to_max".to_string()
    } else if raw_multiplier_bps > BPS_DENOMINATOR {
        "damped_increase".to_string()
    } else {
        "damped_decrease".to_string()
    };
    let retarget_was_clamped = retarget_multiplier_bps == CONSENSUS_RETARGET_MIN_BPS
        || retarget_multiplier_bps == CONSENSUS_RETARGET_MAX_BPS;
    let pow_limit = consensus_pow_limit_target();

    ConsensusDifficultySnapshot {
        best_height: state.dag.best_height,
        next_height: state.dag.best_height.saturating_add(1),
        expected_difficulty: expected_bits,
        expected_bits,
        current_bits,
        expected_target_u64: pow_target_u64(u64::from(expected_bits)),
        expected_target_hex: target_hex(&expected_target),
        current_target_hex: target_hex(&current_target),
        pow_limit_bits: CONSENSUS_POW_LIMIT_BITS,
        pow_limit_target_hex: target_hex(&pow_limit),
        target_block_interval_secs: CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        observed_block_count,
        avg_block_interval_secs,
        retarget_multiplier_bps,
        target_multiplier_bps,
        retarget_min_bps: CONSENSUS_RETARGET_MIN_BPS,
        retarget_max_bps: CONSENSUS_RETARGET_MAX_BPS,
        retarget_was_clamped,
        target_was_clamped_to_pow_limit,
        retarget_rationale,
        retarget_signal_quality,
        policy,
    }
}

fn consensus_pow_limit_target() -> PowTarget {
    target_from_bits(CONSENSUS_POW_LIMIT_BITS)
}

fn consensus_min_target() -> PowTarget {
    target_from_bits(CONSENSUS_MIN_TARGET_BITS)
}

fn consensus_recent_blocks(state: &ChainState, window_size: usize) -> Vec<&Block> {
    state
        .dag
        .selected_chain
        .iter()
        .rev()
        .filter_map(|hash| state.dag.blocks.get(hash))
        .filter(|block| block.header.height > 0 && block.header.timestamp > 0)
        .take(window_size.max(2))
        .collect()
}

fn consensus_recent_blocks_from_parent<'a>(
    state: &'a ChainState,
    parent_hash: &Hash,
    window_size: usize,
) -> Option<Vec<&'a Block>> {
    let limit = window_size.max(2);
    let mut recent = Vec::with_capacity(limit);
    let mut cursor = Some(parent_hash.clone());
    let mut saw_known_block = false;

    while let Some(hash) = cursor {
        let Some(block) = state.dag.blocks.get(&hash) else {
            break;
        };
        saw_known_block = true;
        if recent
            .iter()
            .any(|known| known.hash.as_str() == hash.as_str())
        {
            break;
        }
        if block.header.height > 0 && block.header.timestamp > 0 {
            recent.push(block);
            if recent.len() >= limit {
                break;
            }
        }
        cursor = state.dag.selected_parents.get(&hash).cloned().flatten();
    }

    saw_known_block.then_some(recent)
}

fn consensus_recent_intervals_secs(window: &[&Block]) -> Vec<u64> {
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

fn consensus_recent_block_interval_secs_with_mode(window: &[&Block], use_median: bool) -> u64 {
    let mut intervals = consensus_recent_intervals_secs(window);
    if intervals.is_empty() {
        return 0;
    }
    if use_median {
        consensus_median(&mut intervals)
    } else {
        intervals.iter().copied().sum::<u64>() / (intervals.len() as u64)
    }
}

fn consensus_average_block_interval_secs(
    recent_blocks: &[&Block],
    policy: &ConsensusDifficultyPolicy,
) -> u64 {
    if recent_blocks.len() < 2 {
        policy.target_block_interval_secs
    } else {
        consensus_recent_block_interval_secs_with_mode(recent_blocks, policy.use_median).max(1)
    }
}

fn consensus_current_bits_from_blocks(recent_blocks: &[&Block]) -> u32 {
    recent_blocks
        .iter()
        .find(|block| block.header.height > 0)
        .map(|block| block.header.difficulty)
        .unwrap_or(CONSENSUS_POW_LIMIT_BITS)
}

fn consensus_difficulty_policy() -> ConsensusDifficultyPolicy {
    ConsensusDifficultyPolicy {
        target_block_interval_secs: CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        window_size: CONSENSUS_DIFFICULTY_WINDOW,
        use_median: CONSENSUS_DIFFICULTY_USE_MEDIAN,
        max_future_drift_secs: CONSENSUS_MAX_FUTURE_DRIFT_SECS,
    }
}

fn consensus_retarget_multiplier_bps(avg_block_interval_secs: u64) -> u64 {
    if avg_block_interval_secs == 0 {
        return BPS_DENOMINATOR;
    }

    let raw = CONSENSUS_TARGET_BLOCK_INTERVAL_SECS.saturating_mul(BPS_DENOMINATOR)
        / avg_block_interval_secs.max(1);
    let lower_bound = BPS_DENOMINATOR.saturating_sub(CONSENSUS_RETARGET_DEADBAND_BPS);
    let upper_bound = BPS_DENOMINATOR.saturating_add(CONSENSUS_RETARGET_DEADBAND_BPS);
    if (lower_bound..=upper_bound).contains(&raw) {
        return BPS_DENOMINATOR;
    }

    let deviation = raw as i64 - BPS_DENOMINATOR as i64;
    let damped = BPS_DENOMINATOR as i64 + (deviation / CONSENSUS_RETARGET_DAMPING_DIVISOR as i64);
    (damped as u64).clamp(CONSENSUS_RETARGET_MIN_BPS, CONSENSUS_RETARGET_MAX_BPS)
}

fn consensus_target_multiplier_bps_from_work_multiplier(work_multiplier_bps: u64) -> u64 {
    BPS_RECIPROCAL_NUMERATOR
        .saturating_add(work_multiplier_bps / 2)
        .checked_div(work_multiplier_bps.max(1))
        .unwrap_or(BPS_DENOMINATOR)
}

fn target_to_limbs(target: &PowTarget) -> [u64; 4] {
    let mut limbs = [0u64; 4];
    for (index, limb) in limbs.iter_mut().enumerate() {
        let start = index * 8;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&target[start..start + 8]);
        *limb = u64::from_be_bytes(bytes);
    }
    limbs
}

fn limbs_to_target(limbs: &[u64; 4]) -> PowTarget {
    let mut target = [0u8; 32];
    for (index, limb) in limbs.iter().enumerate() {
        let start = index * 8;
        target[start..start + 8].copy_from_slice(&limb.to_be_bytes());
    }
    target
}

fn scale_target_ratio(target: &PowTarget, numerator: u64, denominator: u64) -> (PowTarget, bool) {
    let limbs = target_to_limbs(target);
    let mut product = [0u64; 5];
    let mut carry = 0u128;
    for index in (0..4).rev() {
        let value = u128::from(limbs[index])
            .saturating_mul(u128::from(numerator))
            .saturating_add(carry);
        product[index + 1] = value as u64;
        carry = value >> 64;
    }
    product[0] = carry as u64;

    let divisor = u128::from(denominator.max(1));
    let mut quotient = [0u64; 5];
    let mut remainder = 0u128;
    for (index, limb) in product.iter().enumerate() {
        let dividend = (remainder << 64) | u128::from(*limb);
        quotient[index] = (dividend / divisor) as u64;
        remainder = dividend % divisor;
    }

    let overflow = quotient[0] != 0;
    let scaled = limbs_to_target(&[quotient[1], quotient[2], quotient[3], quotient[4]]);
    (scaled, overflow)
}

fn consensus_adjust_target_for_interval(
    current_bits: u32,
    avg_block_interval_secs: u64,
) -> (u32, PowTarget, bool) {
    let pow_limit = consensus_pow_limit_target();
    let min_target = consensus_min_target();
    let current_target = target_from_bits(current_bits);
    let current_was_above_pow_limit = current_target > pow_limit;
    let bounded_current = if current_was_above_pow_limit {
        pow_limit
    } else if current_target < min_target {
        min_target
    } else {
        current_target
    };

    let work_multiplier_bps = consensus_retarget_multiplier_bps(avg_block_interval_secs);
    let target_multiplier_bps =
        consensus_target_multiplier_bps_from_work_multiplier(work_multiplier_bps);
    let (scaled, overflow) =
        scale_target_ratio(&bounded_current, target_multiplier_bps, BPS_DENOMINATOR);
    let scaled_was_above_pow_limit = overflow || scaled > pow_limit;
    let target_was_clamped_to_pow_limit = current_was_above_pow_limit || scaled_was_above_pow_limit;
    let bounded = if scaled_was_above_pow_limit {
        pow_limit
    } else if scaled < min_target {
        min_target
    } else {
        scaled
    };
    let bits = bits_from_target(&bounded);
    let canonical_target = target_from_bits(bits);
    (bits, canonical_target, target_was_clamped_to_pow_limit)
}

pub fn expected_difficulty(state: &ChainState) -> u32 {
    consensus_difficulty_snapshot(state).expected_bits
}

pub fn expected_difficulty_for_parent(state: &ChainState, parent_hash: &Hash) -> Option<u32> {
    let policy = consensus_difficulty_policy();
    let recent_blocks =
        consensus_recent_blocks_from_parent(state, parent_hash, policy.window_size)?;
    let avg_block_interval_secs = consensus_average_block_interval_secs(&recent_blocks, &policy);
    let current_bits = consensus_current_bits_from_blocks(&recent_blocks);
    Some(consensus_adjust_target_for_interval(current_bits, avg_block_interval_secs).0)
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

    fn append_header_only_block(state: &mut ChainState, height: u64, timestamp: u64, bits: u32) {
        let parent = state
            .dag
            .selected_chain
            .last()
            .cloned()
            .expect("selected chain has genesis");
        let hash = format!("retarget-selected-{height}");
        state.dag.blocks.insert(
            hash.clone(),
            Block {
                hash: hash.clone(),
                header: BlockHeader {
                    version: 1,
                    parents: vec![parent.clone()],
                    timestamp,
                    difficulty: bits,
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
        state.dag.tips.insert(hash.clone());
        state
            .dag
            .selected_parents
            .insert(hash.clone(), Some(parent));
        state.dag.selected_chain.push(hash);
    }

    fn state_with_fixed_interval_tip(bits: u32, interval_secs: u64, count: u64) -> ChainState {
        let mut state = init_chain_state("test".to_string());
        let start = 1_700_000_000;
        for height in 1..=count {
            append_header_only_block(
                &mut state,
                height,
                start + height.saturating_mul(interval_secs),
                bits,
            );
        }
        state
    }

    fn append_side_header_only_block(
        state: &mut ChainState,
        parent: Hash,
        height: u64,
        timestamp: u64,
        bits: u32,
    ) -> Hash {
        let hash = format!("retarget-side-{height}");
        state.dag.blocks.insert(
            hash.clone(),
            Block {
                hash: hash.clone(),
                header: BlockHeader {
                    version: 1,
                    parents: vec![parent.clone()],
                    timestamp,
                    difficulty: bits,
                    nonce: 0,
                    merkle_root: format!("side-merkle-{height}"),
                    state_root: format!("side-state-{height}"),
                    blue_score: height,
                    height,
                },
                transactions: Vec::new(),
            },
        );
        state
            .dag
            .selected_parents
            .insert(hash.clone(), Some(parent));
        hash
    }

    #[test]
    fn consensus_snapshot_uses_sixty_second_target_and_compact_pow_limit() {
        let state = init_chain_state("test".to_string());
        let snapshot = consensus_difficulty_snapshot(&state);
        assert_eq!(snapshot.target_block_interval_secs, 60);
        assert_eq!(snapshot.expected_bits, CONSENSUS_POW_LIMIT_BITS);
        assert_eq!(snapshot.expected_difficulty, CONSENSUS_POW_LIMIT_BITS);
        assert_eq!(
            snapshot.expected_target_u64,
            pow_target_u64(u64::from(CONSENSUS_POW_LIMIT_BITS))
        );
    }

    #[test]
    fn consensus_target_conversion_uses_expected_compact_bits() {
        let state = init_chain_state("test".to_string());
        let expected = expected_difficulty(&state);
        let target = expected_target_u64(&state);
        assert_eq!(target, crate::target_from_compact(expected));
    }

    #[test]
    fn parent_scoped_expected_bits_match_selected_tip_snapshot() {
        let state = state_with_fixed_interval_tip(0x1f00_ffff, 60, 25);
        let tip = state
            .dag
            .selected_chain
            .last()
            .expect("selected chain has a tip");

        assert_eq!(
            expected_difficulty_for_parent(&state, tip),
            Some(expected_difficulty(&state))
        );
    }

    #[test]
    fn parent_scoped_expected_bits_follow_side_branch_metadata() {
        let mut state = state_with_fixed_interval_tip(0x1f00_ffff, 60, 25);
        let mut parent = state.dag.genesis_hash.clone();
        let start = 1_800_000_000;
        for height in 1..=4 {
            parent = append_side_header_only_block(
                &mut state,
                parent,
                height,
                start + height,
                CONSENSUS_POW_LIMIT_BITS,
            );
        }

        let side_expected = expected_difficulty_for_parent(&state, &parent)
            .expect("side parent should have a DAG-only retarget window");
        assert_ne!(side_expected, expected_difficulty(&state));
        assert!(target_from_bits(side_expected) < consensus_pow_limit_target());
    }

    #[test]
    fn parent_scoped_expected_bits_tolerate_pruned_ancestor_tail() {
        let mut state = state_with_fixed_interval_tip(0x1f00_ffff, 60, 4);
        let tip = state
            .dag
            .selected_chain
            .last()
            .cloned()
            .expect("selected chain has a tip");
        state
            .dag
            .selected_parents
            .insert(tip.clone(), Some("pruned-selected-ancestor".to_string()));

        assert!(expected_difficulty_for_parent(&state, &tip).is_some());
    }

    #[test]
    fn stable_sixty_second_regime_keeps_current_bits() {
        let bits = 0x1f00_ffff;
        let state = state_with_fixed_interval_tip(bits, 60, 25);
        let snapshot = consensus_difficulty_snapshot(&state);

        assert_eq!(snapshot.current_bits, bits);
        assert_eq!(snapshot.expected_bits, bits);
        assert_eq!(snapshot.avg_block_interval_secs, 60);
        assert_eq!(snapshot.retarget_multiplier_bps, BPS_DENOMINATOR);
        assert_eq!(snapshot.target_multiplier_bps, BPS_DENOMINATOR);
        assert!(!snapshot.target_was_clamped_to_pow_limit);
    }

    #[test]
    fn easiest_target_hardens_when_blocks_are_fast() {
        let state = state_with_fixed_interval_tip(CONSENSUS_POW_LIMIT_BITS, 1, 25);
        let snapshot = consensus_difficulty_snapshot(&state);
        let expected = target_from_bits(snapshot.expected_bits);
        let pow_limit = consensus_pow_limit_target();

        assert!(expected < pow_limit);
        assert_ne!(snapshot.expected_bits, CONSENSUS_POW_LIMIT_BITS);
        assert_eq!(snapshot.target_multiplier_bps, 8_000);
    }

    #[test]
    fn zero_second_intervals_harden_instead_of_falling_back_to_target() {
        let state = state_with_fixed_interval_tip(CONSENSUS_POW_LIMIT_BITS, 0, 25);
        let snapshot = consensus_difficulty_snapshot(&state);

        assert_eq!(snapshot.avg_block_interval_secs, 1);
        assert_eq!(snapshot.retarget_multiplier_bps, CONSENSUS_RETARGET_MAX_BPS);
        assert_eq!(snapshot.target_multiplier_bps, 8_000);
        assert!(snapshot.expected_bits != CONSENSUS_POW_LIMIT_BITS);
        assert!(target_from_bits(snapshot.expected_bits) < consensus_pow_limit_target());
    }

    #[test]
    fn legacy_difficulty_one_is_not_an_absorbing_state() {
        let state = state_with_fixed_interval_tip(1, 1, 25);
        let snapshot = consensus_difficulty_snapshot(&state);
        let expected = target_from_bits(snapshot.expected_bits);

        assert_ne!(snapshot.expected_bits, 1);
        assert!(expected < consensus_pow_limit_target());
        assert!(snapshot.target_was_clamped_to_pow_limit);
    }

    #[test]
    fn slow_blocks_relax_target_without_exceeding_pow_limit() {
        let bits = 0x1f00_ffff;
        let state = state_with_fixed_interval_tip(bits, 180, 25);
        let snapshot = consensus_difficulty_snapshot(&state);
        let current = target_from_bits(bits);
        let expected = target_from_bits(snapshot.expected_bits);

        assert!(expected > current);
        assert!(expected <= consensus_pow_limit_target());
        assert_eq!(snapshot.target_multiplier_bps, 12_500);
    }

    #[test]
    fn genesis_timestamp_zero_is_excluded_from_interval_signal() {
        let mut state = init_chain_state("test".to_string());
        append_header_only_block(&mut state, 1, 1_700_000_000, CONSENSUS_POW_LIMIT_BITS);
        let one = consensus_difficulty_snapshot(&state);
        assert_eq!(one.observed_block_count, 1);
        assert_eq!(
            one.avg_block_interval_secs,
            CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );

        append_header_only_block(&mut state, 2, 1_700_000_010, CONSENSUS_POW_LIMIT_BITS);
        let two = consensus_difficulty_snapshot(&state);
        assert_eq!(two.avg_block_interval_secs, 10);
    }

    #[test]
    fn side_branch_blocks_do_not_change_selected_chain_retarget() {
        let mut state = state_with_fixed_interval_tip(0x1f00_ffff, 60, 25);
        let baseline = consensus_difficulty_snapshot(&state);

        let side_hash = "retarget-side-branch".to_string();
        state.dag.blocks.insert(
            side_hash.clone(),
            Block {
                hash: side_hash,
                header: BlockHeader {
                    version: 1,
                    parents: vec![state.dag.genesis_hash.clone()],
                    timestamp: 9_999_999_999,
                    difficulty: CONSENSUS_POW_LIMIT_BITS,
                    nonce: 0,
                    merkle_root: "side-merkle".to_string(),
                    state_root: "side-state".to_string(),
                    blue_score: 10_000,
                    height: 10_000,
                },
                transactions: Vec::new(),
            },
        );

        let after = consensus_difficulty_snapshot(&state);
        assert_eq!(
            after.avg_block_interval_secs,
            baseline.avg_block_interval_secs
        );
        assert_eq!(after.expected_bits, baseline.expected_bits);
    }

    #[test]
    fn target_scaling_is_bounded_and_deterministic() {
        let target = consensus_pow_limit_target();
        let (first, first_overflow) = scale_target_ratio(&target, 8_000, 10_000);
        let (second, second_overflow) = scale_target_ratio(&target, 8_000, 10_000);

        assert_eq!(first, second);
        assert_eq!(first_overflow, second_overflow);
        assert!(!first_overflow);
        assert!(first < target);
    }
}
