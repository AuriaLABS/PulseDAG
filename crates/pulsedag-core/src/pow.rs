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
    /// Canonical public-testnet PoW identifier.
    ///
    /// NOTE: the name remains `KHeavyHash` for network compatibility.
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

/// One-byte discriminant to version the serialized PoW preimage format.
pub const POW_HEADER_PREIMAGE_VERSION: u8 = 1;

pub fn selected_pow_algorithm() -> PowAlgorithm {
    PowAlgorithm::KHeavyHash
}

pub fn selected_pow_name() -> &'static str {
    match selected_pow_algorithm() {
        PowAlgorithm::KHeavyHash => "kHeavyHash",
    }
}

fn encode_len_prefixed_utf8(out: &mut Vec<u8>, value: &str) {
    let len = value.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

/// Canonical PoW header preimage bytes used by both nodes and external miners.
///
/// Field order and encoding are frozen for public testnet:
/// 1) preimage version (`u8`)
/// 2) header.version (`u32`, little-endian)
/// 3) parent count (`u16`, little-endian)
/// 4) each parent hash string as (`u16` byte length LE + UTF-8 bytes)
/// 5) header.timestamp (`u64`, little-endian)
/// 6) header.difficulty (`u32`, little-endian)
/// 7) header.nonce (`u64`, little-endian)
/// 8) header.merkle_root (`u16` length LE + UTF-8 bytes)
/// 9) header.state_root (`u16` length LE + UTF-8 bytes)
/// 10) header.blue_score (`u64`, little-endian)
/// 11) header.height (`u64`, little-endian)
pub fn pow_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.push(POW_HEADER_PREIMAGE_VERSION);
    out.extend_from_slice(&header.version.to_le_bytes());

    let parent_count = header.parents.len() as u16;
    out.extend_from_slice(&parent_count.to_le_bytes());
    for parent in &header.parents {
        encode_len_prefixed_utf8(&mut out, parent);
    }

    out.extend_from_slice(&header.timestamp.to_le_bytes());
    out.extend_from_slice(&header.difficulty.to_le_bytes());
    out.extend_from_slice(&header.nonce.to_le_bytes());
    encode_len_prefixed_utf8(&mut out, &header.merkle_root);
    encode_len_prefixed_utf8(&mut out, &header.state_root);
    out.extend_from_slice(&header.blue_score.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    out
}

/// Debug-oriented helper string that mirrors canonical field order.
pub fn pow_preimage_string(header: &BlockHeader) -> String {
    format!(
        "pv={}|v={}|parents={}|ts={}|difficulty={}|nonce={}|merkle={}|state={}|blue={}|height={}",
        POW_HEADER_PREIMAGE_VERSION,
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
    let preimage = pow_preimage_bytes(header);
    blake3::hash(&preimage).to_hex().to_string()
}

pub fn dev_target_u64(difficulty: u64) -> u64 {
    let difficulty = difficulty.max(1);
    u64::MAX / difficulty
}

pub fn dev_hash_score_u64(header: &BlockHeader) -> u64 {
    let hash_bytes = blake3::hash(&pow_preimage_bytes(header));
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&hash_bytes.as_bytes()[..8]);
    u64::from_be_bytes(prefix)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> BlockHeader {
        BlockHeader {
            version: 1,
            parents: vec!["aa".to_string(), "bb".to_string()],
            timestamp: 1_700_000_000,
            difficulty: 4,
            nonce: 42,
            merkle_root: "merkle-10".to_string(),
            state_root: "state-10".to_string(),
            blue_score: 10,
            height: 10,
        }
    }

    #[test]
    fn preimage_is_stable_and_nonce_sensitive() {
        let mut h1 = sample_header();
        let mut h2 = sample_header();
        h2.nonce = h1.nonce + 1;

        let p1 = pow_preimage_bytes(&h1);
        let p2 = pow_preimage_bytes(&h2);
        assert_ne!(p1, p2, "nonce must change preimage");

        h1.nonce = h2.nonce;
        assert_eq!(pow_preimage_bytes(&h1), p2, "same header => same preimage");
    }

    #[test]
    fn hash_score_uses_big_endian_prefix() {
        let h = sample_header();
        let hash = blake3::hash(&pow_preimage_bytes(&h));
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        let expected = u64::from_be_bytes(bytes);
        assert_eq!(dev_hash_score_u64(&h), expected);
    }

    #[test]
    fn acceptance_rule_matches_target_rule() {
        let h = sample_header();
        let target = dev_target_u64(h.difficulty as u64);
        let score = dev_hash_score_u64(&h);
        assert_eq!(dev_pow_accepts(&h), score <= target);
    }
}
