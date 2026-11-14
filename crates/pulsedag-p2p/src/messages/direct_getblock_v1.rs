//! Fail-closed envelope for a future *dedicated* authenticated GetBlock
//! request-response substream. This module is deliberately NOT wired to
//! Gossipsub, Swarm or the daemon's block-store authority yet.
//!
//! The caller MUST obtain the peer identity from the authenticated transport,
//! never from a self-asserted JSON field. Passing these checks does not admit
//! a block into consensus or prove PoW, merkle root, parents or UTXO state.

use pulsedag_core::types::Block;
use serde::{Deserialize, Serialize};

pub const DIRECT_GETBLOCK_PROTOCOL_V1: u16 = 1;
pub const DIRECT_GETBLOCK_REQUEST_MAX_BYTES: usize = 4096;
pub const DIRECT_GETBLOCK_RESPONSE_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockWireError {
    Invalid(&'static str),
    TooLarge,
    InvalidJson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectGetBlockRequestV1 {
    pub version: u16,
    pub chain_id: String,
    pub request_id: String,
    pub requesting_peer_id: String,
    pub requested_peer_id: String,
    pub block_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectGetBlockResponseV1 {
    pub version: u16,
    pub chain_id: String,
    pub request_id: String,
    pub responding_peer_id: String,
    pub block_hash: String,
    /// A missing block is still an authenticated response to an exact request.
    /// The daemon must separately validate the block and its ancestry.
    pub block: Option<Block>,
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn valid_request_id(request_id: &str) -> bool {
    request_id.len() >= 10
        && request_id.len() <= 256
        && request_id.starts_with("getblock:")
        && request_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b':' | b'-' | b'_' | b'/'))
}

fn valid_peer_id(peer_id: &str) -> bool {
    !peer_id.is_empty()
        && peer_id.len() <= 128
        && peer_id.bytes().all(|c| c.is_ascii_alphanumeric())
}

pub fn validate_direct_getblock_request(
    request: &DirectGetBlockRequestV1,
    expected_chain_id: &str,
    authenticated_remote_peer_id: &str,
    authenticated_local_peer_id: &str,
) -> Result<(), DirectGetBlockWireError> {
    if request.version != DIRECT_GETBLOCK_PROTOCOL_V1 {
        return Err(DirectGetBlockWireError::Invalid("unknown request version"));
    }
    if expected_chain_id.is_empty()
        || expected_chain_id.len() > 96
        || request.chain_id != expected_chain_id
    {
        return Err(DirectGetBlockWireError::Invalid("chain domain mismatch"));
    }
    if !valid_peer_id(authenticated_remote_peer_id)
        || !valid_peer_id(authenticated_local_peer_id)
        || request.requesting_peer_id != authenticated_remote_peer_id
        || request.requested_peer_id != authenticated_local_peer_id
    {
        return Err(DirectGetBlockWireError::Invalid(
            "authenticated peer mismatch",
        ));
    }
    if !valid_request_id(&request.request_id) {
        return Err(DirectGetBlockWireError::Invalid(
            "invalid request correlation id",
        ));
    }
    if !valid_hash(&request.block_hash) {
        return Err(DirectGetBlockWireError::Invalid(
            "noncanonical requested block hash",
        ));
    }
    Ok(())
}

pub fn decode_direct_getblock_request(
    raw: &[u8],
    expected_chain_id: &str,
    authenticated_remote_peer_id: &str,
    authenticated_local_peer_id: &str,
) -> Result<DirectGetBlockRequestV1, DirectGetBlockWireError> {
    if raw.is_empty() || raw.len() > DIRECT_GETBLOCK_REQUEST_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    let request: DirectGetBlockRequestV1 =
        serde_json::from_slice(raw).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    validate_direct_getblock_request(
        &request,
        expected_chain_id,
        authenticated_remote_peer_id,
        authenticated_local_peer_id,
    )?;
    Ok(request)
}

pub fn encode_direct_getblock_request(
    request: &DirectGetBlockRequestV1,
    expected_chain_id: &str,
    authenticated_local_peer_id: &str,
    authenticated_target_peer_id: &str,
) -> Result<Vec<u8>, DirectGetBlockWireError> {
    validate_direct_getblock_request(
        request,
        expected_chain_id,
        authenticated_local_peer_id,
        authenticated_target_peer_id,
    )?;
    let bytes = serde_json::to_vec(request).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    if bytes.len() > DIRECT_GETBLOCK_REQUEST_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    Ok(bytes)
}

pub fn validate_direct_getblock_response(
    response: &DirectGetBlockResponseV1,
    request: &DirectGetBlockRequestV1,
    authenticated_remote_peer_id: &str,
) -> Result<(), DirectGetBlockWireError> {
    if response.version != DIRECT_GETBLOCK_PROTOCOL_V1 || response.version != request.version {
        return Err(DirectGetBlockWireError::Invalid("unknown response version"));
    }
    if response.chain_id != request.chain_id
        || response.request_id != request.request_id
        || response.block_hash != request.block_hash
        || authenticated_remote_peer_id != request.requested_peer_id
        || response.responding_peer_id != authenticated_remote_peer_id
    {
        return Err(DirectGetBlockWireError::Invalid(
            "response origin/hash mismatch",
        ));
    }
    if response
        .block
        .as_ref()
        .is_some_and(|block| block.hash != request.block_hash)
    {
        return Err(DirectGetBlockWireError::Invalid(
            "block hash does not match request",
        ));
    }
    Ok(())
}

pub fn decode_direct_getblock_response(
    raw: &[u8],
    request: &DirectGetBlockRequestV1,
    authenticated_remote_peer_id: &str,
) -> Result<DirectGetBlockResponseV1, DirectGetBlockWireError> {
    if raw.is_empty() || raw.len() > DIRECT_GETBLOCK_RESPONSE_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    let response: DirectGetBlockResponseV1 =
        serde_json::from_slice(raw).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    // Core Block/Header/Transaction types intentionally retain legacy tolerant
    // serde deserialization. Do not silently discard unknown nested wire keys
    // before the separately authenticated, exact-hash consensus validation.
    // This structural roundtrip is intentionally strict at every nested array
    // and object depth and covers fields in future core type extensions.
    let supplied: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    let canonical =
        serde_json::to_value(&response).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    if supplied != canonical {
        return Err(DirectGetBlockWireError::Invalid(
            "unknown or noncanonical nested block response fields",
        ));
    }
    validate_direct_getblock_response(&response, request, authenticated_remote_peer_id)?;
    Ok(response)
}

pub fn encode_direct_getblock_response(
    response: &DirectGetBlockResponseV1,
    request: &DirectGetBlockRequestV1,
    authenticated_local_peer_id: &str,
) -> Result<Vec<u8>, DirectGetBlockWireError> {
    validate_direct_getblock_response(response, request, authenticated_local_peer_id)?;
    let bytes = serde_json::to_vec(response).map_err(|_| DirectGetBlockWireError::InvalidJson)?;
    if bytes.len() > DIRECT_GETBLOCK_RESPONSE_MAX_BYTES {
        return Err(DirectGetBlockWireError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DirectGetBlockRequestV1 {
        DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: "pulsedag-testnet-v3".into(),
            request_id: format!("getblock:peerA:{}:42:1", "a".repeat(64)),
            requesting_peer_id: "peerA".into(),
            requested_peer_id: "peerB".into(),
            block_hash: "a".repeat(64),
        }
    }

    fn response() -> DirectGetBlockResponseV1 {
        DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: "pulsedag-testnet-v3".into(),
            request_id: request().request_id,
            responding_peer_id: "peerB".into(),
            block_hash: "a".repeat(64),
            block: None,
        }
    }

    #[test]
    fn exact_authenticated_peers_and_chain_roundtrip() {
        let req = request();
        let raw =
            encode_direct_getblock_request(&req, "pulsedag-testnet-v3", "peerA", "peerB").unwrap();
        assert_eq!(
            decode_direct_getblock_request(&raw, "pulsedag-testnet-v3", "peerA", "peerB"),
            Ok(req.clone())
        );
        let reply = response();
        let reply_raw = encode_direct_getblock_response(&reply, &req, "peerB").unwrap();
        let decoded = decode_direct_getblock_response(&reply_raw, &req, "peerB").unwrap();
        assert!(decoded.block.is_none());
        assert_eq!(decoded.request_id, req.request_id);
    }

    #[test]
    fn rejects_foreign_and_swapped_authenticated_peers_and_chain() {
        let req = request();
        for (chain, remote, local) in [
            ("pulsedag-mainnet-v3", "peerA", "peerB"),
            ("pulsedag-testnet-v3", "peerEvil", "peerB"),
            ("pulsedag-testnet-v3", "peerA", "peerEvil"),
        ] {
            assert!(validate_direct_getblock_request(&req, chain, remote, local).is_err());
        }
        let mut invalid = req.clone();
        invalid.block_hash = "A".repeat(64);
        assert!(validate_direct_getblock_request(
            &invalid,
            "pulsedag-testnet-v3",
            "peerA",
            "peerB"
        )
        .is_err());
    }

    #[test]
    fn response_must_match_exact_authenticated_request() {
        let req = request();
        let good = response();
        assert!(validate_direct_getblock_response(&good, &req, "peerB").is_ok());
        let mut wrong = response();
        wrong.request_id.push('x');
        assert!(validate_direct_getblock_response(&wrong, &req, "peerB").is_err());
        let mut wrong = response();
        wrong.block_hash = "b".repeat(64);
        assert!(validate_direct_getblock_response(&wrong, &req, "peerB").is_err());
        let mut wrong = response();
        wrong.responding_peer_id = "peerEvil".into();
        assert!(validate_direct_getblock_response(&wrong, &req, "peerB").is_err());
        assert!(validate_direct_getblock_response(&good, &req, "peerEvil").is_err());
    }

    #[test]
    fn reject_unknown_fields_inside_nonnull_block_header_and_signed_transaction() {
        use pulsedag_core::types::{BlockHeader, OutPoint, Transaction, TxInput, TxOutput};

        let req = request();
        let mut reply = response();
        reply.block = Some(Block {
            hash: req.block_hash.clone(),
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 123,
                difficulty: 1,
                nonce: 1,
                merkle_root: "c".repeat(64),
                state_root: "d".repeat(64),
                blue_score: 0,
                height: 1,
            },
            transactions: vec![Transaction {
                txid: "e".repeat(64),
                version: 1,
                inputs: vec![TxInput {
                    previous_output: OutPoint {
                        txid: "f".repeat(64),
                        index: 0,
                    },
                    public_key: "11".repeat(32),
                    signature: "22".repeat(64),
                }],
                outputs: vec![TxOutput {
                    address: "test-only".into(),
                    amount: 10,
                }],
                fee: 1,
                nonce: 1,
            }],
        });
        let valid = encode_direct_getblock_response(&reply, &req, "peerB").unwrap();
        assert!(decode_direct_getblock_response(&valid, &req, "peerB").is_ok());
        let original: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        for pointer in [
            "/block/unapproved_activation",
            "/block/header/unapproved_activation",
            "/block/transactions/0/unapproved_activation",
            "/block/transactions/0/inputs/0/unapproved_activation",
            "/block/transactions/0/inputs/0/previous_output/unapproved_activation",
            "/block/transactions/0/outputs/0/unapproved_activation",
        ] {
            let mut forged = original.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            forged.pointer_mut(parent).unwrap()[key] = serde_json::json!(true);
            let tampered = serde_json::to_vec(&forged).unwrap();
            assert!(
                matches!(
                    decode_direct_getblock_response(&tampered, &req, "peerB"),
                    Err(DirectGetBlockWireError::Invalid(
                        "unknown or noncanonical nested block response fields"
                    ))
                ),
                "unexpected accepted nested field: {pointer}"
            );
        }
    }

    #[test]
    fn reject_oversize_truncation_and_extra_json_fields() {
        let req = request();
        let raw = serde_json::to_vec(&req).unwrap();
        assert!(decode_direct_getblock_request(
            &raw[..raw.len() - 1],
            "pulsedag-testnet-v3",
            "peerA",
            "peerB"
        )
        .is_err());
        assert_eq!(
            decode_direct_getblock_request(
                &vec![b' '; DIRECT_GETBLOCK_REQUEST_MAX_BYTES + 1],
                "pulsedag-testnet-v3",
                "peerA",
                "peerB"
            ),
            Err(DirectGetBlockWireError::TooLarge)
        );
        let mut extra = serde_json::to_value(req.clone()).unwrap();
        extra["unapproved_activation"] = serde_json::json!(true);
        assert!(decode_direct_getblock_request(
            &serde_json::to_vec(&extra).unwrap(),
            "pulsedag-testnet-v3",
            "peerA",
            "peerB"
        )
        .is_err());
        let reply = vec![b' '; DIRECT_GETBLOCK_RESPONSE_MAX_BYTES + 1];
        assert!(matches!(
            decode_direct_getblock_response(&reply, &req, "peerB"),
            Err(DirectGetBlockWireError::TooLarge)
        ));
    }
}
