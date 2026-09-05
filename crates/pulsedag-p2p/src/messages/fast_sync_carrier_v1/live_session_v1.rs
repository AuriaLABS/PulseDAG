use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{
    snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
    snapshot_transfer_payload_digest_v1, ProtocolActivationIdentity,
};

use super::{
    FastSyncCapabilitiesV1, FastSyncChunkRequestV1, FastSyncChunkV1,
    FastSyncCommitmentPageV1, FastSyncTransferSummaryV1, FastSyncWireErrorV1, FastSyncWireV1,
    P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1, P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1,
    P2P_FAST_SYNC_CONTRACT_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncSessionErrorV1 {
    Wire(FastSyncWireErrorV1),
    UnexpectedRequest { kind: &'static str },
    UnexpectedResponse { kind: &'static str },
    MissingCapabilities,
    MissingTransferSummary,
    TransferIdentity(String),
    CommitmentSequence { expected_start: u32, observed_start: u32 },
    CommitmentSetMismatch,
    ChunkConflict { chunk_index: u32 },
    PayloadCommitmentMismatch,
    PayloadLengthMismatch { expected: u64, observed: u64 },
    ArithmeticOverflow,
}

impl From<FastSyncWireErrorV1> for FastSyncSessionErrorV1 {
    fn from(value: FastSyncWireErrorV1) -> Self {
        Self::Wire(value)
    }
}

fn transfer_identity_error(message: impl Into<String>) -> FastSyncSessionErrorV1 {
    FastSyncSessionErrorV1::TransferIdentity(message.into())
}

fn validate_summary_against_capabilities(
    summary: &FastSyncTransferSummaryV1,
    capabilities: &FastSyncCapabilitiesV1,
) -> Result<(), FastSyncSessionErrorV1> {
    if summary.chain_id != capabilities.chain_id
        || summary.genesis_hash != capabilities.genesis_hash
        || summary.protocol_fingerprint != capabilities.protocol_fingerprint
    {
        return Err(transfer_identity_error(
            "fast-sync transfer summary identity differs from negotiated capabilities",
        ));
    }
    if summary.manifest_version != capabilities.manifest_version
        || summary.protocol_snapshot_bundle_format_version
            != capabilities.protocol_snapshot_bundle_format_version
        || summary.storage_schema_version != capabilities.storage_schema_version
        || summary.payload_encoding != capabilities.payload_encoding
    {
        return Err(transfer_identity_error(
            "fast-sync transfer summary version surface differs from negotiated capabilities",
        ));
    }
    if summary.chunk_size > capabilities.max_chunk_bytes {
        return Err(transfer_identity_error(
            "fast-sync transfer chunk size exceeds the negotiated peer capability",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct FastSyncServingSessionV1 {
    expected: ProtocolActivationIdentity,
    capabilities: FastSyncCapabilitiesV1,
    summary: FastSyncTransferSummaryV1,
    commitments: Vec<String>,
    chunks: Vec<Vec<u8>>,
}

impl FastSyncServingSessionV1 {
    pub fn new(
        expected: ProtocolActivationIdentity,
        capabilities: FastSyncCapabilitiesV1,
        summary: FastSyncTransferSummaryV1,
        commitments: Vec<String>,
        chunks: Vec<Vec<u8>>,
    ) -> Result<Self, FastSyncSessionErrorV1> {
        capabilities.validate_for_expected(&expected)?;
        summary.validate_for_expected(&expected)?;
        validate_summary_against_capabilities(&summary, &capabilities)?;

        if commitments.len() != summary.chunk_count as usize
            || chunks.len() != summary.chunk_count as usize
        {
            return Err(transfer_identity_error(
                "fast-sync serving material does not match transfer chunk_count",
            ));
        }

        let commitment_set_id =
            snapshot_transfer_commitment_set_digest_v1(&summary.transfer_id, &commitments);
        if commitment_set_id != summary.commitment_set_id {
            return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
        }

        let mut observed_payload_len = 0_u64;
        for (chunk_index, (commitment, chunk_bytes)) in
            commitments.iter().zip(chunks.iter()).enumerate()
        {
            let chunk_index = u32::try_from(chunk_index)
                .map_err(|_| FastSyncSessionErrorV1::ArithmeticOverflow)?;
            let wire_chunk = FastSyncChunkV1::from_bytes(
                summary.chain_id.clone(),
                summary.transfer_id.clone(),
                chunk_index,
                chunk_bytes,
            )?;
            if wire_chunk.chunk_commitment != *commitment {
                return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
            }
            wire_chunk.decode_verified_bytes(&summary, commitment)?;
            observed_payload_len = observed_payload_len
                .checked_add(chunk_bytes.len() as u64)
                .ok_or(FastSyncSessionErrorV1::ArithmeticOverflow)?;
        }
        if observed_payload_len != summary.payload_len {
            return Err(FastSyncSessionErrorV1::PayloadLengthMismatch {
                expected: summary.payload_len,
                observed: observed_payload_len,
            });
        }

        Ok(Self {
            expected,
            capabilities,
            summary,
            commitments,
            chunks,
        })
    }

    pub fn summary(&self) -> &FastSyncTransferSummaryV1 {
        &self.summary
    }

    pub fn capabilities(&self) -> &FastSyncCapabilitiesV1 {
        &self.capabilities
    }

    pub fn handle_request(
        &self,
        request: &FastSyncWireV1,
    ) -> Result<Vec<FastSyncWireV1>, FastSyncSessionErrorV1> {
        request.validate_for_chain(&self.expected.chain_id)?;
        let responses = match request {
            FastSyncWireV1::CapabilityProbe { .. } => {
                vec![FastSyncWireV1::Capabilities(self.capabilities.clone())]
            }
            FastSyncWireV1::GetTransferSummary { .. } => {
                vec![FastSyncWireV1::TransferSummary(self.summary.clone())]
            }
            FastSyncWireV1::GetCommitmentPage {
                transfer_id,
                start_index,
                limit,
                ..
            } => {
                if transfer_id != &self.summary.transfer_id {
                    return Err(transfer_identity_error(
                        "fast-sync commitment-page request references another transfer",
                    ));
                }
                let start = *start_index as usize;
                if start >= self.commitments.len() {
                    return Err(transfer_identity_error(
                        "fast-sync commitment-page request starts beyond chunk_count",
                    ));
                }
                let end = start
                    .checked_add(usize::from(*limit))
                    .ok_or(FastSyncSessionErrorV1::ArithmeticOverflow)?
                    .min(self.commitments.len());
                let page = FastSyncCommitmentPageV1 {
                    contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                    chain_id: self.summary.chain_id.clone(),
                    transfer_id: self.summary.transfer_id.clone(),
                    start_index: *start_index,
                    commitments: self.commitments[start..end].to_vec(),
                };
                page.validate_against_summary(&self.summary)?;
                vec![FastSyncWireV1::CommitmentPage(page)]
            }
            FastSyncWireV1::GetChunks(chunk_request) => {
                chunk_request.validate_against_summary(&self.summary)?;
                let mut responses = Vec::with_capacity(chunk_request.chunk_indices.len());
                for chunk_index in &chunk_request.chunk_indices {
                    let chunk = self
                        .chunks
                        .get(*chunk_index as usize)
                        .ok_or_else(|| {
                            transfer_identity_error(
                                "fast-sync chunk request references unavailable serving material",
                            )
                        })?;
                    let wire_chunk = FastSyncChunkV1::from_bytes(
                        self.summary.chain_id.clone(),
                        self.summary.transfer_id.clone(),
                        *chunk_index,
                        chunk,
                    )?;
                    if wire_chunk.chunk_commitment
                        != self.commitments[*chunk_index as usize]
                    {
                        return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
                    }
                    responses.push(FastSyncWireV1::Chunk(wire_chunk));
                }
                responses
            }
            FastSyncWireV1::Capabilities(_)
            | FastSyncWireV1::TransferSummary(_)
            | FastSyncWireV1::CommitmentPage(_)
            | FastSyncWireV1::Chunk(_) => {
                return Err(FastSyncSessionErrorV1::UnexpectedRequest {
                    kind: request.kind(),
                });
            }
        };
        for response in &responses {
            response.validate_for_chain(&self.expected.chain_id)?;
        }
        Ok(responses)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSyncDownloadProgressV1 {
    pub capabilities_verified: bool,
    pub summary_verified: bool,
    pub commitment_count: u32,
    pub chunk_count: u32,
    pub verified_chunks: u32,
    pub complete: bool,
}

#[derive(Debug, Clone)]
pub struct FastSyncDownloadSessionV1 {
    expected: ProtocolActivationIdentity,
    capabilities: Option<FastSyncCapabilitiesV1>,
    summary: Option<FastSyncTransferSummaryV1>,
    commitments: Vec<String>,
    commitments_verified: bool,
    chunks: BTreeMap<u32, Vec<u8>>,
    capability_probe_inflight: bool,
    summary_request_inflight: bool,
    commitment_page_inflight: bool,
    inflight_chunks: BTreeSet<u32>,
}

impl FastSyncDownloadSessionV1 {
    pub fn new(expected: ProtocolActivationIdentity) -> Self {
        Self {
            expected,
            capabilities: None,
            summary: None,
            commitments: Vec::new(),
            commitments_verified: false,
            chunks: BTreeMap::new(),
            capability_probe_inflight: false,
            summary_request_inflight: false,
            commitment_page_inflight: false,
            inflight_chunks: BTreeSet::new(),
        }
    }

    pub fn progress(&self) -> FastSyncDownloadProgressV1 {
        let chunk_count = self.summary.as_ref().map_or(0, |summary| summary.chunk_count);
        FastSyncDownloadProgressV1 {
            capabilities_verified: self.capabilities.is_some(),
            summary_verified: self.summary.is_some(),
            commitment_count: self.commitments.len() as u32,
            chunk_count,
            verified_chunks: self.chunks.len() as u32,
            complete: self.is_complete(),
        }
    }

    pub fn verified_chunk_indices(&self) -> Vec<u32> {
        self.chunks.keys().copied().collect()
    }

    pub fn retry_inflight(&mut self) {
        self.capability_probe_inflight = false;
        self.summary_request_inflight = false;
        self.commitment_page_inflight = false;
        self.inflight_chunks.clear();
    }

    pub fn next_request(&mut self) -> Result<Option<FastSyncWireV1>, FastSyncSessionErrorV1> {
        if self.capabilities.is_none() {
            if self.capability_probe_inflight {
                return Ok(None);
            }
            self.capability_probe_inflight = true;
            return Ok(Some(FastSyncWireV1::CapabilityProbe {
                chain_id: self.expected.chain_id.clone(),
            }));
        }

        if self.summary.is_none() {
            if self.summary_request_inflight {
                return Ok(None);
            }
            self.summary_request_inflight = true;
            return Ok(Some(FastSyncWireV1::GetTransferSummary {
                chain_id: self.expected.chain_id.clone(),
            }));
        }

        let summary = self
            .summary
            .as_ref()
            .ok_or(FastSyncSessionErrorV1::MissingTransferSummary)?;
        if !self.commitments_verified {
            if self.commitment_page_inflight {
                return Ok(None);
            }
            let start_index = u32::try_from(self.commitments.len())
                .map_err(|_| FastSyncSessionErrorV1::ArithmeticOverflow)?;
            if start_index >= summary.chunk_count {
                return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
            }
            let capabilities = self
                .capabilities
                .as_ref()
                .ok_or(FastSyncSessionErrorV1::MissingCapabilities)?;
            let limit = capabilities
                .max_commitments_per_page
                .min(P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 as u32)
                .min(summary.chunk_count - start_index);
            let limit = u16::try_from(limit)
                .map_err(|_| FastSyncSessionErrorV1::ArithmeticOverflow)?;
            self.commitment_page_inflight = true;
            return Ok(Some(FastSyncWireV1::GetCommitmentPage {
                chain_id: summary.chain_id.clone(),
                transfer_id: summary.transfer_id.clone(),
                start_index,
                limit,
            }));
        }

        let missing = (0..summary.chunk_count)
            .filter(|index| {
                !self.chunks.contains_key(index) && !self.inflight_chunks.contains(index)
            })
            .take(P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1)
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(None);
        }
        for index in &missing {
            self.inflight_chunks.insert(*index);
        }
        Ok(Some(FastSyncWireV1::GetChunks(FastSyncChunkRequestV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: summary.chain_id.clone(),
            transfer_id: summary.transfer_id.clone(),
            chunk_indices: missing,
        })))
    }

    pub fn accept_response(
        &mut self,
        response: FastSyncWireV1,
    ) -> Result<(), FastSyncSessionErrorV1> {
        response.validate_for_chain(&self.expected.chain_id)?;
        match response {
            FastSyncWireV1::Capabilities(capabilities) => {
                capabilities.validate_for_expected(&self.expected)?;
                if let Some(existing) = self.capabilities.as_ref() {
                    if existing != &capabilities {
                        return Err(transfer_identity_error(
                            "fast-sync peer changed negotiated capabilities mid-session",
                        ));
                    }
                } else {
                    self.capabilities = Some(capabilities);
                }
                self.capability_probe_inflight = false;
            }
            FastSyncWireV1::TransferSummary(summary) => {
                summary.validate_for_expected(&self.expected)?;
                let capabilities = self
                    .capabilities
                    .as_ref()
                    .ok_or(FastSyncSessionErrorV1::MissingCapabilities)?;
                validate_summary_against_capabilities(&summary, capabilities)?;
                if let Some(existing) = self.summary.as_ref() {
                    if existing != &summary {
                        return Err(transfer_identity_error(
                            "fast-sync peer changed transfer summary mid-session",
                        ));
                    }
                } else {
                    self.summary = Some(summary);
                }
                self.summary_request_inflight = false;
            }
            FastSyncWireV1::CommitmentPage(page) => {
                let summary = self
                    .summary
                    .as_ref()
                    .ok_or(FastSyncSessionErrorV1::MissingTransferSummary)?;
                page.validate_against_summary(summary)?;
                let expected_start = u32::try_from(self.commitments.len())
                    .map_err(|_| FastSyncSessionErrorV1::ArithmeticOverflow)?;
                if page.start_index != expected_start {
                    return Err(FastSyncSessionErrorV1::CommitmentSequence {
                        expected_start,
                        observed_start: page.start_index,
                    });
                }
                self.commitments.extend(page.commitments);
                self.commitment_page_inflight = false;
                if self.commitments.len() == summary.chunk_count as usize {
                    let root = snapshot_transfer_commitment_set_digest_v1(
                        &summary.transfer_id,
                        &self.commitments,
                    );
                    if root != summary.commitment_set_id {
                        return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
                    }
                    self.commitments_verified = true;
                }
            }
            FastSyncWireV1::Chunk(chunk) => {
                if !self.commitments_verified {
                    return Err(FastSyncSessionErrorV1::CommitmentSetMismatch);
                }
                let summary = self
                    .summary
                    .as_ref()
                    .ok_or(FastSyncSessionErrorV1::MissingTransferSummary)?;
                let commitment = self
                    .commitments
                    .get(chunk.chunk_index as usize)
                    .ok_or(FastSyncSessionErrorV1::CommitmentSetMismatch)?;
                let chunk_index = chunk.chunk_index;
                let verified = chunk.decode_verified_bytes(summary, commitment)?;
                if let Some(existing) = self.chunks.get(&chunk_index) {
                    if existing != &verified {
                        return Err(FastSyncSessionErrorV1::ChunkConflict { chunk_index });
                    }
                } else {
                    self.chunks.insert(chunk_index, verified);
                }
                self.inflight_chunks.remove(&chunk_index);
            }
            FastSyncWireV1::CapabilityProbe { .. }
            | FastSyncWireV1::GetTransferSummary { .. }
            | FastSyncWireV1::GetCommitmentPage { .. }
            | FastSyncWireV1::GetChunks(_) => {
                return Err(FastSyncSessionErrorV1::UnexpectedResponse {
                    kind: response.kind(),
                });
            }
        }
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.summary.as_ref().is_some_and(|summary| {
            self.commitments_verified && self.chunks.len() == summary.chunk_count as usize
        })
    }

    pub fn completed_payload(&self) -> Result<Option<Vec<u8>>, FastSyncSessionErrorV1> {
        let Some(summary) = self.summary.as_ref() else {
            return Ok(None);
        };
        if !self.is_complete() {
            return Ok(None);
        }
        let payload_capacity = usize::try_from(summary.payload_len)
            .map_err(|_| FastSyncSessionErrorV1::ArithmeticOverflow)?;
        let mut payload = Vec::with_capacity(payload_capacity);
        for chunk_index in 0..summary.chunk_count {
            let chunk = self
                .chunks
                .get(&chunk_index)
                .ok_or(FastSyncSessionErrorV1::ChunkConflict { chunk_index })?;
            payload.extend_from_slice(chunk);
        }
        let observed_len = payload.len() as u64;
        if observed_len != summary.payload_len {
            return Err(FastSyncSessionErrorV1::PayloadLengthMismatch {
                expected: summary.payload_len,
                observed: observed_len,
            });
        }
        if snapshot_transfer_payload_digest_v1(&payload) != summary.transfer_id {
            return Err(FastSyncSessionErrorV1::PayloadCommitmentMismatch);
        }
        Ok(Some(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{snapshot_transfer_chunk_digest_v1, GHOSTDAG_V1_ORDERING_VERSION};

    const CHAIN_ID: &str = "fast-sync-live-session-testnet";

    fn identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            CHAIN_ID.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn fixture() -> (
        Vec<u8>,
        FastSyncServingSessionV1,
        FastSyncDownloadSessionV1,
    ) {
        let chunk_size = 1024_usize;
        let payload = (0..(chunk_size * 5 + 17))
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
        let expected = identity();
        let capabilities = FastSyncCapabilitiesV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            genesis_hash: expected.genesis_hash.clone(),
            protocol_fingerprint: expected.fingerprint().unwrap(),
            manifest_version: 1,
            protocol_snapshot_bundle_format_version: 2,
            storage_schema_version: 1,
            payload_encoding: "bincode-1.3-fast-sync-bundle-v1".to_string(),
            max_chunk_bytes: 24 * 1024,
            max_commitments_per_page: 2,
        };
        let summary = FastSyncTransferSummaryV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: CHAIN_ID.to_string(),
            genesis_hash: expected.genesis_hash.clone(),
            protocol_fingerprint: expected.fingerprint().unwrap(),
            manifest_version: capabilities.manifest_version,
            protocol_snapshot_bundle_format_version: capabilities
                .protocol_snapshot_bundle_format_version,
            storage_schema_version: capabilities.storage_schema_version,
            payload_encoding: capabilities.payload_encoding.clone(),
            transfer_id,
            commitment_set_id,
            payload_len: payload.len() as u64,
            chunk_size: chunk_size as u32,
            chunk_count: chunks.len() as u32,
            best_height: 42,
            selected_tip: "22".repeat(32),
            state_commitment: "33".repeat(32),
            prune_boundary_height: Some(21),
            snapshot_generation: 7,
            accepted_storage_generation: 7,
            delta_start_generation: 7,
            delta_end_generation: 8,
        };
        let server = FastSyncServingSessionV1::new(
            expected.clone(),
            capabilities,
            summary,
            commitments,
            chunks,
        )
        .unwrap();
        let downloader = FastSyncDownloadSessionV1::new(expected);
        (payload, server, downloader)
    }

    fn drive_one_round(
        server: &FastSyncServingSessionV1,
        downloader: &mut FastSyncDownloadSessionV1,
    ) -> bool {
        let Some(request) = downloader.next_request().unwrap() else {
            return false;
        };
        for response in server.handle_request(&request).unwrap() {
            downloader.accept_response(response).unwrap();
        }
        true
    }

    #[test]
    fn server_and_downloader_complete_verified_payload_over_wire_messages() {
        let (payload, server, mut downloader) = fixture();
        for _ in 0..32 {
            if downloader.is_complete() {
                break;
            }
            assert!(drive_one_round(&server, &mut downloader));
        }
        assert!(downloader.is_complete());
        assert_eq!(downloader.completed_payload().unwrap().unwrap(), payload);
        let progress = downloader.progress();
        assert_eq!(progress.verified_chunks, progress.chunk_count);
        assert!(progress.complete);
    }

    #[test]
    fn retry_keeps_verified_chunks_and_only_requests_missing_work() {
        let (_, server, mut downloader) = fixture();
        while !downloader.progress().summary_verified || !downloader.commitments_verified {
            assert!(drive_one_round(&server, &mut downloader));
        }
        let request = downloader.next_request().unwrap().unwrap();
        let responses = server.handle_request(&request).unwrap();
        let first = responses.into_iter().next().unwrap();
        downloader.accept_response(first).unwrap();
        assert_eq!(downloader.verified_chunk_indices(), vec![0]);
        downloader.retry_inflight();
        let FastSyncWireV1::GetChunks(request) = downloader.next_request().unwrap().unwrap() else {
            panic!("expected resumed chunk request");
        };
        assert!(!request.chunk_indices.contains(&0));
        assert!(request.chunk_indices.contains(&1));
    }

    #[test]
    fn tampered_chunk_fails_closed_without_marking_it_verified() {
        let (_, server, mut downloader) = fixture();
        while !downloader.commitments_verified {
            assert!(drive_one_round(&server, &mut downloader));
        }
        let request = downloader.next_request().unwrap().unwrap();
        let mut responses = server.handle_request(&request).unwrap();
        let FastSyncWireV1::Chunk(mut chunk) = responses.remove(0) else {
            panic!("expected chunk response");
        };
        chunk.data_hex.replace_range(0..2, "00");
        assert!(downloader
            .accept_response(FastSyncWireV1::Chunk(chunk))
            .is_err());
        assert!(downloader.verified_chunk_indices().is_empty());
    }

    #[test]
    fn server_rejects_requests_for_another_transfer() {
        let (_, server, _) = fixture();
        let request = FastSyncWireV1::GetCommitmentPage {
            chain_id: CHAIN_ID.to_string(),
            transfer_id: "ff".repeat(32),
            start_index: 0,
            limit: 1,
        };
        assert!(server.handle_request(&request).is_err());
    }

    #[test]
    fn downloader_rejects_wrong_protocol_identity_before_transfer_use() {
        let (_, server, _) = fixture();
        let mut wrong = identity();
        wrong.chain_id = "other-fast-sync-chain".to_string();
        let mut downloader = FastSyncDownloadSessionV1::new(wrong);
        let request = downloader.next_request().unwrap().unwrap();
        assert!(server.handle_request(&request).is_err());
    }
}
