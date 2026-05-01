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
    pub retarget_min_bps: u64,
    pub retarget_max_bps: u64,
    pub retarget_was_clamped: bool,
    pub retarget_rationale: String,
    pub retarget_signal_quality: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowHeaderPreimage<'a> {
    pub version: u32,
    pub parents: &'a [String],
    pub timestamp: u64,
    pub difficulty: u32,
    pub nonce: u64,
    pub merkle_root: &'a str,
    pub state_root: &'a str,
    pub blue_score: u64,
    pub height: u64,
}

impl<'a> PowHeaderPreimage<'a> {
    pub fn from_header(header: &'a BlockHeader) -> Self {
        Self {
            version: header.version,
            parents: &header.parents,
            timestamp: header.timestamp,
            difficulty: header.difficulty,
            nonce: header.nonce,
            merkle_root: &header.merkle_root,
            state_root: &header.state_root,
            blue_score: header.blue_score,
            height: header.height,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes_checked()
            .expect("validated PoW preimage encoding")
    }

    pub fn to_bytes_checked(&self) -> Result<Vec<u8>, PowRejectReason> {
        validate_pow_preimage_encoding_view(self)?;
        let mut out = Vec::with_capacity(256);
        out.push(POW_HEADER_PREIMAGE_VERSION);
        out.extend_from_slice(&self.version.to_le_bytes());

        let parent_count = self.parents.len() as u16;
        out.extend_from_slice(&parent_count.to_le_bytes());
        for parent in self.parents {
            encode_len_prefixed_utf8(&mut out, parent);
        }

        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out.extend_from_slice(&self.difficulty.to_le_bytes());
        out.extend_from_slice(&self.nonce.to_le_bytes());
        encode_len_prefixed_utf8(&mut out, self.merkle_root);
        encode_len_prefixed_utf8(&mut out, self.state_root);
        out.extend_from_slice(&self.blue_score.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        Ok(out)
    }

    pub fn to_debug_string(&self) -> String {
        format!(
            "pv={}|v={}|parents={}|ts={}|difficulty={}|nonce={}|merkle={}|state={}|blue={}|height={}",
            POW_HEADER_PREIMAGE_VERSION,
            self.version,
            self.parents.join(","),
            self.timestamp,
            self.difficulty,
            self.nonce,
            self.merkle_root,
            self.state_root,
            self.blue_score,
            self.height,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowEvaluation {
    pub algorithm: PowAlgorithm,
    pub hash_hex: String,
    pub score_u64: u64,
    pub target_u64: u64,
    pub accepted: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PowRejectReason {
    ParentCountTooLarge,
    ParentHashTooLong,
    MerkleRootTooLong,
    StateRootTooLong,
    ScoreAboveTarget,
}

impl PowRejectReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::ParentCountTooLarge => "parent_count_too_large",
            Self::ParentHashTooLong => "parent_hash_too_long",
            Self::MerkleRootTooLong => "merkle_root_too_long",
            Self::StateRootTooLong => "state_root_too_long",
            Self::ScoreAboveTarget => "score_above_target",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PowValidationResult {
    pub algorithm: &'static str,
    pub accepted: bool,
    pub hash_hex: Option<String>,
    pub score_u64: Option<u64>,
    pub target_u64: u64,
    pub difficulty: u32,
    pub rejection_code: Option<&'static str>,
    pub rejection_reason: Option<PowRejectReason>,
}

pub trait PowEngine {
    fn algorithm(&self) -> PowAlgorithm;
    fn hash_preimage_hex(&self, preimage: &[u8]) -> String;
    fn score_preimage_u64(&self, preimage: &[u8]) -> u64;
    fn target_u64(&self, difficulty: u64) -> u64 {
        let difficulty = difficulty.max(1);
        u64::MAX / difficulty
    }
    fn evaluate_preimage(&self, preimage: &[u8], difficulty: u64) -> PowEvaluation {
        let hash_hex = self.hash_preimage_hex(preimage);
        let score_u64 = self.score_preimage_u64(preimage);
        let target_u64 = self.target_u64(difficulty);
        PowEvaluation {
            algorithm: self.algorithm(),
            hash_hex,
            score_u64,
            target_u64,
            accepted: score_u64 <= target_u64,
        }
    }
    fn evaluate_header(&self, header: &BlockHeader) -> PowEvaluation {
        match PowHeaderPreimage::from_header(header).to_bytes_checked() {
            Ok(preimage) => self.evaluate_preimage(&preimage, header.difficulty as u64),
            Err(_) => PowEvaluation {
                algorithm: self.algorithm(),
                hash_hex: String::new(),
                score_u64: u64::MAX,
                target_u64: self.target_u64(header.difficulty as u64),
                accepted: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CanonicalPowEngine;

impl PowEngine for CanonicalPowEngine {
    fn algorithm(&self) -> PowAlgorithm {
        PowAlgorithm::KHeavyHash
    }

    fn hash_preimage_hex(&self, preimage: &[u8]) -> String {
        blake3::hash(preimage).to_hex().to_string()
    }

    fn score_preimage_u64(&self, preimage: &[u8]) -> u64 {
        let hash = blake3::hash(preimage);
        let mut prefix = [0u8; 8];
        prefix.copy_from_slice(&hash.as_bytes()[..8]);
        u64::from_be_bytes(prefix)
    }
}

pub fn canonical_pow_engine() -> CanonicalPowEngine {
    CanonicalPowEngine
}

fn encode_len_prefixed_utf8(out: &mut Vec<u8>, value: &str) {
    let len = value.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn validate_pow_preimage_encoding_view(
    header: &PowHeaderPreimage<'_>,
) -> Result<(), PowRejectReason> {
    if header.parents.len() > u16::MAX as usize {
        return Err(PowRejectReason::ParentCountTooLarge);
    }
    if header.parents.iter().any(|p| p.len() > u16::MAX as usize) {
        return Err(PowRejectReason::ParentHashTooLong);
    }
    if header.merkle_root.len() > u16::MAX as usize {
        return Err(PowRejectReason::MerkleRootTooLong);
    }
    if header.state_root.len() > u16::MAX as usize {
        return Err(PowRejectReason::StateRootTooLong);
    }
    Ok(())
}

pub fn validate_pow_preimage_encoding(header: &BlockHeader) -> Result<(), PowRejectReason> {
    validate_pow_preimage_encoding_view(&PowHeaderPreimage::from_header(header))
}

pub fn validate_pow_header(header: &BlockHeader) -> Result<(), PowRejectReason> {
    let result = pow_validation_result(header);
    if result.accepted {
        Ok(())
    } else {
        Err(result
            .rejection_reason
            .unwrap_or(PowRejectReason::ScoreAboveTarget))
    }
}

pub fn pow_validation_result(header: &BlockHeader) -> PowValidationResult {
    let target_u64 = pow_target_u64(header.difficulty as u64);
    if let Err(reason) = validate_pow_preimage_encoding(header) {
        return PowValidationResult {
            algorithm: selected_pow_name(),
            accepted: false,
            hash_hex: None,
            score_u64: None,
            target_u64,
            difficulty: header.difficulty,
            rejection_code: Some(reason.code()),
            rejection_reason: Some(reason),
        };
    }
    let evaluation = pow_evaluate(header);
    let rejection_reason = if evaluation.accepted {
        None
    } else {
        Some(PowRejectReason::ScoreAboveTarget)
    };
    PowValidationResult {
        algorithm: selected_pow_name(),
        accepted: evaluation.accepted,
        hash_hex: Some(evaluation.hash_hex),
        score_u64: Some(evaluation.score_u64),
        target_u64: evaluation.target_u64,
        difficulty: header.difficulty,
        rejection_code: rejection_reason.map(|r| r.code()),
        rejection_reason,
    }
}
/// Canonical PoW header preimage bytes used by both nodes and external miners.
///
/// v2.2.8 PoW hardening note: this freezes deterministic hashing inputs only;
/// it is not a final production difficulty-adjustment scheme.
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
    PowHeaderPreimage::from_header(header)
        .to_bytes_checked()
        .unwrap_or_default()
}

/// Debug-oriented helper string that mirrors canonical field order.
pub fn pow_preimage_string(header: &BlockHeader) -> String {
    PowHeaderPreimage::from_header(header).to_debug_string()
}

pub fn pow_hash(header: &BlockHeader) -> [u8; 32] {
    blake3::hash(&pow_preimage_bytes(header)).into()
}

pub fn pow_hash_hex(header: &BlockHeader) -> String {
    canonical_pow_engine().evaluate_header(header).hash_hex
}

pub fn target_from_compact(compact_difficulty: u32) -> u64 {
    pow_target_u64(compact_difficulty as u64)
}

pub fn compact_from_target(target_u64: u64) -> u32 {
    if target_u64 == 0 {
        return u32::MAX;
    }
    let difficulty = (u64::MAX / target_u64).max(1);
    difficulty.min(u32::MAX as u64) as u32
}

pub fn pow_target_u64(difficulty: u64) -> u64 {
    canonical_pow_engine().target_u64(difficulty)
}

pub fn verify_work(header: &BlockHeader) -> bool {
    let score = pow_hash_score_u64(header);
    let target = target_from_compact(header.difficulty);
    score <= target
}

pub fn pow_hash_score_u64(header: &BlockHeader) -> u64 {
    canonical_pow_engine().evaluate_header(header).score_u64
}

pub fn pow_accepts(header: &BlockHeader) -> bool {
    canonical_pow_engine().evaluate_header(header).accepted
}

pub fn pow_evaluate(header: &BlockHeader) -> PowEvaluation {
    canonical_pow_engine().evaluate_header(header)
}

pub fn mine_header(mut header: BlockHeader, max_tries: u64) -> (BlockHeader, bool, u64, String) {
    let tries = max_tries.max(1);
    for i in 0..tries {
        header.nonce = i;
        let evaluation = pow_evaluate(&header);
        if evaluation.accepted {
            return (header, true, i + 1, evaluation.hash_hex);
        }
    }
    let evaluation = pow_evaluate(&header);
    (header, false, tries, evaluation.hash_hex)
}

pub fn dev_surrogate_pow_hash(header: &BlockHeader) -> String {
    pow_hash_hex(header)
}

pub fn dev_target_u64(difficulty: u64) -> u64 {
    pow_target_u64(difficulty)
}

pub fn dev_hash_score_u64(header: &BlockHeader) -> u64 {
    pow_hash_score_u64(header)
}

pub fn dev_pow_accepts(header: &BlockHeader) -> bool {
    pow_accepts(header)
}

pub fn dev_mine_header(header: BlockHeader, max_tries: u64) -> (BlockHeader, bool, u64, String) {
    mine_header(header, max_tries)
}

pub const DEV_TARGET_BLOCK_INTERVAL_SECS: u64 = 60;
pub const DEV_DIFFICULTY_WINDOW: usize = 20;
pub const DEV_MAX_FUTURE_DRIFT_SECS: u64 = 120;
pub const DEV_DIFFICULTY_USE_MEDIAN: bool = false;
const DEV_RETARGET_DEADBAND_BPS: u64 = 800;
const DEV_RETARGET_DAMPING_DIVISOR: u64 = 2;
const DEV_RETARGET_MIN_BPS: u64 = 8_000;
const DEV_RETARGET_MAX_BPS: u64 = 12_500;
const DEV_RETARGET_MIN_BPS_FLOOR: u64 = 1_000;
const DEV_RETARGET_MAX_BPS_CEIL: u64 = 20_000;

pub fn dev_retarget_deadband_bps() -> u64 {
    read_env_u64("PULSEDAG_RETARGET_DEADBAND_BPS", DEV_RETARGET_DEADBAND_BPS).min(9_999)
}

pub fn dev_retarget_damping_divisor() -> u64 {
    read_env_u64(
        "PULSEDAG_RETARGET_DAMPING_DIVISOR",
        DEV_RETARGET_DAMPING_DIVISOR,
    )
}

pub fn dev_retarget_min_bps() -> u64 {
    read_env_u64("PULSEDAG_RETARGET_MIN_BPS", DEV_RETARGET_MIN_BPS)
        .clamp(DEV_RETARGET_MIN_BPS_FLOOR, 10_000)
}

pub fn dev_retarget_max_bps() -> u64 {
    let min_bps = dev_retarget_min_bps();
    read_env_u64("PULSEDAG_RETARGET_MAX_BPS", DEV_RETARGET_MAX_BPS)
        .clamp(10_000, DEV_RETARGET_MAX_BPS_CEIL)
        .max(min_bps)
}

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
    let deadband = dev_retarget_deadband_bps();
    let lower_bound = 10_000u64.saturating_sub(deadband);
    let upper_bound = 10_000u64.saturating_add(deadband);
    if (lower_bound..=upper_bound).contains(&raw) {
        return 10_000;
    }

    let deviation = raw as i64 - 10_000;
    let damped = 10_000i64 + (deviation / dev_retarget_damping_divisor() as i64);
    (damped as u64).clamp(dev_retarget_min_bps(), dev_retarget_max_bps())
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
    let retarget_min_bps = dev_retarget_min_bps();
    let retarget_max_bps = dev_retarget_max_bps();
    let retarget_multiplier_bps = dev_retarget_multiplier_bps(avg_block_interval_secs);
    let raw_multiplier_bps = policy
        .target_block_interval_secs
        .saturating_mul(10_000)
        .checked_div(avg_block_interval_secs.max(1))
        .unwrap_or(10_000);
    let suggested_difficulty =
        dev_adjust_difficulty_for_interval(current_difficulty, avg_block_interval_secs);
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
    } else if retarget_multiplier_bps == retarget_min_bps {
        "clamped_to_min".to_string()
    } else if retarget_multiplier_bps == retarget_max_bps {
        "clamped_to_max".to_string()
    } else if raw_multiplier_bps > 10_000 {
        "damped_increase".to_string()
    } else {
        "damped_decrease".to_string()
    };
    let retarget_was_clamped =
        retarget_multiplier_bps == retarget_min_bps || retarget_multiplier_bps == retarget_max_bps;

    DevDifficultySnapshot {
        algorithm: selected_pow_name(),
        best_height: state.dag.best_height,
        observed_block_count,
        avg_block_interval_secs,
        current_difficulty,
        suggested_difficulty,
        target_u64: pow_target_u64(suggested_difficulty),
        retarget_multiplier_bps,
        retarget_min_bps,
        retarget_max_bps,
        retarget_was_clamped,
        retarget_rationale,
        retarget_signal_quality,
        policy,
    }
}

pub fn dev_recommended_difficulty_for_chain(state: &ChainState) -> u64 {
    dev_difficulty_snapshot(state).suggested_difficulty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader, Transaction},
    };

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
        assert_eq!(pow_hash_score_u64(&h), expected);
    }

    #[test]
    fn acceptance_rule_matches_target_rule() {
        let h = sample_header();
        let evaluation = pow_evaluate(&h);
        assert_eq!(
            pow_accepts(&h),
            evaluation.score_u64 <= evaluation.target_u64
        );
    }

    #[test]
    fn known_header_matches_known_pow_hash() {
        let h = sample_header();
        assert_eq!(
            pow_hash_hex(&h),
            "98384a054292e340f392d2a7f53e623a26213375b10073790d2d82954fee0a89"
        );
    }

    #[test]
    fn target_accepts_when_score_is_below_threshold() {
        let mut h = sample_header();
        h.difficulty = 1;
        assert!(pow_accepts(&h));
    }

    #[test]
    fn target_rejects_when_score_is_above_threshold() {
        let mut h = sample_header();
        h.difficulty = u32::MAX;
        assert!(!pow_accepts(&h));
    }

    #[test]
    fn engine_evaluation_is_deterministic_for_same_header() {
        let h = sample_header();
        let first = pow_evaluate(&h);
        let second = pow_evaluate(&h);
        assert_eq!(first, second);
    }

    #[test]
    fn preimage_evaluation_matches_header_evaluation() {
        let h = sample_header();
        let engine = canonical_pow_engine();
        let preimage = PowHeaderPreimage::from_header(&h).to_bytes();
        let from_header = engine.evaluate_header(&h);
        let from_preimage = engine.evaluate_preimage(&preimage, h.difficulty as u64);
        assert_eq!(from_header, from_preimage);
    }

    #[test]
    fn oversized_parent_count_rejected_without_panic() {
        let mut h = sample_header();
        h.parents = vec!["p".to_string(); (u16::MAX as usize) + 1];
        let result = pow_validation_result(&h);
        assert!(!result.accepted);
        assert_eq!(
            result.rejection_reason,
            Some(PowRejectReason::ParentCountTooLarge)
        );
        assert!(pow_preimage_bytes(&h).is_empty());
        assert!(!pow_accepts(&h));
    }

    #[test]
    fn oversized_parent_hash_rejected_without_panic() {
        let mut h = sample_header();
        h.parents = vec!["x".repeat((u16::MAX as usize) + 1)];
        let result = pow_validation_result(&h);
        assert!(!result.accepted);
        assert_eq!(
            result.rejection_reason,
            Some(PowRejectReason::ParentHashTooLong)
        );
        assert!(pow_preimage_bytes(&h).is_empty());
    }

    #[test]
    fn oversized_merkle_root_rejected_without_panic() {
        let mut h = sample_header();
        h.merkle_root = "m".repeat((u16::MAX as usize) + 1);
        let result = pow_validation_result(&h);
        assert!(!result.accepted);
        assert_eq!(
            result.rejection_reason,
            Some(PowRejectReason::MerkleRootTooLong)
        );
        assert!(pow_preimage_bytes(&h).is_empty());
    }

    #[test]
    fn oversized_state_root_rejected_without_panic() {
        let mut h = sample_header();
        h.state_root = "s".repeat((u16::MAX as usize) + 1);
        let result = pow_validation_result(&h);
        assert!(!result.accepted);
        assert_eq!(
            result.rejection_reason,
            Some(PowRejectReason::StateRootTooLong)
        );
        assert!(pow_preimage_bytes(&h).is_empty());
    }

    fn append_block(
        state: &mut crate::state::ChainState,
        height: u64,
        timestamp: u64,
        difficulty: u32,
    ) {
        let hash = format!("block-{height}");
        let parent = if height == 1 {
            state.dag.genesis_hash.clone()
        } else {
            format!("block-{}", height - 1)
        };

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
                transactions: Vec::<Transaction>::new(),
            },
        );
        state.dag.best_height = height;
        state.dag.tips.clear();
        state.dag.tips.insert(hash);
    }

    fn build_chain_with_intervals(
        intervals_secs: &[u64],
        difficulty: u32,
    ) -> crate::state::ChainState {
        let mut state = init_chain_state("pow-test".into());
        let mut timestamp = 0u64;
        for (idx, interval) in intervals_secs.iter().enumerate() {
            timestamp = timestamp.saturating_add(*interval);
            append_block(&mut state, (idx + 1) as u64, timestamp, difficulty);
        }
        state
    }

    #[test]
    fn difficulty_rises_on_sudden_hashpower_increase() {
        let mut intervals = vec![60; 12];
        intervals.extend(vec![15; 8]);
        let chain = build_chain_with_intervals(&intervals, 100);

        let suggested = dev_recommended_difficulty_for_chain(&chain);
        assert!(
            suggested > 100,
            "expected increased difficulty, got {suggested}"
        );
        assert!(
            suggested <= 125,
            "single retarget should remain bounded, got {suggested}"
        );
    }

    #[test]
    fn difficulty_drops_on_sudden_hashpower_drop() {
        let mut intervals = vec![60; 12];
        intervals.extend(vec![180; 8]);
        let chain = build_chain_with_intervals(&intervals, 100);

        let suggested = dev_recommended_difficulty_for_chain(&chain);
        assert!(
            suggested < 100,
            "expected decreased difficulty, got {suggested}"
        );
        assert!(
            suggested >= 80,
            "single retarget should remain bounded, got {suggested}"
        );
    }

    #[test]
    fn stable_regime_stays_near_current_difficulty() {
        let chain = build_chain_with_intervals(&vec![60; 20], 100);
        assert_eq!(dev_recommended_difficulty_for_chain(&chain), 100);

        let near_target_fast = build_chain_with_intervals(&vec![56; 20], 100);
        assert_eq!(dev_recommended_difficulty_for_chain(&near_target_fast), 100);

        let near_target_slow = build_chain_with_intervals(&vec![64; 20], 100);
        assert_eq!(dev_recommended_difficulty_for_chain(&near_target_slow), 100);
    }

    #[test]
    fn alternating_intervals_do_not_cause_extreme_oscillation() {
        let mut difficulty = 100u64;
        let mut observed = Vec::new();
        for i in 0..32 {
            let interval = if i % 2 == 0 { 30 } else { 120 };
            difficulty = dev_adjust_difficulty_for_interval(difficulty, interval);
            observed.push(difficulty);
        }

        let min = *observed.iter().min().unwrap();
        let max = *observed.iter().max().unwrap();
        assert!(max <= 130, "expected bounded upside, got {max}");
        assert!(min >= 80, "expected bounded downside, got {min}");
    }

    #[test]
    fn retarget_bounds_and_determinism_hold() {
        let chain = build_chain_with_intervals(&vec![10; 20], 200);
        let first = dev_difficulty_snapshot(&chain);
        let second = dev_difficulty_snapshot(&chain);
        assert_eq!(
            first.retarget_multiplier_bps,
            second.retarget_multiplier_bps
        );
        assert_eq!(first.suggested_difficulty, second.suggested_difficulty);
        assert!(first.retarget_multiplier_bps >= first.retarget_min_bps);
        assert!(first.retarget_multiplier_bps <= first.retarget_max_bps);
    }

    #[test]
    fn low_signal_snapshot_uses_explicit_diagnostics() {
        let chain = build_chain_with_intervals(&[60], 100);
        let snapshot = dev_difficulty_snapshot(&chain);
        assert_eq!(snapshot.retarget_signal_quality, "low");
        assert_eq!(snapshot.retarget_rationale, "insufficient_signal");
        assert_eq!(snapshot.retarget_multiplier_bps, 10_000);
        assert!(!snapshot.retarget_was_clamped);
    }

    #[test]
    fn valid_nonce_passes_and_invalid_nonce_fails() {
        let mut h = sample_header();
        h.difficulty = 10_000;
        let mut found = None;
        for nonce in 0..500_000u64 {
            h.nonce = nonce;
            if verify_work(&h) {
                let mut next = h.clone();
                next.nonce = nonce.saturating_add(1);
                if !verify_work(&next) {
                    found = Some((h.clone(), next));
                    break;
                }
            }
        }
        let (valid, invalid) = found.expect("must find valid nonce with invalid successor");
        assert!(verify_work(&valid));
        assert!(!verify_work(&invalid));
    }

    #[test]
    fn mutating_header_after_mining_invalidates_pow() {
        let mut mined = sample_header();
        mined.difficulty = 10_000;
        let mut found = None;
        for nonce in 0..500_000u64 {
            mined.nonce = nonce;
            if verify_work(&mined) {
                found = Some(mined.clone());
                break;
            }
        }
        let mined = found.expect("must find valid work");
        assert!(verify_work(&mined));

        let mut mutated = mined.clone();
        mutated.merkle_root.push_str("-tampered");
        assert!(!verify_work(&mutated));
    }

    #[test]
    fn verify_work_matches_boundary_semantics() {
        let h = sample_header();
        let score = pow_hash_score_u64(&h);
        let exact = compact_from_target(score);

        let mut pass = h.clone();
        pass.difficulty = exact;
        assert!(verify_work(&pass));

        if exact > 1 {
            let mut fail = h;
            fail.difficulty = exact.saturating_add(1);
            assert!(!verify_work(&fail));
        }
    }

    #[test]
    fn different_nonce_changes_pow_hash_bytes() {
        let mut h1 = sample_header();
        let mut h2 = sample_header();
        h2.nonce = h1.nonce.saturating_add(77);
        assert_ne!(pow_hash(&h1), pow_hash(&h2));

        h1.nonce = h2.nonce;
        assert_eq!(pow_hash(&h1), pow_hash(&h2));
    }
}
