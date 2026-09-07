use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::ProtocolActivationIdentity;

use super::{
    verify_fast_sync_commitment_pages_v1, FastSyncCapabilitiesV1, FastSyncChunkRequestV1,
    FastSyncChunkV1, FastSyncCommitmentPageV1, FastSyncTransferSummaryV1, FastSyncWireErrorV1,
    P2P_FAST_SYNC_CONTRACT_VERSION, P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncMultiSourceErrorV1 {
    Wire(FastSyncWireErrorV1),
    EmptyPeerId,
    MissingCanonicalTransfer,
    CapabilitySurfaceMismatch {
        peer_id: String,
    },
    TransferSummaryMismatch {
        peer_id: String,
    },
    PeerNotEligible {
        peer_id: String,
    },
    CommitmentSetUnverified,
    CommitmentSetChanged,
    InvalidRequestBatchSize {
        observed: usize,
        maximum: usize,
    },
    ChunkNotAssigned {
        peer_id: String,
        chunk_index: u32,
    },
    ChunkAssignedToAnotherPeer {
        peer_id: String,
        assigned_peer_id: String,
        chunk_index: u32,
    },
    ChunkAlreadyVerified {
        chunk_index: u32,
    },
    ArithmeticOverflow,
}

impl From<FastSyncWireErrorV1> for FastSyncMultiSourceErrorV1 {
    fn from(value: FastSyncWireErrorV1) -> Self {
        Self::Wire(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSyncPeerChunkRequestV1 {
    pub peer_id: String,
    pub request: FastSyncChunkRequestV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSyncMultiSourceProgressV1 {
    pub eligible_peer_count: u32,
    pub canonical_transfer_selected: bool,
    pub commitment_set_verified: bool,
    pub chunk_count: u32,
    pub verified_chunks: u32,
    pub inflight_chunks: u32,
    pub complete: bool,
}

#[derive(Debug, Clone)]
pub struct FastSyncMultiSourcePlannerV1 {
    expected: ProtocolActivationIdentity,
    canonical_summary: Option<FastSyncTransferSummaryV1>,
    commitments: Option<Vec<String>>,
    eligible_peers: BTreeSet<String>,
    verified_chunks: BTreeSet<u32>,
    assignments: BTreeMap<u32, String>,
}

fn require_peer_id(peer_id: &str) -> Result<(), FastSyncMultiSourceErrorV1> {
    if peer_id.trim().is_empty() {
        Err(FastSyncMultiSourceErrorV1::EmptyPeerId)
    } else {
        Ok(())
    }
}

fn capability_surface_matches_summary(
    capabilities: &FastSyncCapabilitiesV1,
    summary: &FastSyncTransferSummaryV1,
) -> bool {
    capabilities.chain_id == summary.chain_id
        && capabilities.genesis_hash == summary.genesis_hash
        && capabilities.protocol_fingerprint == summary.protocol_fingerprint
        && capabilities.manifest_version == summary.manifest_version
        && capabilities.protocol_snapshot_bundle_format_version
            == summary.protocol_snapshot_bundle_format_version
        && capabilities.storage_schema_version == summary.storage_schema_version
        && capabilities.payload_encoding == summary.payload_encoding
        && summary.chunk_size <= capabilities.max_chunk_bytes
}

impl FastSyncMultiSourcePlannerV1 {
    pub fn new(expected: ProtocolActivationIdentity) -> Self {
        Self {
            expected,
            canonical_summary: None,
            commitments: None,
            eligible_peers: BTreeSet::new(),
            verified_chunks: BTreeSet::new(),
            assignments: BTreeMap::new(),
        }
    }

    pub fn canonical_summary(&self) -> Option<&FastSyncTransferSummaryV1> {
        self.canonical_summary.as_ref()
    }

    pub fn commitments(&self) -> Option<&[String]> {
        self.commitments.as_deref()
    }

    pub fn eligible_peers(&self) -> Vec<String> {
        self.eligible_peers.iter().cloned().collect()
    }

    pub fn verified_chunk_indices(&self) -> Vec<u32> {
        self.verified_chunks.iter().copied().collect()
    }

    pub fn progress(&self) -> FastSyncMultiSourceProgressV1 {
        let chunk_count = self
            .canonical_summary
            .as_ref()
            .map_or(0, |summary| summary.chunk_count);
        let verified_chunks = u32::try_from(self.verified_chunks.len()).unwrap_or(u32::MAX);
        let inflight_chunks = u32::try_from(self.assignments.len()).unwrap_or(u32::MAX);
        FastSyncMultiSourceProgressV1 {
            eligible_peer_count: u32::try_from(self.eligible_peers.len()).unwrap_or(u32::MAX),
            canonical_transfer_selected: self.canonical_summary.is_some(),
            commitment_set_verified: self.commitments.is_some(),
            chunk_count,
            verified_chunks,
            inflight_chunks,
            complete: chunk_count != 0 && verified_chunks == chunk_count,
        }
    }

    /// Admit a peer only when its protocol identity, fast-sync version surface,
    /// and complete transfer summary agree with the canonical transfer. The
    /// first admitted peer selects the canonical summary; all later peers must
    /// advertise that exact summary before they can share chunk work.
    pub fn admit_peer(
        &mut self,
        peer_id: &str,
        capabilities: &FastSyncCapabilitiesV1,
        summary: &FastSyncTransferSummaryV1,
    ) -> Result<(), FastSyncMultiSourceErrorV1> {
        require_peer_id(peer_id)?;
        capabilities.validate_for_expected(&self.expected)?;
        summary.validate_for_expected(&self.expected)?;
        if !capability_surface_matches_summary(capabilities, summary) {
            return Err(FastSyncMultiSourceErrorV1::CapabilitySurfaceMismatch {
                peer_id: peer_id.to_string(),
            });
        }

        if let Some(canonical) = self.canonical_summary.as_ref() {
            if canonical != summary {
                return Err(FastSyncMultiSourceErrorV1::TransferSummaryMismatch {
                    peer_id: peer_id.to_string(),
                });
            }
        } else {
            self.canonical_summary = Some(summary.clone());
        }
        self.eligible_peers.insert(peer_id.to_string());
        Ok(())
    }

    /// Verify a complete page set against the canonical summary's
    /// commitment_set_id before any peer can receive chunk assignments.
    pub fn finalize_commitment_pages(
        &mut self,
        pages: &[FastSyncCommitmentPageV1],
    ) -> Result<(), FastSyncMultiSourceErrorV1> {
        let summary = self
            .canonical_summary
            .as_ref()
            .ok_or(FastSyncMultiSourceErrorV1::MissingCanonicalTransfer)?;
        let commitments = verify_fast_sync_commitment_pages_v1(summary, pages)?;
        if let Some(existing) = self.commitments.as_ref() {
            if existing != &commitments {
                return Err(FastSyncMultiSourceErrorV1::CommitmentSetChanged);
            }
            return Ok(());
        }
        self.commitments = Some(commitments);
        Ok(())
    }

    /// Seed chunks already verified by a durable resume journal. This is only
    /// accepted after the commitment root has been finalized, so restart state
    /// cannot bypass the canonical commitment-set gate.
    pub fn seed_verified_chunk_indices(
        &mut self,
        indices: impl IntoIterator<Item = u32>,
    ) -> Result<(), FastSyncMultiSourceErrorV1> {
        if self.commitments.is_none() {
            return Err(FastSyncMultiSourceErrorV1::CommitmentSetUnverified);
        }
        let summary = self
            .canonical_summary
            .as_ref()
            .ok_or(FastSyncMultiSourceErrorV1::MissingCanonicalTransfer)?;
        let indices = indices.into_iter().collect::<Vec<_>>();
        for index in &indices {
            if *index >= summary.chunk_count {
                return Err(FastSyncMultiSourceErrorV1::Wire(
                    FastSyncWireErrorV1::InvalidShape(format!(
                        "fast-sync seeded chunk index {index} is beyond chunk_count {}",
                        summary.chunk_count
                    )),
                ));
            }
        }
        for index in indices {
            self.assignments.remove(&index);
            self.verified_chunks.insert(index);
        }
        Ok(())
    }

    /// Assign at most one bounded request to each currently eligible peer. Work
    /// is distributed round-robin over the canonical peer ordering and no chunk
    /// index can be inflight at more than one peer simultaneously.
    pub fn plan_chunk_requests(
        &mut self,
        max_chunks_per_peer: usize,
    ) -> Result<Vec<FastSyncPeerChunkRequestV1>, FastSyncMultiSourceErrorV1> {
        if max_chunks_per_peer == 0 || max_chunks_per_peer > P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1
        {
            return Err(FastSyncMultiSourceErrorV1::InvalidRequestBatchSize {
                observed: max_chunks_per_peer,
                maximum: P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1,
            });
        }
        let summary = self
            .canonical_summary
            .as_ref()
            .ok_or(FastSyncMultiSourceErrorV1::MissingCanonicalTransfer)?;
        if self.commitments.is_none() {
            return Err(FastSyncMultiSourceErrorV1::CommitmentSetUnverified);
        }
        if self.eligible_peers.is_empty() {
            return Ok(Vec::new());
        }

        let peers = self.eligible_peers.iter().cloned().collect::<Vec<_>>();
        let mut by_peer = peers
            .iter()
            .cloned()
            .map(|peer_id| (peer_id, Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let mut peer_cursor = 0_usize;
        for chunk_index in 0..summary.chunk_count {
            if self.verified_chunks.contains(&chunk_index)
                || self.assignments.contains_key(&chunk_index)
            {
                continue;
            }

            let mut selected = None;
            for offset in 0..peers.len() {
                let candidate = &peers[(peer_cursor + offset) % peers.len()];
                let assigned = by_peer
                    .get(candidate)
                    .ok_or(FastSyncMultiSourceErrorV1::ArithmeticOverflow)?;
                if assigned.len() < max_chunks_per_peer {
                    selected = Some(((peer_cursor + offset) % peers.len(), candidate.clone()));
                    break;
                }
            }
            let Some((selected_index, peer_id)) = selected else {
                break;
            };
            by_peer
                .get_mut(&peer_id)
                .ok_or(FastSyncMultiSourceErrorV1::ArithmeticOverflow)?
                .push(chunk_index);
            self.assignments.insert(chunk_index, peer_id);
            peer_cursor = (selected_index + 1) % peers.len();
        }

        let mut requests = Vec::new();
        for (peer_id, chunk_indices) in by_peer {
            if chunk_indices.is_empty() {
                continue;
            }
            let request = FastSyncChunkRequestV1 {
                contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                chain_id: summary.chain_id.clone(),
                transfer_id: summary.transfer_id.clone(),
                chunk_indices,
            };
            request.validate_against_summary(summary)?;
            requests.push(FastSyncPeerChunkRequestV1 { peer_id, request });
        }
        Ok(requests)
    }

    /// Release all unfinished work owned by one peer. Verified chunks remain
    /// complete; only inflight assignments become schedulable elsewhere.
    pub fn remove_peer(&mut self, peer_id: &str) -> Result<Vec<u32>, FastSyncMultiSourceErrorV1> {
        require_peer_id(peer_id)?;
        self.eligible_peers.remove(peer_id);
        let released = self
            .assignments
            .iter()
            .filter_map(|(index, assigned)| (assigned == peer_id).then_some(*index))
            .collect::<Vec<_>>();
        for index in &released {
            self.assignments.remove(index);
        }
        Ok(released)
    }

    /// Verify one response against the canonical commitment and the scheduler's
    /// exact peer assignment. The caller receives verified bytes for durable
    /// storage; this planner retains only completion metadata.
    pub fn accept_assigned_chunk(
        &mut self,
        peer_id: &str,
        chunk: &FastSyncChunkV1,
    ) -> Result<Vec<u8>, FastSyncMultiSourceErrorV1> {
        require_peer_id(peer_id)?;
        if !self.eligible_peers.contains(peer_id) {
            return Err(FastSyncMultiSourceErrorV1::PeerNotEligible {
                peer_id: peer_id.to_string(),
            });
        }
        let summary = self
            .canonical_summary
            .as_ref()
            .ok_or(FastSyncMultiSourceErrorV1::MissingCanonicalTransfer)?;
        let commitments = self
            .commitments
            .as_ref()
            .ok_or(FastSyncMultiSourceErrorV1::CommitmentSetUnverified)?;
        if self.verified_chunks.contains(&chunk.chunk_index) {
            return Err(FastSyncMultiSourceErrorV1::ChunkAlreadyVerified {
                chunk_index: chunk.chunk_index,
            });
        }
        let assigned_peer = self.assignments.get(&chunk.chunk_index).ok_or_else(|| {
            FastSyncMultiSourceErrorV1::ChunkNotAssigned {
                peer_id: peer_id.to_string(),
                chunk_index: chunk.chunk_index,
            }
        })?;
        if assigned_peer != peer_id {
            return Err(FastSyncMultiSourceErrorV1::ChunkAssignedToAnotherPeer {
                peer_id: peer_id.to_string(),
                assigned_peer_id: assigned_peer.clone(),
                chunk_index: chunk.chunk_index,
            });
        }
        let commitment = commitments
            .get(chunk.chunk_index as usize)
            .ok_or(FastSyncMultiSourceErrorV1::CommitmentSetUnverified)?;
        let verified = chunk.decode_verified_bytes(summary, commitment)?;
        self.assignments.remove(&chunk.chunk_index);
        self.verified_chunks.insert(chunk.chunk_index);
        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
        snapshot_transfer_chunk_digest_v1, snapshot_transfer_payload_digest_v1,
        GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "fast-sync-multisource-testnet";

    struct Fixture {
        expected: ProtocolActivationIdentity,
        capabilities: FastSyncCapabilitiesV1,
        summary: FastSyncTransferSummaryV1,
        commitments: Vec<String>,
        chunks: Vec<Vec<u8>>,
        pages: Vec<FastSyncCommitmentPageV1>,
    }

    fn fixture() -> Fixture {
        let expected = ProtocolActivationIdentity::activated_v2(
            CHAIN_ID.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        );
        let chunk_size = 512_usize;
        let payload = (0..(chunk_size * 7 + 19))
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
            max_commitments_per_page: 3,
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
            best_height: 500,
            selected_tip: "22".repeat(32),
            state_commitment: "33".repeat(32),
            prune_boundary_height: Some(250),
            snapshot_generation: 9,
            accepted_storage_generation: 9,
            delta_start_generation: 9,
            delta_end_generation: 10,
        };
        let pages = commitments
            .chunks(3)
            .enumerate()
            .map(|(page_index, page)| FastSyncCommitmentPageV1 {
                contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
                chain_id: CHAIN_ID.to_string(),
                transfer_id: summary.transfer_id.clone(),
                start_index: (page_index * 3) as u32,
                commitments: page.to_vec(),
            })
            .collect();
        Fixture {
            expected,
            capabilities,
            summary,
            commitments,
            chunks,
            pages,
        }
    }

    fn admitted_planner(fixture: &Fixture) -> FastSyncMultiSourcePlannerV1 {
        let mut planner = FastSyncMultiSourcePlannerV1::new(fixture.expected.clone());
        planner
            .admit_peer("peer-a", &fixture.capabilities, &fixture.summary)
            .unwrap();
        planner
            .admit_peer("peer-b", &fixture.capabilities, &fixture.summary)
            .unwrap();
        planner
    }

    #[test]
    fn canonical_peers_share_non_overlapping_chunk_work_after_root_verification() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        assert!(matches!(
            planner.plan_chunk_requests(2),
            Err(FastSyncMultiSourceErrorV1::CommitmentSetUnverified)
        ));
        planner.finalize_commitment_pages(&fixture.pages).unwrap();

        let requests = planner.plan_chunk_requests(2).unwrap();
        assert_eq!(requests.len(), 2);
        let a = requests
            .iter()
            .find(|request| request.peer_id == "peer-a")
            .unwrap();
        let b = requests
            .iter()
            .find(|request| request.peer_id == "peer-b")
            .unwrap();
        assert_eq!(a.request.chunk_indices.len(), 2);
        assert_eq!(b.request.chunk_indices.len(), 2);
        assert!(a
            .request
            .chunk_indices
            .iter()
            .all(|index| !b.request.chunk_indices.contains(index)));
        assert_eq!(planner.progress().inflight_chunks, 4);
    }

    #[test]
    fn peer_with_different_transfer_summary_cannot_join_canonical_pool() {
        let fixture = fixture();
        let mut planner = FastSyncMultiSourcePlannerV1::new(fixture.expected.clone());
        planner
            .admit_peer("peer-a", &fixture.capabilities, &fixture.summary)
            .unwrap();
        let mut conflicting = fixture.summary.clone();
        conflicting.commitment_set_id = "44".repeat(32);
        let error = planner
            .admit_peer("peer-b", &fixture.capabilities, &conflicting)
            .unwrap_err();
        assert!(matches!(
            error,
            FastSyncMultiSourceErrorV1::TransferSummaryMismatch { .. }
        ));
        assert_eq!(planner.eligible_peers(), vec!["peer-a".to_string()]);
    }

    #[test]
    fn corrupted_commitment_page_set_fails_before_any_chunk_assignment() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        let mut pages = fixture.pages.clone();
        pages[0].commitments[0] = "55".repeat(32);
        assert!(planner.finalize_commitment_pages(&pages).is_err());
        assert!(matches!(
            planner.plan_chunk_requests(1),
            Err(FastSyncMultiSourceErrorV1::CommitmentSetUnverified)
        ));
        assert_eq!(planner.progress().inflight_chunks, 0);
    }

    #[test]
    fn assigned_chunk_is_bound_to_the_selected_peer_and_canonical_commitment() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
        let requests = planner.plan_chunk_requests(1).unwrap();
        let request = requests.first().unwrap();
        let chunk_index = request.request.chunk_indices[0];
        let chunk = FastSyncChunkV1::from_bytes(
            CHAIN_ID,
            fixture.summary.transfer_id.clone(),
            chunk_index,
            &fixture.chunks[chunk_index as usize],
        )
        .unwrap();
        let wrong_peer = if request.peer_id == "peer-a" {
            "peer-b"
        } else {
            "peer-a"
        };
        assert!(matches!(
            planner.accept_assigned_chunk(wrong_peer, &chunk),
            Err(FastSyncMultiSourceErrorV1::ChunkAssignedToAnotherPeer { .. })
        ));
        let verified = planner
            .accept_assigned_chunk(&request.peer_id, &chunk)
            .unwrap();
        assert_eq!(verified, fixture.chunks[chunk_index as usize]);
        assert!(planner.verified_chunk_indices().contains(&chunk_index));
    }

    #[test]
    fn disconnect_releases_only_that_peers_unfinished_assignments() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
        let requests = planner.plan_chunk_requests(2).unwrap();
        let peer_a = requests
            .iter()
            .find(|request| request.peer_id == "peer-a")
            .unwrap();
        let released_expected = peer_a.request.chunk_indices.clone();
        let released = planner.remove_peer("peer-a").unwrap();
        assert_eq!(released, released_expected);
        assert_eq!(planner.eligible_peers(), vec!["peer-b".to_string()]);

        let retry = planner.plan_chunk_requests(8).unwrap();
        assert_eq!(retry.len(), 1);
        assert_eq!(retry[0].peer_id, "peer-b");
        for index in released_expected {
            assert!(retry[0].request.chunk_indices.contains(&index));
        }
    }

    #[test]
    fn invalid_durable_resume_seed_is_atomic() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
        let invalid = fixture.summary.chunk_count;
        assert!(planner.seed_verified_chunk_indices([0, invalid]).is_err());
        assert!(planner.verified_chunk_indices().is_empty());
        assert_eq!(planner.progress().verified_chunks, 0);
    }

    #[test]
    fn durable_resume_indices_are_never_scheduled_again() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        assert!(matches!(
            planner.seed_verified_chunk_indices([0, 1]),
            Err(FastSyncMultiSourceErrorV1::CommitmentSetUnverified)
        ));
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
        planner.seed_verified_chunk_indices([0, 1]).unwrap();
        let requests = planner.plan_chunk_requests(8).unwrap();
        assert!(requests.iter().all(|request| request
            .request
            .chunk_indices
            .iter()
            .all(|index| *index != 0 && *index != 1)));
        assert_eq!(planner.progress().verified_chunks, 2);
    }

    #[test]
    fn finalized_commitments_are_exactly_the_root_verified_sequence() {
        let fixture = fixture();
        let mut planner = admitted_planner(&fixture);
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
        assert_eq!(
            planner.commitments().unwrap(),
            fixture.commitments.as_slice()
        );
        // Re-finalizing the same root-verified page set must be idempotent.
        planner.finalize_commitment_pages(&fixture.pages).unwrap();
    }
}
