use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::types::Block;
use pulsedag_core::{
    errors::PulseError, snapshot_transfer::snapshot_transfer_commitment_set_digest_v1,
    ActivatedV2P2pRuntime, ChainState, ProtocolActivationIdentity,
};
use pulsedag_p2p::messages::fast_sync_carrier_v1::{
    live_session_v1::FastSyncServingSessionV1, verify_fast_sync_commitment_pages_v1,
    FastSyncCapabilitiesV1, FastSyncChunkRequestV1, FastSyncCommitmentPageV1,
    FastSyncTransferSummaryV1, FastSyncWireV1, P2P_FAST_SYNC_CONTRACT_VERSION,
    P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1, P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1,
    P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1,
};
use pulsedag_p2p::{P2pHandle, P2pStatus, RemoteSelectedTipStatus};
use pulsedag_storage::{
    FastSyncNetworkTransferPlanV1, SnapshotVerificationReport, Storage,
    FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION, FAST_SYNC_SNAPSHOT_MANIFEST_VERSION,
    FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1, PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
    STORAGE_SCHEMA_VERSION,
};

const FAST_SYNC_REQUEST_RETRY_SECS: u64 = 5;
const FAST_SYNC_CAPABILITY_PROBE_RETRY_SECS: u64 = 5;
const FAST_SYNC_CLEAN_DISCOVERY_SECS: u64 = 30;

fn bootstrap_error(message: impl Into<String>) -> PulseError {
    PulseError::Internal(format!("fast-sync bootstrap: {}", message.into()))
}

fn map_session_error(error: impl std::fmt::Debug) -> PulseError {
    bootstrap_error(format!("session contract failed closed: {error:?}"))
}

pub fn local_fast_sync_capabilities_v1(
    expected: &ProtocolActivationIdentity,
) -> Result<FastSyncCapabilitiesV1, PulseError> {
    let capabilities = FastSyncCapabilitiesV1 {
        contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
        chain_id: expected.chain_id.clone(),
        genesis_hash: expected.genesis_hash.clone(),
        protocol_fingerprint: expected.fingerprint().map_err(bootstrap_error)?,
        manifest_version: FAST_SYNC_SNAPSHOT_MANIFEST_VERSION,
        protocol_snapshot_bundle_format_version: PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
        storage_schema_version: STORAGE_SCHEMA_VERSION,
        payload_encoding: FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1.to_string(),
        max_chunk_bytes: P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1 as u32,
        max_commitments_per_page: P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 as u32,
    };
    capabilities
        .validate_for_expected(expected)
        .map_err(map_session_error)?;
    Ok(capabilities)
}

fn compatible_remote_capabilities(
    local: &FastSyncCapabilitiesV1,
    remote: &FastSyncCapabilitiesV1,
    expected: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    remote
        .validate_for_expected(expected)
        .map_err(map_session_error)?;
    if remote.manifest_version != local.manifest_version
        || remote.protocol_snapshot_bundle_format_version
            != local.protocol_snapshot_bundle_format_version
        || remote.storage_schema_version != local.storage_schema_version
        || remote.payload_encoding != local.payload_encoding
    {
        return Err(bootstrap_error(
            "remote capability version/storage surface differs from local authority",
        ));
    }
    Ok(())
}

fn remote_is_eligible_source(
    remote: &RemoteSelectedTipStatus,
    eligible: &BTreeSet<String>,
    local_height: u64,
) -> bool {
    eligible.contains(&remote.peer_id)
        && remote.connected
        && remote.direct_request_capable
        && remote.selected_height > local_height
}

pub fn select_clean_bootstrap_source_peer(
    status: &P2pStatus,
    eligible_fast_sync_peers: &[String],
    local_height: u64,
) -> Option<String> {
    let eligible = eligible_fast_sync_peers
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut candidates = status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote_is_eligible_source(remote, &eligible, local_height))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .selected_height
            .cmp(&left.selected_height)
            .then_with(|| left.peer_id.cmp(&right.peer_id))
    });
    candidates.first().map(|remote| remote.peer_id.clone())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSyncBootstrapOutcome {
    Idle,
    Progress,
    Imported(SnapshotVerificationReport),
}

#[derive(Debug, Clone)]
struct PendingRequest {
    kind: &'static str,
    sent_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct FastSyncBootstrapController {
    expected: ProtocolActivationIdentity,
    local_capabilities: FastSyncCapabilitiesV1,
    peer_capabilities: BTreeMap<String, FastSyncCapabilitiesV1>,
    source_peer: Option<String>,
    summary: Option<FastSyncTransferSummaryV1>,
    commitment_pages: BTreeMap<u32, FastSyncCommitmentPageV1>,
    plan: Option<FastSyncNetworkTransferPlanV1>,
    missing_chunks: BTreeSet<u32>,
    inflight_chunks: BTreeSet<u32>,
    pending_request: Option<PendingRequest>,
    imported: bool,
}

impl FastSyncBootstrapController {
    pub fn new(expected: ProtocolActivationIdentity) -> Result<Self, PulseError> {
        let local_capabilities = local_fast_sync_capabilities_v1(&expected)?;
        Ok(Self {
            expected,
            local_capabilities,
            peer_capabilities: BTreeMap::new(),
            source_peer: None,
            summary: None,
            commitment_pages: BTreeMap::new(),
            plan: None,
            missing_chunks: BTreeSet::new(),
            inflight_chunks: BTreeSet::new(),
            pending_request: None,
            imported: false,
        })
    }

    pub fn local_capabilities(&self) -> &FastSyncCapabilitiesV1 {
        &self.local_capabilities
    }

    pub fn source_peer(&self) -> Option<&str> {
        self.source_peer.as_deref()
    }

    pub fn has_peer_capabilities(&self, peer_id: &str) -> bool {
        self.peer_capabilities.contains_key(peer_id)
    }

    pub fn imported(&self) -> bool {
        self.imported
    }

    pub fn note_capabilities(
        &mut self,
        peer_id: &str,
        capabilities: FastSyncCapabilitiesV1,
    ) -> Result<(), PulseError> {
        compatible_remote_capabilities(&self.local_capabilities, &capabilities, &self.expected)?;
        if let Some(existing) = self.peer_capabilities.get(peer_id) {
            if existing != &capabilities {
                return Err(bootstrap_error(format!(
                    "peer {peer_id} changed fast-sync capabilities mid-bootstrap"
                )));
            }
        } else {
            self.peer_capabilities
                .insert(peer_id.to_string(), capabilities);
        }
        Ok(())
    }

    pub fn maybe_select_source(
        &mut self,
        status: &P2pStatus,
        eligible_fast_sync_peers: &[String],
        local_height: u64,
    ) -> Option<String> {
        if self.source_peer.is_some() || self.imported {
            return self.source_peer.clone();
        }
        let peer =
            select_clean_bootstrap_source_peer(status, eligible_fast_sync_peers, local_height)?;
        if !self.peer_capabilities.contains_key(&peer) {
            return None;
        }
        self.source_peer = Some(peer.clone());
        self.pending_request = None;
        Some(peer)
    }

    pub fn abandon_source(&mut self) {
        self.source_peer = None;
        self.summary = None;
        self.commitment_pages.clear();
        self.plan = None;
        self.missing_chunks.clear();
        self.inflight_chunks.clear();
        self.pending_request = None;
    }

    fn pending_is_fresh(&self, kind: &'static str, now_unix: u64) -> bool {
        self.pending_request.as_ref().is_some_and(|pending| {
            pending.kind == kind
                && now_unix.saturating_sub(pending.sent_at_unix) < FAST_SYNC_REQUEST_RETRY_SECS
        })
    }

    fn set_pending(&mut self, kind: &'static str, now_unix: u64) {
        self.pending_request = Some(PendingRequest {
            kind,
            sent_at_unix: now_unix,
        });
    }

    fn clear_pending(&mut self) {
        self.pending_request = None;
    }

    pub fn next_request(&mut self, now_unix: u64) -> Result<Option<FastSyncWireV1>, PulseError> {
        if self.imported || self.source_peer.is_none() {
            return Ok(None);
        }
        if self.summary.is_none() {
            if self.pending_is_fresh("summary", now_unix) {
                return Ok(None);
            }
            self.set_pending("summary", now_unix);
            return Ok(Some(FastSyncWireV1::GetTransferSummary {
                chain_id: self.expected.chain_id.clone(),
            }));
        }

        let summary = self
            .summary
            .clone()
            .ok_or_else(|| bootstrap_error("summary disappeared"))?;
        if self.plan.is_none() {
            if self.pending_is_fresh("commitments", now_unix) {
                return Ok(None);
            }
            let start_index = self
                .commitment_pages
                .values()
                .map(|page| page.commitments.len() as u32)
                .sum::<u32>();
            if start_index >= summary.chunk_count {
                return Err(bootstrap_error(
                    "commitment pages reached chunk_count without a verified plan",
                ));
            }
            let limit = (summary.chunk_count - start_index)
                .min(self.local_capabilities.max_commitments_per_page)
                .min(P2P_FAST_SYNC_MAX_COMMITMENTS_PER_PAGE_V1 as u32);
            let limit = u16::try_from(limit)
                .map_err(|_| bootstrap_error("commitment request limit exceeds u16"))?;
            self.set_pending("commitments", now_unix);
            return Ok(Some(FastSyncWireV1::GetCommitmentPage {
                chain_id: summary.chain_id.clone(),
                transfer_id: summary.transfer_id.clone(),
                start_index,
                limit,
            }));
        }

        let indices = self
            .missing_chunks
            .iter()
            .copied()
            .filter(|index| !self.inflight_chunks.contains(index))
            .take(P2P_FAST_SYNC_MAX_CHUNKS_PER_REQUEST_V1)
            .collect::<Vec<_>>();
        if indices.is_empty() {
            if !self.missing_chunks.is_empty() && !self.pending_is_fresh("chunks", now_unix) {
                self.inflight_chunks.clear();
                self.clear_pending();
                return self.next_request(now_unix);
            }
            return Ok(None);
        }
        for index in &indices {
            self.inflight_chunks.insert(*index);
        }
        self.set_pending("chunks", now_unix);
        Ok(Some(FastSyncWireV1::GetChunks(FastSyncChunkRequestV1 {
            contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
            chain_id: summary.chain_id.clone(),
            transfer_id: summary.transfer_id.clone(),
            chunk_indices: indices,
        })))
    }

    fn require_source_response(&self, peer_id: &str) -> Result<(), PulseError> {
        match self.source_peer.as_deref() {
            Some(source) if source == peer_id => Ok(()),
            Some(source) => Err(bootstrap_error(format!(
                "response from non-authoritative peer {peer_id}; selected source is {source}"
            ))),
            None => Err(bootstrap_error(
                "received transfer response before source selection",
            )),
        }
    }

    fn build_plan(
        &self,
        summary: &FastSyncTransferSummaryV1,
        commitments: Vec<String>,
    ) -> FastSyncNetworkTransferPlanV1 {
        FastSyncNetworkTransferPlanV1 {
            plan_version: FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
            payload_encoding: summary.payload_encoding.clone(),
            chain_id: summary.chain_id.clone(),
            genesis_hash: summary.genesis_hash.clone(),
            protocol_fingerprint: summary.protocol_fingerprint.clone(),
            manifest_version: summary.manifest_version,
            protocol_snapshot_bundle_format_version: summary
                .protocol_snapshot_bundle_format_version,
            storage_schema_version: summary.storage_schema_version,
            transfer_id: summary.transfer_id.clone(),
            commitment_set_id: summary.commitment_set_id.clone(),
            payload_len: summary.payload_len,
            chunk_size: summary.chunk_size,
            chunk_count: summary.chunk_count,
            chunk_commitments: commitments,
            best_height: summary.best_height,
            selected_tip: summary.selected_tip.clone(),
            state_commitment: summary.state_commitment.clone(),
            prune_boundary_height: summary.prune_boundary_height,
            snapshot_generation: summary.snapshot_generation,
            accepted_storage_generation: summary.accepted_storage_generation,
            delta_start_generation: summary.delta_start_generation,
            delta_end_generation: summary.delta_end_generation,
        }
    }

    pub fn accept_response(
        &mut self,
        storage: &Storage,
        peer_id: &str,
        wire: FastSyncWireV1,
    ) -> Result<FastSyncBootstrapOutcome, PulseError> {
        match wire {
            FastSyncWireV1::Capabilities(capabilities) => {
                self.note_capabilities(peer_id, capabilities)?;
                Ok(FastSyncBootstrapOutcome::Progress)
            }
            FastSyncWireV1::TransferSummary(summary) => {
                self.require_source_response(peer_id)?;
                summary
                    .validate_for_expected(&self.expected)
                    .map_err(map_session_error)?;
                let remote = self.peer_capabilities.get(peer_id).ok_or_else(|| {
                    bootstrap_error("selected source has no negotiated capabilities")
                })?;
                compatible_remote_capabilities(&self.local_capabilities, remote, &self.expected)?;
                if summary.manifest_version != remote.manifest_version
                    || summary.protocol_snapshot_bundle_format_version
                        != remote.protocol_snapshot_bundle_format_version
                    || summary.storage_schema_version != remote.storage_schema_version
                    || summary.payload_encoding != remote.payload_encoding
                    || summary.chunk_size > remote.max_chunk_bytes
                {
                    return Err(bootstrap_error(
                        "transfer summary differs from negotiated capability surface",
                    ));
                }
                if let Some(existing) = self.summary.as_ref() {
                    if existing != &summary {
                        return Err(bootstrap_error(
                            "selected source changed transfer summary mid-bootstrap",
                        ));
                    }
                } else {
                    self.summary = Some(summary);
                }
                self.clear_pending();
                Ok(FastSyncBootstrapOutcome::Progress)
            }
            FastSyncWireV1::CommitmentPage(page) => {
                self.require_source_response(peer_id)?;
                let summary = self
                    .summary
                    .clone()
                    .ok_or_else(|| bootstrap_error("commitment page arrived before summary"))?;
                page.validate_against_summary(&summary)
                    .map_err(map_session_error)?;
                let expected_start = self
                    .commitment_pages
                    .values()
                    .map(|known| known.commitments.len() as u32)
                    .sum::<u32>();
                if page.start_index != expected_start {
                    return Err(bootstrap_error(format!(
                        "commitment page sequence mismatch: expected {expected_start}, observed {}",
                        page.start_index
                    )));
                }
                self.commitment_pages.insert(page.start_index, page);
                self.clear_pending();

                let covered = self
                    .commitment_pages
                    .values()
                    .map(|known| known.commitments.len() as u32)
                    .sum::<u32>();
                if covered == summary.chunk_count {
                    let pages = self.commitment_pages.values().cloned().collect::<Vec<_>>();
                    let commitments = verify_fast_sync_commitment_pages_v1(&summary, &pages)
                        .map_err(map_session_error)?;
                    let plan = self.build_plan(&summary, commitments);
                    plan.validate_for_expected(&self.expected)?;
                    let status = storage
                        .begin_fast_sync_network_persisted_resume_v1(&plan, &self.expected)?;
                    self.missing_chunks = status.missing_chunk_indices.into_iter().collect();
                    self.plan = Some(plan);
                }
                Ok(FastSyncBootstrapOutcome::Progress)
            }
            FastSyncWireV1::Chunk(chunk) => {
                self.require_source_response(peer_id)?;
                let summary = self
                    .summary
                    .clone()
                    .ok_or_else(|| bootstrap_error("chunk arrived before summary"))?;
                let plan = self
                    .plan
                    .clone()
                    .ok_or_else(|| bootstrap_error("chunk arrived before verified commitments"))?;
                let commitment = plan
                    .chunk_commitments
                    .get(chunk.chunk_index as usize)
                    .ok_or_else(|| bootstrap_error("chunk index is outside verified plan"))?;
                let chunk_index = chunk.chunk_index;
                let verified = chunk
                    .decode_verified_bytes(&summary, commitment)
                    .map_err(map_session_error)?;
                storage.persist_fast_sync_network_resume_chunk_v1(
                    &plan,
                    &self.expected,
                    chunk_index,
                    &verified,
                )?;
                self.missing_chunks.remove(&chunk_index);
                self.inflight_chunks.remove(&chunk_index);
                if self.inflight_chunks.is_empty() {
                    self.clear_pending();
                }
                if !self.missing_chunks.is_empty() {
                    return Ok(FastSyncBootstrapOutcome::Progress);
                }

                let chunks =
                    storage.load_fast_sync_network_resume_chunks_v1(&plan, &self.expected)?;
                let report = storage
                    .import_complete_fast_sync_bootstrap_v1(&plan, &chunks, &self.expected)?
                    .0;
                storage.clear_fast_sync_network_resume_v1(&plan, &self.expected)?;
                self.imported = true;
                self.clear_pending();
                Ok(FastSyncBootstrapOutcome::Imported(report))
            }
            FastSyncWireV1::CapabilityProbe { .. }
            | FastSyncWireV1::GetTransferSummary { .. }
            | FastSyncWireV1::GetCommitmentPage { .. }
            | FastSyncWireV1::GetChunks(_) => Err(bootstrap_error(
                "request was routed to the downloader response path",
            )),
        }
    }
}

pub fn build_fast_sync_serving_session_v1(
    storage: &Storage,
    expected: &ProtocolActivationIdentity,
    capabilities: FastSyncCapabilitiesV1,
) -> Result<FastSyncServingSessionV1, PulseError> {
    capabilities
        .validate_for_expected(expected)
        .map_err(map_session_error)?;
    let (bundle, _) = storage.export_fast_sync_snapshot_bundle_v1(expected)?;
    let chunk_size = P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1;
    let (prepared, _) =
        storage.prepare_fast_sync_snapshot_transfer_v1(&bundle, expected, chunk_size)?;
    let manifest = &prepared.plan.snapshot_manifest;
    let commitments = prepared.plan.chunk_commitments.clone();
    let commitment_set_id =
        snapshot_transfer_commitment_set_digest_v1(&prepared.plan.transfer_id, &commitments);
    let summary = FastSyncTransferSummaryV1 {
        contract_version: P2P_FAST_SYNC_CONTRACT_VERSION,
        chain_id: manifest.chain_id.clone(),
        genesis_hash: manifest.genesis_hash.clone(),
        protocol_fingerprint: manifest.protocol_fingerprint.clone(),
        manifest_version: manifest.manifest_version,
        protocol_snapshot_bundle_format_version: manifest.protocol_snapshot_bundle_format_version,
        storage_schema_version: manifest.storage_schema_version,
        payload_encoding: prepared.plan.payload_encoding.clone(),
        transfer_id: prepared.plan.transfer_id.clone(),
        commitment_set_id,
        payload_len: prepared.plan.payload_len,
        chunk_size: prepared.plan.chunk_size,
        chunk_count: prepared.plan.chunk_count,
        best_height: manifest.best_height,
        selected_tip: manifest.selected_tip.clone(),
        state_commitment: manifest.state_commitment.clone(),
        prune_boundary_height: manifest.prune_boundary_height,
        snapshot_generation: manifest.snapshot_generation,
        accepted_storage_generation: manifest.accepted_storage_generation,
        delta_start_generation: manifest.delta_start_generation,
        delta_end_generation: manifest.delta_end_generation,
    };
    let mut chunks = Vec::with_capacity(prepared.plan.chunk_count as usize);
    for chunk_index in 0..prepared.plan.chunk_count {
        chunks.push(prepared.chunk(chunk_index)?.to_vec());
    }
    FastSyncServingSessionV1::new(expected.clone(), capabilities, summary, commitments, chunks)
        .map_err(map_session_error)
}

pub fn serve_fast_sync_request_v1(
    storage: &Storage,
    expected: &ProtocolActivationIdentity,
    capabilities: &FastSyncCapabilitiesV1,
    serving_session: &mut Option<FastSyncServingSessionV1>,
    request: &FastSyncWireV1,
) -> Result<Vec<FastSyncWireV1>, PulseError> {
    request
        .validate_for_chain(&expected.chain_id)
        .map_err(map_session_error)?;
    match request {
        FastSyncWireV1::CapabilityProbe { .. } => {
            Ok(vec![FastSyncWireV1::Capabilities(capabilities.clone())])
        }
        FastSyncWireV1::GetTransferSummary { .. } => {
            let fresh =
                build_fast_sync_serving_session_v1(storage, expected, capabilities.clone())?;
            let responses = fresh.handle_request(request).map_err(map_session_error)?;
            *serving_session = Some(fresh);
            Ok(responses)
        }
        FastSyncWireV1::GetCommitmentPage { .. } | FastSyncWireV1::GetChunks(_) => {
            let session = serving_session.as_ref().ok_or_else(|| {
                bootstrap_error("transfer request arrived before a serving summary was established")
            })?;
            session.handle_request(request).map_err(map_session_error)
        }
        FastSyncWireV1::Capabilities(_)
        | FastSyncWireV1::TransferSummary(_)
        | FastSyncWireV1::CommitmentPage(_)
        | FastSyncWireV1::Chunk(_) => Err(bootstrap_error(
            "response was routed to the serving request path",
        )),
    }
}

pub fn fast_sync_clean_storage_candidate_v1(
    expected: &ProtocolActivationIdentity,
    persisted_blocks: &[Block],
) -> bool {
    persisted_blocks.is_empty()
        || (persisted_blocks.len() == 1
            && persisted_blocks[0].header.height == 0
            && persisted_blocks[0].hash == expected.genesis_hash)
}

#[derive(Debug, Clone)]
pub struct FastSyncImportedStateV1 {
    pub report: SnapshotVerificationReport,
    pub chain_state: ChainState,
    pub runtime: ActivatedV2P2pRuntime,
}

pub struct FastSyncDaemonRuntimeV1 {
    expected: ProtocolActivationIdentity,
    local_capabilities: FastSyncCapabilitiesV1,
    controller: Option<FastSyncBootstrapController>,
    serving_sessions: BTreeMap<String, Option<FastSyncServingSessionV1>>,
    discovery_started_at_unix: u64,
    capability_probe_sent_at: BTreeMap<String, u64>,
    fallback_to_normal_sync: bool,
}

impl FastSyncDaemonRuntimeV1 {
    pub fn new(
        expected: ProtocolActivationIdentity,
        clean_bootstrap: bool,
        now_unix: u64,
    ) -> Result<Self, PulseError> {
        let local_capabilities = local_fast_sync_capabilities_v1(&expected)?;
        let controller = clean_bootstrap
            .then(|| FastSyncBootstrapController::new(expected.clone()))
            .transpose()?;
        Ok(Self {
            expected,
            local_capabilities,
            controller,
            serving_sessions: BTreeMap::new(),
            discovery_started_at_unix: now_unix,
            capability_probe_sent_at: BTreeMap::new(),
            fallback_to_normal_sync: false,
        })
    }

    pub fn authority_active(&self) -> bool {
        !self.fallback_to_normal_sync
            && self
                .controller
                .as_ref()
                .is_some_and(|controller| !controller.imported())
    }

    pub fn fallback_to_normal_sync(&self) -> bool {
        self.fallback_to_normal_sync
    }

    fn discovery_expired(&self, now_unix: u64) -> bool {
        now_unix.saturating_sub(self.discovery_started_at_unix) >= FAST_SYNC_CLEAN_DISCOVERY_SECS
    }

    fn probe_is_due(&self, peer_id: &str, now_unix: u64) -> bool {
        self.capability_probe_sent_at
            .get(peer_id)
            .map(|sent_at| {
                now_unix.saturating_sub(*sent_at) >= FAST_SYNC_CAPABILITY_PROBE_RETRY_SECS
            })
            .unwrap_or(true)
    }

    pub fn drive(
        &mut self,
        p2p: &dyn P2pHandle,
        local_height: u64,
        now_unix: u64,
    ) -> Result<(), PulseError> {
        if !self.authority_active() {
            return Ok(());
        }

        let probe_candidates = p2p.protocol_sync_eligible_peers_v1()?;
        let fast_sync_eligible_peers = p2p.fast_sync_eligible_peers_v1()?;
        let peers_to_probe = {
            let controller = self
                .controller
                .as_ref()
                .ok_or_else(|| bootstrap_error("clean bootstrap controller disappeared"))?;
            probe_candidates
                .iter()
                .filter(|peer_id| {
                    !controller.has_peer_capabilities(peer_id)
                        && self.probe_is_due(peer_id, now_unix)
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        for peer_id in peers_to_probe {
            p2p.send_fast_sync_v1(
                &peer_id,
                &FastSyncWireV1::CapabilityProbe {
                    chain_id: self.expected.chain_id.clone(),
                },
            )?;
            self.capability_probe_sent_at.insert(peer_id, now_unix);
        }

        let status = p2p.status()?;
        let source = {
            let controller = self
                .controller
                .as_mut()
                .ok_or_else(|| bootstrap_error("clean bootstrap controller disappeared"))?;
            controller.maybe_select_source(&status, &fast_sync_eligible_peers, local_height)
        };

        if source.is_none()
            && self
                .controller
                .as_ref()
                .and_then(|controller| controller.source_peer())
                .is_none()
            && self.discovery_expired(now_unix)
        {
            self.fallback_to_normal_sync = true;
            return Ok(());
        }

        let request = {
            let controller = self
                .controller
                .as_mut()
                .ok_or_else(|| bootstrap_error("clean bootstrap controller disappeared"))?;
            let source_peer = controller.source_peer().map(str::to_string);
            let request = controller.next_request(now_unix)?;
            source_peer.zip(request)
        };
        if let Some((peer_id, wire)) = request {
            p2p.send_fast_sync_v1(&peer_id, &wire)?;
        }
        Ok(())
    }

    pub fn handle_inbound(
        &mut self,
        p2p: &dyn P2pHandle,
        storage: &Storage,
        peer_id: &str,
        wire: &FastSyncWireV1,
    ) -> Result<Option<FastSyncImportedStateV1>, PulseError> {
        if matches!(
            wire,
            FastSyncWireV1::CapabilityProbe { .. }
                | FastSyncWireV1::GetTransferSummary { .. }
                | FastSyncWireV1::GetCommitmentPage { .. }
                | FastSyncWireV1::GetChunks(_)
        ) {
            let serving_session = self
                .serving_sessions
                .entry(peer_id.to_string())
                .or_insert(None);
            let responses = serve_fast_sync_request_v1(
                storage,
                &self.expected,
                &self.local_capabilities,
                serving_session,
                wire,
            )?;
            for response in responses {
                p2p.send_fast_sync_v1(peer_id, &response)?;
            }
            return Ok(None);
        }

        let Some(controller) = self.controller.as_mut() else {
            return Ok(None);
        };
        match controller.accept_response(storage, peer_id, wire.clone()) {
            Ok(FastSyncBootstrapOutcome::Imported(report)) => {
                let (chain_state, runtime) =
                    storage.load_activated_v2_p2p_runtime_snapshot(&self.expected)?;
                Ok(Some(FastSyncImportedStateV1 {
                    report,
                    chain_state,
                    runtime,
                }))
            }
            Ok(FastSyncBootstrapOutcome::Idle | FastSyncBootstrapOutcome::Progress) => Ok(None),
            Err(error) => {
                if controller.source_peer() == Some(peer_id) {
                    controller.abandon_source();
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{genesis_v2::init_chain_state_v2, GHOSTDAG_V1_ORDERING_VERSION};
    use pulsedag_p2p::messages::fast_sync_carrier_v1::FastSyncWireV1;

    fn identity() -> ProtocolActivationIdentity {
        let state = init_chain_state_v2("fast-sync-daemon-testnet".to_string()).unwrap();
        ProtocolActivationIdentity::activated_v2(
            state.chain_id,
            state.dag.genesis_hash,
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn temp_db_path(name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedagd-fast-sync-bootstrap-{name}-{unique}"))
            .to_string_lossy()
            .into_owned()
    }

    fn remote(peer: &str, height: u64) -> RemoteSelectedTipStatus {
        RemoteSelectedTipStatus {
            peer_id: peer.to_string(),
            selected_height: height,
            connected: true,
            direct_request_capable: true,
            ..RemoteSelectedTipStatus::default()
        }
    }

    #[test]
    fn clean_source_selection_is_highest_height_then_peer_id() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("legacy-high", 900),
                remote("fast-b", 220),
                remote("fast-a", 220),
                remote("fast-low", 210),
            ],
            ..P2pStatus::default()
        };
        let eligible = vec![
            "fast-a".to_string(),
            "fast-b".to_string(),
            "fast-low".to_string(),
        ];
        assert_eq!(
            select_clean_bootstrap_source_peer(&status, &eligible, 0),
            Some("fast-a".to_string())
        );
        assert_eq!(
            select_clean_bootstrap_source_peer(&status, &eligible, 220),
            None
        );
    }

    #[test]
    fn capability_surface_is_protocol_and_storage_bound() {
        let expected = identity();
        let caps = local_fast_sync_capabilities_v1(&expected).unwrap();
        assert_eq!(caps.chain_id, expected.chain_id);
        assert_eq!(caps.genesis_hash, expected.genesis_hash);
        assert_eq!(caps.storage_schema_version, STORAGE_SCHEMA_VERSION);
        assert_eq!(caps.manifest_version, FAST_SYNC_SNAPSHOT_MANIFEST_VERSION);
        assert_eq!(
            caps.max_chunk_bytes as usize,
            P2P_FAST_SYNC_MAX_CHUNK_BYTES_V1
        );
    }

    #[test]
    fn serving_session_is_fresh_protocol_bound_material() {
        let path = temp_db_path("serve");
        let storage = Storage::open(&path).unwrap();
        let expected = identity();
        let state = init_chain_state_v2(expected.chain_id.clone()).unwrap();
        let genesis = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        storage
            .persist_activated_v2_p2p_blocks_and_runtime(
                &[genesis],
                &expected,
                &state,
                &pulsedag_core::ActivatedV2P2pRuntime::default(),
            )
            .unwrap();
        let caps = local_fast_sync_capabilities_v1(&expected).unwrap();
        let mut serving = None;
        let responses = serve_fast_sync_request_v1(
            &storage,
            &expected,
            &caps,
            &mut serving,
            &FastSyncWireV1::GetTransferSummary {
                chain_id: expected.chain_id.clone(),
            },
        )
        .unwrap();
        assert!(matches!(
            responses.as_slice(),
            [FastSyncWireV1::TransferSummary(_)]
        ));
        assert!(serving.is_some());
        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn clean_storage_candidate_accepts_empty_and_exact_genesis_only() {
        let expected = identity();
        let state = init_chain_state_v2(expected.chain_id.clone()).unwrap();
        let genesis = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        assert!(fast_sync_clean_storage_candidate_v1(&expected, &[]));
        assert!(fast_sync_clean_storage_candidate_v1(
            &expected,
            std::slice::from_ref(&genesis)
        ));

        let mut non_genesis = genesis.clone();
        non_genesis.header.height = 1;
        assert!(!fast_sync_clean_storage_candidate_v1(
            &expected,
            std::slice::from_ref(&non_genesis)
        ));
        assert!(!fast_sync_clean_storage_candidate_v1(
            &expected,
            &[genesis.clone(), genesis]
        ));
    }

    #[test]
    fn daemon_runtime_authority_is_clean_node_only() {
        let expected = identity();
        let clean = FastSyncDaemonRuntimeV1::new(expected.clone(), true, 100).unwrap();
        let existing = FastSyncDaemonRuntimeV1::new(expected, false, 100).unwrap();
        assert!(clean.authority_active());
        assert!(!existing.authority_active());
    }

    #[test]
    fn clean_discovery_window_is_bounded_and_saturating() {
        let expected = identity();
        let runtime = FastSyncDaemonRuntimeV1::new(expected, true, 100).unwrap();
        assert!(!runtime.discovery_expired(99));
        assert!(!runtime.discovery_expired(129));
        assert!(runtime.discovery_expired(130));
    }
}
