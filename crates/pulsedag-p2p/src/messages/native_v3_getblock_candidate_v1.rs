//! Inert native-v3 GetBlock response *candidate* boundary for the first network.
//!
//! The current production direct GetBlock Swarm is HARD OFF and its legacy
//! response carries `types::Block`. Never treat such a block as native v3.
//! This module is NOT registered as a transport codec and makes NO block
//! accepted: it checks a requested, Noise-peer-bound response's bytes,
//! envelope commitments, and real PoW before handing a candidate to an
//! as-yet-unimplemented authoritative native-v3 daemon admission path.
//!
//! Genesis must be obtained from an independently frozen identity. Ordered
//! GHOSTDAG/GD-08, state roots, signatures, subsidy, UTXO, checkpoints,
//! selected parents and retarget remain separate mandatory fail-closed gates.

use pulsedag_core::{
    block_envelope_v3::{
        validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3,
    },
    pow_v3::verify_v3_header_pow,
};
use serde::{Deserialize, Serialize};

use super::direct_getblock_v1::{
    validate_direct_getblock_request, DirectGetBlockRequestV1, DirectGetBlockWireError,
    DIRECT_GETBLOCK_PROTOCOL_V1,
};

/// Transport allocation limit, NOT a frozen block consensus-size rule.
pub const NATIVE_V3_GETBLOCK_CANDIDATE_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const NATIVE_V3_GETBLOCK_CANDIDATE_VERSION: u16 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeV3GetBlockCandidateResponse {
    pub version: u16,
    pub chain_id: String,
    pub request_id: String,
    pub responding_peer_id: String,
    pub block_hash: String,
    /// None means an authenticated exact-hash miss, NOT an accepted absence
    /// proof, checkpoint, negative cache entry or consensus decision.
    pub block: Option<BlockEnvelopeV3>,
}

pub fn validate_native_v3_getblock_candidate_response(
    response: &NativeV3GetBlockCandidateResponse,
    request: &DirectGetBlockRequestV1,
    expected_chain_id: &str,
    authenticated_requester: &str,
    authenticated_responder: &str,
) -> Result<(), DirectGetBlockWireError> {
    // The request MUST already have an authenticated transport peer binding.
    // This revalidation prevents a caller-provided request JSON from granting
    // authority to a different authenticated response peer or chain.
    validate_direct_getblock_request(
        request,
        expected_chain_id,
        authenticated_requester,
        authenticated_responder,
    )?;
    if request.version != DIRECT_GETBLOCK_PROTOCOL_V1
        || response.version != NATIVE_V3_GETBLOCK_CANDIDATE_VERSION
    {
        return Err(DirectGetBlockWireError::Invalid("unsupported native-v3 response version"));
    }
    if response.chain_id != expected_chain_id
        || response.request_id != request.request_id
        || response.responding_peer_id != authenticated_responder
        || response.block_hash != request.block_hash
    {
        return Err(DirectGetBlockWireError::Invalid("native-v3 response peer/chain/request/hash mismatch"));
    }
    if let Some(block) = &response.block {
        if block.hash != request.block_hash || block.chain_id != expected_chain_id {
            return Err(DirectGetBlockWireError::Invalid("native-v3 candidate is not the requested block"));
        }
        // The genesis is pinned through a separate trusted bootstrap path,
        // never accepted as a generic unmined GetBlock response candidate.
        if block.header.height == 0 || block.header.state_root == "00".repeat(32) {
            return Err(DirectGetBlockWireError::Invalid("genesis or unsealed state root is not a GetBlock candidate"));
        }
        validate_dormant_native_v3_monetary_body_preflight(block, expected_chain_id)
            .map_err(|_| DirectGetBlockWireError::Invalid("native-v3 reward/body/header commitment invalid"))?;
        verify_v3_header_pow(&block.header, expected_chain_id)
            .map_err(|_| DirectGetBlockWireError::Invalid("native-v3 KHeavyHash PoW invalid"))?;
    }
    Ok(())
}

pub fn decode_native_v3_getblock_candidate_response(
    bytes: &[u8],
    request: &DirectGetBlockRequestV1,
    expected_chain_id: &str,
    authenticated_requester: &str,
    authenticated_responder: &str,
) -> Result<NativeV3GetBlockCandidateResponse, DirectGetBlockWireError> {
    if bytes.is_empty() || bytes.len() > NATIVE_V3_GETBLOCK_CANDIDATE_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    let decoded: NativeV3GetBlockCandidateResponse =
        serde_json::from_slice(bytes).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    // The underlying legacy Transaction serde permits unknown nested keys.
    // Do not silently discard those keys in a new native-only candidate.
    let supplied: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    let canonical =
        serde_json::to_value(&decoded).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    if supplied != canonical {
        return Err(DirectGetBlockWireError::Invalid("unknown or noncanonical nested native-v3 fields"));
    }
    validate_native_v3_getblock_candidate_response(
        &decoded,
        request,
        expected_chain_id,
        authenticated_requester,
        authenticated_responder,
    )?;
    Ok(decoded)
}

pub fn encode_native_v3_getblock_candidate_response(
    response: &NativeV3GetBlockCandidateResponse,
    request: &DirectGetBlockRequestV1,
    expected_chain_id: &str,
    authenticated_requester: &str,
    authenticated_responder: &str,
) -> Result<Vec<u8>, DirectGetBlockWireError> {
    validate_native_v3_getblock_candidate_response(
        response,
        request,
        expected_chain_id,
        authenticated_requester,
        authenticated_responder,
    )?;
    let bytes = serde_json::to_vec(response).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    if bytes.len() > NATIVE_V3_GETBLOCK_CANDIDATE_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "first-public-native-v3-testnet-candidate";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
    fn fixture() -> (DirectGetBlockRequestV1, NativeV3GetBlockCandidateResponse) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let transactions = vec![
            build_reward_claim_transaction_v3("pulse1native-miner", 1, CHAIN).unwrap()
        ];
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash],
            timestamp_ns: GENESIS_NS + 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 1,
        };
        let mut found = false;
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                found = true;
                break;
            }
        }
        assert!(found);
        let block = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        let request = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: CHAIN.into(),
            request_id: format!("getblock:peerA:{}:1", block.hash),
            requesting_peer_id: "peerA".into(),
            requested_peer_id: "peerB".into(),
            block_hash: block.hash.clone(),
        };
        let response = NativeV3GetBlockCandidateResponse {
            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
            chain_id: CHAIN.into(),
            request_id: request.request_id.clone(),
            responding_peer_id: "peerB".into(),
            block_hash: block.hash.clone(),
            block: Some(block),
        };
        (request, response)
    }

    #[test]
    fn exact_native_v3_pow_envelope_candidate_is_authenticated_not_admitted() {
        let (req, response) = fixture();
        let raw = encode_native_v3_getblock_candidate_response(
            &response, &req, CHAIN, "peerA", "peerB"
        ).unwrap();
        let decoded = decode_native_v3_getblock_candidate_response(
            &raw, &req, CHAIN, "peerA", "peerB"
        ).unwrap();
        assert_eq!(decoded.block.unwrap().hash, req.block_hash);
        assert!(decode_native_v3_getblock_candidate_response(
            &raw, &req, CHAIN, "peerA", "peerC"
        ).is_err());
        assert!(decode_native_v3_getblock_candidate_response(
            &raw, &req, "foreign-chain", "peerA", "peerB"
        ).is_err());
    }

    #[test]
    fn uncorrelated_or_modified_native_block_is_never_accepted_as_a_candidate() {
        let (req, response) = fixture();
        let mut changed = response.clone();
        changed.request_id.push_str("-replayed");
        assert!(validate_native_v3_getblock_candidate_response(
            &changed, &req, CHAIN, "peerA", "peerB"
        ).is_err());
        let mut changed = response.clone();
        changed.block.as_mut().unwrap().header.nonce += 1;
        assert!(validate_native_v3_getblock_candidate_response(
            &changed, &req, CHAIN, "peerA", "peerB"
        ).is_err());
        let mut changed = response.clone();
        changed.block.as_mut().unwrap().header.state_root = "00".repeat(32);
        assert!(validate_native_v3_getblock_candidate_response(
            &changed, &req, CHAIN, "peerA", "peerB"
        ).is_err());
        let mut changed = response.clone();
        changed.block.as_mut().unwrap().chain_id = "foreign-chain".into();
        assert!(validate_native_v3_getblock_candidate_response(
            &changed, &req, CHAIN, "peerA", "peerB"
        ).is_err());
    }

    #[test]
    fn no_old_core_or_unknown_nested_wire_fields_are_a_native_v3_response() {
        let (req, response) = fixture();
        let mut json = serde_json::to_value(&response).unwrap();
        json["block"]["transactions"][0]["unrecognized"] = serde_json::json!(1);
        let raw = serde_json::to_vec(&json).unwrap();
        assert!(decode_native_v3_getblock_candidate_response(
            &raw, &req, CHAIN, "peerA", "peerB"
        ).is_err());
        // A zero-allocation, unmined v3 genesis uses a different frozen path.
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut response_genesis = response;
        response_genesis.block = Some(genesis);
        assert!(validate_native_v3_getblock_candidate_response(
            &response_genesis, &req, CHAIN, "peerA", "peerB"
        ).is_err());
    }

    #[test]
    fn authenticated_missing_reply_is_not_block_admission_or_negative_consensus() {
        let (req, mut reply) = fixture();
        reply.block = None;
        let bytes = encode_native_v3_getblock_candidate_response(
            &reply, &req, CHAIN, "peerA", "peerB"
        ).unwrap();
        let decoded = decode_native_v3_getblock_candidate_response(
            &bytes, &req, CHAIN, "peerA", "peerB"
        ).unwrap();
        assert!(decoded.block.is_none());
        assert!(decode_native_v3_getblock_candidate_response(
            &vec![0_u8; NATIVE_V3_GETBLOCK_CANDIDATE_MAX_BYTES + 1],
            &req, CHAIN, "peerA", "peerB"
        ).is_err());
    }
}
