//! Dormant v3 nanosecond candidate context preflight.
//!
//! This composes existing v3 block-envelope, selected-parent retarget,
//! nanosecond timestamp and KHeavyHash primitives. It does NOT independently
//! derive or authenticate selected-parent/blue-work authority, transaction
//! signatures, UTXO/reward state or protocol activation. Therefore passing
//! this function is never authorization for mined/P2P admission, persistence,
//! or block broadcast. The production daemon startup gate stays fail-closed.

use std::collections::BTreeMap;

use crate::{
    block_envelope_v3::{validate_block_envelope_v3, BlockEnvelopeV3},
    errors::PulseError,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    pow_v3::{validate_v3_pow_target, verify_v3_header_pow},
    retarget_context_v3::expected_difficulty_for_selected_parent_v3_ns,
    retarget_v3::validate_candidate_timestamp_v3_ns,
    types::BlockId,
};

/// The result of validating a candidate against a caller-supplied, already
/// consensus-verified ancestry context. This is *not* a block-acceptance token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3CandidatePreflight {
    pub block_hash: BlockId,
    pub selected_parent: BlockId,
    pub expected_difficulty_bits: u32,
    pub timestamp_ns: u64,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(message.into())
}

/// Check that all independent v3 *header-level* contracts agree for a
/// non-genesis candidate. `headers` and `selected_parents` MUST come from
/// authenticated canonical DAG state, not an untrusted P2P/RPC payload.
///
/// This helper cannot and must not replace state replay, PoW validation of
/// historical parents, selected-parent work authority, protocol identity,
/// contracts-off enforcement or deterministic reward/UTXO acceptance.
pub fn preflight_v3_candidate_context(
    candidate: &BlockEnvelopeV3,
    chain_id: &str,
    selected_parent: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    selected_parents: &BTreeMap<BlockId, Option<BlockId>>,
    now_ns: u64,
) -> Result<V3CandidatePreflight, PulseError> {
    validate_block_envelope_v3(candidate, chain_id)?;

    if candidate.header.height == 0 {
        return Err(invalid(
            "v3 genesis requires a separately frozen genesis identity, not candidate preflight",
        ));
    }
    // The dormant v3 pre-nonce builder deliberately sets an all-zero state
    // root. This placeholder is not an authoritative replay commitment:
    // never let an otherwise valid PoW/header make it look admission-ready.
    if candidate.header.state_root == "00".repeat(32) {
        return Err(invalid(
            "v3 candidate state root is unsealed; authoritative replay is required",
        ));
    }
    if !candidate.header.parents.contains(selected_parent) {
        return Err(invalid(
            "v3 selected parent is not listed in candidate header",
        ));
    }

    let mut greatest_parent_height = 0u64;
    let mut newest_parent_timestamp_ns = 0u64;
    for parent in &candidate.header.parents {
        let header = headers
            .get(parent)
            .ok_or_else(|| invalid(format!("v3 candidate missing parent header {parent}")))?;
        let computed = compute_block_hash_v3(header, chain_id)?;
        if computed != *parent {
            return Err(invalid(format!(
                "v3 candidate parent {parent} has a mismatched chain-bound v3 header hash"
            )));
        }
        validate_v3_pow_target(header.difficulty)?;
        greatest_parent_height = greatest_parent_height.max(header.height);
        newest_parent_timestamp_ns = newest_parent_timestamp_ns.max(header.timestamp_ns);
    }
    let expected_height = greatest_parent_height
        .checked_add(1)
        .ok_or_else(|| invalid("v3 candidate height overflow"))?;
    if candidate.header.height != expected_height {
        return Err(invalid(format!(
            "v3 candidate height {} differs from expected {}",
            candidate.header.height, expected_height
        )));
    }

    validate_candidate_timestamp_v3_ns(
        candidate.header.timestamp_ns,
        newest_parent_timestamp_ns,
        now_ns,
    )
    .map_err(|error| invalid(format!("v3 candidate nanosecond timestamp: {error}")))?;

    let retarget = expected_difficulty_for_selected_parent_v3_ns(
        chain_id,
        selected_parent,
        headers,
        selected_parents,
    )
    .map_err(|error| invalid(format!("v3 selected-parent retarget: {error}")))?;
    if candidate.header.difficulty != retarget.expected_bits {
        return Err(invalid(format!(
            "v3 candidate compact difficulty {} differs from expected {}",
            candidate.header.difficulty, retarget.expected_bits
        )));
    }
    verify_v3_header_pow(&candidate.header, chain_id)?;

    Ok(V3CandidatePreflight {
        block_hash: candidate.hash.clone(),
        selected_parent: selected_parent.clone(),
        expected_difficulty_bits: retarget.expected_bits,
        timestamp_ns: candidate.header.timestamp_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3, pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3, types::compute_merkle_root,
    };

    const CHAIN: &str = "pulsedag-v3-context-candidate";
    const BASE_NS: u64 = 1_800_000_000_000_000_000;
    const TARGET_BITS: u32 = 0x207f_ffff;

    fn header(height: u64, parents: Vec<BlockId>, timestamp_ns: u64) -> BlockHeaderV3 {
        BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns,
            difficulty: TARGET_BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        }
    }

    type History = (
        BTreeMap<BlockId, BlockHeaderV3>,
        BTreeMap<BlockId, Option<BlockId>>,
        BlockId,
    );

    fn single_genesis() -> History {
        let genesis = header(0, vec![], BASE_NS);
        let hash = compute_block_hash_v3(&genesis, CHAIN).unwrap();
        let mut headers = BTreeMap::new();
        headers.insert(hash.clone(), genesis);
        let mut selected = BTreeMap::new();
        selected.insert(hash.clone(), None);
        (headers, selected, hash)
    }

    fn mine_candidate(parent: &str, timestamp_ns: u64) -> BlockEnvelopeV3 {
        let mut candidate = header(1, vec![parent.to_string()], timestamp_ns);
        for nonce in 0..512 {
            candidate.nonce = nonce;
            let pow = evaluate_v3_header_pow(&candidate, CHAIN).unwrap();
            if pow.pow.accepted {
                return build_block_envelope_v3(CHAIN, candidate, vec![]).unwrap();
            }
        }
        panic!("test fixture could not find a valid easy-target nonce");
    }

    #[test]
    fn valid_five_hundred_ms_candidate_agrees_on_all_header_rules() {
        let (headers, selected, genesis) = single_genesis();
        let candidate = mine_candidate(&genesis, BASE_NS + 500_000_000);
        let result = preflight_v3_candidate_context(
            &candidate,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            BASE_NS + 500_000_000,
        )
        .unwrap();
        assert_eq!(result.block_hash, candidate.hash);
        assert_eq!(result.selected_parent, genesis);
        assert_eq!(result.expected_difficulty_bits, TARGET_BITS);
        assert_eq!(result.timestamp_ns, BASE_NS + 500_000_000);
    }

    #[test]
    fn wrong_chain_and_selected_parent_or_missing_history_fail() {
        let (mut headers, selected, genesis) = single_genesis();
        let candidate = mine_candidate(&genesis, BASE_NS + 500_000_000);
        assert!(preflight_v3_candidate_context(
            &candidate,
            "another-network",
            &genesis,
            &headers,
            &selected,
            BASE_NS,
        )
        .is_err());
        assert!(preflight_v3_candidate_context(
            &candidate,
            CHAIN,
            &"ff".repeat(32),
            &headers,
            &selected,
            BASE_NS,
        )
        .is_err());
        headers.remove(&genesis);
        assert!(preflight_v3_candidate_context(
            &candidate, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());
    }

    #[test]
    fn parent_hash_height_and_nanosecond_order_fail_closed() {
        let (mut headers, selected, genesis) = single_genesis();
        let candidate = mine_candidate(&genesis, BASE_NS + 500_000_000);
        headers.get_mut(&genesis).unwrap().nonce += 1;
        assert!(preflight_v3_candidate_context(
            &candidate, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());

        let (headers, selected, genesis) = single_genesis();
        let mut wrong_height = candidate.clone();
        wrong_height.header.height = 2;
        wrong_height.hash = compute_block_hash_v3(&wrong_height.header, CHAIN).unwrap();
        assert!(preflight_v3_candidate_context(
            &wrong_height,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            BASE_NS,
        )
        .is_err());

        let mut stale = candidate.clone();
        stale.header.timestamp_ns = BASE_NS;
        stale.hash = compute_block_hash_v3(&stale.header, CHAIN).unwrap();
        assert!(preflight_v3_candidate_context(
            &stale, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());

        let mut future = candidate;
        future.header.timestamp_ns = BASE_NS + 2_000_000_000;
        future.hash = compute_block_hash_v3(&future.header, CHAIN).unwrap();
        assert!(preflight_v3_candidate_context(
            &future, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());
    }

    #[test]
    fn difficulty_and_pow_are_checked_after_linkage() {
        let (headers, selected, genesis) = single_genesis();
        let mut candidate = mine_candidate(&genesis, BASE_NS + 500_000_000);
        candidate.header.difficulty = 0x1e0f_ffff;
        candidate.hash = compute_block_hash_v3(&candidate.header, CHAIN).unwrap();
        assert!(preflight_v3_candidate_context(
            &candidate, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());

        candidate.header.difficulty = TARGET_BITS;
        let mut found_rejected = false;
        for nonce in 0..64 {
            candidate.header.nonce = nonce;
            let proof = evaluate_v3_header_pow(&candidate.header, CHAIN).unwrap();
            if !proof.pow.accepted {
                candidate.hash = compute_block_hash_v3(&candidate.header, CHAIN).unwrap();
                found_rejected = true;
                break;
            }
        }
        assert!(found_rejected);
        assert!(preflight_v3_candidate_context(
            &candidate, CHAIN, &genesis, &headers, &selected, BASE_NS,
        )
        .is_err());
    }

    #[test]
    fn dormant_zero_state_root_is_rejected_even_with_valid_v3_pow() {
        let (headers, selected, genesis) = single_genesis();
        let mut candidate = mine_candidate(&genesis, BASE_NS + 500_000_000);
        candidate.header.state_root = "00".repeat(32);
        let mut found_pow = false;
        for nonce in 0..512 {
            candidate.header.nonce = nonce;
            let evaluation = evaluate_v3_header_pow(&candidate.header, CHAIN).unwrap();
            if evaluation.pow.accepted {
                candidate.hash = evaluation.block_hash;
                found_pow = true;
                break;
            }
        }
        assert!(
            found_pow,
            "fixture must reach a valid PoW despite the placeholder"
        );
        let error = preflight_v3_candidate_context(
            &candidate,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            BASE_NS + 500_000_000,
        )
        .unwrap_err();
        assert!(error.to_string().contains("state root is unsealed"));
        // This guard does not reject a correctly sealed ordinary candidate.
        let sealed = mine_candidate(&genesis, BASE_NS + 500_000_000);
        assert!(preflight_v3_candidate_context(
            &sealed,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            BASE_NS + 500_000_000,
        )
        .is_ok());
    }

    #[test]
    fn dormant_pre_nonce_handoff_requires_state_root_change_and_fresh_v3_pow() {
        use crate::pre_nonce_template_v3::build_dormant_v3_pre_nonce_template;

        let (headers, selected, genesis) = single_genesis();
        let timestamp_ns = BASE_NS + 500_000_000;
        let dormant = build_dormant_v3_pre_nonce_template(
            CHAIN,
            vec![genesis.clone()],
            &genesis,
            &headers,
            &selected,
            timestamp_ns,
            timestamp_ns,
            1,
            vec![],
        )
        .unwrap();

        assert!(!dormant.ready_for_nonce_search());
        assert_eq!(dormant.envelope().header.timestamp_ns, timestamp_ns);
        assert_eq!(dormant.envelope().header.difficulty, TARGET_BITS);
        assert_eq!(dormant.envelope().header.state_root, "00".repeat(32));

        // Even a real v3 PoW nonce cannot turn an unsealed pre-state
        // template into a candidate eligible for the dormant preflight.
        let mut unsealed_header = dormant.envelope().header.clone();
        let mut unsealed_pow_found = false;
        for nonce in 0..512 {
            unsealed_header.nonce = nonce;
            if evaluate_v3_header_pow(&unsealed_header, CHAIN)
                .unwrap()
                .pow
                .accepted
            {
                unsealed_pow_found = true;
                break;
            }
        }
        assert!(unsealed_pow_found, "unsealed PoW fixture must be mineable");
        let unsealed = build_block_envelope_v3(CHAIN, unsealed_header, vec![]).unwrap();
        let error = preflight_v3_candidate_context(
            &unsealed,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            timestamp_ns,
        )
        .unwrap_err();
        assert!(error.to_string().contains("state root is unsealed"));

        // Changing the committed root invalidates the original envelope
        // identity; a new root requires its own correctly bound hash/PoW.
        // This test root is NOT an authoritative state replay proof.
        let mut tampered = unsealed.clone();
        tampered.header.state_root = "44".repeat(32);
        assert!(preflight_v3_candidate_context(
            &tampered,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            timestamp_ns,
        )
        .is_err());

        let mut sealed_header = tampered.header;
        let mut sealed_pow_found = false;
        for nonce in 0..512 {
            sealed_header.nonce = nonce;
            if evaluate_v3_header_pow(&sealed_header, CHAIN)
                .unwrap()
                .pow
                .accepted
            {
                sealed_pow_found = true;
                break;
            }
        }
        assert!(
            sealed_pow_found,
            "changed-root PoW fixture must be mineable"
        );
        let sealed = build_block_envelope_v3(CHAIN, sealed_header, vec![]).unwrap();
        assert_ne!(unsealed.hash, sealed.hash);
        let checked = preflight_v3_candidate_context(
            &sealed,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            timestamp_ns,
        )
        .unwrap();
        assert_eq!(checked.block_hash, sealed.hash);
        assert_eq!(checked.timestamp_ns, timestamp_ns);
        assert_eq!(checked.expected_difficulty_bits, TARGET_BITS);
        assert!(!dormant.ready_for_nonce_search());
    }

    #[test]
    fn genesis_is_outside_candidate_acceptance_preflight() {
        let (headers, selected, genesis) = single_genesis();
        let genesis_candidate =
            build_block_envelope_v3(CHAIN, headers[&genesis].clone(), vec![]).unwrap();
        assert!(preflight_v3_candidate_context(
            &genesis_candidate,
            CHAIN,
            &genesis,
            &headers,
            &selected,
            BASE_NS,
        )
        .is_err());
    }
}
