//! Dormant, nonce-free v3 nanosecond mining-template foundation.
//!
//! This only assembles a structurally valid v3 header from an independently
//! authenticated DAG parent context and frozen 500ms retarget policy. The
//! all-zero state root is a placeholder, NOT a validated state commitment.
//! No live mining, mined/P2P admission, storage, RPC or startup calls this.
//! In particular this must never be interpreted as permission to mine.

use std::collections::BTreeMap;

use crate::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    errors::PulseError,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    pow_v3::validate_v3_pow_target,
    protocol::BLOCK_HEADER_VERSION_V3,
    retarget_context_v3::expected_difficulty_for_selected_parent_v3_ns,
    retarget_v3::validate_candidate_timestamp_v3_ns,
    types::{compute_merkle_root, BlockId, Transaction},
};

/// A pre-state, pre-PoW template. Its state root and nonce are NOT authoritative.
/// No conversion into a mined block or "ready" token is supplied.
#[derive(Debug, Clone)]
pub struct DormantV3PreNonceTemplate {
    envelope: BlockEnvelopeV3,
    selected_parent: BlockId,
}

impl DormantV3PreNonceTemplate {
    pub fn envelope(&self) -> &BlockEnvelopeV3 {
        &self.envelope
    }

    pub fn selected_parent(&self) -> &BlockId {
        &self.selected_parent
    }

    /// State replay, selected-parent blue-work authority, and PoW are absent.
    pub fn ready_for_nonce_search(&self) -> bool {
        false
    }
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("dormant v3 mining template: {}", message.into()))
}

/// Assemble an UNACTIVATED nanosecond v3 candidate before state-root finalization.
///
/// The parent headers, selected-parent links and blue_score MUST be derived
/// from independently verified canonical v3 consensus metadata. This helper
/// checks cryptographic parent linkage and time/difficulty consistency, but
/// does not authenticate DAG blue work, historical PoW, monetary rewards,
/// transaction signatures, state replay or frozen network/genesis identity.
/// Do not feed this result to active miners or P2P acceptance.
#[allow(clippy::too_many_arguments)]
pub fn build_dormant_v3_pre_nonce_template(
    chain_id: &str,
    parents: Vec<BlockId>,
    selected_parent: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    selected_parents: &BTreeMap<BlockId, Option<BlockId>>,
    timestamp_ns: u64,
    now_ns: u64,
    blue_score: u64,
    transactions: Vec<Transaction>,
) -> Result<DormantV3PreNonceTemplate, PulseError> {
    if parents.is_empty() || !parents.contains(selected_parent) {
        return Err(invalid(
            "non-genesis candidate requires its selected parent in the parent set",
        ));
    }
    if parents.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid("parents must be unique and canonically sorted"));
    }

    let mut highest_parent = 0u64;
    let mut newest_timestamp = 0u64;
    for parent in &parents {
        let header = headers
            .get(parent)
            .ok_or_else(|| invalid(format!("missing authenticated parent header {parent}")))?;
        if compute_block_hash_v3(header, chain_id)? != *parent {
            return Err(invalid(format!(
                "parent {parent} is not bound to its v3 header and chain"
            )));
        }
        validate_v3_pow_target(header.difficulty)?;
        highest_parent = highest_parent.max(header.height);
        newest_timestamp = newest_timestamp.max(header.timestamp_ns);
    }
    let height = highest_parent
        .checked_add(1)
        .ok_or_else(|| invalid("candidate height overflow"))?;
    validate_candidate_timestamp_v3_ns(timestamp_ns, newest_timestamp, now_ns)
        .map_err(|error| invalid(format!("candidate nanosecond timestamp: {error}")))?;
    let expected = expected_difficulty_for_selected_parent_v3_ns(
        chain_id,
        selected_parent,
        headers,
        selected_parents,
    )
    .map_err(|error| invalid(format!("selected-parent retarget: {error}")))?;
    validate_v3_pow_target(expected.expected_bits)?;

    let header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: expected.expected_bits,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        // Placeholder only. An authoritative v3 replay must supply and
        // validate the final state root before mining can ever be enabled.
        state_root: "00".repeat(32),
        blue_score,
        height,
    };
    let envelope = build_block_envelope_v3(chain_id, header, transactions)?;
    Ok(DormantV3PreNonceTemplate {
        envelope,
        selected_parent: selected_parent.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::compute_merkle_root;

    const CHAIN: &str = "pulsedag-v3-pre-nonce-test";
    const BASE_NS: u64 = 1_800_000_000_000_000_000;
    const BITS: u32 = 0x207f_ffff;

    fn genesis_context() -> (
        BTreeMap<BlockId, BlockHeaderV3>,
        BTreeMap<BlockId, Option<BlockId>>,
        BlockId,
    ) {
        let genesis = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![],
            timestamp_ns: BASE_NS,
            difficulty: BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32),
            blue_score: 0,
            height: 0,
        };
        let hash = compute_block_hash_v3(&genesis, CHAIN).unwrap();
        let mut headers = BTreeMap::new();
        headers.insert(hash.clone(), genesis);
        let mut selected = BTreeMap::new();
        selected.insert(hash.clone(), None);
        (headers, selected, hash)
    }

    #[test]
    fn constructs_only_a_dormant_nanosecond_template() {
        let (headers, selected, genesis) = genesis_context();
        let timestamp = BASE_NS + 500_000_000;
        let template = build_dormant_v3_pre_nonce_template(
            CHAIN,
            vec![genesis.clone()],
            &genesis,
            &headers,
            &selected,
            timestamp,
            timestamp,
            1,
            vec![],
        )
        .unwrap();
        assert!(!template.ready_for_nonce_search());
        assert_eq!(template.selected_parent(), &genesis);
        assert_eq!(template.envelope().header.height, 1);
        assert_eq!(template.envelope().header.version, BLOCK_HEADER_VERSION_V3);
        assert_eq!(template.envelope().header.timestamp_ns, timestamp);
        assert_eq!(template.envelope().header.difficulty, BITS);
        assert_eq!(template.envelope().header.nonce, 0);
        assert_eq!(template.envelope().header.state_root, "00".repeat(32));
        assert_eq!(
            template.envelope().hash,
            compute_block_hash_v3(&template.envelope().header, CHAIN).unwrap()
        );
    }

    #[test]
    fn rejects_untrusted_or_missing_selected_parent_context() {
        let (mut headers, mut selected, genesis) = genesis_context();
        let timestamp = BASE_NS + 500_000_000;
        let prepare = |headers: &BTreeMap<BlockId, BlockHeaderV3>,
                       selected: &BTreeMap<BlockId, Option<BlockId>>,
                       parents: Vec<BlockId>,
                       chosen: &BlockId| {
            build_dormant_v3_pre_nonce_template(
                CHAIN,
                parents,
                chosen,
                headers,
                selected,
                timestamp,
                timestamp,
                1,
                vec![],
            )
        };
        assert!(prepare(&headers, &selected, vec![], &genesis).is_err());
        assert!(prepare(&headers, &selected, vec![genesis.clone()], &"ff".repeat(32)).is_err());
        assert!(prepare(
            &headers,
            &selected,
            vec![genesis.clone(), genesis.clone()],
            &genesis
        )
        .is_err());
        let old = headers.get_mut(&genesis).unwrap();
        old.nonce += 1;
        assert!(prepare(&headers, &selected, vec![genesis.clone()], &genesis).is_err());
        let (mut headers, _, genesis) = genesis_context();
        selected.clear();
        assert!(prepare(&headers, &selected, vec![genesis.clone()], &genesis).is_err());
        headers.remove(&genesis);
        assert!(prepare(&headers, &selected, vec![genesis.clone()], &genesis).is_err());
    }

    #[test]
    fn rejects_stale_future_and_cross_chain_templates() {
        let (headers, selected, genesis) = genesis_context();
        let prepare = |chain: &str, timestamp: u64, now: u64| {
            build_dormant_v3_pre_nonce_template(
                chain,
                vec![genesis.clone()],
                &genesis,
                &headers,
                &selected,
                timestamp,
                now,
                1,
                vec![],
            )
        };
        assert!(prepare(CHAIN, BASE_NS, BASE_NS).is_err());
        assert!(prepare(CHAIN, BASE_NS + 2_000_000_000, BASE_NS).is_err());
        assert!(prepare("another-v3-chain", BASE_NS + 500_000_000, BASE_NS).is_err());
    }
}
