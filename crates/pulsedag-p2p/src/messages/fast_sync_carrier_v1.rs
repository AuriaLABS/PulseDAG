use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::{
    snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
    snapshot_transfer_chunk_digest_v1, ProtocolActivationIdentity,
};
use serde::de::{self, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

use super::NetworkMessage;

pub const FAST_SYNC_EXTENSION_FIELD_V1: &str = "pulsedag_fast_sync_v1";
pub const P2P_FAST_SYNC_CONTRACT_VERSION: u32 = 1;
pub const P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1: usize = 60 * 1024;
pub const P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1: usize = 256;
pub const P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1: usize = 24 * 1024;
pub const P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1: usize = 256;
pub const P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1: usize = 128;
pub const P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1: u32 = 131_072;
pub const P2P_FAST_SYNC_MAX_TRANSFER_BYTES_V1: u64 = 16 * 1024 * 1024 * 1024;
pub const P2P_FAST_SYNC_MAX_CHAIN_ID_BYTES_V1: usize = 128;
pub const P2P_FAST_SYNC_MAX_TARGET_PEER_ID_BYTES_V1: usize = 256;
pub const P2P_FAST_SYNC_MAX_PAYLOAD_ENCODING_BYTES_V1: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncCapabilitiesV1 {
    pub contract_version: u32,
    pub chain_id: String,
    pub genesis_hash: String,
    pub protocol_fingerprint: String,
    pub manifest_version: u32,
    pub protocol_snapshot_bundle_format_version: u32,
    pub storage_schema_version: u32,
    pub payload_encoding: String,
    pub max_chunk_bytes: u32,
    pub max_commitments_per_page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncTransferSummaryV1 {
    pub contract_version: u32,
    pub chain_id: String,
    pub genesis_hash: String,
    pub protocol_fingerprint: String,
    pub manifest_version: u32,
    pub protocol_snapshot_bundle_format_version: u32,
    pub storage_schema_version: u32,
    pub payload_encoding: String,
    pub transfer_id: String,
    pub commitment_set_id: String,
    pub payload_len: u64,
    pub chunk_size: u32,
    pub chunk_count: u32,
    pub best_height: u64,
    pub selected_tip: String,
    pub state_commitment: String,
    pub prune_boundary_height: Option<u64>,
    pub snapshot_generation: u64,
    pub accepted_storage_generation: u64,
    pub delta_start_generation: u64,
    pub delta_end_generation: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncCommitmentPageV1 {
    pub contract_version: u32,
    pub chain_id: String,
    pub transfer_id: String,
    pub start_index: u32,
    pub commitments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncChunkRequestV1 {
    pub contract_version: u32,
    pub chain_id: String,
    pub transfer_id: String,
    pub chunk_indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncChunkV1 {
    pub contract_version: u32,
    pub chain_id: String,
    pub transfer_id: String,
    pub chunk_index: u32,
    pub chunk_commitment: String,
    pub data_hex: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "fast_sync_type", content = "payload", rename_all = "snake_case")]
pub enum FastSyncWireV1 {
    CapabilityProbe {
        chain_id: String,
    },
    Capabilities(FastSyncCapabilitiesV1),
    GetTransferSummary {
        chain_id: String,
    },
    TransferSummary(FastSyncTransferSummaryV1),
    GetCommitmentPage {
        chain_id: String,
        transfer_id: String,
        start_index: u32,
        limit: u16,
    },
    CommitmentPage(FastSyncCommitmentPageV1),
    GetChunks(FastSyncChunkRequestV1),
    Chunk(FastSyncChunkV1),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FastSyncCarrierV1 {
    pub target_peer_id: String,
    pub wire: FastSyncWireV1,
}

#[derive(Debug, Clone)]
pub struct DecodedNetworkMessageWithFastSyncV1 {
    pub message: NetworkMessage,
    pub fast_sync: Option<FastSyncCarrierV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncWireErrorV1 {
    InvalidShape(String),
    ChainIdMismatch { expected: String, observed: String },
    ProtocolIdentity(String),
    CommitmentSetMismatch,
    ChunkCommitmentMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncCarrierErrorV1 {
    Json(String),
    InvalidJsonRoot,
    UnsupportedCarrierKind { kind: String },
    EmptyTargetPeerId,
    TargetPeerIdTooLarge { observed: usize, maximum: usize },
    CarrierTooLarge { observed: usize, maximum: usize },
    FastSync(FastSyncWireErrorV1),
}

fn invalid_shape(message: impl Into<String>) -> FastSyncWireErrorV1 {
    FastSyncWireErrorV1::InvalidShape(message.into())
}

fn is_canonical_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_contract_and_chain(
    contract_version: u32,
    chain_id: &str,
) -> Result<(), FastSyncWireErrorV1> {
    if contract_version != P2P_FAST_SYNC_CONTRACT_VERSION {
        return Err(invalid_shape(format!(
            "fast-sync contract version {contract_version} does not match {}",
            P2P_FAST_SYNC_CONTRACT_VERSION
        )));
    }
    if chain_id.is_empty() {
        return Err(invalid_shape("fast-sync chain_id must be non-empty"));
    }
    if chain_id.len() > P2P_FAST_SYNC_MAX_CHAIN_ID_BYTES_V1 {
        return Err(invalid_shape(format!(
            "fast-sync chain_id length {} exceeds {}",
            chain_id.len(),
            P2P_FAST_SYNC_MAX_CHAIN_ID_BYTES_V1
        )));
    }
    Ok(())
}

fn validate_transfer_id(value: &str, field: &str) -> Result<(), FastSyncWireErrorV1> {
    if !is_canonical_sha256_hex(value) {
        return Err(invalid_shape(format!(
            "fast-sync {field} must be canonical lowercase SHA-256 hex"
        )));
    }
    Ok(())
}

fn required_chunk_count(payload_len: u64, chunk_size: u32) -> Result<u32, FastSyncWireErrorV1> {
    if chunk_size == 0 {
        return Err(invalid_shape("fast-sync chunk_size must be non-zero"));
    }
    let chunk_size = u64::from(chunk_size);
    let count = payload_len
        .checked_add(chunk_size - 1)
        .ok_or_else(|| invalid_shape("fast-sync chunk count overflow"))?
        / chunk_size;
    u32::try_from(count).map_err(|_| invalid_shape("fast-sync chunk count exceeds u32"))
}

fn validate_identity_fields(
    chain_id: &str,
    genesis_hash: &str,
    protocol_fingerprint: &str,
    expected: &ProtocolActivationIdentity,
) -> Result<(), FastSyncWireErrorV1> {
    if chain_id != expected.chain_id {
        return Err(FastSyncWireErrorV1::ChainIdMismatch {
            expected: expected.chain_id.clone(),
            observed: chain_id.to_string(),
        });
    }
    if genesis_hash != expected.genesis_hash {
        return Err(FastSyncWireErrorV1::ProtocolIdentity(
            "fast-sync genesis hash does not match expected protocol identity".to_string(),
        ));
    }
    let expected_fingerprint = expected
        .fingerprint()
        .map_err(FastSyncWireErrorV1::ProtocolIdentity)?;
    if protocol_fingerprint != expected_fingerprint {
        return Err(FastSyncWireErrorV1::ProtocolIdentity(
            "fast-sync protocol fingerprint does not match expected identity".to_string(),
        ));
    }
    Ok(())
}

impl FastSyncCapabilitiesV1 {
    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        validate_contract_and_chain(self.contract_version, &self.chain_id)?;
        validate_transfer_id(&self.genesis_hash, "genesis_hash")?;
        validate_transfer_id(&self.protocol_fingerprint, "protocol_fingerprint")?;
        if self.manifest_version == 0
            || self.protocol_snapshot_bundle_format_version == 0
            || self.storage_schema_version == 0
        {
            return Err(invalid_shape(
                "fast-sync manifest/bundle/storage versions must be non-zero",
            ));
        }
        if self.payload_encoding.is_empty()
            || self.payload_encoding.len() > P2P_FAST_SYNC_MAX_PAYLOAD_ENCODING_BYTES_V1
        {
            return Err(invalid_shape("fast-sync payload_encoding is invalid"));
        }
        let max_chunk_bytes = usize::try_from(self.max_chunk_bytes)
            .map_err(|_| invalid_shape("fast-sync max_chunk_bytes does not fit usize"))?;
        if !(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1..=P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1)
            .contains(&max_chunk_bytes)
        {
            return Err(invalid_shape(format!(
                "fast-sync max_chunk_bytes {max_chunk_bytes} outside supported range {}..={}",
                P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1, P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1
            )));
        }
        if self.max_commitments_per_page == 0
            || self.max_commitments_per_page as usize > P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1
        {
            return Err(invalid_shape(
                "fast-sync max_commitments_per_page exceeds the wire bound",
            ));
        }
        Ok(())
    }

    pub fn validate_for_expected(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), FastSyncWireErrorV1> {
        self.validate_shape()?;
        validate_identity_fields(
            &self.chain_id,
            &self.genesis_hash,
            &self.protocol_fingerprint,
            expected,
        )
    }
}

impl FastSyncTransferSummaryV1 {
    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        validate_contract_and_chain(self.contract_version, &self.chain_id)?;
        validate_transfer_id(&self.genesis_hash, "genesis_hash")?;
        validate_transfer_id(&self.protocol_fingerprint, "protocol_fingerprint")?;
        validate_transfer_id(&self.transfer_id, "transfer_id")?;
        validate_transfer_id(&self.commitment_set_id, "commitment_set_id")?;
        validate_transfer_id(&self.selected_tip, "selected_tip")?;
        validate_transfer_id(&self.state_commitment, "state_commitment")?;
        if self.manifest_version == 0
            || self.protocol_snapshot_bundle_format_version == 0
            || self.storage_schema_version == 0
        {
            return Err(invalid_shape(
                "fast-sync manifest/bundle/storage versions must be non-zero",
            ));
        }
        if self.payload_encoding.is_empty()
            || self.payload_encoding.len() > P2P_FAST_SYNC_MAX_PAYLOAD_ENCODING_BYTES_V1
        {
            return Err(invalid_shape("fast-sync payload_encoding is invalid"));
        }
        if self.payload_len == 0 || self.payload_len > P2P_FAST_SYNC_MAX_TRANSFER_BYTES_V1 {
            return Err(invalid_shape(format!(
                "fast-sync payload_len {} outside supported range 1..={}",
                self.payload_len, P2P_FAST_SYNC_MAX_TRANSFER_BYTES_V1
            )));
        }
        let chunk_size = usize::try_from(self.chunk_size)
            .map_err(|_| invalid_shape("fast-sync chunk_size does not fit usize"))?;
        if !(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1..=P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1)
            .contains(&chunk_size)
        {
            return Err(invalid_shape(format!(
                "fast-sync chunk_size {chunk_size} outside supported range {}..={}",
                P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1, P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1
            )));
        }
        let required = required_chunk_count(self.payload_len, self.chunk_size)?;
        if self.chunk_count == 0
            || self.chunk_count > P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1
            || self.chunk_count != required
        {
            return Err(invalid_shape(format!(
                "fast-sync chunk_count {} does not match required {} within maximum {}",
                self.chunk_count, required, P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1
            )));
        }
        if let Some(prune_boundary_height) = self.prune_boundary_height {
            if prune_boundary_height > self.best_height {
                return Err(invalid_shape(
                    "fast-sync prune boundary cannot exceed best height",
                ));
            }
        }
        if self.delta_start_generation > self.delta_end_generation {
            return Err(invalid_shape(
                "fast-sync delta generation range is reversed",
            ));
        }
        Ok(())
    }

    pub fn validate_for_expected(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), FastSyncWireErrorV1> {
        self.validate_shape()?;
        validate_identity_fields(
            &self.chain_id,
            &self.genesis_hash,
            &self.protocol_fingerprint,
            expected,
        )
    }

    fn expected_chunk_len(&self, chunk_index: u32) -> Result<usize, FastSyncWireErrorV1> {
        if chunk_index >= self.chunk_count {
            return Err(invalid_shape(format!(
                "fast-sync chunk index {chunk_index} outside chunk_count {}",
                self.chunk_count
            )));
        }
        let chunk_size = u64::from(self.chunk_size);
        let start = u64::from(chunk_index)
            .checked_mul(chunk_size)
            .ok_or_else(|| invalid_shape("fast-sync chunk offset overflow"))?;
        let end = start
            .checked_add(chunk_size)
            .ok_or_else(|| invalid_shape("fast-sync chunk end overflow"))?
            .min(self.payload_len);
        usize::try_from(end - start)
            .map_err(|_| invalid_shape("fast-sync chunk length does not fit usize"))
    }
}

impl FastSyncCommitmentPageV1 {
    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        validate_contract_and_chain(self.contract_version, &self.chain_id)?;
        validate_transfer_id(&self.transfer_id, "transfer_id")?;
        if self.commitments.is_empty()
            || self.commitments.len() > P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1
        {
            return Err(invalid_shape("fast-sync commitment page size is invalid"));
        }
        if self.start_index >= P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1 {
            return Err(invalid_shape(
                "fast-sync commitment page start_index exceeds transfer bound",
            ));
        }
        let end = usize::try_from(self.start_index)
            .ok()
            .and_then(|start| start.checked_add(self.commitments.len()))
            .ok_or_else(|| invalid_shape("fast-sync commitment page range overflow"))?;
        if end > P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1 as usize {
            return Err(invalid_shape(
                "fast-sync commitment page exceeds transfer chunk bound",
            ));
        }
        if self
            .commitments
            .iter()
            .any(|commitment| !is_canonical_sha256_hex(commitment))
        {
            return Err(invalid_shape(
                "fast-sync commitment page contains a non-canonical commitment",
            ));
        }
        Ok(())
    }

    pub fn validate_against_summary(
        &self,
        summary: &FastSyncTransferSummaryV1,
    ) -> Result<(), FastSyncWireErrorV1> {
        self.validate_shape()?;
        summary.validate_shape()?;
        if self.chain_id != summary.chain_id || self.transfer_id != summary.transfer_id {
            return Err(invalid_shape(
                "fast-sync commitment page does not belong to the transfer summary",
            ));
        }
        let end = self.start_index as usize + self.commitments.len();
        if end > summary.chunk_count as usize {
            return Err(invalid_shape(
                "fast-sync commitment page extends past transfer chunk_count",
            ));
        }
        Ok(())
    }
}

impl FastSyncChunkRequestV1 {
    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        validate_contract_and_chain(self.contract_version, &self.chain_id)?;
        validate_transfer_id(&self.transfer_id, "transfer_id")?;
        if self.chunk_indices.is_empty()
            || self.chunk_indices.len() > P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1
        {
            return Err(invalid_shape("fast-sync chunk request size is invalid"));
        }
        if self
            .chunk_indices
            .iter()
            .any(|index| *index >= P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1)
        {
            return Err(invalid_shape(
                "fast-sync chunk request contains an out-of-range index",
            ));
        }
        if self.chunk_indices.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(invalid_shape(
                "fast-sync chunk request indices must be strictly increasing",
            ));
        }
        Ok(())
    }

    pub fn validate_against_summary(
        &self,
        summary: &FastSyncTransferSummaryV1,
    ) -> Result<(), FastSyncWireErrorV1> {
        self.validate_shape()?;
        summary.validate_shape()?;
        if self.chain_id != summary.chain_id || self.transfer_id != summary.transfer_id {
            return Err(invalid_shape(
                "fast-sync chunk request does not belong to the transfer summary",
            ));
        }
        if self
            .chunk_indices
            .iter()
            .any(|index| *index >= summary.chunk_count)
        {
            return Err(invalid_shape(
                "fast-sync chunk request contains an index beyond chunk_count",
            ));
        }
        Ok(())
    }
}

impl FastSyncChunkV1 {
    pub fn from_bytes(
        chain_id: impl Into<String>,
        transfer_id: impl Into<String>,
        chunk_index: u32,
        bytes: &[u8],
    ) -> Result<Self, FastSyncWireErrorV1> {
        let chain_id = chain_id.into();
        let transfer_id = transfer_id.into();
        validate_contract_and_chain(P2P_FAST_SYNC_CONTRACT_VERSION, &chain_id)?;
        validate_transfer_id(&transfer_id, "transfer_id")?;
        if chunk_index >= P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1 {
            return Err(invalid_shape(
                "fast-sync chunk index exceeds transfer bound",
            ));
        }
        if bytes.is_empty() || bytes.len() > P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1 {
            return Err(invalid_shape(format!(
                "fast-sync raw chunk length {} outside supported range 1..={}",
                bytes.len(),
                P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1
            )));
        }
        let chunk_commitment = snapshot_transfer_chunk_digest_v1(&transfer_id, chunk_index, bytes);
        Ok(Self {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id,
            transfer_id,
            chunk_index,
            chunk_commitment,
            data_hex: encode_lower_hex(bytes),
        })
    }

    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        validate_contract_and_chain(self.contract_version, &self.chain_id)?;
        validate_transfer_id(&self.transfer_id, "transfer_id")?;
        validate_transfer_id(&self.chunk_commitment, "chunk_commitment")?;
        if self.chunk_index >= P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1 {
            return Err(invalid_shape(
                "fast-sync chunk index exceeds transfer bound",
            ));
        }
        if self.data_hex.is_empty()
            || self.data_hex.len() > P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1 * 2
            || !self.data_hex.len().is_multiple_of(2)
            || !self
                .data_hex
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(invalid_shape("fast-sync chunk data_hex is invalid"));
        }
        Ok(())
    }

    pub fn decode_verified_bytes(
        &self,
        summary: &FastSyncTransferSummaryV1,
        expected_commitment: &str,
    ) -> Result<Vec<u8>, FastSyncWireErrorV1> {
        self.validate_shape()?;
        summary.validate_shape()?;
        validate_transfer_id(expected_commitment, "expected chunk commitment")?;
        if self.chain_id != summary.chain_id || self.transfer_id != summary.transfer_id {
            return Err(invalid_shape(
                "fast-sync chunk does not belong to the transfer summary",
            ));
        }
        if self.chunk_index >= summary.chunk_count {
            return Err(invalid_shape(
                "fast-sync chunk index is beyond transfer chunk_count",
            ));
        }
        if self.chunk_commitment != expected_commitment {
            return Err(FastSyncWireErrorV1::ChunkCommitmentMismatch);
        }
        let bytes = decode_lower_hex(&self.data_hex)?;
        let expected_len = summary.expected_chunk_len(self.chunk_index)?;
        if bytes.len() != expected_len {
            return Err(invalid_shape(format!(
                "fast-sync chunk length {} does not match expected {expected_len}",
                bytes.len()
            )));
        }
        let actual = snapshot_transfer_chunk_digest_v1(&self.transfer_id, self.chunk_index, &bytes);
        if actual != self.chunk_commitment {
            return Err(FastSyncWireErrorV1::ChunkCommitmentMismatch);
        }
        Ok(bytes)
    }
}

pub fn verify_fast_sync_commitment_pages_v1(
    summary: &FastSyncTransferSummaryV1,
    pages: &[FastSyncCommitmentPageV1],
) -> Result<Vec<String>, FastSyncWireErrorV1> {
    summary.validate_shape()?;
    if pages.is_empty() {
        return Err(invalid_shape("fast-sync commitment page set is empty"));
    }
    let mut ordered = pages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|page| page.start_index);
    let mut commitments = Vec::with_capacity(summary.chunk_count as usize);
    let mut expected_start = 0_u32;
    for page in ordered {
        page.validate_against_summary(summary)?;
        if page.start_index != expected_start {
            return Err(invalid_shape(format!(
                "fast-sync commitment pages have a gap or overlap at index {expected_start}"
            )));
        }
        commitments.extend(page.commitments.iter().cloned());
        expected_start = expected_start
            .checked_add(page.commitments.len() as u32)
            .ok_or_else(|| invalid_shape("fast-sync commitment page index overflow"))?;
    }
    if expected_start != summary.chunk_count {
        return Err(invalid_shape(format!(
            "fast-sync commitment pages cover {expected_start} chunks; expected {}",
            summary.chunk_count
        )));
    }
    let root = snapshot_transfer_commitment_set_digest_v1(&summary.transfer_id, &commitments);
    if root != summary.commitment_set_id {
        return Err(FastSyncWireErrorV1::CommitmentSetMismatch);
    }
    Ok(commitments)
}

impl FastSyncWireV1 {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CapabilityProbe { .. } => "CapabilityProbe",
            Self::Capabilities(_) => "Capabilities",
            Self::GetTransferSummary { .. } => "GetTransferSummary",
            Self::TransferSummary(_) => "TransferSummary",
            Self::GetCommitmentPage { .. } => "GetCommitmentPage",
            Self::CommitmentPage(_) => "CommitmentPage",
            Self::GetChunks(_) => "GetChunks",
            Self::Chunk(_) => "Chunk",
        }
    }

    pub fn chain_id(&self) -> &str {
        match self {
            Self::CapabilityProbe { chain_id }
            | Self::GetTransferSummary { chain_id }
            | Self::GetCommitmentPage { chain_id, .. } => chain_id,
            Self::Capabilities(value) => &value.chain_id,
            Self::TransferSummary(value) => &value.chain_id,
            Self::CommitmentPage(value) => &value.chain_id,
            Self::GetChunks(value) => &value.chain_id,
            Self::Chunk(value) => &value.chain_id,
        }
    }

    pub fn validate_shape(&self) -> Result<(), FastSyncWireErrorV1> {
        match self {
            Self::CapabilityProbe { chain_id } | Self::GetTransferSummary { chain_id } => {
                validate_contract_and_chain(P2P_FAST_SYNC_CONTRACT_VERSION, chain_id)
            }
            Self::Capabilities(value) => value.validate_shape(),
            Self::TransferSummary(value) => value.validate_shape(),
            Self::GetCommitmentPage {
                chain_id,
                transfer_id,
                start_index,
                limit,
            } => {
                validate_contract_and_chain(P2P_FAST_SYNC_CONTRACT_VERSION, chain_id)?;
                validate_transfer_id(transfer_id, "transfer_id")?;
                if *start_index >= P2P_FAST_SYNC_MAX_TRANSFER_CHUNKS_V1 {
                    return Err(invalid_shape(
                        "fast-sync commitment-page request start_index exceeds transfer bound",
                    ));
                }
                if *limit == 0 || usize::from(*limit) > P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 {
                    return Err(invalid_shape(
                        "fast-sync commitment-page request limit exceeds wire bound",
                    ));
                }
                Ok(())
            }
            Self::CommitmentPage(value) => value.validate_shape(),
            Self::GetChunks(value) => value.validate_shape(),
            Self::Chunk(value) => value.validate_shape(),
        }
    }

    pub fn validate_for_chain(&self, expected_chain_id: &str) -> Result<(), FastSyncWireErrorV1> {
        self.validate_shape()?;
        if self.chain_id() != expected_chain_id {
            return Err(FastSyncWireErrorV1::ChainIdMismatch {
                expected: expected_chain_id.to_string(),
                observed: self.chain_id().to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
struct BoundedVec<T, const MAXIMUM: usize>(Vec<T>);

struct BoundedVecVisitor<T, const MAXIMUM: usize> {
    marker: PhantomData<T>,
}

impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedVecVisitor<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    type Value = BoundedVec<T, MAXIMUM>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a sequence with at most {MAXIMUM} items")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAXIMUM));
        while values.len() < MAXIMUM {
            match sequence.next_element::<T>()? {
                Some(value) => values.push(value),
                None => return Ok(BoundedVec(values)),
            }
        }
        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(format!(
                "sequence exceeds maximum item count {MAXIMUM}"
            )));
        }
        Ok(BoundedVec(values))
    }
}

impl<'de, T, const MAXIMUM: usize> Deserialize<'de> for BoundedVec<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(BoundedVecVisitor::<T, MAXIMUM> {
            marker: PhantomData,
        })
    }
}

#[derive(Debug, Deserialize)]
struct FastSyncCommitmentPageDecodeV1 {
    contract_version: u32,
    chain_id: String,
    transfer_id: String,
    start_index: u32,
    commitments: BoundedVec<String, P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1>,
}

impl From<FastSyncCommitmentPageDecodeV1> for FastSyncCommitmentPageV1 {
    fn from(value: FastSyncCommitmentPageDecodeV1) -> Self {
        Self {
            contract_version: value.contract_version,
            chain_id: value.chain_id,
            transfer_id: value.transfer_id,
            start_index: value.start_index,
            commitments: value.commitments.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct FastSyncChunkRequestDecodeV1 {
    contract_version: u32,
    chain_id: String,
    transfer_id: String,
    chunk_indices: BoundedVec<u32, P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1>,
}

impl From<FastSyncChunkRequestDecodeV1> for FastSyncChunkRequestV1 {
    fn from(value: FastSyncChunkRequestDecodeV1) -> Self {
        Self {
            contract_version: value.contract_version,
            chain_id: value.chain_id,
            transfer_id: value.transfer_id,
            chunk_indices: value.chunk_indices.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct FastSyncChunkRawV1<'a> {
    contract_version: u32,
    chain_id: String,
    transfer_id: String,
    chunk_index: u32,
    chunk_commitment: String,
    #[serde(borrow)]
    data_hex: &'a RawValue,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FastSyncKindV1 {
    CapabilityProbe,
    Capabilities,
    GetTransferSummary,
    TransferSummary,
    GetCommitmentPage,
    CommitmentPage,
    GetChunks,
    Chunk,
}

#[derive(Debug, Deserialize)]
struct FastSyncWireRawV1<'a> {
    fast_sync_type: FastSyncKindV1,
    #[serde(borrow)]
    payload: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct FastSyncCarrierRawV1<'a> {
    target_peer_id: String,
    #[serde(borrow)]
    wire: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct FastSyncExtensionRawV1<'a> {
    #[serde(default, borrow, rename = "pulsedag_fast_sync_v1")]
    fast_sync: Option<&'a RawValue>,
}

#[derive(Debug, Deserialize)]
struct FastSyncTargetV1<'a> {
    #[serde(default, borrow)]
    target_peer_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct FastSyncTargetExtensionV1<'a> {
    #[serde(default, borrow, rename = "pulsedag_fast_sync_v1")]
    fast_sync: Option<FastSyncTargetV1<'a>>,
}

#[derive(Debug, Deserialize)]
struct ChainOnlyV1 {
    chain_id: String,
}

#[derive(Debug, Deserialize)]
struct CommitmentPageRequestDecodeV1 {
    chain_id: String,
    transfer_id: String,
    start_index: u32,
    limit: u16,
}

fn parse_payload<'de, T>(raw: &'de RawValue, field: &str) -> Result<T, FastSyncCarrierErrorV1>
where
    T: Deserialize<'de>,
{
    serde_json::from_str(raw.get())
        .map_err(|error| FastSyncCarrierErrorV1::Json(format!("fast-sync {field}: {error}")))
}

fn decode_chunk_payload(raw: &RawValue) -> Result<FastSyncChunkV1, FastSyncCarrierErrorV1> {
    let raw: FastSyncChunkRawV1<'_> = parse_payload(raw, "chunk payload")?;
    if raw.data_hex.get().len() > P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1 * 2 + 2 {
        return Err(FastSyncCarrierErrorV1::FastSync(invalid_shape(
            "fast-sync chunk data_hex exceeds the pre-decode wire bound",
        )));
    }
    let data_hex: String = serde_json::from_str(raw.data_hex.get()).map_err(|error| {
        FastSyncCarrierErrorV1::Json(format!("fast-sync chunk data_hex: {error}"))
    })?;
    Ok(FastSyncChunkV1 {
        contract_version: raw.contract_version,
        chain_id: raw.chain_id,
        transfer_id: raw.transfer_id,
        chunk_index: raw.chunk_index,
        chunk_commitment: raw.chunk_commitment,
        data_hex,
    })
}

fn decode_wire(raw: &RawValue) -> Result<FastSyncWireV1, FastSyncCarrierErrorV1> {
    let wire: FastSyncWireRawV1<'_> = parse_payload(raw, "wire")?;
    let decoded = match wire.fast_sync_type {
        FastSyncKindV1::CapabilityProbe => {
            let payload: ChainOnlyV1 = parse_payload(wire.payload, "capability probe")?;
            FastSyncWireV1::CapabilityProbe {
                chain_id: payload.chain_id,
            }
        }
        FastSyncKindV1::Capabilities => {
            FastSyncWireV1::Capabilities(parse_payload(wire.payload, "capabilities")?)
        }
        FastSyncKindV1::GetTransferSummary => {
            let payload: ChainOnlyV1 = parse_payload(wire.payload, "transfer summary request")?;
            FastSyncWireV1::GetTransferSummary {
                chain_id: payload.chain_id,
            }
        }
        FastSyncKindV1::TransferSummary => {
            FastSyncWireV1::TransferSummary(parse_payload(wire.payload, "transfer summary")?)
        }
        FastSyncKindV1::GetCommitmentPage => {
            let payload: CommitmentPageRequestDecodeV1 =
                parse_payload(wire.payload, "commitment page request")?;
            FastSyncWireV1::GetCommitmentPage {
                chain_id: payload.chain_id,
                transfer_id: payload.transfer_id,
                start_index: payload.start_index,
                limit: payload.limit,
            }
        }
        FastSyncKindV1::CommitmentPage => {
            let payload: FastSyncCommitmentPageDecodeV1 =
                parse_payload(wire.payload, "commitment page")?;
            FastSyncWireV1::CommitmentPage(payload.into())
        }
        FastSyncKindV1::GetChunks => {
            let payload: FastSyncChunkRequestDecodeV1 =
                parse_payload(wire.payload, "chunk request")?;
            FastSyncWireV1::GetChunks(payload.into())
        }
        FastSyncKindV1::Chunk => FastSyncWireV1::Chunk(decode_chunk_payload(wire.payload)?),
    };
    decoded
        .validate_shape()
        .map_err(FastSyncCarrierErrorV1::FastSync)?;
    Ok(decoded)
}

fn decode_extension(bytes: &[u8]) -> Result<Option<FastSyncCarrierV1>, FastSyncCarrierErrorV1> {
    let extension: FastSyncExtensionRawV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    extension
        .fast_sync
        .map(|raw| {
            let carrier: FastSyncCarrierRawV1<'_> = parse_payload(raw, "carrier")?;
            Ok(FastSyncCarrierV1 {
                target_peer_id: carrier.target_peer_id,
                wire: decode_wire(carrier.wire)?,
            })
        })
        .transpose()
}

fn validate_carrier_for_message(
    message: &NetworkMessage,
    carrier: &FastSyncCarrierV1,
) -> Result<(), FastSyncCarrierErrorV1> {
    if !matches!(message, NetworkMessage::Tips { .. }) {
        return Err(FastSyncCarrierErrorV1::UnsupportedCarrierKind {
            kind: message.kind().to_string(),
        });
    }
    if carrier.target_peer_id.trim().is_empty() {
        return Err(FastSyncCarrierErrorV1::EmptyTargetPeerId);
    }
    if carrier.target_peer_id.len() > P2P_FAST_SYNC_MAX_TARGET_PEER_ID_BYTES_V1 {
        return Err(FastSyncCarrierErrorV1::TargetPeerIdTooLarge {
            observed: carrier.target_peer_id.len(),
            maximum: P2P_FAST_SYNC_MAX_TARGET_PEER_ID_BYTES_V1,
        });
    }
    carrier
        .wire
        .validate_for_chain(message.chain_id())
        .map_err(FastSyncCarrierErrorV1::FastSync)
}

fn ensure_extension_transport_bound(bytes: &[u8]) -> Result<bool, FastSyncCarrierErrorV1> {
    let presence: FastSyncExtensionRawV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    let present = presence.fast_sync.is_some();
    if present && bytes.len() > P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1 {
        return Err(FastSyncCarrierErrorV1::CarrierTooLarge {
            observed: bytes.len(),
            maximum: P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1,
        });
    }
    Ok(present)
}

pub fn attach_fast_sync_carrier_v1(
    encoded_network_message: &[u8],
    carrier: &FastSyncCarrierV1,
) -> Result<Vec<u8>, FastSyncCarrierErrorV1> {
    let message: NetworkMessage = serde_json::from_slice(encoded_network_message)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    validate_carrier_for_message(&message, carrier)?;
    let mut value: Value = serde_json::from_slice(encoded_network_message)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or(FastSyncCarrierErrorV1::InvalidJsonRoot)?;
    object.insert(
        FAST_SYNC_EXTENSION_FIELD_V1.to_string(),
        serde_json::to_value(carrier)
            .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?,
    );
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    if encoded.len() > P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1 {
        return Err(FastSyncCarrierErrorV1::CarrierTooLarge {
            observed: encoded.len(),
            maximum: P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1,
        });
    }
    Ok(encoded)
}

pub fn encode_network_message_with_fast_sync_v1(
    message: &NetworkMessage,
    carrier: Option<&FastSyncCarrierV1>,
) -> Result<Vec<u8>, FastSyncCarrierErrorV1> {
    let legacy = serde_json::to_vec(message)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    match carrier {
        Some(carrier) => attach_fast_sync_carrier_v1(&legacy, carrier),
        None => Ok(legacy),
    }
}

pub fn decode_network_message_with_fast_sync_v1(
    bytes: &[u8],
) -> Result<DecodedNetworkMessageWithFastSyncV1, FastSyncCarrierErrorV1> {
    let present = ensure_extension_transport_bound(bytes)?;
    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    let fast_sync = if present {
        decode_extension(bytes)?
    } else {
        None
    };
    if let Some(carrier) = fast_sync.as_ref() {
        validate_carrier_for_message(&message, carrier)?;
    }
    Ok(DecodedNetworkMessageWithFastSyncV1 { message, fast_sync })
}

pub fn decode_network_message_with_fast_sync_for_peer_v1(
    bytes: &[u8],
    local_peer_id: &str,
) -> Result<DecodedNetworkMessageWithFastSyncV1, FastSyncCarrierErrorV1> {
    let target_extension: FastSyncTargetExtensionV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    let has_extension = target_extension.fast_sync.is_some();
    let addressed = target_extension
        .fast_sync
        .as_ref()
        .and_then(|target| target.target_peer_id)
        .is_some_and(|target| target == local_peer_id);
    if has_extension && bytes.len() > P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1 {
        return Err(FastSyncCarrierErrorV1::CarrierTooLarge {
            observed: bytes.len(),
            maximum: P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1,
        });
    }
    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| FastSyncCarrierErrorV1::Json(error.to_string()))?;
    let fast_sync = if addressed {
        let carrier = decode_extension(bytes)?.ok_or_else(|| {
            FastSyncCarrierErrorV1::Json(
                "fast-sync target present without full carrier".to_string(),
            )
        })?;
        validate_carrier_for_message(&message, &carrier)?;
        Some(carrier)
    } else {
        None
    };
    Ok(DecodedNetworkMessageWithFastSyncV1 { message, fast_sync })
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>, FastSyncWireErrorV1> {
    if !value.len().is_multiple_of(2) {
        return Err(invalid_shape("fast-sync hex payload has odd length"));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let input = value.as_bytes();
    for pair in input.chunks_exact(2) {
        let high = lower_hex_nibble(pair[0])?;
        let low = lower_hex_nibble(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn lower_hex_nibble(value: u8) -> Result<u8, FastSyncWireErrorV1> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(invalid_shape(
            "fast-sync hex payload must use lowercase canonical hex",
        )),
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
        snapshot_transfer_chunk_digest_v1, snapshot_transfer_payload_digest_v1,
        GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "fast-sync-wire-testnet";
    const LOCAL_PEER: &str = "peer-fast-sync-local";

    fn identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            CHAIN_ID.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn tips() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: vec!["22".repeat(32)],
            inventory: None,
        }
    }

    fn capabilities() -> FastSyncCapabilitiesV1 {
        FastSyncCapabilitiesV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            genesis_hash: identity().genesis_hash,
            protocol_fingerprint: identity().fingerprint().unwrap(),
            manifest_version: 1,
            protocol_snapshot_bundle_format_version: 2,
            storage_schema_version: 1,
            payload_encoding: "bincode-1.3-fast-sync-bundle-v1".to_string(),
            max_chunk_bytes: P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1 as u32,
            max_commitments_per_page: P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 as u32,
        }
    }

    fn transfer_material(chunk_size: usize) -> (Vec<Vec<u8>>, Vec<String>, String, String) {
        let payload = (0..(chunk_size * 2 + 17))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let transfer_id = snapshot_transfer_payload_digest_v1(&payload);
        let chunks = payload
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        let commitments = chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| {
                snapshot_transfer_chunk_digest_v1(&transfer_id, index as u32, chunk)
            })
            .collect::<Vec<_>>();
        let commitment_set_id =
            snapshot_transfer_commitment_set_digest_v1(&transfer_id, &commitments);
        (chunks, commitments, transfer_id, commitment_set_id)
    }

    fn summary(chunk_size: usize) -> (FastSyncTransferSummaryV1, Vec<Vec<u8>>, Vec<String>) {
        let (chunks, commitments, transfer_id, commitment_set_id) = transfer_material(chunk_size);
        let payload_len = chunks.iter().map(Vec::len).sum::<usize>() as u64;
        (
            FastSyncTransferSummaryV1 {
                contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                chain_id: CHAIN_ID.to_string(),
                genesis_hash: identity().genesis_hash,
                protocol_fingerprint: identity().fingerprint().unwrap(),
                manifest_version: 1,
                protocol_snapshot_bundle_format_version: 2,
                storage_schema_version: 1,
                payload_encoding: "bincode-1.3-fast-sync-bundle-v1".to_string(),
                transfer_id,
                commitment_set_id,
                payload_len,
                chunk_size: chunk_size as u32,
                chunk_count: chunks.len() as u32,
                best_height: 42,
                selected_tip: "33".repeat(32),
                state_commitment: "44".repeat(32),
                prune_boundary_height: Some(21),
                snapshot_generation: 7,
                accepted_storage_generation: 7,
                delta_start_generation: 7,
                delta_end_generation: 9,
            },
            chunks,
            commitments,
        )
    }

    fn carrier(wire: FastSyncWireV1) -> FastSyncCarrierV1 {
        FastSyncCarrierV1 {
            target_peer_id: LOCAL_PEER.to_string(),
            wire,
        }
    }

    #[test]
    fn legacy_decode_ignores_fast_sync_extension_and_targeted_decode_recovers_it() {
        let encoded = encode_network_message_with_fast_sync_v1(
            &tips(),
            Some(&carrier(FastSyncWireV1::Capabilities(capabilities()))),
        )
        .unwrap();
        assert!(serde_json::from_slice::<NetworkMessage>(&encoded).is_ok());
        let decoded =
            decode_network_message_with_fast_sync_for_peer_v1(&encoded, LOCAL_PEER).unwrap();
        assert!(matches!(
            decoded.fast_sync.map(|carrier| carrier.wire),
            Some(FastSyncWireV1::Capabilities(_))
        ));
    }

    #[test]
    fn bystander_skips_malformed_fast_sync_payload_but_target_fails_closed() {
        let mut base = serde_json::to_string(&tips()).unwrap();
        assert_eq!(base.pop(), Some('}'));
        let oversized = vec!["aa".repeat(32); P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 + 1];
        let payload = serde_json::json!({
            "contract_version": P2P_FAST_SYNC_CONTRACT_VERSION,
            "chain_id": CHAIN_ID,
            "transfer_id": "bb".repeat(32),
            "start_index": 0,
            "commitments": oversized,
        });
        let encoded = format!(
            "{base},\"{FAST_SYNC_EXTENSION_FIELD_V1}\":{{\"target_peer_id\":\"{LOCAL_PEER}\",\"wire\":{{\"fast_sync_type\":\"commitment_page\",\"payload\":{payload}}}}}}}"
        )
        .into_bytes();
        let bystander =
            decode_network_message_with_fast_sync_for_peer_v1(&encoded, "other-peer").unwrap();
        assert!(bystander.fast_sync.is_none());
        assert!(decode_network_message_with_fast_sync_for_peer_v1(&encoded, LOCAL_PEER).is_err());
    }

    #[test]
    fn commitment_pages_are_bounded_complete_and_root_bound() {
        let (summary, _, commitments) = summary(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1);
        let pages = vec![
            FastSyncCommitmentPageV1 {
                contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                chain_id: CHAIN_ID.to_string(),
                transfer_id: summary.transfer_id.clone(),
                start_index: 0,
                commitments: commitments[..2].to_vec(),
            },
            FastSyncCommitmentPageV1 {
                contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                chain_id: CHAIN_ID.to_string(),
                transfer_id: summary.transfer_id.clone(),
                start_index: 2,
                commitments: commitments[2..].to_vec(),
            },
        ];
        assert_eq!(
            verify_fast_sync_commitment_pages_v1(&summary, &pages).unwrap(),
            commitments
        );
        let mut tampered = summary.clone();
        tampered.commitment_set_id = "ff".repeat(32);
        assert_eq!(
            verify_fast_sync_commitment_pages_v1(&tampered, &pages),
            Err(FastSyncWireErrorV1::CommitmentSetMismatch)
        );
    }

    #[test]
    fn chunk_requires_summary_commitment_and_exact_indexed_digest() {
        let (summary, chunks, commitments) = summary(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1);
        let chunk = FastSyncChunkV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            transfer_id: summary.transfer_id.clone(),
            chunk_index: 1,
            chunk_commitment: commitments[1].clone(),
            data_hex: encode_lower_hex(&chunks[1]),
        };
        assert_eq!(
            chunk
                .decode_verified_bytes(&summary, &commitments[1])
                .unwrap(),
            chunks[1]
        );
        let mut tampered = chunk.clone();
        tampered.data_hex.replace_range(0..2, "00");
        assert_eq!(
            tampered.decode_verified_bytes(&summary, &commitments[1]),
            Err(FastSyncWireErrorV1::ChunkCommitmentMismatch)
        );
    }

    #[test]
    fn maximum_wire_chunk_stays_below_live_carrier_ceiling() {
        let bytes = vec![0x5a; P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1];
        let transfer_id = snapshot_transfer_payload_digest_v1(&bytes);
        let chunk = FastSyncChunkV1::from_bytes(CHAIN_ID, transfer_id, 0, &bytes).unwrap();
        let wire = FastSyncWireV1::Chunk(chunk);
        let encoded =
            encode_network_message_with_fast_sync_v1(&tips(), Some(&carrier(wire))).unwrap();
        assert!(encoded.len() <= P2P_FAST_SYNC_TRANSPORT_MAX_BYTES_V1);
    }

    #[test]
    fn chunk_requests_are_canonical_and_summary_bounded() {
        let (summary, _, _) = summary(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1);
        let request = FastSyncChunkRequestV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            transfer_id: summary.transfer_id.clone(),
            chunk_indices: vec![0, 2],
        };
        request.validate_against_summary(&summary).unwrap();
        let mut noncanonical = request.clone();
        noncanonical.chunk_indices = vec![2, 1];
        assert!(noncanonical.validate_against_summary(&summary).is_err());
        let mut out_of_range = request;
        out_of_range.chunk_indices = vec![summary.chunk_count];
        assert!(out_of_range.validate_against_summary(&summary).is_err());
    }

    #[test]
    fn expected_protocol_identity_is_checked_before_bootstrap_use() {
        let caps = capabilities();
        caps.validate_for_expected(&identity()).unwrap();
        let mut wrong = identity();
        wrong.chain_id = "other-chain".to_string();
        assert!(caps.validate_for_expected(&wrong).is_err());

        let (summary, _, _) = summary(P2P_FAST_SYNC_MIN_CHUNK_BYTES_V1);
        summary.validate_for_expected(&identity()).unwrap();
        assert!(summary.validate_for_expected(&wrong).is_err());
    }
}
