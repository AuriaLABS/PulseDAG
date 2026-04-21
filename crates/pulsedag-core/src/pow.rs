use crate::{state::ChainState, types::BlockHeader};

fn read_env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 1)
        .unwrap_or(default)
}

fn read_env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PowAlgorithm {
    KHeavyHash,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevDifficultyPolicy {
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub use_median: bool,
    pub max_future_drift_secs: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevDifficultySnapshot {
    pub algorithm: &'static str,
    pub best_height: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    pub current_difficulty: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub retarget_multiplier_bps: u64,
    pub policy: DevDifficultyPolicy,
}

pub fn selected_pow_algorithm() -> PowAlgorithm {
    PowAlgorithm::KHeavyHash
}

pub fn selected_pow_name() -> &'static str {
    match selected_pow_algorithm() {
        PowAlgorithm::KHeavyHash => "kHeavyHash",
    }
}

pub fn pow_preimage_string(header: &BlockHeader) -> String {
    format!(
        "v={}|parents={}|ts={}|difficulty={}|nonce={}|merkle={}|state={}|blue={}|height={}",
        header.version,
        header.parents.join(","),
        header.timestamp,
        header.difficulty,
        header.nonce,
        header.merkle_root,
        header.state_root,
        header.blue_score,
        header.height,
    )
}

pub fn dev_surrogate_pow_hash(header: &BlockHeader) -> String {
    let preimage = pow_preimage_string(header);
    blake3::hash(preimage.as_bytes()).to_hex().to_string()
}

pub fn dev_target_u64(difficulty: u64) -> u64 {
    let difficulty = difficulty.max(1);
    u64::MAX / difficulty
}

pub fn dev_hash_score_u64(header: &BlockHeader) -> u64 {
    let hash = dev_surrogate_pow_hash(header);
    let prefix = &hash[..16.min(hash.len())];
    u64::from_str_radix(prefix, 16).unwrap_or(u64::MAX)
}

pub fn dev_pow_accepts(header: &BlockHeader) -> bool {
    dev_hash_score_u64(header) <= dev_target_u64(header.difficulty.into())
}

pub fn dev_mine_header(
    mut header: BlockHeader,
    max_tries: u64,
) -> (BlockHeader, bool, u64, String) {
    let tries = max_tries.max(1);
    for i in 0..tries {
        header.nonce = i;
        let hash_hex = dev_surrogate_pow_hash(&header);
        if dev_pow_accepts(&header) {
            return (header, true, i + 1, hash_hex);
        }
    }
    let hash_hex = dev_surrogate_pow_hash(&header);
    (header, false, tries, hash_hex)
}

pub const DEV_TARGET_BLOCK_INTERVAL_SECS: u64 = 60;
pub const DEV_DIFFICULTY_WINDOW: usize = 20;
pub const DEV_MAX_FUTURE_DRIFT_SECS: u64 = 120;
pub const DEV_DIFFICULTY_USE_MEDIAN: bool = false;

pub fn dev_target_block_interval_secs() -> u64 {
    read_env_u64(
        "PULSEDAG_TARGET_BLOCK_INTERVAL_SECS",
        DEV_TARGET_BLOCK_INTERVAL_SECS,
    )
}

pub fn dev_difficulty_window() -> usize {
    read_env_usize("PULSEDAG_DIFFICULTY_WINDOW", DEV_DIFFICULTY_WINDOW)
}

pub fn dev_difficulty_use_median() -> bool {
    read_env_bool("PULSEDAG_DIFFICULTY_USE_MEDIAN", DEV_DIFFICULTY_USE_MEDIAN)
}

pub fn dev_max_future_drift_secs() -> u64 {
    read_env_u64(
        "PULSEDAG_MAX_FUTURE_DRIFT_SECS",
        dev_target_block_interval_secs()
            .saturating_mul(2)
            .max(DEV_MAX_FUTURE_DRIFT_SECS),
    )
}

pub fn dev_base_difficulty(best_height: u64) -> u64 {
    match best_height {
        0..=9 => 1,
        10..=49 => 2,
        50..=199 => 4,
        _ => 8,
    }
}

pub fn dev_retarget_multiplier_bps(avg_block_interval_secs: u64) -> u64 {
    if avg_block_interval_secs == 0 {
        return 10_000;
    }
    let target = dev_target_block_interval_secs().max(1);
    let raw = target.saturating_mul(10_000) / avg_block_interval_secs.max(1);
    raw.clamp(5_000, 20_000)
}

pub fn dev_adjust_difficulty_for_interval(current: u64, avg_block_interval_secs: u64) -> u64 {
    if avg_block_interval_secs == 0 {
        return current.max(1);
    }
    let multiplier_bps = dev_retarget_multiplier_bps(avg_block_interval_secs);
    let adjusted = current
        .max(1)
        .saturating_mul(multiplier_bps)
        .saturating_add(5_000)
        / 10_000;
    adjusted.max(1)
}

fn recent_blocks(state: &ChainState, window_size: usize) -> Vec<&crate::types::Block> {
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

fn recent_intervals_secs(state: &ChainState, window_size: usize) -> Vec<u64> {
    let window = recent_blocks(state, window_size);
    let mut intervals = Vec::new();
    for pair in window.windows(2) {
        let newer = pair[0].header.timestamp;
        let older = pair[1].header.timestamp;
        intervals.push(newer.saturating_sub(older));
    }
    intervals
}

fn median(values: &mut [u64]) -> u64 {
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

pub fn dev_recent_avg_block_interval_secs(state: &ChainState, window_size: usize) -> u64 {
    dev_recent_block_interval_secs_with_mode(state, window_size, dev_difficulty_use_median())
}

pub fn dev_recent_block_interval_secs_with_mode(
    state: &ChainState,
    window_size: usize,
    use_median: bool,
) -> u64 {
    let mut intervals = recent_intervals_secs(state, window_size);
    if intervals.is_empty() {
        return 0;
    }
    if use_median {
        median(&mut intervals)
    } else {
        intervals.iter().copied().sum::<u64>() / (intervals.len() as u64)
    }
}

pub fn dev_recommended_difficulty(best_height: u64) -> u64 {
    dev_base_difficulty(best_height)
}

pub fn dev_current_difficulty_for_chain(state: &ChainState) -> u64 {
    state
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| u64::from(b.header.difficulty).max(1))
        .unwrap_or_else(|| dev_base_difficulty(state.dag.best_height))
}

pub fn dev_difficulty_policy() -> DevDifficultyPolicy {
    DevDifficultyPolicy {
        target_block_interval_secs: dev_target_block_interval_secs(),
        window_size: dev_difficulty_window(),
        use_median: dev_difficulty_use_median(),
        max_future_drift_secs: dev_max_future_drift_secs(),
    }
}

pub fn dev_difficulty_snapshot(state: &ChainState) -> DevDifficultySnapshot {
    let policy = dev_difficulty_policy();
    let observed_block_count = recent_blocks(state, policy.window_size).len();
    let interval =
        dev_recent_block_interval_secs_with_mode(state, policy.window_size, policy.use_median);
    let avg_block_interval_secs = if interval == 0 {
        policy.target_block_interval_secs
    } else {
        interval
    };
    let current_difficulty = dev_current_difficulty_for_chain(state);
    let retarget_multiplier_bps = dev_retarget_multiplier_bps(avg_block_interval_secs);
    let suggested_difficulty =
        dev_adjust_difficulty_for_interval(current_difficulty, avg_block_interval_secs);

    DevDifficultySnapshot {
        algorithm: selected_pow_name(),
        best_height: state.dag.best_height,
        observed_block_count,
        avg_block_interval_secs,
        current_difficulty,
        suggested_difficulty,
        target_u64: dev_target_u64(suggested_difficulty),
        retarget_multiplier_bps,
        policy,
    }
}

pub fn dev_recommended_difficulty_for_chain(state: &ChainState) -> u64 {
    dev_difficulty_snapshot(state).suggested_difficulty
}
