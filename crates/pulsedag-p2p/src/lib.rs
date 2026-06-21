pub mod messages;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::ping;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identity, Multiaddr, PeerId, SwarmBuilder};
use pulsedag_core::{
    errors::PulseError,
    rank_sync_candidates,
    types::{compute_block_hash, compute_merkle_root, Block, Hash as PulseHash, Transaction},
    RankedSyncPeer, SyncPeerCandidate,
};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::messages::{
    message_id_for_block, message_id_for_tx, topic_names, BlockHeaderAnnouncement, HeaderInventory,
    NetworkMessage,
};

pub const P2P_MODE_MEMORY_SIMULATED: &str = "memory-simulated";
pub const P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON: &str = "libp2p-dev-loopback-skeleton";
pub const P2P_MODE_LIBP2P_REAL: &str = "libp2p-real";

pub fn mode_connected_peers_are_real_network(mode: &str) -> bool {
    mode == P2P_MODE_LIBP2P_REAL
}

pub fn connected_peers_semantics(mode: &str) -> &'static str {
    if mode_connected_peers_are_real_network(mode) {
        "real-network-connected-peers"
    } else {
        "simulated-or-internal-peer-observations"
    }
}

#[derive(Debug, Clone, Default)]
pub struct PeerScoreComponents {
    pub base_score: i32,
    pub recent_block_bonus: i64,
    pub recent_rate_limit_penalty: i64,
    pub connection_age_bonus: i64,
    pub failure_penalty: i64,
    pub bounded_score: i64,
}

#[derive(Debug, Clone)]
pub struct PeerRecoveryStatus {
    pub peer_id: String,
    pub chain_id: Option<String>,
    pub chain_id_compatible: bool,
    pub last_activity_unix: Option<u64>,
    pub score: i32,
    pub fail_streak: u32,
    pub lifecycle_tier: String,
    pub recovery_tier: String,
    pub recovery_reason: Option<String>,
    pub connected: bool,
    pub last_seen_unix: Option<u64>,
    pub last_successful_connect_unix: Option<u64>,
    pub next_retry_unix: u64,
    pub reconnect_attempts: u64,
    pub recovery_success_count: u64,
    pub last_recovery_unix: Option<u64>,
    pub recent_failures_unix: Vec<u64>,
    pub cooldown_suppressed_count: u64,
    pub flap_suppressed_count: u64,
    pub flap_events: u32,
    pub suppression_until_unix: Option<u64>,
    pub last_error: Option<String>,
    pub last_error_unix: Option<u64>,
    pub last_error_source: Option<String>,
    pub health_states: Vec<String>,
    pub eligible_for_sync: bool,
    pub last_successful_block_unix: Option<u64>,
    pub last_rate_limited_unix: Option<u64>,
    pub connection_age_secs: Option<u64>,
    pub score_components: PeerScoreComponents,
}

#[derive(Debug, Clone)]
pub struct PeerConnectionFinalState {
    pub peer_id: String,
    pub direction: String,
    pub state: String,
    pub active_connections: usize,
    pub last_event_unix: Option<u64>,
    pub last_error: Option<String>,
    pub last_disconnect_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SeenCacheEntry {
    pub message_id: String,
    pub block_hash: Option<String>,
    pub txid: Option<String>,
    pub first_seen_unix: u64,
    pub last_seen_unix: u64,
    pub peer_source: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct P2pStatus {
    pub chain_id: String,
    pub mode: String,
    pub peer_id: String,
    pub listening: Vec<String>,
    pub connected_peers: Vec<String>,
    pub topics: Vec<String>,
    pub mdns: bool,
    pub kademlia: bool,
    pub broadcasted_messages: usize,
    pub publish_attempts: usize,
    pub seen_message_ids: usize,
    pub queued_messages: usize,
    pub queued_block_messages: usize,
    pub queued_non_block_messages: usize,
    pub queue_max_depth: usize,
    pub dequeued_block_messages: usize,
    pub dequeued_non_block_messages: usize,
    pub queue_block_priority_picks: usize,
    pub queue_priority_tx_lane_picks: usize,
    pub queue_standard_tx_lane_picks: usize,
    pub queue_non_block_fair_picks: usize,
    pub queue_starvation_relief_picks: usize,
    pub queue_backpressure_drops: usize,
    pub inbound_messages: usize,
    pub runtime_started: bool,
    pub runtime_mode_detail: String,
    pub swarm_events_seen: usize,
    pub subscriptions_active: usize,
    pub last_message_kind: Option<String>,
    pub last_swarm_event: Option<String>,
    pub per_topic_publishes: HashMap<String, usize>,
    pub inbound_decode_failed: usize,
    pub inbound_chain_mismatch_dropped: usize,
    pub inbound_duplicates_suppressed: usize,
    pub outbound_duplicates_suppressed: usize,
    pub inv_blocks_received: usize,
    pub inv_hashes_known: usize,
    pub inv_hashes_requested: usize,
    pub header_requests_received: u64,
    pub header_requests_sent: u64,
    pub headers_received: u64,
    pub headers_sent: u64,
    pub headers_announced: u64,
    pub dependency_fetches_scheduled: u64,
    pub parent_first_fetches: u64,
    pub relay_loop_prevented: usize,
    pub seen_cache_ttl_secs: u64,
    pub recovery_rebroadcast_ttl_secs: u64,
    pub max_inventory_length: usize,
    pub max_request_fanout: usize,
    pub tx_inbound_received: usize,
    pub tx_inbound_accepted: usize,
    pub tx_inbound_duplicate: usize,
    pub tx_inbound_invalid: usize,
    pub tx_relayed: usize,
    pub tx_relay_suppressed_budget: usize,
    pub tx_relay_suppressed_duplicate: usize,
    pub tx_outbound_duplicates_suppressed: usize,
    pub tx_outbound_first_seen_relayed: usize,
    pub tx_outbound_recovery_relayed: usize,
    pub tx_outbound_priority_relayed: usize,
    pub tx_outbound_budget_suppressed: usize,
    pub tx_outbound_recovery_budget_suppressed: usize,
    pub block_outbound_duplicates_suppressed: usize,
    pub block_outbound_first_seen_relayed: usize,
    pub block_outbound_recovery_relayed: usize,
    pub last_drop_reason: Option<String>,
    pub peer_reconnect_attempts: u64,
    pub peer_recovery_success_count: u64,
    pub last_peer_recovery_unix: Option<u64>,
    pub peer_cooldown_suppressed_count: u64,
    pub peer_flap_suppressed_count: u64,
    pub peer_message_rate_limited_count: u64,
    pub peer_effective_count: usize,
    pub peer_min_target_missed_total: u64,
    pub peer_min_target_reconnect_attempt_total: u64,
    pub peer_min_target_reconnect_success_total: u64,
    pub peer_below_target_duration_seconds: u64,
    pub peer_below_target_blocked_reason: Option<String>,
    pub peer_known_connected_total: usize,
    pub peer_known_disconnected_total: usize,
    pub peer_known_cooldown_total: usize,
    pub peer_known_rate_limited_total: usize,
    pub peer_known_dialable_total: usize,
    pub peer_recovery_state: String,
    pub peer_cooldown_bypassed_for_connectivity_total: u64,
    pub peer_rate_limit_recovery_suppressed_total: u64,
    pub peer_rate_limit_by_kind_total: HashMap<String, u64>,
    pub peer_suppressed_dial_count: u64,
    pub peers_under_cooldown: usize,
    pub peers_under_flap_guard: usize,
    pub peer_lifecycle_healthy: usize,
    pub peer_lifecycle_watch: usize,
    pub peer_lifecycle_degraded: usize,
    pub peer_lifecycle_cooldown: usize,
    pub peer_lifecycle_recovering: usize,
    pub peer_retention_active_total: usize,
    pub peer_retention_recovering_total: usize,
    pub peer_retention_cooldown_total: usize,
    pub peer_sync_eligible_total: usize,
    pub peer_sync_suppressed_total: usize,
    pub degraded_mode: String,
    pub connection_shaping_active: bool,
    pub peer_recovery: Vec<PeerRecoveryStatus>,
    pub sync_candidates: Vec<RankedSyncPeer>,
    pub selected_sync_peer: Option<String>,
    pub connection_slot_budget: usize,
    pub connected_slots_in_use: usize,
    pub available_connection_slots: usize,
    pub sync_selection_sticky_until_unix: Option<u64>,
    pub topology_bucket_count: usize,
    pub topology_distinct_buckets: usize,
    pub topology_dominant_bucket_share_bps: u16,
    pub topology_diversity_score_bps: u16,
    pub blocks_requested: u64,
    pub blocks_received: u64,
    pub invalid_blocks_received: u64,
    pub orphan_blocks_received: u64,
    pub duplicate_blocks_received: u64,
    pub peer_penalties: u64,
    pub active_connections_by_peer: HashMap<String, usize>,
    pub active_connection_total: usize,
    pub last_connection_established_peer: Option<String>,
    pub last_connection_closed_peer: Option<String>,
    pub last_connection_closed_remaining_count: Option<usize>,
    pub last_outgoing_connection_error_peer: Option<String>,
    pub last_incoming_connection_error_peer: Option<String>,
    pub last_dial_error: Option<String>,
    pub last_disconnect_reason: Option<String>,
    pub last_peer_state_transition: Option<String>,
    pub bootstrap_dial_attempts: u64,
    pub bootstrap_dial_successes: u64,
    pub bootstrap_dial_failures: u64,
    pub bootstrap_connected_peer_ids: Vec<String>,
    pub bootnodes_configured: Vec<String>,
    pub bootnodes_connected: Vec<String>,
    pub pending_bootnode_dials: Vec<String>,
    pub bootnode_redial_attempts: u64,
    pub bootnode_redial_successes: u64,
    pub bootnode_redial_failures: u64,
    pub bootnode_reconnect_scheduled_total: u64,
    pub bootnode_reconnect_skipped_cooldown_total: u64,
    pub bootnode_reconnect_forced_from_cooldown_total: u64,
    pub bootnode_reconnect_success_total: u64,
    pub isolated_bootnode_reconnect_active: bool,
    pub peer_zero_count_duration_seconds: u64,
    pub peer_zero_reconnect_attempt_total: u64,
    pub peer_zero_reconnect_success_total: u64,
    pub peer_reconnect_suppressed_by_cooldown_total: u64,
    pub peer_reconnect_suppressed_by_rate_limit_total: u64,
    pub peer_min_target_recovered_total: u64,
    pub last_peer_reconnect_blocked_reason: Option<String>,
    pub bootnode_next_redial_at: HashMap<String, u64>,
    pub bootnode_redial_backoff_secs: HashMap<String, u64>,
    pub last_bootnode_dial_error: Option<String>,
    pub gossipsub_peer_count: usize,
    pub subscribed_topics: Vec<String>,
    pub connection_established_total: u64,
    pub connection_closed_total: u64,
    pub last_connection_closed_reason: Option<String>,
    pub disconnect_reason_counts: HashMap<String, u64>,
    pub peer_lifecycle_event_counters: HashMap<String, u64>,
    pub last_error_by_peer: HashMap<String, String>,
    pub inbound_peer_final_state: Vec<PeerConnectionFinalState>,
    pub outbound_peer_final_state: Vec<PeerConnectionFinalState>,
}

pub trait P2pHandle: Send + Sync {
    fn broadcast_transaction(&self, tx: &Transaction) -> Result<(), PulseError>;
    fn broadcast_block(&self, block: &Block) -> Result<(), PulseError>;
    fn request_tips(&self) -> Result<(), PulseError> {
        Ok(())
    }
    fn send_tips(&self, _tips: &[PulseHash]) -> Result<(), PulseError> {
        Ok(())
    }
    fn request_block_headers(&self, _hashes: &[PulseHash]) -> Result<(), PulseError> {
        Ok(())
    }
    fn send_block_headers(&self, _headers: &[BlockHeaderAnnouncement]) -> Result<(), PulseError> {
        Ok(())
    }
    fn request_block(&self, _hash: &PulseHash) -> Result<(), PulseError> {
        Ok(())
    }
    fn announce_block_inventory(&self, _hashes: &[PulseHash]) -> Result<(), PulseError> {
        Ok(())
    }
    fn request_headers(
        &self,
        _locator: &[PulseHash],
        _stop_hash: Option<&PulseHash>,
        _limit: usize,
    ) -> Result<(), PulseError> {
        Ok(())
    }
    fn send_headers(&self, _headers: &[HeaderInventory]) -> Result<(), PulseError> {
        Ok(())
    }
    fn send_block_data(
        &self,
        _request_hash: Option<&PulseHash>,
        _block: Option<&Block>,
    ) -> Result<(), PulseError> {
        Ok(())
    }
    fn status(&self) -> Result<P2pStatus, PulseError>;
}

#[derive(Debug, Clone)]
pub struct Libp2pConfig {
    pub chain_id: String,
    pub listen_addr: String,
    pub bootstrap: Vec<String>,
    pub enable_mdns: bool,
    pub enable_kademlia: bool,
    pub connection_slot_budget: usize,
    pub sync_selection_stickiness_secs: u64,
    pub runtime: Libp2pRuntimeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Libp2pRuntimeMode {
    DevLoopbackSkeleton,
    RealSwarm,
}

#[derive(Debug, Clone)]
pub enum P2pMode {
    Memory {
        chain_id: String,
        peers: Vec<String>,
    },
    Libp2p(Libp2pConfig),
}

#[derive(Debug, Clone)]
pub enum InboundEvent {
    Transaction(Transaction),
    Block(Block),
    BlockAnnouncement {
        hash: String,
    },
    BlockInventory {
        hashes: Vec<PulseHash>,
    },
    GetTips,
    Tips {
        tips: Vec<PulseHash>,
    },
    GetHeaders {
        locator: Vec<PulseHash>,
        stop_hash: Option<PulseHash>,
        limit: usize,
    },
    Headers {
        headers: Vec<HeaderInventory>,
    },
    GetBlockHeaders {
        hashes: Vec<PulseHash>,
    },
    BlockHeaders {
        headers: Vec<BlockHeaderAnnouncement>,
    },
    GetBlock {
        hash: PulseHash,
    },
    BlockDataMissing {
        hash: Option<PulseHash>,
    },
    PeerConnected(String),
}

#[derive(Debug, Clone)]
enum OutboundMessage {
    Transaction(Transaction),
    Block(Block),
    GetTips,
    Tips(Vec<PulseHash>),
    InvBlock(Vec<PulseHash>),
    GetHeaders {
        locator: Vec<PulseHash>,
        stop_hash: Option<PulseHash>,
        limit: usize,
    },
    Headers(Vec<HeaderInventory>),
    GetBlockHeaders(Vec<PulseHash>),
    BlockHeaders(Vec<BlockHeaderAnnouncement>),
    GetBlock(PulseHash),
    BlockData {
        block: Option<Block>,
        request_hash: Option<PulseHash>,
    },
}

pub struct P2pStack {
    pub handle: Arc<dyn P2pHandle>,
    pub inbound_rx: Option<mpsc::UnboundedReceiver<InboundEvent>>,
}

#[derive(Default)]
struct InnerState {
    broadcasted_messages: usize,
    publish_attempts: usize,
    inbound_messages: usize,
    chain_id: String,
    connected_peers: Vec<String>,
    seen_message_ids: HashSet<String>,
    queued_messages: usize,
    queued_block_messages: usize,
    queued_non_block_messages: usize,
    queue_max_depth: usize,
    dequeued_block_messages: usize,
    dequeued_non_block_messages: usize,
    queue_block_priority_picks: usize,
    queue_priority_tx_lane_picks: usize,
    queue_standard_tx_lane_picks: usize,
    queue_non_block_fair_picks: usize,
    queue_starvation_relief_picks: usize,
    queue_backpressure_drops: usize,
    topics: Vec<String>,
    mode: String,
    peer_id: String,
    listening: Vec<String>,
    mdns: bool,
    kademlia: bool,
    runtime_started: bool,
    runtime_mode_detail: String,
    swarm_events_seen: usize,
    subscriptions_active: usize,
    last_message_kind: Option<String>,
    last_swarm_event: Option<String>,
    per_topic_publishes: HashMap<String, usize>,
    inbound_decode_failed: usize,
    inbound_chain_mismatch_dropped: usize,
    inbound_duplicates_suppressed: usize,
    outbound_duplicates_suppressed: usize,
    inv_blocks_received: usize,
    inv_hashes_known: usize,
    inv_hashes_requested: usize,
    header_requests_received: u64,
    header_requests_sent: u64,
    headers_received: u64,
    headers_sent: u64,
    headers_announced: u64,
    block_headers_requested: u64,
    block_header_batches_received: u64,
    block_headers_received: u64,
    dependency_fetches_scheduled: u64,
    parent_first_fetches: u64,
    relay_loop_prevented: usize,
    tx_inbound_received: usize,
    tx_inbound_accepted: usize,
    tx_inbound_duplicate: usize,
    tx_inbound_invalid: usize,
    tx_inbound_rate_window_started_unix: u64,
    tx_inbound_rate_window_count: usize,
    blocks_requested: u64,
    blocks_received: u64,
    invalid_blocks_received: u64,
    orphan_blocks_received: u64,
    duplicate_blocks_received: u64,
    peer_penalties: u64,
    tx_outbound_duplicates_suppressed: usize,
    tx_outbound_first_seen_relayed: usize,
    tx_outbound_recovery_relayed: usize,
    tx_outbound_priority_relayed: usize,
    tx_outbound_budget_suppressed: usize,
    tx_outbound_recovery_budget_suppressed: usize,
    block_outbound_duplicates_suppressed: usize,
    block_outbound_first_seen_relayed: usize,
    block_outbound_recovery_relayed: usize,
    last_drop_reason: Option<String>,
    inbound_seen_at_unix: HashMap<String, u64>,
    inbound_seen_cache: HashMap<String, SeenCacheEntry>,
    known_block_hashes: HashSet<String>,
    known_txids: HashSet<String>,
    outbound_tx_seen_at_unix: HashMap<String, u64>,
    outbound_block_seen_at_unix: HashMap<String, u64>,
    outbound_tx_recovery_relay_generation: HashMap<String, u64>,
    outbound_block_recovery_relay_generation: HashMap<String, u64>,
    recovery_rebroadcast_generation: u64,
    recovery_rebroadcast_until_unix: u64,
    recovery_rebroadcast_budget_window_started_unix: u64,
    recovery_rebroadcast_budget_used: usize,
    tx_budget_window_started_unix: u64,
    tx_budget_window_relays: usize,
    peer_book: HashMap<String, PeerHealth>,
    active_connections: HashMap<String, usize>,
    last_connection_established_peer: Option<String>,
    last_connection_closed_peer: Option<String>,
    last_connection_closed_remaining_count: Option<usize>,
    last_outgoing_connection_error_peer: Option<String>,
    last_incoming_connection_error_peer: Option<String>,
    last_dial_error: Option<String>,
    last_disconnect_reason: Option<String>,
    last_peer_state_transition: Option<String>,
    bootstrap_dial_attempts: u64,
    bootstrap_dial_successes: u64,
    bootstrap_dial_failures: u64,
    bootstrap_connected_peer_ids: Vec<String>,
    bootnodes_configured: Vec<String>,
    pending_bootnode_dials: HashSet<String>,
    bootnode_redial_attempts: u64,
    bootnode_redial_successes: u64,
    bootnode_redial_failures: u64,
    bootnode_reconnect_scheduled_total: u64,
    bootnode_reconnect_skipped_cooldown_total: u64,
    bootnode_reconnect_forced_from_cooldown_total: u64,
    bootnode_reconnect_success_total: u64,
    peer_zero_since_unix: Option<u64>,
    peer_zero_reconnect_attempt_total: u64,
    peer_zero_reconnect_success_total: u64,
    peer_reconnect_suppressed_by_cooldown_total: u64,
    peer_reconnect_suppressed_by_rate_limit_total: u64,
    peer_min_target_recovered_total: u64,
    last_peer_reconnect_blocked_reason: Option<String>,
    pending_bootnode_dial_started_at: HashMap<String, u64>,
    bootnode_next_redial_at: HashMap<String, u64>,
    bootnode_redial_backoff_secs: HashMap<String, u64>,
    last_bootnode_dial_error: Option<String>,
    connection_established_total: u64,
    connection_closed_total: u64,
    last_connection_closed_reason: Option<String>,
    disconnect_reason_counts: HashMap<String, u64>,
    peer_lifecycle_event_counters: HashMap<String, u64>,
    last_error_by_peer: HashMap<String, String>,
    peer_connection_final_state: HashMap<String, PeerConnectionFinalState>,
    peer_state_path: Option<PathBuf>,
    peer_reconnect_attempts: u64,
    peer_recovery_success_count: u64,
    last_peer_recovery_unix: Option<u64>,
    peer_cooldown_suppressed_count: u64,
    peer_flap_suppressed_count: u64,
    peer_message_rate_limited_count: u64,
    peer_min_target_missed_total: u64,
    peer_min_target_reconnect_attempt_total: u64,
    peer_min_target_reconnect_success_total: u64,
    peer_below_target_since_unix: Option<u64>,
    peer_below_target_blocked_reason: Option<String>,
    peer_cooldown_bypassed_for_connectivity_total: u64,
    peer_rate_limit_recovery_suppressed_total: u64,
    peer_rate_limit_by_kind_total: HashMap<String, u64>,
    peer_suppressed_dial_count: u64,
    connection_slot_budget: usize,
    sync_selection_stickiness_secs: u64,
    selected_sync_peer: Option<String>,
    sync_selection_sticky_until_unix: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct PeerHealth {
    score: i32,
    fail_streak: u32,
    next_retry_unix: u64,
    connected: bool,
    last_seen_unix: Option<u64>,
    remote_chain_id: Option<String>,
    chain_id_compatible: bool,
    last_successful_connect_unix: Option<u64>,
    reconnect_attempts: u64,
    recovery_success_count: u64,
    last_recovery_unix: Option<u64>,
    last_failure_unix: Option<u64>,
    recent_failures_unix: Vec<u64>,
    flap_events: u32,
    suppressed_until_unix: u64,
    cooldown_suppressed_count: u64,
    flap_suppressed_count: u64,
    last_error: Option<String>,
    last_error_unix: Option<u64>,
    last_error_source: Option<String>,
    chain_mismatch_streak: u32,
    invalid_block_announce_streak: u32,
    inbound_window_started_unix: u64,
    inbound_window_count: usize,
    dial_window_started_unix: u64,
    dial_attempts_in_window: usize,
    last_successful_block_unix: Option<u64>,
    last_rate_limited_unix: Option<u64>,
}

impl Default for PeerHealth {
    fn default() -> Self {
        Self {
            score: 100,
            fail_streak: 0,
            next_retry_unix: 0,
            connected: true,
            last_seen_unix: None,
            remote_chain_id: None,
            chain_id_compatible: true,
            last_successful_connect_unix: None,
            reconnect_attempts: 0,
            recovery_success_count: 0,
            last_recovery_unix: None,
            last_failure_unix: None,
            recent_failures_unix: vec![],
            flap_events: 0,
            suppressed_until_unix: 0,
            cooldown_suppressed_count: 0,
            flap_suppressed_count: 0,
            last_error: None,
            last_error_unix: None,
            last_error_source: None,
            chain_mismatch_streak: 0,
            invalid_block_announce_streak: 0,
            inbound_window_started_unix: 0,
            inbound_window_count: 0,
            dial_window_started_unix: 0,
            dial_attempts_in_window: 0,
            last_successful_block_unix: None,
            last_rate_limited_unix: None,
        }
    }
}

#[derive(Clone)]
pub struct MemoryP2pHandle {
    inner: Arc<Mutex<InnerState>>,
}

impl MemoryP2pHandle {
    pub fn new(
        chain_id: String,
        peers: Vec<String>,
    ) -> (Self, mpsc::UnboundedReceiver<InboundEvent>) {
        let (_inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let mut state = InnerState::default();
        state.chain_id = chain_id.clone();
        state.mode = P2P_MODE_MEMORY_SIMULATED.into();
        state.runtime_mode_detail = "in-process-dispatch".into();
        state.peer_id = "memory".into();
        state.listening = vec!["memory://local".into()];
        state.connected_peers = peers;
        state.topics = topic_names(&chain_id);
        state.subscriptions_active = state.topics.len();
        state.runtime_started = true;
        (
            Self {
                inner: Arc::new(Mutex::new(state)),
            },
            inbound_rx,
        )
    }
}

impl P2pHandle for MemoryP2pHandle {
    fn broadcast_transaction(&self, tx: &Transaction) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        let tx_id = message_id_for_tx(tx);
        if !should_relay_outbound_tx(&mut inner, &tx_id, now_unix()) {
            inner.last_drop_reason = Some("duplicate_tx_outbound".into());
            return Ok(());
        }
        if !admit_tx_relay_under_budget(&mut inner, &tx_id, tx.fee, now_unix()) {
            inner.last_drop_reason = Some("tx_budget_suppressed".into());
            return Ok(());
        }
        record_outbound_tx_relay(&mut inner, &tx_id, now_unix());
        inner.known_txids.insert(tx.txid.clone());
        inner.seen_message_ids.insert(tx_id);
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.last_message_kind = Some("tx".into());
        *inner
            .per_topic_publishes
            .entry("memory-txs".into())
            .or_insert(0) += 1;
        Ok(())
    }

    fn broadcast_block(&self, block: &Block) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        let block_id = message_id_for_block(block);
        if !should_relay_outbound_block(&mut inner, &block_id, now_unix()) {
            inner.last_drop_reason = Some("duplicate_block_outbound".into());
            return Ok(());
        }
        inner.known_block_hashes.insert(block.hash.clone());
        inner.seen_message_ids.insert(block_id);
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.last_message_kind = Some("block".into());
        *inner
            .per_topic_publishes
            .entry("memory-blocks".into())
            .or_insert(0) += 1;
        Ok(())
    }

    fn request_tips(&self) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.last_message_kind = Some("get-tips".into());
        Ok(())
    }

    fn send_tips(&self, _tips: &[PulseHash]) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.last_message_kind = Some("tips".into());
        Ok(())
    }

    fn request_block_headers(&self, hashes: &[PulseHash]) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.block_headers_requested = inner
            .block_headers_requested
            .saturating_add(hashes.len() as u64);
        inner.last_message_kind = Some("get-block-headers".into());
        Ok(())
    }

    fn send_block_headers(&self, headers: &[BlockHeaderAnnouncement]) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.block_header_batches_received = inner.block_header_batches_received.saturating_add(1);
        inner.block_headers_received = inner
            .block_headers_received
            .saturating_add(headers.len() as u64);
        inner.last_message_kind = Some("block-headers".into());
        Ok(())
    }

    fn request_block(&self, _hash: &PulseHash) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.blocks_requested = inner.blocks_requested.saturating_add(1);
        inner.last_message_kind = Some("get-block".into());
        Ok(())
    }

    fn announce_block_inventory(&self, hashes: &[PulseHash]) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        for hash in hashes {
            inner.known_block_hashes.insert(hash.clone());
        }
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.headers_announced = inner.headers_announced.saturating_add(hashes.len() as u64);
        inner.last_message_kind = Some("inv-block".into());
        Ok(())
    }

    fn request_headers(
        &self,
        _locator: &[PulseHash],
        _stop_hash: Option<&PulseHash>,
        _limit: usize,
    ) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.header_requests_sent = inner.header_requests_sent.saturating_add(1);
        inner.last_message_kind = Some("get-headers".into());
        Ok(())
    }

    fn send_headers(&self, headers: &[HeaderInventory]) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.headers_sent = inner.headers_sent.saturating_add(headers.len() as u64);
        inner.last_message_kind = Some("headers".into());
        Ok(())
    }

    fn send_block_data(
        &self,
        _request_hash: Option<&PulseHash>,
        _block: Option<&Block>,
    ) -> Result<(), PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        inner.publish_attempts += 1;
        inner.broadcasted_messages += 1;
        inner.last_message_kind = Some("block-data".into());
        Ok(())
    }

    fn status(&self) -> Result<P2pStatus, PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        refresh_connected_peers_from_health(&mut inner);
        enforce_connectivity_aware_cooldown_floor(&mut inner, now_unix());
        let (
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_lifecycle_healthy,
            peer_lifecycle_watch,
            peer_lifecycle_degraded,
            peer_lifecycle_cooldown,
            peer_lifecycle_recovering,
            peer_retention_active_total,
            peer_retention_recovering_total,
            peer_retention_cooldown_total,
            peer_sync_eligible_total,
            peer_sync_suppressed_total,
            degraded_mode,
            peer_recovery,
        ) = peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer =
            update_selected_sync_peer(&mut inner, &sync_candidates, now_unix());
        let connected_slots_in_use = inner.connected_peers.len();
        let available_connection_slots = inner
            .connection_slot_budget
            .saturating_sub(connected_slots_in_use);
        let (
            topology_bucket_count,
            topology_distinct_buckets,
            topology_dominant_bucket_share_bps,
            topology_diversity_score_bps,
        ) = topology_stats_for_connected_peers(&inner.connected_peers);
        let (inbound_peer_final_state, outbound_peer_final_state) =
            peer_final_state_snapshots(&inner);
        let peer_target_accounting = peer_target_accounting(&inner, now_unix());
        Ok(P2pStatus {
            chain_id: inner.chain_id.clone(),
            mode: inner.mode.clone(),
            peer_id: inner.peer_id.clone(),
            listening: inner.listening.clone(),
            connected_peers: inner.connected_peers.clone(),
            topics: inner.topics.clone(),
            mdns: inner.mdns,
            kademlia: inner.kademlia,
            broadcasted_messages: inner.broadcasted_messages,
            publish_attempts: inner.publish_attempts,
            seen_message_ids: inner.seen_message_ids.len(),
            queued_messages: inner.queued_messages,
            queued_block_messages: inner.queued_block_messages,
            queued_non_block_messages: inner.queued_non_block_messages,
            queue_max_depth: inner.queue_max_depth,
            dequeued_block_messages: inner.dequeued_block_messages,
            dequeued_non_block_messages: inner.dequeued_non_block_messages,
            queue_block_priority_picks: inner.queue_block_priority_picks,
            queue_priority_tx_lane_picks: inner.queue_priority_tx_lane_picks,
            queue_standard_tx_lane_picks: inner.queue_standard_tx_lane_picks,
            queue_non_block_fair_picks: inner.queue_non_block_fair_picks,
            queue_starvation_relief_picks: inner.queue_starvation_relief_picks,
            queue_backpressure_drops: inner.queue_backpressure_drops,
            inbound_messages: inner.inbound_messages,
            runtime_started: inner.runtime_started,
            runtime_mode_detail: inner.runtime_mode_detail.clone(),
            swarm_events_seen: inner.swarm_events_seen,
            subscriptions_active: inner.subscriptions_active,
            last_message_kind: inner.last_message_kind.clone(),
            last_swarm_event: inner.last_swarm_event.clone(),
            per_topic_publishes: inner.per_topic_publishes.clone(),
            inbound_decode_failed: inner.inbound_decode_failed,
            inbound_chain_mismatch_dropped: inner.inbound_chain_mismatch_dropped,
            inbound_duplicates_suppressed: inner.inbound_duplicates_suppressed,
            outbound_duplicates_suppressed: inner.outbound_duplicates_suppressed,
            inv_blocks_received: inner.inv_blocks_received,
            inv_hashes_known: inner.inv_hashes_known,
            inv_hashes_requested: inner.inv_hashes_requested,
            header_requests_received: inner.header_requests_received,
            header_requests_sent: inner.header_requests_sent,
            headers_received: inner.headers_received,
            headers_sent: inner.headers_sent,
            headers_announced: inner.headers_announced,
            dependency_fetches_scheduled: inner.dependency_fetches_scheduled,
            parent_first_fetches: inner.parent_first_fetches,
            relay_loop_prevented: inner.relay_loop_prevented,
            seen_cache_ttl_secs: MESSAGE_DEDUP_WINDOW_SECS,
            recovery_rebroadcast_ttl_secs: RECOVERY_REBROADCAST_GRACE_SECS,
            max_inventory_length: MAX_INV_BLOCK_HASHES,
            max_request_fanout: MAX_INV_BLOCK_REQUEST_FANOUT,
            tx_inbound_received: inner.tx_inbound_received,
            tx_inbound_accepted: inner.tx_inbound_accepted,
            tx_inbound_duplicate: inner.tx_inbound_duplicate,
            tx_inbound_invalid: inner.tx_inbound_invalid,
            tx_relayed: inner
                .tx_outbound_first_seen_relayed
                .saturating_add(inner.tx_outbound_recovery_relayed),
            tx_relay_suppressed_budget: inner.tx_outbound_budget_suppressed,
            tx_relay_suppressed_duplicate: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_duplicates_suppressed: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_first_seen_relayed: inner.tx_outbound_first_seen_relayed,
            tx_outbound_recovery_relayed: inner.tx_outbound_recovery_relayed,
            tx_outbound_priority_relayed: inner.tx_outbound_priority_relayed,
            tx_outbound_budget_suppressed: inner.tx_outbound_budget_suppressed,
            tx_outbound_recovery_budget_suppressed: inner.tx_outbound_recovery_budget_suppressed,
            block_outbound_duplicates_suppressed: inner.block_outbound_duplicates_suppressed,
            block_outbound_first_seen_relayed: inner.block_outbound_first_seen_relayed,
            block_outbound_recovery_relayed: inner.block_outbound_recovery_relayed,
            last_drop_reason: inner.last_drop_reason.clone(),
            peer_reconnect_attempts: inner.peer_reconnect_attempts,
            peer_recovery_success_count: inner.peer_recovery_success_count,
            last_peer_recovery_unix: inner.last_peer_recovery_unix,
            peer_cooldown_suppressed_count: inner.peer_cooldown_suppressed_count,
            peer_flap_suppressed_count: inner.peer_flap_suppressed_count,
            peer_message_rate_limited_count: inner.peer_message_rate_limited_count,
            peer_effective_count: inner.connected_peers.len().max(peer_sync_eligible_total),
            peer_min_target_missed_total: inner.peer_min_target_missed_total,
            peer_min_target_reconnect_attempt_total: inner.peer_min_target_reconnect_attempt_total,
            peer_min_target_reconnect_success_total: inner.peer_min_target_reconnect_success_total,
            peer_below_target_duration_seconds: inner
                .peer_below_target_since_unix
                .map(|since| now_unix().saturating_sub(since))
                .unwrap_or(0),
            peer_below_target_blocked_reason: inner.peer_below_target_blocked_reason.clone(),
            peer_known_connected_total: peer_target_accounting.connected,
            peer_known_disconnected_total: peer_target_accounting.disconnected,
            peer_known_cooldown_total: peer_target_accounting.cooldown,
            peer_known_rate_limited_total: peer_target_accounting.rate_limited,
            peer_known_dialable_total: peer_target_accounting.dialable,
            peer_recovery_state: peer_recovery_state(&inner),
            peer_cooldown_bypassed_for_connectivity_total: inner
                .peer_cooldown_bypassed_for_connectivity_total,
            peer_rate_limit_recovery_suppressed_total: inner
                .peer_rate_limit_recovery_suppressed_total,
            peer_rate_limit_by_kind_total: inner.peer_rate_limit_by_kind_total.clone(),
            peer_suppressed_dial_count: inner.peer_suppressed_dial_count,
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_lifecycle_healthy,
            peer_lifecycle_watch,
            peer_lifecycle_degraded,
            peer_lifecycle_cooldown,
            peer_lifecycle_recovering,
            peer_retention_active_total,
            peer_retention_recovering_total,
            peer_retention_cooldown_total,
            peer_sync_eligible_total,
            peer_sync_suppressed_total,
            degraded_mode,
            connection_shaping_active: mode_connected_peers_are_real_network(&inner.mode),
            peer_recovery,
            sync_candidates,
            selected_sync_peer,
            connection_slot_budget: inner.connection_slot_budget,
            connected_slots_in_use,
            available_connection_slots,
            sync_selection_sticky_until_unix: (inner.sync_selection_sticky_until_unix > 0)
                .then_some(inner.sync_selection_sticky_until_unix),
            topology_bucket_count,
            topology_distinct_buckets,
            topology_dominant_bucket_share_bps,
            topology_diversity_score_bps,
            blocks_requested: inner.blocks_requested,
            blocks_received: inner.blocks_received,
            invalid_blocks_received: inner.invalid_blocks_received,
            orphan_blocks_received: inner.orphan_blocks_received,
            duplicate_blocks_received: inner.duplicate_blocks_received,
            peer_penalties: inner.peer_penalties,
            active_connections_by_peer: inner.active_connections.clone(),
            active_connection_total: inner.active_connections.values().copied().sum(),
            last_connection_established_peer: inner.last_connection_established_peer.clone(),
            last_connection_closed_peer: inner.last_connection_closed_peer.clone(),
            last_connection_closed_remaining_count: inner.last_connection_closed_remaining_count,
            last_outgoing_connection_error_peer: inner.last_outgoing_connection_error_peer.clone(),
            last_incoming_connection_error_peer: inner.last_incoming_connection_error_peer.clone(),
            last_dial_error: inner.last_dial_error.clone(),
            last_disconnect_reason: inner.last_disconnect_reason.clone(),
            last_peer_state_transition: inner.last_peer_state_transition.clone(),
            bootstrap_dial_attempts: inner.bootstrap_dial_attempts,
            bootstrap_dial_successes: inner.bootstrap_dial_successes,
            bootstrap_dial_failures: inner.bootstrap_dial_failures,
            bootstrap_connected_peer_ids: inner.bootstrap_connected_peer_ids.clone(),
            bootnodes_configured: inner.bootnodes_configured.clone(),
            bootnodes_connected: inner
                .bootnodes_configured
                .iter()
                .filter_map(|addr| parse_bootnode_multiaddr(addr).map(|(peer, _)| peer.to_string()))
                .filter(|peer| inner.active_connections.get(peer).copied().unwrap_or(0) > 0)
                .collect(),
            pending_bootnode_dials: inner.pending_bootnode_dials.iter().cloned().collect(),
            bootnode_redial_attempts: inner.bootnode_redial_attempts,
            bootnode_redial_successes: inner.bootnode_redial_successes,
            bootnode_redial_failures: inner.bootnode_redial_failures,
            bootnode_reconnect_scheduled_total: inner.bootnode_reconnect_scheduled_total,
            bootnode_reconnect_skipped_cooldown_total: inner
                .bootnode_reconnect_skipped_cooldown_total,
            bootnode_reconnect_forced_from_cooldown_total: inner
                .bootnode_reconnect_forced_from_cooldown_total,
            bootnode_reconnect_success_total: inner.bootnode_reconnect_success_total,
            isolated_bootnode_reconnect_active: isolated_bootnode_reconnect_active(&inner),
            peer_zero_count_duration_seconds: inner
                .peer_zero_since_unix
                .map(|since| now_unix().saturating_sub(since))
                .unwrap_or(0),
            peer_zero_reconnect_attempt_total: inner.peer_zero_reconnect_attempt_total,
            peer_zero_reconnect_success_total: inner.peer_zero_reconnect_success_total,
            peer_reconnect_suppressed_by_cooldown_total: inner
                .peer_reconnect_suppressed_by_cooldown_total,
            peer_reconnect_suppressed_by_rate_limit_total: inner
                .peer_reconnect_suppressed_by_rate_limit_total,
            peer_min_target_recovered_total: inner.peer_min_target_recovered_total,
            last_peer_reconnect_blocked_reason: inner.last_peer_reconnect_blocked_reason.clone(),
            bootnode_next_redial_at: inner.bootnode_next_redial_at.clone(),
            bootnode_redial_backoff_secs: inner.bootnode_redial_backoff_secs.clone(),
            last_bootnode_dial_error: inner.last_bootnode_dial_error.clone(),
            gossipsub_peer_count: inner.active_connections.len(),
            subscribed_topics: inner.topics.clone(),
            connection_established_total: inner.connection_established_total,
            connection_closed_total: inner.connection_closed_total,
            last_connection_closed_reason: inner.last_connection_closed_reason.clone(),
            disconnect_reason_counts: inner.disconnect_reason_counts.clone(),
            peer_lifecycle_event_counters: inner.peer_lifecycle_event_counters.clone(),
            last_error_by_peer: inner.last_error_by_peer.clone(),
            inbound_peer_final_state,
            outbound_peer_final_state,
        })
    }
}

#[derive(Clone)]
pub struct Libp2pHandle {
    inner: Arc<Mutex<InnerState>>,
    outbound_tx: mpsc::UnboundedSender<OutboundMessage>,
}

impl Libp2pHandle {
    pub fn new(
        cfg: Libp2pConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<InboundEvent>), PulseError> {
        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = local_key.public().to_peer_id();
        let topics = topic_names(&cfg.chain_id);
        let topic_objs = topics
            .iter()
            .map(|t| gossipsub::IdentTopic::new(t.clone()))
            .collect::<Vec<_>>();

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<OutboundMessage>();
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundEvent>();

        let real_network_connectivity = matches!(cfg.runtime, Libp2pRuntimeMode::RealSwarm);
        let (mode, runtime_mode_detail) = match cfg.runtime {
            Libp2pRuntimeMode::DevLoopbackSkeleton => (
                P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON.to_string(),
                "swarm-poll-loop-skeleton".to_string(),
            ),
            Libp2pRuntimeMode::RealSwarm => (
                P2P_MODE_LIBP2P_REAL.to_string(),
                "swarm-poll-loop-real".to_string(),
            ),
        };
        let peer_book = parse_bootstrap(&cfg.bootstrap)
            .into_iter()
            .map(|(peer_id, _)| {
                let mut health = PeerHealth::default();
                if real_network_connectivity {
                    health.connected = false;
                }
                (peer_id.to_string(), health)
            })
            .collect();
        let mut state = InnerState {
            chain_id: cfg.chain_id.clone(),
            mode,
            runtime_mode_detail,
            peer_id: peer_id.to_string(),
            listening: vec![cfg.listen_addr.clone()],
            peer_book,
            active_connections: HashMap::new(),
            last_connection_established_peer: None,
            last_connection_closed_peer: None,
            last_connection_closed_remaining_count: None,
            last_outgoing_connection_error_peer: None,
            last_incoming_connection_error_peer: None,
            last_dial_error: None,
            last_disconnect_reason: None,
            last_peer_state_transition: None,
            bootstrap_dial_attempts: 0,
            bootstrap_dial_successes: 0,
            bootstrap_dial_failures: 0,
            bootstrap_connected_peer_ids: Vec::new(),
            bootnodes_configured: cfg.bootstrap.clone(),
            bootnode_redial_attempts: 0,
            bootnode_redial_successes: 0,
            bootnode_redial_failures: 0,
            bootnode_reconnect_scheduled_total: 0,
            bootnode_reconnect_skipped_cooldown_total: 0,
            bootnode_reconnect_forced_from_cooldown_total: 0,
            bootnode_reconnect_success_total: 0,
            peer_zero_since_unix: None,
            peer_zero_reconnect_attempt_total: 0,
            peer_zero_reconnect_success_total: 0,
            peer_reconnect_suppressed_by_cooldown_total: 0,
            peer_reconnect_suppressed_by_rate_limit_total: 0,
            peer_min_target_recovered_total: 0,
            last_peer_reconnect_blocked_reason: None,
            pending_bootnode_dial_started_at: HashMap::new(),
            last_bootnode_dial_error: None,
            connection_established_total: 0,
            connection_closed_total: 0,
            last_connection_closed_reason: None,
            disconnect_reason_counts: HashMap::new(),
            peer_lifecycle_event_counters: HashMap::new(),
            last_error_by_peer: HashMap::new(),
            peer_connection_final_state: HashMap::new(),
            peer_state_path: peer_state_path(),
            connection_slot_budget: cfg.connection_slot_budget.max(1),
            sync_selection_stickiness_secs: cfg.sync_selection_stickiness_secs,
            ..InnerState::default()
        };
        if let Some(path) = state.peer_state_path.as_ref() {
            for (peer, health) in load_peer_book(path) {
                state.peer_book.insert(peer, health);
            }
        }
        if real_network_connectivity {
            for health in state.peer_book.values_mut() {
                health.connected = false;
            }
        }
        refresh_connected_peers_from_health(&mut state);
        state.topics = topics.clone();
        state.subscriptions_active = topics.len();
        state.mdns = cfg.enable_mdns;
        state.kademlia = cfg.enable_kademlia;
        state.runtime_started = true;
        let inner = Arc::new(Mutex::new(state));

        match cfg.runtime {
            Libp2pRuntimeMode::DevLoopbackSkeleton => {
                tokio::spawn(run_libp2p_skeleton_runtime(
                    cfg,
                    peer_id,
                    topic_objs,
                    inner.clone(),
                    outbound_rx,
                    inbound_tx,
                ));
            }
            Libp2pRuntimeMode::RealSwarm => {
                tokio::spawn(run_libp2p_real_runtime(
                    cfg,
                    local_key,
                    topic_objs,
                    inner.clone(),
                    outbound_rx,
                    inbound_tx,
                ));
            }
        }

        Ok((Self { inner, outbound_tx }, inbound_rx))
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn peer_jitter(peer: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    peer.hash(&mut hasher);
    hasher.finish() % 3
}

const FLAP_WINDOW: StdDuration = StdDuration::from_secs(45);
const FLAP_BASE_COOLDOWN: u64 = 30;
const PEER_SCORE_MIN: i32 = -200;
const PEER_SCORE_MAX: i32 = 200;
const PEER_SUCCESS_BASE_BONUS: i32 = 8;
const PEER_FAILURE_BASE_PENALTY: i32 = 12;
const PEER_FAILURE_STREAK_PENALTY: i32 = 4;
const PEER_FAILURE_RECENT_PENALTY: i32 = 2;
const BACKOFF_BASE_SECS: u64 = 2;
const BACKOFF_EXP_CAP: u32 = 8;
const BACKOFF_MAX_SECS: u64 = 300;
const MESSAGE_DEDUP_WINDOW_SECS: u64 = 120;
const MAX_INV_BLOCK_HASHES: usize = 512;
const MAX_INV_BLOCK_REQUEST_FANOUT: usize = 64;
const TX_INBOUND_DEDUP_WINDOW_SECS: u64 = 120;
const TX_OUTBOUND_DEDUP_WINDOW_SECS: u64 = 30;
const MAX_TX_MESSAGE_BYTES: usize = 64 * 1024;
const TX_INBOUND_RATE_WINDOW_SECS: u64 = 1;
const TX_INBOUND_SOFT_MAX_PER_WINDOW: usize = 128;
const PEER_INBOUND_RATE_WINDOW_SECS: u64 = 1;
const PEER_MAX_INBOUND_MESSAGES_PER_WINDOW: usize = 192;
const PEER_DIAL_ATTEMPT_WINDOW_SECS: u64 = 60;
const PEER_MAX_DIAL_ATTEMPTS_PER_WINDOW: usize = 8;
const PEER_VALID_RELAY_BONUS: i32 = 2;
const PEER_MALFORMED_MESSAGE_PENALTY: i32 = 18;
const PEER_CHAIN_MISMATCH_PENALTY: i32 = 10;
const PEER_RATE_LIMIT_PENALTY: i32 = 8;
const PEER_INVALID_BLOCK_PENALTY: i32 = 24;
const PEER_INVALID_ANNOUNCEMENT_PENALTY: i32 = 12;
const PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_THRESHOLD: u32 = 3;
const PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_SECS: u64 = 90;
const BLOCK_OUTBOUND_DEDUP_WINDOW_SECS: u64 = 30;
const RECOVERY_REBROADCAST_GRACE_SECS: u64 = 8;
const RECOVERY_REBROADCAST_BUDGET_WINDOW_SECS: u64 = 8;
const RECOVERY_REBROADCAST_BUDGET_PER_WINDOW: usize = 256;
const MAX_DEDUP_TRACKED_IDS: usize = 16_384;
const BLOCK_PRIORITY_BURST_LIMIT: usize = 8;
const PRIORITY_TX_BURST_LIMIT: usize = 3;
const TX_PRIORITY_FEE_THRESHOLD: u64 = 1_000;
const TX_RELAY_BUDGET_WINDOW_SECS: u64 = 1;
const TX_RELAY_BUDGET_PER_WINDOW: usize = 256;
const TX_RELAY_BUDGET_OVERFLOW_SAMPLE_EVERY: u64 = 8;
const TX_BUDGET_LOAD_SHED_QUEUE_DEPTH_THRESHOLD: usize = 512;
const OUTBOUND_QUEUE_SOFT_CAP: usize = 1024;
const CONNECTION_SHAPING_DEGRADED_CAP_DIVISOR: usize = 3;
const CONNECTION_SHAPING_MIN_HEALTHY_SLOTS: usize = 1;
const CONNECTION_PRESSURE_DEGRADED_BPS_THRESHOLD: usize = 4_500;
const CONNECTION_PRESSURE_MIN_BUDGET_DIVISOR: usize = 2;
const CONNECTION_PRESSURE_RECOVERY_BOOST_BPS: usize = 2_500;
const TOPOLOGY_BUCKET_COUNT: usize = 8;
const TOPOLOGY_BUCKET_SOFT_CAP_DIVISOR: usize = 2;
const DEGRADED_MODE_DEGRADED_BPS_THRESHOLD: usize = 4_000;

fn peer_lifecycle_tier(health: &PeerHealth, now: u64) -> &'static str {
    if health.suppressed_until_unix > now || health.next_retry_unix > now {
        return "cooldown";
    }
    if health.connected && health.fail_streak == 0 && health.score >= 90 && health.flap_events == 0
    {
        return "healthy";
    }
    if health.fail_streak >= 3 || health.score < 60 {
        return "degraded";
    }
    if health.fail_streak > 0 || !health.recent_failures_unix.is_empty() || health.flap_events > 0 {
        return "recovering";
    }
    "watch"
}

fn peer_recovery_tier(health: &PeerHealth, now: u64) -> &'static str {
    if health.suppressed_until_unix > now {
        "quarantine"
    } else if health.next_retry_unix > now || health.fail_streak >= 2 {
        "assisted"
    } else if peer_lifecycle_tier(health, now) == "recovering" {
        "recovering"
    } else {
        "steady"
    }
}

fn peer_recovery_reason(health: &PeerHealth, now: u64) -> Option<String> {
    if let Some(error) = health
        .last_error
        .as_ref()
        .filter(|error| !error.trim().is_empty())
    {
        return Some(format!("last_error: {error}"));
    }
    if health.suppressed_until_unix > now {
        return Some(format!(
            "flap guard active until {}",
            health.suppressed_until_unix
        ));
    }
    if health.next_retry_unix > now {
        return Some(format!("retry cooldown until {}", health.next_retry_unix));
    }
    if !health.connected {
        return Some("peer is disconnected".to_string());
    }
    if health.fail_streak > 0 {
        return Some(format!("{} consecutive failure(s)", health.fail_streak));
    }
    if health.flap_events > 0 {
        return Some(format!(
            "{} flap event(s) in recovery window",
            health.flap_events
        ));
    }
    if !health.recent_failures_unix.is_empty() {
        return Some(format!(
            "{} recent failure(s) still in recovery window",
            health.recent_failures_unix.len()
        ));
    }
    if health.score < 90 {
        return Some(format!(
            "peer score below healthy threshold: {}",
            health.score
        ));
    }
    None
}

fn peer_score_components(health: &PeerHealth, now: u64) -> PeerScoreComponents {
    let recent_block_bonus = health
        .last_successful_block_unix
        .and_then(|last| now.checked_sub(last))
        .map(|age| 36i64.saturating_sub(age as i64 / 2).max(0))
        .unwrap_or(0);
    let recent_rate_limit_penalty = health
        .last_rate_limited_unix
        .and_then(|last| now.checked_sub(last))
        .map(|age| 48i64.saturating_sub(age as i64).max(0))
        .unwrap_or(0);
    let connection_age_bonus = health
        .last_successful_connect_unix
        .and_then(|last| now.checked_sub(last))
        .map(|age| ((age / 30) as i64).min(24))
        .unwrap_or(0);
    let failure_penalty = (health.fail_streak as i64).min(8) * 32
        + (health.recent_failures_unix.len() as i64).min(8) * 12;
    let bounded_score = (health.score as i64 + recent_block_bonus + connection_age_bonus
        - recent_rate_limit_penalty
        - failure_penalty)
        .clamp(PEER_SCORE_MIN as i64, PEER_SCORE_MAX as i64);
    PeerScoreComponents {
        base_score: health.score,
        recent_block_bonus,
        recent_rate_limit_penalty,
        connection_age_bonus,
        failure_penalty,
        bounded_score,
    }
}

fn peer_rate_limited_recently(health: &PeerHealth, now: u64) -> bool {
    health.last_rate_limited_unix.is_some_and(|last| {
        now.saturating_sub(last) <= PEER_INBOUND_RATE_WINDOW_SECS.saturating_mul(4)
    })
}

fn peer_eligible_for_sync(peer_id: &str, health: &PeerHealth, active: bool, now: u64) -> bool {
    health.connected
        && health.chain_id_compatible
        && is_valid_peer_id(peer_id)
        && (active || (health.next_retry_unix <= now && health.suppressed_until_unix <= now))
}

fn peer_health_states(peer_id: &str, health: &PeerHealth, active: bool, now: u64) -> Vec<String> {
    let mut states = Vec::new();
    if health.connected {
        states.push("connected".to_string());
    }
    if active {
        states.push("active".to_string());
    }
    if peer_lifecycle_tier(health, now) == "recovering" {
        states.push("recovering".to_string());
    }
    if health.next_retry_unix > now || health.suppressed_until_unix > now {
        states.push("cooling_down".to_string());
    }
    if peer_rate_limited_recently(health, now) {
        states.push("rate_limited".to_string());
    }
    if peer_eligible_for_sync(peer_id, health, active, now) {
        states.push("eligible_for_sync".to_string());
    }
    states
}

type PeerRecoverySnapshot = (
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    String,
    Vec<PeerRecoveryStatus>,
);

fn peer_recovery_snapshot(state: &InnerState) -> PeerRecoverySnapshot {
    let now = now_unix();
    let mut peer_recovery = state
        .peer_book
        .iter()
        .map(|(peer_id, health)| {
            let active = state.active_connections.get(peer_id).copied().unwrap_or(0) > 0;
            let eligible_for_sync = peer_eligible_for_sync(peer_id, health, active, now);
            PeerRecoveryStatus {
                peer_id: peer_id.clone(),
                chain_id: health.remote_chain_id.clone(),
                chain_id_compatible: health.chain_id_compatible,
                last_activity_unix: health
                    .last_seen_unix
                    .or(health.last_successful_connect_unix)
                    .or(health.last_failure_unix)
                    .or(health.last_recovery_unix),
                score: health.score,
                fail_streak: health.fail_streak,
                lifecycle_tier: peer_lifecycle_tier(health, now).to_string(),
                recovery_tier: peer_recovery_tier(health, now).to_string(),
                recovery_reason: peer_recovery_reason(health, now),
                connected: health.connected,
                last_seen_unix: health.last_seen_unix,
                last_successful_connect_unix: health.last_successful_connect_unix,
                next_retry_unix: health.next_retry_unix,
                reconnect_attempts: health.reconnect_attempts,
                recovery_success_count: health.recovery_success_count,
                last_recovery_unix: health.last_recovery_unix,
                recent_failures_unix: health.recent_failures_unix.clone(),
                cooldown_suppressed_count: health.cooldown_suppressed_count,
                flap_suppressed_count: health.flap_suppressed_count,
                flap_events: health.flap_events,
                suppression_until_unix: (health.suppressed_until_unix > now)
                    .then_some(health.suppressed_until_unix),
                last_error: health.last_error.clone(),
                last_error_unix: health.last_error_unix,
                last_error_source: health.last_error_source.clone(),
                health_states: peer_health_states(peer_id, health, active, now),
                eligible_for_sync,
                last_successful_block_unix: health.last_successful_block_unix,
                last_rate_limited_unix: health.last_rate_limited_unix,
                connection_age_secs: health
                    .last_successful_connect_unix
                    .map(|connected_at| now.saturating_sub(connected_at)),
                score_components: peer_score_components(health, now),
            }
        })
        .collect::<Vec<_>>();
    peer_recovery.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    let peers_under_cooldown = peer_recovery
        .iter()
        .filter(|peer| peer.next_retry_unix > now)
        .count();
    let peers_under_flap_guard = peer_recovery
        .iter()
        .filter(|peer| peer.suppression_until_unix.is_some())
        .count();
    let peer_lifecycle_healthy = peer_recovery
        .iter()
        .filter(|peer| peer.lifecycle_tier == "healthy")
        .count();
    let peer_lifecycle_watch = peer_recovery
        .iter()
        .filter(|peer| peer.lifecycle_tier == "watch")
        .count();
    let peer_lifecycle_degraded = peer_recovery
        .iter()
        .filter(|peer| peer.lifecycle_tier == "degraded")
        .count();
    let peer_lifecycle_cooldown = peer_recovery
        .iter()
        .filter(|peer| peer.lifecycle_tier == "cooldown")
        .count();
    let peer_lifecycle_recovering = peer_recovery
        .iter()
        .filter(|peer| peer.lifecycle_tier == "recovering")
        .count();
    let peer_retention_active_total = peer_recovery
        .iter()
        .filter(|peer| peer.health_states.iter().any(|state| state == "active"))
        .count();
    let peer_retention_recovering_total = peer_lifecycle_recovering;
    let peer_retention_cooldown_total = peer_lifecycle_cooldown;
    let peer_sync_eligible_total = peer_recovery
        .iter()
        .filter(|peer| peer.eligible_for_sync)
        .count();
    let peer_sync_suppressed_total = peer_recovery
        .iter()
        .filter(|peer| !peer.eligible_for_sync)
        .filter(|peer| {
            peer.connected
                || peer
                    .health_states
                    .iter()
                    .any(|state| state == "cooling_down")
        })
        .count();
    let degraded_mode = if peer_recovery.is_empty() {
        "unknown"
    } else {
        let degraded_like =
            peer_lifecycle_degraded + peer_lifecycle_cooldown + peer_lifecycle_recovering;
        if degraded_like.saturating_mul(10_000)
            >= peer_recovery.len() * DEGRADED_MODE_DEGRADED_BPS_THRESHOLD
        {
            "explicit-degraded"
        } else {
            "normal"
        }
    };
    (
        peers_under_cooldown,
        peers_under_flap_guard,
        peer_lifecycle_healthy,
        peer_lifecycle_watch,
        peer_lifecycle_degraded,
        peer_lifecycle_cooldown,
        peer_lifecycle_recovering,
        peer_retention_active_total,
        peer_retention_recovering_total,
        peer_retention_cooldown_total,
        peer_sync_eligible_total,
        peer_sync_suppressed_total,
        degraded_mode.to_string(),
        peer_recovery,
    )
}

fn peer_final_state_snapshots(
    state: &InnerState,
) -> (Vec<PeerConnectionFinalState>, Vec<PeerConnectionFinalState>) {
    let mut inbound = Vec::new();
    let mut outbound = Vec::new();
    for final_state in state.peer_connection_final_state.values() {
        match final_state.direction.as_str() {
            "inbound" => inbound.push(final_state.clone()),
            "outbound" => outbound.push(final_state.clone()),
            _ => {}
        }
    }
    inbound.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    outbound.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    (inbound, outbound)
}

fn record_peer_lifecycle_event(state: &mut InnerState, event: &str) {
    *state
        .peer_lifecycle_event_counters
        .entry(event.to_string())
        .or_insert(0) += 1;
}

fn record_peer_error(state: &mut InnerState, peer: &str, source: &str, error: String, now: u64) {
    state
        .last_error_by_peer
        .insert(peer.to_string(), error.clone());
    let health = state.peer_book.entry(peer.to_string()).or_default();
    health.last_error = Some(error);
    health.last_error_unix = Some(now);
    health.last_error_source = Some(source.to_string());
}

fn connection_direction_from_endpoint_debug(endpoint: &str) -> &'static str {
    if endpoint.contains("Dialer") {
        "outbound"
    } else if endpoint.contains("Listener") {
        "inbound"
    } else {
        "unknown"
    }
}

fn sync_candidates_snapshot(state: &InnerState) -> Vec<RankedSyncPeer> {
    let now = now_unix();
    let candidates = state
        .peer_book
        .iter()
        .filter(|(peer_id, _)| is_valid_peer_id(peer_id))
        .map(|(peer_id, health)| SyncPeerCandidate {
            peer_id: peer_id.clone(),
            score: health.score,
            fail_streak: health.fail_streak,
            connected: health.connected,
            next_retry_unix: health.next_retry_unix,
            suppressed_until_unix: health.suppressed_until_unix,
            recovery_success_count: health.recovery_success_count,
            recent_failures: health.recent_failures_unix.len(),
            last_successful_block_unix: health.last_successful_block_unix,
            last_rate_limited_unix: health.last_rate_limited_unix,
            last_successful_connect_unix: health.last_successful_connect_unix,
        })
        .collect::<Vec<_>>();
    rank_sync_candidates(&candidates, now)
}

fn is_valid_peer_id(peer_id: &str) -> bool {
    if peer_id.trim().is_empty() {
        return false;
    }
    if peer_id.contains("/p2p/")
        || peer_id.contains("/ip4/")
        || peer_id.contains("/ip6/")
        || peer_id.contains("/tcp/")
        || peer_id.contains("/udp/")
    {
        return false;
    }
    // Reject full multiaddr strings while allowing stable test/local synthetic IDs.
    peer_id.parse::<Multiaddr>().is_err()
}

fn topology_bucket_for_peer(peer_id: &str) -> usize {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    peer_id.hash(&mut hasher);
    (hasher.finish() as usize) % TOPOLOGY_BUCKET_COUNT.max(1)
}

fn topology_stats_for_connected_peers(peers: &[String]) -> (usize, usize, u16, u16) {
    if peers.is_empty() {
        return (TOPOLOGY_BUCKET_COUNT, 0, 0, 0);
    }
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for peer in peers {
        *counts.entry(topology_bucket_for_peer(peer)).or_insert(0) += 1;
    }
    let total = peers.len().max(1);
    let dominant = counts.values().copied().max().unwrap_or(0);
    let distinct = counts.len();
    let dominant_share_bps = ((dominant * 10_000) / total).min(10_000) as u16;
    let bucket_coverage_bps = ((distinct * 10_000) / TOPOLOGY_BUCKET_COUNT.max(1)).min(10_000);
    let dominance_penalty_bps = 10_000usize.saturating_sub(dominant_share_bps as usize);
    let diversity_score_bps = ((bucket_coverage_bps + dominance_penalty_bps) / 2).min(10_000);
    (
        TOPOLOGY_BUCKET_COUNT,
        distinct,
        dominant_share_bps,
        diversity_score_bps as u16,
    )
}

fn refresh_connected_peers_from_health(state: &mut InnerState) {
    if mode_connected_peers_are_real_network(&state.mode) {
        let mut active = state
            .active_connections
            .iter()
            .filter_map(|(peer_id, connections)| {
                (*connections > 0 && is_valid_peer_id(peer_id)).then_some(peer_id.clone())
            })
            .collect::<Vec<_>>();
        active.sort();
        let active_set = active.iter().cloned().collect::<HashSet<_>>();
        let has_active_connections = !active_set.is_empty();
        let budget = adaptive_connection_slot_budget(state, now_unix());
        let now = now_unix();
        let mut active_peer_ids = state
            .active_connections
            .iter()
            .filter_map(|(peer_id, connections)| (*connections > 0).then_some(peer_id.clone()))
            .collect::<HashSet<_>>();
        let mut eligible = Vec::new();
        for peer in sync_candidates_snapshot(state) {
            let is_eligible = state
                .peer_book
                .get(&peer.peer_id)
                .map(|health| {
                    health.connected
                        && health.chain_id_compatible
                        && (peer.excluded_until_unix.is_none()
                            || active_peer_ids.contains(&peer.peer_id))
                })
                .unwrap_or(false);
            if is_eligible {
                active_peer_ids.remove(&peer.peer_id);
                eligible.push(peer.peer_id);
            }
        }
        let mut active_remainder = active_peer_ids
            .into_iter()
            .filter(|peer_id| {
                state
                    .peer_book
                    .get(peer_id)
                    .map(|health| health.chain_id_compatible)
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        active_remainder.sort();
        eligible.extend(active_remainder);
        let mut healthy = Vec::new();
        let mut recovering = Vec::new();
        let mut watch = Vec::new();
        let mut degraded = Vec::new();
        for peer_id in eligible {
            let tier = state
                .peer_book
                .get(&peer_id)
                .map(|health| peer_lifecycle_tier(health, now))
                .unwrap_or("degraded");
            match tier {
                "healthy" => healthy.push(peer_id),
                "recovering" => recovering.push(peer_id),
                "watch" => watch.push(peer_id),
                _ => degraded.push(peer_id),
            }
        }
        let mut shaped = Vec::new();
        let min_healthy_slots = CONNECTION_SHAPING_MIN_HEALTHY_SLOTS.min(budget);
        shaped.extend(healthy.iter().take(min_healthy_slots).cloned());
        shaped.extend(healthy.iter().skip(min_healthy_slots).cloned());
        shaped.extend(recovering);
        shaped.extend(watch);
        let remaining = budget.saturating_sub(shaped.len());
        if remaining > 0 {
            let degraded_cap = if !healthy.is_empty() {
                budget.max(1) / CONNECTION_SHAPING_DEGRADED_CAP_DIVISOR.max(1)
            } else {
                remaining
            }
            .max(1);
            shaped.extend(degraded.into_iter().take(remaining.min(degraded_cap)));
        }
        let topology_soft_cap = std::cmp::max(1, budget.div_ceil(TOPOLOGY_BUCKET_SOFT_CAP_DIVISOR));
        let mut bucket_counts: HashMap<usize, usize> = HashMap::new();
        let mut selected = Vec::with_capacity(if has_active_connections {
            active.len()
        } else {
            std::cmp::min(shaped.len(), budget)
        });
        let mut deferred = Vec::new();
        for peer_id in shaped {
            if !has_active_connections && selected.len() >= budget {
                break;
            }
            let bucket = topology_bucket_for_peer(&peer_id);
            if bucket_counts.get(&bucket).copied().unwrap_or(0) < topology_soft_cap {
                *bucket_counts.entry(bucket).or_insert(0) += 1;
                selected.push(peer_id);
            } else {
                deferred.push(peer_id);
            }
        }
        for peer_id in deferred {
            if !has_active_connections && selected.len() >= budget {
                break;
            }
            selected.push(peer_id);
        }
        if has_active_connections {
            for peer_id in active {
                let useful = state
                    .peer_book
                    .get(&peer_id)
                    .map(|health| health.chain_id_compatible)
                    .unwrap_or(true);
                if useful
                    && !selected
                        .iter()
                        .any(|selected_peer| selected_peer == &peer_id)
                {
                    selected.push(peer_id);
                }
            }
        }
        state.connected_peers = selected;
    } else {
        state.connected_peers.clear();
    }
}

fn adaptive_connection_slot_budget(state: &InnerState, now: u64) -> usize {
    if state.connection_slot_budget == 0 {
        return usize::MAX;
    }
    let total = state
        .peer_book
        .values()
        .filter(|health| health.connected)
        .count();
    if total == 0 {
        return state.connection_slot_budget;
    }
    let degraded = state
        .peer_book
        .values()
        .filter(|health| health.connected)
        .filter(|health| matches!(peer_lifecycle_tier(health, now), "degraded" | "cooldown"))
        .count();
    let recovery = state
        .peer_book
        .values()
        .filter(|health| health.connected)
        .filter(|health| peer_lifecycle_tier(health, now) == "recovering")
        .count();
    let degraded_share_bps = (degraded * 10_000) / total.max(1);
    let mut budget = state.connection_slot_budget;
    if degraded_share_bps >= CONNECTION_PRESSURE_DEGRADED_BPS_THRESHOLD {
        let min_budget = std::cmp::max(
            CONNECTION_SHAPING_MIN_HEALTHY_SLOTS,
            state.connection_slot_budget / CONNECTION_PRESSURE_MIN_BUDGET_DIVISOR.max(1),
        );
        budget = budget.max(1).saturating_sub(1).max(min_budget);
    }
    if recovery > 0 {
        let boost = ((state.connection_slot_budget * CONNECTION_PRESSURE_RECOVERY_BOOST_BPS)
            / 10_000)
            .max(1);
        budget = budget
            .saturating_add(boost.min(recovery))
            .min(state.connection_slot_budget);
    }
    budget.max(1)
}

fn update_selected_sync_peer(
    state: &mut InnerState,
    sync_candidates: &[RankedSyncPeer],
    now: u64,
) -> Option<String> {
    const SYNC_SELECTION_SWITCH_MARGIN: i64 = 20;
    const SYNC_SELECTION_MIN_HOLD_SECS: u64 = 12;

    let rank_score_for = |peer_id: &str| -> i64 {
        sync_candidates
            .iter()
            .find(|peer| peer.peer_id == peer_id)
            .map(|peer| peer.rank_score)
            .unwrap_or(i64::MIN / 2)
    };
    let preferred = sync_candidates
        .iter()
        .filter(|candidate| is_valid_peer_id(&candidate.peer_id))
        .filter(|candidate| candidate.excluded_until_unix.is_none())
        .max_by(|a, b| {
            a.rank_score
                .cmp(&b.rank_score)
                .then_with(|| b.peer_id.cmp(&a.peer_id))
        })
        .map(|candidate| candidate.peer_id.clone())
        .or_else(|| {
            state
                .connected_peers
                .iter()
                .filter(|peer_id| is_valid_peer_id(peer_id))
                .min()
                .cloned()
        });
    let preferred_rank_score = preferred
        .as_deref()
        .map(rank_score_for)
        .unwrap_or(i64::MIN / 2);
    let current_is_eligible = state
        .selected_sync_peer
        .as_ref()
        .map(|peer| {
            state.connected_peers.contains(peer)
                || sync_candidates.iter().any(|candidate| {
                    candidate.peer_id == *peer && candidate.excluded_until_unix.is_none()
                })
        })
        .unwrap_or(false);
    let sticky_active = state.sync_selection_sticky_until_unix > now;

    if sticky_active && current_is_eligible {
        return state.selected_sync_peer.clone();
    }

    if let (Some(current_peer), Some(next_peer)) =
        (state.selected_sync_peer.as_deref(), preferred.as_deref())
    {
        if current_peer != next_peer && current_is_eligible {
            let current_rank_score = rank_score_for(current_peer);
            let switch_delta = preferred_rank_score.saturating_sub(current_rank_score);
            if switch_delta < SYNC_SELECTION_SWITCH_MARGIN {
                state.sync_selection_sticky_until_unix = now.saturating_add(
                    state
                        .sync_selection_stickiness_secs
                        .max(SYNC_SELECTION_MIN_HOLD_SECS),
                );
                return state.selected_sync_peer.clone();
            }
        }
    }

    if let Some(next_peer) = preferred {
        let changed = state.selected_sync_peer.as_deref() != Some(next_peer.as_str());
        if changed {
            state.selected_sync_peer = Some(next_peer.clone());
        }
        if state.sync_selection_stickiness_secs > 0 {
            state.sync_selection_sticky_until_unix =
                now.saturating_add(state.sync_selection_stickiness_secs);
        }
        return Some(next_peer);
    }

    state.selected_sync_peer = None;
    state.sync_selection_sticky_until_unix = 0;
    None
}

#[derive(Default)]
struct OutboundPriorityQueue {
    blocks: std::collections::VecDeque<OutboundMessage>,
    priority_txs: std::collections::VecDeque<OutboundMessage>,
    standard_txs: std::collections::VecDeque<OutboundMessage>,
    consecutive_block_picks: usize,
    consecutive_priority_tx_picks: usize,
    tx_recovery_credit: usize,
}

fn track_queue_depth_on_enqueue(state: &mut InnerState) {
    state.queue_max_depth = state.queue_max_depth.max(state.queued_messages);
}

fn queue_backpressure_reject(state: &mut InnerState, reason: &str) -> bool {
    if state.queued_messages >= OUTBOUND_QUEUE_SOFT_CAP {
        state.queue_backpressure_drops = state.queue_backpressure_drops.saturating_add(1);
        state.last_drop_reason = Some(reason.to_string());
        return true;
    }
    false
}

fn enqueue_outbound_message(
    _inner: &Arc<Mutex<InnerState>>,
    queue: &mut OutboundPriorityQueue,
    msg: OutboundMessage,
) {
    match msg {
        OutboundMessage::Block(block) => {
            queue.blocks.push_back(OutboundMessage::Block(block));
        }
        OutboundMessage::InvBlock(hashes) => {
            queue.blocks.push_back(OutboundMessage::InvBlock(hashes));
        }
        OutboundMessage::GetHeaders {
            locator,
            stop_hash,
            limit,
        } => {
            queue.blocks.push_back(OutboundMessage::GetHeaders {
                locator,
                stop_hash,
                limit,
            });
        }
        OutboundMessage::Headers(headers) => {
            queue.blocks.push_back(OutboundMessage::Headers(headers));
        }
        OutboundMessage::GetBlockHeaders(hashes) => {
            queue
                .blocks
                .push_back(OutboundMessage::GetBlockHeaders(hashes));
        }
        OutboundMessage::BlockHeaders(headers) => {
            queue
                .blocks
                .push_back(OutboundMessage::BlockHeaders(headers));
        }
        OutboundMessage::GetBlock(hash) => {
            queue.blocks.push_back(OutboundMessage::GetBlock(hash));
        }
        OutboundMessage::BlockData {
            block,
            request_hash,
        } => {
            queue.blocks.push_back(OutboundMessage::BlockData {
                block,
                request_hash,
            });
        }
        OutboundMessage::GetTips => {
            queue.standard_txs.push_back(OutboundMessage::GetTips);
        }
        OutboundMessage::Tips(tips) => {
            queue.standard_txs.push_back(OutboundMessage::Tips(tips));
        }
        OutboundMessage::Transaction(tx) => {
            if tx.fee >= TX_PRIORITY_FEE_THRESHOLD {
                queue
                    .priority_txs
                    .push_back(OutboundMessage::Transaction(tx));
            } else {
                queue
                    .standard_txs
                    .push_back(OutboundMessage::Transaction(tx));
            }
        }
    }
}

fn pop_outbound_message(
    inner: &Arc<Mutex<InnerState>>,
    queue: &mut OutboundPriorityQueue,
) -> Option<OutboundMessage> {
    let blocks_waiting = !queue.blocks.is_empty();
    let priority_waiting = !queue.priority_txs.is_empty();
    let standard_waiting = !queue.standard_txs.is_empty();
    if !blocks_waiting && !priority_waiting && !standard_waiting {
        return None;
    }
    let tx_waiting = priority_waiting || standard_waiting;
    let take_tx_for_fairness =
        blocks_waiting && tx_waiting && queue.consecutive_block_picks >= BLOCK_PRIORITY_BURST_LIMIT;
    let pick_priority_lane = priority_waiting
        && (!standard_waiting || queue.consecutive_priority_tx_picks < PRIORITY_TX_BURST_LIMIT);
    if blocks_waiting && tx_waiting && queue.consecutive_block_picks >= BLOCK_PRIORITY_BURST_LIMIT {
        queue.tx_recovery_credit = queue.tx_recovery_credit.saturating_add(2);
    }
    let force_tx_recovery = tx_waiting && queue.tx_recovery_credit > 0;
    let picked = if take_tx_for_fairness || force_tx_recovery {
        queue.consecutive_block_picks = 0;
        if pick_priority_lane {
            queue.consecutive_priority_tx_picks =
                queue.consecutive_priority_tx_picks.saturating_add(1);
            queue.priority_txs.pop_front()
        } else {
            queue.consecutive_priority_tx_picks = 0;
            queue.standard_txs.pop_front()
        }
    } else if blocks_waiting {
        queue.consecutive_block_picks = queue.consecutive_block_picks.saturating_add(1);
        queue.consecutive_priority_tx_picks = 0;
        queue.blocks.pop_front()
    } else if pick_priority_lane {
        queue.consecutive_block_picks = 0;
        queue.consecutive_priority_tx_picks = queue.consecutive_priority_tx_picks.saturating_add(1);
        queue.priority_txs.pop_front()
    } else {
        queue.consecutive_block_picks = 0;
        queue.consecutive_priority_tx_picks = 0;
        queue.standard_txs.pop_front()
    };
    if let Some(OutboundMessage::Transaction(_)) = picked.as_ref() {
        queue.tx_recovery_credit = queue.tx_recovery_credit.saturating_sub(1);
    }
    if let (Some(msg), Ok(mut guard)) = (picked.as_ref(), inner.lock()) {
        guard.queued_messages = guard.queued_messages.saturating_sub(1);
        match msg {
            OutboundMessage::Block(_)
            | OutboundMessage::InvBlock(_)
            | OutboundMessage::GetHeaders { .. }
            | OutboundMessage::Headers(_)
            | OutboundMessage::GetBlockHeaders(_)
            | OutboundMessage::BlockHeaders(_)
            | OutboundMessage::GetBlock(_)
            | OutboundMessage::BlockData { .. } => {
                guard.queued_block_messages = guard.queued_block_messages.saturating_sub(1);
                guard.dequeued_block_messages = guard.dequeued_block_messages.saturating_add(1);
                guard.queue_block_priority_picks =
                    guard.queue_block_priority_picks.saturating_add(1);
            }
            OutboundMessage::Transaction(_)
            | OutboundMessage::GetTips
            | OutboundMessage::Tips(_) => {
                guard.queued_non_block_messages = guard.queued_non_block_messages.saturating_sub(1);
                guard.dequeued_non_block_messages =
                    guard.dequeued_non_block_messages.saturating_add(1);
                guard.queue_non_block_fair_picks =
                    guard.queue_non_block_fair_picks.saturating_add(1);
                if blocks_waiting || priority_waiting {
                    guard.queue_starvation_relief_picks =
                        guard.queue_starvation_relief_picks.saturating_add(1);
                }
                if pick_priority_lane {
                    guard.queue_priority_tx_lane_picks =
                        guard.queue_priority_tx_lane_picks.saturating_add(1);
                } else {
                    guard.queue_standard_tx_lane_picks =
                        guard.queue_standard_tx_lane_picks.saturating_add(1);
                }
            }
        }
    }
    picked
}

fn drain_outbound_rx_to_priority_queue(
    inner: &Arc<Mutex<InnerState>>,
    outbound_rx: &mut mpsc::UnboundedReceiver<OutboundMessage>,
    queue: &mut OutboundPriorityQueue,
) {
    while let Ok(msg) = outbound_rx.try_recv() {
        enqueue_outbound_message(inner, queue, msg);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PeerBookSnapshot {
    #[serde(default)]
    persisted_at_unix: u64,
    peer_book: HashMap<String, PeerHealth>,
}

const PEER_RECORD_MAX_AGE_SECS: u64 = 60 * 60 * 24 * 30;
const RECENT_FAILURES_KEEP: usize = 8;

fn peer_state_path() -> Option<PathBuf> {
    std::env::var("PULSEDAG_P2P_PEER_STATE_PATH")
        .ok()
        .map(PathBuf::from)
}

fn load_peer_book(path: &PathBuf) -> HashMap<String, PeerHealth> {
    let now = now_unix();
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PeerBookSnapshot>(&bytes).ok())
        .map(|snapshot| sanitize_loaded_peer_book(snapshot.peer_book, now))
        .unwrap_or_default()
}

fn persist_peer_book(path: &PathBuf, peer_book: &HashMap<String, PeerHealth>) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let snapshot = PeerBookSnapshot {
        persisted_at_unix: now_unix(),
        peer_book: peer_book.clone(),
    };
    if let Ok(bytes) = serde_json::to_vec(&snapshot) {
        let _ = fs::write(path, bytes);
    }
}

fn sanitize_loaded_peer_book(
    peer_book: HashMap<String, PeerHealth>,
    now: u64,
) -> HashMap<String, PeerHealth> {
    let stale_before = now.saturating_sub(PEER_RECORD_MAX_AGE_SECS);
    peer_book
        .into_iter()
        .filter_map(|(peer, mut health)| {
            let last_activity = health
                .last_seen_unix
                .or(health.last_successful_connect_unix)
                .or(health.last_failure_unix)
                .or(health.last_recovery_unix)
                .unwrap_or(0);
            if last_activity > 0 && last_activity < stale_before {
                return None;
            }
            health.last_seen_unix = health.last_seen_unix.filter(|v| *v <= now);
            health.last_successful_connect_unix =
                health.last_successful_connect_unix.filter(|v| *v <= now);
            health.last_recovery_unix = health.last_recovery_unix.filter(|v| *v <= now);
            health.last_failure_unix = health.last_failure_unix.filter(|v| *v <= now);
            health
                .recent_failures_unix
                .retain(|ts| *ts <= now && *ts >= stale_before);
            if health.recent_failures_unix.len() > RECENT_FAILURES_KEEP {
                let keep_from = health.recent_failures_unix.len() - RECENT_FAILURES_KEEP;
                health.recent_failures_unix = health.recent_failures_unix.split_off(keep_from);
            }
            if health.next_retry_unix > now.saturating_add(PEER_RECORD_MAX_AGE_SECS) {
                health.next_retry_unix = now;
            }
            if health.suppressed_until_unix > now.saturating_add(PEER_RECORD_MAX_AGE_SECS) {
                health.suppressed_until_unix = 0;
            }
            health.connected = false;
            Some((peer, health))
        })
        .collect()
}

fn persist_peer_state_if_configured(state: &InnerState) {
    if let Some(path) = state.peer_state_path.as_ref() {
        persist_peer_book(path, &state.peer_book);
    }
}

fn register_peer_result(inner: &Arc<Mutex<InnerState>>, peer: &str, success: bool) {
    register_peer_result_at(inner, peer, success, now_unix());
}

fn register_peer_result_at(inner: &Arc<Mutex<InnerState>>, peer: &str, success: bool, now: u64) {
    if let Ok(mut guard) = inner.lock() {
        guard.peer_reconnect_attempts = guard.peer_reconnect_attempts.saturating_add(1);
        let mut counted_cooldown_suppression = false;
        let mut counted_flap_suppression = false;
        let mut trigger_rebroadcast_window = false;
        let mut suppressed_dial = false;
        {
            let local_chain_id = guard.chain_id.clone();
            let mode = guard.mode.clone();
            let health = guard.peer_book.entry(peer.to_string()).or_default();
            if now.saturating_sub(health.dial_window_started_unix) >= PEER_DIAL_ATTEMPT_WINDOW_SECS
            {
                health.dial_window_started_unix = now;
                health.dial_attempts_in_window = 0;
            }
            health.dial_attempts_in_window = health.dial_attempts_in_window.saturating_add(1);
            if !success && health.dial_attempts_in_window > PEER_MAX_DIAL_ATTEMPTS_PER_WINDOW {
                health.cooldown_suppressed_count =
                    health.cooldown_suppressed_count.saturating_add(1);
                health.next_retry_unix = health
                    .next_retry_unix
                    .max(now.saturating_add(BACKOFF_MAX_SECS));
                health.suppressed_until_unix =
                    health.suppressed_until_unix.max(health.next_retry_unix);
                counted_cooldown_suppression = true;
                suppressed_dial = true;
            }
            if suppressed_dial {
                health.last_seen_unix = Some(now);
            } else {
                health.reconnect_attempts = health.reconnect_attempts.saturating_add(1);
                health.last_seen_unix = Some(now);
                if success {
                    let remote_chain_compatible = health
                        .remote_chain_id
                        .as_deref()
                        .is_none_or(|remote| remote == local_chain_id.as_str());
                    health.connected = true;
                    health.fail_streak = 0;
                    health.next_retry_unix = now;
                    trigger_rebroadcast_window = true;
                    let success_bonus = if health.score < 0 {
                        PEER_SUCCESS_BASE_BONUS + 4
                    } else {
                        PEER_SUCCESS_BASE_BONUS
                    };
                    health.score =
                        (health.score + success_bonus).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                    health.recovery_success_count = health.recovery_success_count.saturating_add(1);
                    health.last_recovery_unix = Some(now);
                    health.last_successful_connect_unix = Some(now);
                    if mode_connected_peers_are_real_network(&mode) {
                        health.chain_id_compatible = remote_chain_compatible;
                    }
                    if health
                        .last_failure_unix
                        .map(|last_fail| now.saturating_sub(last_fail) <= FLAP_WINDOW.as_secs())
                        .unwrap_or(false)
                    {
                        health.flap_events = health.flap_events.saturating_add(1);
                    } else {
                        health.flap_events = 0;
                    }
                    health.suppressed_until_unix = 0;
                } else {
                    let previous_next_retry_unix = health.next_retry_unix;
                    if health.next_retry_unix > now {
                        health.cooldown_suppressed_count =
                            health.cooldown_suppressed_count.saturating_add(1);
                        counted_cooldown_suppression = true;
                    }
                    health.connected = false;
                    health.fail_streak = health.fail_streak.saturating_add(1);
                    let adaptive_penalty = PEER_FAILURE_BASE_PENALTY
                        + (health.fail_streak as i32 * PEER_FAILURE_STREAK_PENALTY)
                        + (health.recent_failures_unix.len() as i32 * PEER_FAILURE_RECENT_PENALTY);
                    health.score =
                        (health.score - adaptive_penalty).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                    let exp = health.fail_streak.min(BACKOFF_EXP_CAP);
                    let base_backoff = BACKOFF_BASE_SECS.saturating_pow(exp);
                    let bounded_backoff = base_backoff.min(BACKOFF_MAX_SECS);
                    let mut next_retry_unix =
                        now.saturating_add(bounded_backoff + peer_jitter(peer));
                    if health
                        .last_recovery_unix
                        .map(|last_ok| now.saturating_sub(last_ok) <= FLAP_WINDOW.as_secs())
                        .unwrap_or(false)
                    {
                        health.flap_events = health.flap_events.saturating_add(1);
                    } else {
                        health.flap_events = 0;
                    }
                    if health.flap_events >= 2 {
                        let flap_cooldown =
                            FLAP_BASE_COOLDOWN.saturating_mul(health.flap_events as u64);
                        health.suppressed_until_unix = now.saturating_add(flap_cooldown);
                        next_retry_unix = next_retry_unix.max(health.suppressed_until_unix);
                        health.flap_suppressed_count =
                            health.flap_suppressed_count.saturating_add(1);
                        counted_flap_suppression = true;
                    }
                    health.last_failure_unix = Some(now);
                    health.recent_failures_unix.push(now);
                    if health.recent_failures_unix.len() > RECENT_FAILURES_KEEP {
                        let keep_from = health.recent_failures_unix.len() - RECENT_FAILURES_KEEP;
                        health.recent_failures_unix =
                            health.recent_failures_unix.split_off(keep_from);
                    }
                    health.next_retry_unix = next_retry_unix.max(previous_next_retry_unix);
                }
            }
        }
        if suppressed_dial {
            guard.peer_suppressed_dial_count = guard.peer_suppressed_dial_count.saturating_add(1);
        }
        if trigger_rebroadcast_window {
            guard.recovery_rebroadcast_generation =
                guard.recovery_rebroadcast_generation.saturating_add(1);
            guard.recovery_rebroadcast_until_unix =
                now.saturating_add(RECOVERY_REBROADCAST_GRACE_SECS);
        }
        if success {
            guard.peer_recovery_success_count = guard.peer_recovery_success_count.saturating_add(1);
            guard.last_peer_recovery_unix = Some(now);
        }
        if counted_cooldown_suppression {
            guard.peer_cooldown_suppressed_count =
                guard.peer_cooldown_suppressed_count.saturating_add(1);
        }
        if counted_flap_suppression {
            guard.peer_flap_suppressed_count = guard.peer_flap_suppressed_count.saturating_add(1);
        }

        refresh_connected_peers_from_health(&mut guard);
        persist_peer_state_if_configured(&guard);
    }
}

fn record_publish(
    inner: &Arc<Mutex<InnerState>>,
    topic: &str,
    message_kind: &str,
    message_id: &str,
) {
    if let Ok(mut guard) = inner.lock() {
        guard.publish_attempts += 1;
        guard.broadcasted_messages += 1;
        guard.last_message_kind = Some(message_kind.to_string());
        guard.seen_message_ids.insert(message_id.to_string());
        *guard
            .per_topic_publishes
            .entry(topic.to_string())
            .or_insert(0) += 1;
    }
}

fn trim_old_entries(seen: &mut HashMap<String, u64>, now: u64, window_secs: u64) {
    let keep_after = now.saturating_sub(window_secs);
    seen.retain(|_, seen_at| *seen_at >= keep_after);
    if seen.len() > MAX_DEDUP_TRACKED_IDS {
        let mut entries = seen
            .iter()
            .map(|(id, seen_at)| (id.clone(), *seen_at))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, seen_at)| *seen_at);
        let remove_count = seen.len().saturating_sub(MAX_DEDUP_TRACKED_IDS);
        for (id, _) in entries.into_iter().take(remove_count) {
            seen.remove(&id);
        }
    }
}

fn trim_inbound_seen_cache(state: &mut InnerState, now: u64, ttl_secs: u64) {
    state
        .inbound_seen_cache
        .retain(|_, entry| now.saturating_sub(entry.last_seen_unix) <= ttl_secs);
    if state.inbound_seen_cache.len() > MAX_DEDUP_TRACKED_IDS {
        let mut entries = state
            .inbound_seen_cache
            .iter()
            .map(|(id, entry)| (id.clone(), entry.last_seen_unix))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, last_seen)| *last_seen);
        let remove_count = state
            .inbound_seen_cache
            .len()
            .saturating_sub(MAX_DEDUP_TRACKED_IDS);
        for (id, _) in entries.into_iter().take(remove_count) {
            state.inbound_seen_cache.remove(&id);
        }
    }
}

fn mark_inbound_seen_with_metadata(
    state: &mut InnerState,
    id: String,
    block_hash: Option<String>,
    txid: Option<String>,
    peer_source: Option<&str>,
    ttl_secs: u64,
    now: u64,
) -> bool {
    trim_old_entries(&mut state.inbound_seen_at_unix, now, ttl_secs);
    trim_inbound_seen_cache(state, now, ttl_secs);
    match state.inbound_seen_cache.get_mut(&id) {
        Some(entry) if now.saturating_sub(entry.last_seen_unix) <= ttl_secs => {
            entry.last_seen_unix = now;
            if entry.peer_source.is_none() {
                entry.peer_source = peer_source.map(str::to_string);
            }
            state.inbound_seen_at_unix.insert(id, now);
            false
        }
        _ => {
            state.inbound_seen_at_unix.insert(id.clone(), now);
            state.seen_message_ids.insert(id.clone());
            if let Some(hash) = block_hash.as_ref() {
                state.known_block_hashes.insert(hash.clone());
            }
            if let Some(tx) = txid.as_ref() {
                state.known_txids.insert(tx.clone());
            }
            state.inbound_seen_cache.insert(
                id.clone(),
                SeenCacheEntry {
                    message_id: id,
                    block_hash,
                    txid,
                    first_seen_unix: now,
                    last_seen_unix: now,
                    peer_source: peer_source.map(str::to_string),
                },
            );
            true
        }
    }
}

fn mark_inbound_id_seen(state: &mut InnerState, id: String, now: u64) -> bool {
    mark_inbound_seen_with_metadata(state, id, None, None, None, MESSAGE_DEDUP_WINDOW_SECS, now)
}

fn mark_inbound_block_seen(
    state: &mut InnerState,
    id: String,
    block_hash: String,
    peer_source: Option<&str>,
    now: u64,
) -> bool {
    mark_inbound_seen_with_metadata(
        state,
        id,
        Some(block_hash),
        None,
        peer_source,
        MESSAGE_DEDUP_WINDOW_SECS,
        now,
    )
}

fn mark_inbound_tx_seen(
    state: &mut InnerState,
    id: String,
    txid: String,
    peer_source: Option<&str>,
    now: u64,
) -> bool {
    mark_inbound_seen_with_metadata(
        state,
        id,
        None,
        Some(txid),
        peer_source,
        TX_INBOUND_DEDUP_WINDOW_SECS,
        now,
    )
}

fn admit_inbound_tx_rate(state: &mut InnerState, now: u64) -> bool {
    if now.saturating_sub(state.tx_inbound_rate_window_started_unix) >= TX_INBOUND_RATE_WINDOW_SECS
    {
        state.tx_inbound_rate_window_started_unix = now;
        state.tx_inbound_rate_window_count = 0;
    }
    if state.tx_inbound_rate_window_count < TX_INBOUND_SOFT_MAX_PER_WINDOW {
        state.tx_inbound_rate_window_count = state.tx_inbound_rate_window_count.saturating_add(1);
        return true;
    }
    false
}

fn should_relay_outbound_tx(state: &mut InnerState, id: &str, now: u64) -> bool {
    trim_old_entries(
        &mut state.outbound_tx_seen_at_unix,
        now,
        TX_OUTBOUND_DEDUP_WINDOW_SECS,
    );
    match state.outbound_tx_seen_at_unix.get(id) {
        Some(last_seen) if now.saturating_sub(*last_seen) <= TX_OUTBOUND_DEDUP_WINDOW_SECS => {
            let mut recovery_relays =
                std::mem::take(&mut state.outbound_tx_recovery_relay_generation);
            let allow = should_allow_recovery_rebroadcast(
                state,
                &mut recovery_relays,
                state.recovery_rebroadcast_generation,
                state.recovery_rebroadcast_until_unix,
                id,
                now,
            );
            state.outbound_tx_recovery_relay_generation = recovery_relays;
            if allow {
                return true;
            }
            state.tx_outbound_duplicates_suppressed =
                state.tx_outbound_duplicates_suppressed.saturating_add(1);
            state.outbound_duplicates_suppressed =
                state.outbound_duplicates_suppressed.saturating_add(1);
            state.relay_loop_prevented = state.relay_loop_prevented.saturating_add(1);
            false
        }
        _ => true,
    }
}

fn admit_tx_relay_under_budget(state: &mut InnerState, tx_id: &str, fee: u64, now: u64) -> bool {
    if fee >= TX_PRIORITY_FEE_THRESHOLD {
        state.tx_outbound_priority_relayed = state.tx_outbound_priority_relayed.saturating_add(1);
        return true;
    }
    let under_load = state.queued_messages >= TX_BUDGET_LOAD_SHED_QUEUE_DEPTH_THRESHOLD;
    if !under_load {
        return true;
    }
    if now.saturating_sub(state.tx_budget_window_started_unix) >= TX_RELAY_BUDGET_WINDOW_SECS {
        state.tx_budget_window_started_unix = now;
        state.tx_budget_window_relays = 0;
    }
    if state.tx_budget_window_relays < TX_RELAY_BUDGET_PER_WINDOW {
        state.tx_budget_window_relays = state.tx_budget_window_relays.saturating_add(1);
        return true;
    }
    if message_id_hash(tx_id).is_multiple_of(TX_RELAY_BUDGET_OVERFLOW_SAMPLE_EVERY) {
        return true;
    }
    state.tx_outbound_budget_suppressed = state.tx_outbound_budget_suppressed.saturating_add(1);
    false
}

fn record_outbound_tx_relay(state: &mut InnerState, id: &str, now: u64) {
    let within_window = state
        .outbound_tx_seen_at_unix
        .get(id)
        .is_some_and(|last_seen| now.saturating_sub(*last_seen) <= TX_OUTBOUND_DEDUP_WINDOW_SECS);
    if within_window {
        if now <= state.recovery_rebroadcast_until_unix
            && state.recovery_rebroadcast_generation > 0
            && matches!(
                state.outbound_tx_recovery_relay_generation.get(id),
                Some(generation) if *generation == state.recovery_rebroadcast_generation
            )
        {
            state.tx_outbound_recovery_relayed =
                state.tx_outbound_recovery_relayed.saturating_add(1);
        }
    } else {
        state.tx_outbound_first_seen_relayed =
            state.tx_outbound_first_seen_relayed.saturating_add(1);
    }
    state.outbound_tx_seen_at_unix.insert(id.to_string(), now);
}

fn should_relay_outbound_block(state: &mut InnerState, id: &str, now: u64) -> bool {
    trim_old_entries(
        &mut state.outbound_block_seen_at_unix,
        now,
        BLOCK_OUTBOUND_DEDUP_WINDOW_SECS,
    );
    match state.outbound_block_seen_at_unix.get(id) {
        Some(last_seen) if now.saturating_sub(*last_seen) <= BLOCK_OUTBOUND_DEDUP_WINDOW_SECS => {
            let mut recovery_relays =
                std::mem::take(&mut state.outbound_block_recovery_relay_generation);
            if should_allow_recovery_rebroadcast(
                state,
                &mut recovery_relays,
                state.recovery_rebroadcast_generation,
                state.recovery_rebroadcast_until_unix,
                id,
                now,
            ) {
                state
                    .outbound_block_seen_at_unix
                    .insert(id.to_string(), now);
                state.block_outbound_recovery_relayed =
                    state.block_outbound_recovery_relayed.saturating_add(1);
                state.outbound_block_recovery_relay_generation = recovery_relays;
                return true;
            }
            state.outbound_block_recovery_relay_generation = recovery_relays;
            state.block_outbound_duplicates_suppressed =
                state.block_outbound_duplicates_suppressed.saturating_add(1);
            state.outbound_duplicates_suppressed =
                state.outbound_duplicates_suppressed.saturating_add(1);
            state.relay_loop_prevented = state.relay_loop_prevented.saturating_add(1);
            false
        }
        _ => {
            state
                .outbound_block_seen_at_unix
                .insert(id.to_string(), now);
            state.block_outbound_first_seen_relayed =
                state.block_outbound_first_seen_relayed.saturating_add(1);
            true
        }
    }
}

fn should_allow_recovery_rebroadcast(
    state: &mut InnerState,
    recovery_relays: &mut HashMap<String, u64>,
    current_generation: u64,
    recovery_until_unix: u64,
    id: &str,
    now: u64,
) -> bool {
    if current_generation == 0 || now > recovery_until_unix {
        return false;
    }
    if now.saturating_sub(state.recovery_rebroadcast_budget_window_started_unix)
        >= RECOVERY_REBROADCAST_BUDGET_WINDOW_SECS
    {
        state.recovery_rebroadcast_budget_window_started_unix = now;
        state.recovery_rebroadcast_budget_used = 0;
    }
    if state.recovery_rebroadcast_budget_used >= RECOVERY_REBROADCAST_BUDGET_PER_WINDOW {
        state.tx_outbound_recovery_budget_suppressed = state
            .tx_outbound_recovery_budget_suppressed
            .saturating_add(1);
        return false;
    }
    if recovery_relays.len() > MAX_DEDUP_TRACKED_IDS {
        recovery_relays.retain(|_, generation| *generation == current_generation);
    }
    match recovery_relays.get(id) {
        Some(previous_generation) if *previous_generation == current_generation => false,
        _ => {
            recovery_relays.insert(id.to_string(), current_generation);
            state.recovery_rebroadcast_budget_used =
                state.recovery_rebroadcast_budget_used.saturating_add(1);
            true
        }
    }
}

fn message_id_hash(id: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

fn note_swarm_event(inner: &Arc<Mutex<InnerState>>, label: impl Into<String>) {
    if let Ok(mut guard) = inner.lock() {
        guard.swarm_events_seen += 1;
        guard.last_swarm_event = Some(label.into());
    }
}

#[derive(Debug, Clone, Copy)]
enum PeerMessageOutcome {
    ValidRelay,
    Malformed,
    ChainMismatch,
    RateLimited,
    InvalidBlock,
    InvalidAnnouncement,
}

fn score_peer_message_outcome(
    state: &mut InnerState,
    peer: &str,
    outcome: PeerMessageOutcome,
    now: u64,
) {
    let local_chain_id = state.chain_id.clone();
    let outcome_reason = match outcome {
        PeerMessageOutcome::ValidRelay => None,
        PeerMessageOutcome::Malformed => Some("malformed_message"),
        PeerMessageOutcome::ChainMismatch => Some("chain_mismatch"),
        PeerMessageOutcome::RateLimited => Some("rate_limited"),
        PeerMessageOutcome::InvalidBlock => Some("invalid_block"),
        PeerMessageOutcome::InvalidAnnouncement => Some("invalid_announcement"),
    };
    let mut entered_recovery = false;
    {
        let health = state.peer_book.entry(peer.to_string()).or_default();
        health.last_seen_unix = Some(now);
        if let Some(reason) = outcome_reason {
            health.last_error = Some(reason.to_string());
            health.last_error_unix = Some(now);
            health.last_error_source = Some("peer_message_outcome".to_string());
        }
        match outcome {
            PeerMessageOutcome::ValidRelay => {
                health.remote_chain_id = Some(local_chain_id);
                health.chain_id_compatible = true;
                health.chain_mismatch_streak = 0;
                health.score =
                    (health.score + PEER_VALID_RELAY_BONUS).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                if health.fail_streak > 0 && health.score >= 80 {
                    health.fail_streak = health.fail_streak.saturating_sub(1);
                }
            }
            PeerMessageOutcome::Malformed => {
                health.chain_mismatch_streak = 0;
                health.score = (health.score - PEER_MALFORMED_MESSAGE_PENALTY)
                    .clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.fail_streak = health.fail_streak.saturating_add(1);
                health.last_failure_unix = Some(now);
            }
            PeerMessageOutcome::ChainMismatch => {
                health.remote_chain_id = None;
                health.chain_id_compatible = false;
                health.connected = false;
                health.chain_mismatch_streak = health.chain_mismatch_streak.saturating_add(1);
                let penalty = PEER_CHAIN_MISMATCH_PENALTY
                    + (health.chain_mismatch_streak.saturating_sub(1) as i32 * 4);
                health.score = (health.score - penalty).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.fail_streak = health.fail_streak.saturating_add(1);
                health.last_failure_unix = Some(now);
                if health.chain_mismatch_streak >= 3 {
                    let cooldown =
                        FLAP_BASE_COOLDOWN.saturating_mul(health.chain_mismatch_streak as u64);
                    health.suppressed_until_unix = health
                        .suppressed_until_unix
                        .max(now.saturating_add(cooldown));
                    health.next_retry_unix =
                        health.next_retry_unix.max(health.suppressed_until_unix);
                }
            }
            PeerMessageOutcome::RateLimited => {
                health.last_rate_limited_unix = Some(now);
                health.score =
                    (health.score - PEER_RATE_LIMIT_PENALTY).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.fail_streak = health.fail_streak.saturating_add(1);
                health.last_failure_unix = Some(now);
            }
            PeerMessageOutcome::InvalidBlock => {
                health.chain_mismatch_streak = 0;
                health.score = (health.score - PEER_INVALID_BLOCK_PENALTY)
                    .clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.fail_streak = health.fail_streak.saturating_add(1);
                health.invalid_block_announce_streak =
                    health.invalid_block_announce_streak.saturating_add(1);
                health.last_failure_unix = Some(now);
                if health.invalid_block_announce_streak
                    >= PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_THRESHOLD
                {
                    health.suppressed_until_unix = health
                        .suppressed_until_unix
                        .max(now.saturating_add(PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_SECS));
                    health.next_retry_unix =
                        health.next_retry_unix.max(health.suppressed_until_unix);
                }
            }
            PeerMessageOutcome::InvalidAnnouncement => {
                health.score = (health.score - PEER_INVALID_ANNOUNCEMENT_PENALTY)
                    .clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.fail_streak = health.fail_streak.saturating_add(1);
                health.invalid_block_announce_streak =
                    health.invalid_block_announce_streak.saturating_add(1);
                health.last_failure_unix = Some(now);
                if health.invalid_block_announce_streak
                    >= PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_THRESHOLD
                {
                    health.suppressed_until_unix = health
                        .suppressed_until_unix
                        .max(now.saturating_add(PEER_INVALID_BLOCK_ANNOUNCE_COOLDOWN_SECS));
                    health.next_retry_unix =
                        health.next_retry_unix.max(health.suppressed_until_unix);
                }
            }
        }
        if outcome_reason.is_some() {
            health.recent_failures_unix.push(now);
            if health.recent_failures_unix.len() > RECENT_FAILURES_KEEP {
                let keep_from = health.recent_failures_unix.len() - RECENT_FAILURES_KEEP;
                health.recent_failures_unix = health.recent_failures_unix.split_off(keep_from);
            }
            if health.score < 40 || health.fail_streak >= 5 {
                health.connected = false;
                health.next_retry_unix = health
                    .next_retry_unix
                    .max(now.saturating_add(BACKOFF_MAX_SECS / 2));
            }
            entered_recovery = !health.connected
                || health.next_retry_unix > now
                || health.suppressed_until_unix > now;
        }
    }
    if let Some(reason) = outcome_reason {
        state
            .last_error_by_peer
            .insert(peer.to_string(), reason.to_string());
        state.peer_penalties = state.peer_penalties.saturating_add(1);
        if entered_recovery {
            let disconnect_reason = format!("peer_health:{reason}");
            *state
                .disconnect_reason_counts
                .entry(disconnect_reason)
                .or_insert(0) += 1;
            record_peer_lifecycle_event(state, "peer_health_recovery");
        }
    }
}

fn private_rehearsal_min_useful_peer_target(state: &InnerState) -> usize {
    let configured_bootnodes = state
        .bootnodes_configured
        .iter()
        .filter(|addr| parse_bootnode_multiaddr(addr).is_some())
        .count();
    let known_peers = state.peer_book.len().max(configured_bootnodes);
    if known_peers >= 4 {
        4
    } else if known_peers >= 2 || !state.bootnodes_configured.is_empty() {
        2.min(known_peers.max(1))
    } else {
        1
    }
}

fn useful_peer_count(state: &InnerState) -> usize {
    if !state.connected_peers.is_empty() {
        return state.connected_peers.len();
    }
    state
        .active_connections
        .iter()
        .filter(|(peer_id, connections)| {
            **connections > 0
                && state
                    .peer_book
                    .get(*peer_id)
                    .map(|health| health.chain_id_compatible)
                    .unwrap_or(true)
        })
        .count()
}

fn peer_recovery_state(state: &InnerState) -> String {
    let useful = useful_peer_count(state);
    let target = private_rehearsal_min_useful_peer_target(state);
    if useful == 0 && target > 0 {
        "isolated".to_string()
    } else if useful < target {
        "below_target".to_string()
    } else {
        "healthy".to_string()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PeerTargetAccounting {
    connected: usize,
    disconnected: usize,
    cooldown: usize,
    rate_limited: usize,
    dialable: usize,
}

fn peer_target_accounting(state: &InnerState, now: u64) -> PeerTargetAccounting {
    let mut accounting = PeerTargetAccounting::default();
    for (peer, health) in &state.peer_book {
        let active =
            health.connected || state.active_connections.get(peer).copied().unwrap_or(0) > 0;
        if active {
            accounting.connected = accounting.connected.saturating_add(1);
            continue;
        }
        accounting.disconnected = accounting.disconnected.saturating_add(1);
        let cooling_down = health.suppressed_until_unix > now || health.next_retry_unix > now;
        if cooling_down {
            accounting.cooldown = accounting.cooldown.saturating_add(1);
        }
        let rate_limited = peer_rate_limited_recently(health, now);
        if rate_limited {
            accounting.rate_limited = accounting.rate_limited.saturating_add(1);
        }
        if !cooling_down && !rate_limited && health.chain_id_compatible {
            accounting.dialable = accounting.dialable.saturating_add(1);
        }
    }
    accounting
}

fn peer_below_target_accounting_reason(accounting: PeerTargetAccounting) -> String {
    format!(
        "connected={} disconnected={} cooldown={} rate_limited={} dialable={}",
        accounting.connected,
        accounting.disconnected,
        accounting.cooldown,
        accounting.rate_limited,
        accounting.dialable
    )
}

fn enforce_connectivity_aware_cooldown_floor(state: &mut InnerState, now: u64) {
    let target = private_rehearsal_min_useful_peer_target(state);
    let active_count = useful_peer_count(state);
    if active_count == 0 && (!state.bootnodes_configured.is_empty() || !state.peer_book.is_empty())
    {
        state.peer_zero_since_unix.get_or_insert(now);
        if !state.bootnodes_configured.is_empty() {
            for bootnode in &state.bootnodes_configured {
                if let Some((peer_id, _)) = parse_bootnode_multiaddr(bootnode) {
                    state
                        .bootnode_next_redial_at
                        .insert(peer_id.to_string(), now);
                    state
                        .bootnode_redial_backoff_secs
                        .entry(peer_id.to_string())
                        .or_insert(1);
                }
            }
        }
    } else if active_count > 0 {
        if state.peer_zero_since_unix.is_some() {
            state.peer_zero_reconnect_success_total =
                state.peer_zero_reconnect_success_total.saturating_add(1);
        }
        state.peer_zero_since_unix = None;
        state.last_peer_reconnect_blocked_reason = None;
    }
    if state.peer_book.is_empty() || active_count >= target {
        if active_count >= target && target > 0 {
            state.peer_min_target_recovered_total =
                state.peer_min_target_recovered_total.saturating_add(1);
            if state.peer_below_target_since_unix.take().is_some() {
                state.peer_min_target_reconnect_success_total = state
                    .peer_min_target_reconnect_success_total
                    .saturating_add(1);
            }
            state.peer_below_target_blocked_reason = None;
        }
        return;
    }
    state.peer_below_target_since_unix.get_or_insert(now);
    state.peer_min_target_missed_total = state.peer_min_target_missed_total.saturating_add(1);
    let needed = target.saturating_sub(active_count);
    let mut scheduled = 0usize;
    for bootnode in &state.bootnodes_configured {
        if scheduled >= needed {
            break;
        }
        if let Some((peer_id, _)) = parse_bootnode_multiaddr(bootnode) {
            let peer = peer_id.to_string();
            if state.active_connections.get(&peer).copied().unwrap_or(0) > 0
                || state.pending_bootnode_dials.contains(&peer)
            {
                continue;
            }
            state.bootnode_next_redial_at.insert(peer.clone(), now);
            state.bootnode_redial_backoff_secs.entry(peer).or_insert(1);
            scheduled = scheduled.saturating_add(1);
        }
    }
    if scheduled > 0 {
        state.peer_min_target_reconnect_attempt_total = state
            .peer_min_target_reconnect_attempt_total
            .saturating_add(scheduled as u64);
    }
    let mut bypassed = 0usize;
    let mut rate_limited = 0usize;
    for health in state.peer_book.values_mut() {
        if bypassed >= needed {
            break;
        }
        if health.connected {
            continue;
        }
        if peer_rate_limited_recently(health, now) {
            rate_limited = rate_limited.saturating_add(1);
            health.last_rate_limited_unix = None;
        }
        let suppressed = health.suppressed_until_unix > now || health.next_retry_unix > now;
        if suppressed || active_count == 0 {
            health.suppressed_until_unix = now;
            health.next_retry_unix = now;
            bypassed = bypassed.saturating_add(1);
        }
    }
    if bypassed > 0 {
        state.peer_cooldown_bypassed_for_connectivity_total = state
            .peer_cooldown_bypassed_for_connectivity_total
            .saturating_add(bypassed as u64);
        state.peer_reconnect_suppressed_by_cooldown_total = state
            .peer_reconnect_suppressed_by_cooldown_total
            .saturating_add(bypassed as u64);
        state.last_peer_reconnect_blocked_reason = None;
        state.peer_below_target_blocked_reason = if scheduled > 0 {
            Some("bootnode_redial_scheduled".to_string())
        } else {
            None
        };
    } else if scheduled > 0 {
        state.peer_below_target_blocked_reason = Some("bootnode_redial_scheduled".to_string());
    } else if active_count == 0 {
        state.last_peer_reconnect_blocked_reason = Some("no_eligible_peer_after_floor".to_string());
        state.peer_below_target_blocked_reason = Some("no_eligible_peer_after_floor".to_string());
    } else {
        state.peer_below_target_blocked_reason = Some(format!(
            "no_disconnected_peer_available {}",
            peer_below_target_accounting_reason(peer_target_accounting(state, now))
        ));
    }
    if rate_limited > 0 {
        state.peer_reconnect_suppressed_by_rate_limit_total = state
            .peer_reconnect_suppressed_by_rate_limit_total
            .saturating_add(rate_limited as u64);
    }
}

fn admit_peer_inbound_message(state: &mut InnerState, peer: Option<&str>, now: u64) -> bool {
    let Some(peer) = peer else {
        return true;
    };
    let health = state.peer_book.entry(peer.to_string()).or_default();
    if now.saturating_sub(health.inbound_window_started_unix) >= PEER_INBOUND_RATE_WINDOW_SECS {
        health.inbound_window_started_unix = now;
        health.inbound_window_count = 0;
    }
    health.inbound_window_count = health.inbound_window_count.saturating_add(1);
    if health.inbound_window_count <= PEER_MAX_INBOUND_MESSAGES_PER_WINDOW {
        return true;
    }
    state.peer_message_rate_limited_count = state.peer_message_rate_limited_count.saturating_add(1);
    *state
        .peer_rate_limit_by_kind_total
        .entry("peer_inbound".to_string())
        .or_insert(0) += 1;
    if state.active_connections.len() <= private_rehearsal_min_useful_peer_target(state) {
        state.peer_rate_limit_recovery_suppressed_total = state
            .peer_rate_limit_recovery_suppressed_total
            .saturating_add(1);
    }
    score_peer_message_outcome(state, peer, PeerMessageOutcome::RateLimited, now);
    false
}

fn validate_inbound_block_shape(block: &Block) -> Result<(), &'static str> {
    if block.hash.is_empty() {
        return Err("empty_block_hash");
    }
    let mut parents = HashSet::new();
    for parent in &block.header.parents {
        if parent.is_empty() || !parents.insert(parent) {
            return Err("invalid_parent_set");
        }
    }
    let looks_canonical_hash =
        block.hash.len() == 64 && block.hash.chars().all(|ch| ch.is_ascii_hexdigit());
    if looks_canonical_hash && compute_block_hash(&block.header) != block.hash {
        return Err("block_hash_mismatch");
    }
    let looks_canonical_merkle = block.header.merkle_root.len() == 64
        && block
            .header
            .merkle_root
            .chars()
            .all(|ch| ch.is_ascii_hexdigit());
    if looks_canonical_merkle
        && compute_merkle_root(&block.transactions) != block.header.merkle_root
    {
        return Err("merkle_root_mismatch");
    }
    Ok(())
}

fn validate_block_announcement_hash(hash: &str) -> bool {
    !hash.trim().is_empty() && hash.len() <= 128
}

fn validate_header_announcement(header: &BlockHeaderAnnouncement) -> bool {
    validate_block_announcement_hash(&header.hash)
        && header.header.height > 0
        && !header.header.parents.is_empty()
        && {
            let mut parents = HashSet::new();
            header
                .header
                .parents
                .iter()
                .all(|parent| !parent.is_empty() && parents.insert(parent))
        }
}

fn sort_headers_parents_first(headers: &mut [BlockHeaderAnnouncement]) {
    let heights = headers
        .iter()
        .map(|header| (header.hash.clone(), header.header.height))
        .collect::<HashMap<_, _>>();
    headers.sort_by(|a, b| {
        let a_depends_on_b = a.header.parents.iter().any(|parent| parent == &b.hash);
        let b_depends_on_a = b.header.parents.iter().any(|parent| parent == &a.hash);
        match (a_depends_on_b, b_depends_on_a) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a
                .header
                .height
                .cmp(&b.header.height)
                .then_with(|| heights.get(&a.hash).cmp(&heights.get(&b.hash)))
                .then_with(|| a.hash.cmp(&b.hash)),
        }
    });
}

fn dispatch_network_message(
    expected_chain_id: &str,
    bytes: &[u8],
    source_peer: Option<&str>,
    inner: &Arc<Mutex<InnerState>>,
    inbound_tx: &mpsc::UnboundedSender<InboundEvent>,
) {
    let parsed = serde_json::from_slice::<NetworkMessage>(bytes);
    let msg = match parsed {
        Ok(v) => v,
        Err(_) => {
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_decode_failed += 1;
                guard.last_drop_reason = Some("decode_failed".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::Malformed,
                        now_unix(),
                    );
                    refresh_connected_peers_from_health(&mut guard);
                    persist_peer_state_if_configured(&guard);
                }
            }
            return;
        }
    };

    if let Ok(mut guard) = inner.lock() {
        if !admit_peer_inbound_message(&mut guard, source_peer, now_unix()) {
            guard.last_drop_reason = Some("peer_inbound_rate_limited".into());
            refresh_connected_peers_from_health(&mut guard);
            persist_peer_state_if_configured(&guard);
            return;
        }
    }

    match msg {
        NetworkMessage::NewTransaction {
            chain_id,
            transaction,
        } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_tx".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if bytes.len() > MAX_TX_MESSAGE_BYTES {
                if let Ok(mut guard) = inner.lock() {
                    guard.tx_inbound_received = guard.tx_inbound_received.saturating_add(1);
                    guard.tx_inbound_invalid = guard.tx_inbound_invalid.saturating_add(1);
                    guard.last_drop_reason = Some("tx_message_too_large".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::Malformed,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let id = message_id_for_tx(&transaction);
            if let Ok(mut guard) = inner.lock() {
                guard.tx_inbound_received = guard.tx_inbound_received.saturating_add(1);
                if !admit_inbound_tx_rate(&mut guard, now_unix()) {
                    guard.tx_inbound_invalid = guard.tx_inbound_invalid.saturating_add(1);
                    guard.last_drop_reason = Some("tx_inbound_rate_limited".into());
                    return;
                }
                if !mark_inbound_tx_seen(
                    &mut guard,
                    id,
                    transaction.txid.clone(),
                    source_peer,
                    now_unix(),
                ) {
                    guard.inbound_duplicates_suppressed += 1;
                    guard.tx_inbound_duplicate = guard.tx_inbound_duplicate.saturating_add(1);
                    guard.last_drop_reason = Some("duplicate_tx".into());
                    return;
                }
                guard.inbound_messages += 1;
                guard.tx_inbound_accepted = guard.tx_inbound_accepted.saturating_add(1);
                guard.last_message_kind = Some("tx-inbound".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::Transaction(transaction));
        }
        NetworkMessage::NewBlock { chain_id, block }
        | NetworkMessage::Block { chain_id, block } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Err(reason) = validate_inbound_block_shape(&block) {
                if let Ok(mut guard) = inner.lock() {
                    guard.blocks_received = guard.blocks_received.saturating_add(1);
                    guard.invalid_blocks_received = guard.invalid_blocks_received.saturating_add(1);
                    guard.last_drop_reason = Some(format!("invalid_block_{reason}"));
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::InvalidBlock,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let id = message_id_for_block(&block);
            if let Ok(mut guard) = inner.lock() {
                guard.blocks_received = guard.blocks_received.saturating_add(1);
                if !mark_inbound_block_seen(
                    &mut guard,
                    id,
                    block.hash.clone(),
                    source_peer,
                    now_unix(),
                ) {
                    guard.inbound_duplicates_suppressed += 1;
                    guard.duplicate_blocks_received =
                        guard.duplicate_blocks_received.saturating_add(1);
                    guard.last_drop_reason = Some("duplicate_block".into());
                    return;
                }
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("block-inbound".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::Block(block));
        }
        NetworkMessage::BlockAnnounce { chain_id, hash }
        | NetworkMessage::NewBlockHash { chain_id, hash } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block_announce".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if !validate_block_announcement_hash(&hash) {
                if let Ok(mut guard) = inner.lock() {
                    guard.invalid_blocks_received = guard.invalid_blocks_received.saturating_add(1);
                    guard.last_drop_reason = Some("invalid_block_announce_hash".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::InvalidAnnouncement,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let id = format!("block-announce:{}", hash);
            if let Ok(mut guard) = inner.lock() {
                if !mark_inbound_id_seen(&mut guard, id, now_unix()) {
                    guard.inbound_duplicates_suppressed += 1;
                    guard.duplicate_blocks_received =
                        guard.duplicate_blocks_received.saturating_add(1);
                    guard.last_drop_reason = Some("duplicate_block_announce".into());
                    return;
                }
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("block-announce".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::BlockAnnouncement { hash });
        }
        NetworkMessage::InvBlock { chain_id, hashes } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_inv_block".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let oversized = hashes.len() > MAX_INV_BLOCK_HASHES;
            let capped_hashes = hashes
                .into_iter()
                .take(MAX_INV_BLOCK_HASHES)
                .collect::<Vec<_>>();
            let mut requested = Vec::new();
            if let Ok(mut guard) = inner.lock() {
                guard.inv_blocks_received = guard.inv_blocks_received.saturating_add(1);
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("inv-block".into());
                if oversized {
                    guard.last_drop_reason = Some("inv_block_oversized_capped".into());
                }
                for hash in capped_hashes {
                    let id = format!("block-inv:{}", hash);
                    let known = guard.known_block_hashes.contains(&hash)
                        || guard
                            .inbound_seen_cache
                            .values()
                            .any(|entry| entry.block_hash.as_deref() == Some(hash.as_str()))
                        || guard.inbound_seen_at_unix.contains_key(&id);
                    if known {
                        guard.inv_hashes_known = guard.inv_hashes_known.saturating_add(1);
                        guard.inbound_duplicates_suppressed =
                            guard.inbound_duplicates_suppressed.saturating_add(1);
                        continue;
                    }
                    if requested.len() >= MAX_INV_BLOCK_REQUEST_FANOUT {
                        guard.last_drop_reason = Some("inv_block_request_fanout_capped".into());
                        continue;
                    }
                    if mark_inbound_block_seen(
                        &mut guard,
                        id,
                        hash.clone(),
                        source_peer,
                        now_unix(),
                    ) {
                        guard.inv_hashes_requested = guard.inv_hashes_requested.saturating_add(1);
                        requested.push(hash);
                    } else {
                        guard.inv_hashes_known = guard.inv_hashes_known.saturating_add(1);
                        guard.inbound_duplicates_suppressed =
                            guard.inbound_duplicates_suppressed.saturating_add(1);
                    }
                }
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            if !requested.is_empty() {
                let _ = inbound_tx.send(InboundEvent::BlockInventory { hashes: requested });
            }
        }
        NetworkMessage::GetHeaders {
            chain_id,
            locator,
            stop_hash,
            limit,
        } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_get_headers".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.header_requests_received = guard.header_requests_received.saturating_add(1);
                guard.last_message_kind = Some("get-headers".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::GetHeaders {
                locator,
                stop_hash,
                limit,
            });
        }
        NetworkMessage::Headers { chain_id, headers } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_headers".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let mut accepted_headers = Vec::new();
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.headers_received =
                    guard.headers_received.saturating_add(headers.len() as u64);
                guard.last_message_kind = Some("headers".into());
                for item in headers {
                    if item.hash.is_empty() || compute_block_hash(&item.header) != item.hash {
                        guard.invalid_blocks_received =
                            guard.invalid_blocks_received.saturating_add(1);
                        guard.last_drop_reason = Some("invalid_header_hash".into());
                        if let Some(peer) = source_peer {
                            score_peer_message_outcome(
                                &mut guard,
                                peer,
                                PeerMessageOutcome::InvalidAnnouncement,
                                now_unix(),
                            );
                        }
                        continue;
                    }
                    guard.known_block_hashes.insert(item.hash.clone());
                    accepted_headers.push(item);
                }
                if !accepted_headers.is_empty() {
                    guard.dependency_fetches_scheduled = guard
                        .dependency_fetches_scheduled
                        .saturating_add(accepted_headers.len() as u64);
                }
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            if !accepted_headers.is_empty() {
                let _ = inbound_tx.send(InboundEvent::Headers {
                    headers: accepted_headers,
                });
            }
        }
        NetworkMessage::GetTips { chain_id } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_get_tips".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("get-tips".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::GetTips);
        }
        NetworkMessage::Tips { chain_id, tips } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_tips".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("tips".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::Tips { tips });
        }

        NetworkMessage::GetBlockHeaders { chain_id, hashes } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_get_block_headers".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let oversized = hashes.len() > MAX_INV_BLOCK_HASHES;
            let hashes = hashes
                .into_iter()
                .take(MAX_INV_BLOCK_HASHES)
                .filter(|hash| validate_block_announcement_hash(hash))
                .collect::<Vec<_>>();
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.block_headers_requested = guard
                    .block_headers_requested
                    .saturating_add(hashes.len() as u64);
                guard.last_message_kind = Some("get-block-headers".into());
                if oversized {
                    guard.last_drop_reason = Some("get_block_headers_oversized_capped".into());
                }
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::GetBlockHeaders { hashes });
        }
        NetworkMessage::BlockHeaders { chain_id, headers } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block_headers".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            let oversized = headers.len() > MAX_INV_BLOCK_REQUEST_FANOUT;
            let mut headers = headers
                .into_iter()
                .take(MAX_INV_BLOCK_REQUEST_FANOUT)
                .filter(validate_header_announcement)
                .collect::<Vec<_>>();
            sort_headers_parents_first(&mut headers);
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.block_header_batches_received =
                    guard.block_header_batches_received.saturating_add(1);
                guard.block_headers_received = guard
                    .block_headers_received
                    .saturating_add(headers.len() as u64);
                guard.last_message_kind = Some("block-headers".into());
                if oversized {
                    guard.last_drop_reason = Some("block_headers_oversized_capped".into());
                }
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::BlockHeaders { headers });
        }
        NetworkMessage::GetBlock { chain_id, hash } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_get_block".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Ok(mut guard) = inner.lock() {
                guard.inbound_messages += 1;
                guard.blocks_requested = guard.blocks_requested.saturating_add(1);
                guard.last_message_kind = Some("get-block".into());
                if let Some(peer) = source_peer {
                    score_peer_message_outcome(
                        &mut guard,
                        peer,
                        PeerMessageOutcome::ValidRelay,
                        now_unix(),
                    );
                }
            }
            let _ = inbound_tx.send(InboundEvent::GetBlock { hash });
        }
        NetworkMessage::BlockData {
            chain_id,
            block,
            request_hash,
        } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block_data".into());
                    if let Some(peer) = source_peer {
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ChainMismatch,
                            now_unix(),
                        );
                        refresh_connected_peers_from_health(&mut guard);
                        persist_peer_state_if_configured(&guard);
                    }
                }
                return;
            }
            if let Some(block) = block {
                if let Err(reason) = validate_inbound_block_shape(&block) {
                    if let Ok(mut guard) = inner.lock() {
                        guard.blocks_received = guard.blocks_received.saturating_add(1);
                        guard.invalid_blocks_received =
                            guard.invalid_blocks_received.saturating_add(1);
                        guard.last_drop_reason = Some(format!("invalid_block_data_{reason}"));
                        if let Some(peer) = source_peer {
                            score_peer_message_outcome(
                                &mut guard,
                                peer,
                                PeerMessageOutcome::InvalidBlock,
                                now_unix(),
                            );
                            refresh_connected_peers_from_health(&mut guard);
                            persist_peer_state_if_configured(&guard);
                        }
                    }
                    return;
                }
                let id = message_id_for_block(&block);
                if let Ok(mut guard) = inner.lock() {
                    guard.blocks_received = guard.blocks_received.saturating_add(1);
                    if !mark_inbound_block_seen(
                        &mut guard,
                        id,
                        block.hash.clone(),
                        source_peer,
                        now_unix(),
                    ) {
                        guard.inbound_duplicates_suppressed += 1;
                        guard.duplicate_blocks_received =
                            guard.duplicate_blocks_received.saturating_add(1);
                        guard.last_drop_reason = Some("duplicate_block_data".into());
                        return;
                    }
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("block-data".into());
                    if let Some(peer) = source_peer {
                        let now = now_unix();
                        guard
                            .peer_book
                            .entry(peer.to_string())
                            .or_default()
                            .last_successful_block_unix = Some(now);
                        score_peer_message_outcome(
                            &mut guard,
                            peer,
                            PeerMessageOutcome::ValidRelay,
                            now,
                        );
                    }
                }
                let _ = inbound_tx.send(InboundEvent::Block(block));
            } else {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("block-data-missing".into());
                }
                let _ = inbound_tx.send(InboundEvent::BlockDataMissing { hash: request_hash });
            }
        }
        NetworkMessage::Reject { chain_id, .. } | NetworkMessage::Error { chain_id, .. } => {
            if chain_id == expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("peer-reject-or-error".into());
                }
            }
        }
    }
}

fn fake_swarm_bootstrap_events(
    local_peer_id: PeerId,
    cfg: &Libp2pConfig,
    inner: &Arc<Mutex<InnerState>>,
    inbound_tx: &mpsc::UnboundedSender<InboundEvent>,
) {
    note_swarm_event(inner, format!("swarm-created:{}", local_peer_id));
    let _ = inbound_tx.send(InboundEvent::PeerConnected(format!(
        "local:{}",
        local_peer_id
    )));
    note_swarm_event(inner, "listen-started");
    for addr in &cfg.bootstrap {
        let _ = inbound_tx.send(InboundEvent::PeerConnected(addr.clone()));
        note_swarm_event(inner, format!("bootstrap-seen:{}", addr));
        register_peer_result(inner, addr, true);
    }
    if let Ok(mut guard) = inner.lock() {
        guard.listening = vec![cfg.listen_addr.clone()];
        persist_peer_state_if_configured(&guard);
    }
}

async fn run_libp2p_runtime(
    cfg: Libp2pConfig,
    local_peer_id: PeerId,
    topics: Vec<gossipsub::IdentTopic>,
    inner: Arc<Mutex<InnerState>>,
    mut outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    inbound_tx: mpsc::UnboundedSender<InboundEvent>,
) {
    fake_swarm_bootstrap_events(local_peer_id, &cfg, &inner, &inbound_tx);
    let mut outbound_queue = OutboundPriorityQueue::default();
    loop {
        tokio::select! {
            Some(msg) = outbound_rx.recv() => {
                enqueue_outbound_message(&inner, &mut outbound_queue, msg);
                drain_outbound_rx_to_priority_queue(&inner, &mut outbound_rx, &mut outbound_queue);
                while let Some(msg) = pop_outbound_message(&inner, &mut outbound_queue) {
                    let (wire, topic_name, message_kind, message_id) = match msg {
                    OutboundMessage::Transaction(tx) => {
                        let topic_name = format!("{}-txs", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
                            chain_id: cfg.chain_id.clone(),
                            transaction: tx.clone(),
                        });
                        let message_id = message_id_for_tx(&tx);
                        (wire, topic_name, "tx", message_id)
                    }
                    OutboundMessage::Block(block) => {
                        let topic_name = format!("{}-blocks", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::NewBlock {
                            chain_id: cfg.chain_id.clone(),
                            block: block.clone(),
                        });
                        let message_id = message_id_for_block(&block);
                        (wire, topic_name, "block", message_id)
                    }
                    OutboundMessage::InvBlock(hashes) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:inv-block:{}", hashes.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::InvBlock { chain_id: cfg.chain_id.clone(), hashes });
                        (wire, topic_name, "inv-block", message_id)
                    }
                    OutboundMessage::GetHeaders { locator, stop_hash, limit } => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let stop_part = stop_hash.as_deref().unwrap_or("none");
                        let message_id = format!("sync:get-headers:{}:{stop_part}:{limit}", locator.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::GetHeaders { chain_id: cfg.chain_id.clone(), locator, stop_hash, limit });
                        (wire, topic_name, "get-headers", message_id)
                    }
                    OutboundMessage::Headers(headers) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let hashes = headers.iter().map(|h| h.hash.as_str()).collect::<Vec<_>>().join(",");
                        let message_id = format!("sync:headers:{hashes}");
                        let wire = serde_json::to_vec(&NetworkMessage::Headers { chain_id: cfg.chain_id.clone(), headers });
                        (wire, topic_name, "headers", message_id)
                    }
                    OutboundMessage::GetTips => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::GetTips { chain_id: cfg.chain_id.clone() });
                        (wire, topic_name, "get-tips", "sync:get-tips".to_string())
                    }
                    OutboundMessage::Tips(tips) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:tips:{}", tips.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::Tips { chain_id: cfg.chain_id.clone(), tips });
                        (wire, topic_name, "tips", message_id)
                    }
                    OutboundMessage::GetBlockHeaders(hashes) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:get-block-headers:{}", hashes.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::GetBlockHeaders { chain_id: cfg.chain_id.clone(), hashes });
                        (wire, topic_name, "get-block-headers", message_id)
                    }
                    OutboundMessage::BlockHeaders(headers) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:block-headers:{}", headers.iter().map(|h| h.hash.as_str()).collect::<Vec<_>>().join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::BlockHeaders { chain_id: cfg.chain_id.clone(), headers });
                        (wire, topic_name, "block-headers", message_id)
                    }
                    OutboundMessage::GetBlock(hash) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:get-block:{hash}");
                        let wire = serde_json::to_vec(&NetworkMessage::GetBlock { chain_id: cfg.chain_id.clone(), hash });
                        (wire, topic_name, "get-block", message_id)
                    }
                    OutboundMessage::BlockData { block, request_hash } => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = block.as_ref().map(message_id_for_block).unwrap_or_else(|| {
                            request_hash
                                .as_ref()
                                .map(|hash| format!("sync:block-data-missing:{hash}"))
                                .unwrap_or_else(|| "sync:block-data:none".to_string())
                        });
                        let wire = serde_json::to_vec(&NetworkMessage::BlockData { chain_id: cfg.chain_id.clone(), block, request_hash });
                        (wire, topic_name, "block-data", message_id)
                    }
                };

                    note_swarm_event(&inner, format!("publish-attempt:{topic_name}"));
                    record_publish(&inner, &topic_name, message_kind, &message_id);

                    if let Ok(bytes) = wire {
                        // v0.6.9 keeps one canonical wire path while the actual Swarm
                        // publish/poll code is staged. The bytes are decoded through the
                        // same dispatcher that the live Swarm loop will use.
                        dispatch_network_message(&cfg.chain_id, &bytes, None, &inner, &inbound_tx);
                    }

                    let _ = &topics; // reserved for real gossipsub publish bindings
                }
            }
            _ = sleep(Duration::from_secs(5)) => {
                let heartbeat = serde_json::to_vec(&NetworkMessage::GetTips {
                    chain_id: cfg.chain_id.clone(),
                });
                note_swarm_event(&inner, "heartbeat:get-tips");
                if let Ok(bytes) = heartbeat {
                    dispatch_network_message(&cfg.chain_id, &bytes, None, &inner, &inbound_tx);
                }
            }
            _ = sleep(Duration::from_secs(13)) => {
                let event = "NewListenAddr(/memory/placeholder)".to_string();
                note_swarm_event(&inner, format!("swarm-skeleton:{event}"));
            }
            else => break,
        }
    }
}

#[derive(NetworkBehaviour)]
struct PulseBehaviour {
    gossipsub: gossipsub::Behaviour,
    ping: ping::Behaviour,
}

fn parse_bootnode_multiaddr(input: &str) -> Option<(PeerId, Multiaddr)> {
    let address = input.parse::<Multiaddr>().ok()?;
    let mut peer_id = None;
    for protocol in address.iter() {
        if let libp2p::multiaddr::Protocol::P2p(id) = protocol {
            peer_id = Some(id);
        }
    }
    peer_id.map(|id| (id, address))
}

fn parse_bootstrap(bootstrap: &[String]) -> Vec<(PeerId, Multiaddr)> {
    bootstrap
        .iter()
        .filter_map(|addr| parse_bootnode_multiaddr(addr))
        .collect()
}

fn is_configured_bootnode_peer(guard: &InnerState, peer_id: &PeerId) -> bool {
    let peer = peer_id.to_string();
    guard
        .bootnodes_configured
        .iter()
        .any(|addr| addr.contains(&peer))
}

fn bootnode_generic_cooldown_active(state: &InnerState, peer_id: &PeerId, now: u64) -> bool {
    state
        .peer_book
        .get(&peer_id.to_string())
        .is_some_and(|health| health.next_retry_unix > now || health.suppressed_until_unix > now)
}

fn has_useful_connected_peers(state: &InnerState) -> bool {
    !state.connected_peers.is_empty()
}

fn should_force_bootnode_redial_for_peer(state: &InnerState, peer_id: &str) -> bool {
    mode_connected_peers_are_real_network(&state.mode)
        && !state.bootnodes_configured.is_empty()
        && !has_useful_connected_peers(state)
        && !state.pending_bootnode_dials.contains(peer_id)
}

fn isolated_bootnode_reconnect_active(state: &InnerState) -> bool {
    mode_connected_peers_are_real_network(&state.mode)
        && !state.bootnodes_configured.is_empty()
        && !has_useful_connected_peers(state)
        && (!state.pending_bootnode_dials.is_empty() || !state.bootnode_next_redial_at.is_empty())
}

fn record_bootnode_reconnect_schedule(
    inner: &Arc<Mutex<InnerState>>,
    peer_id: &PeerId,
    now: u64,
) -> bool {
    inner
        .lock()
        .map(|mut guard| {
            let generic_cooldown_active = bootnode_generic_cooldown_active(&guard, peer_id, now);
            guard.bootnode_redial_attempts = guard.bootnode_redial_attempts.saturating_add(1);
            guard.bootnode_reconnect_scheduled_total =
                guard.bootnode_reconnect_scheduled_total.saturating_add(1);
            if generic_cooldown_active {
                guard.bootnode_reconnect_skipped_cooldown_total = guard
                    .bootnode_reconnect_skipped_cooldown_total
                    .saturating_add(1);
                guard.bootnode_reconnect_forced_from_cooldown_total = guard
                    .bootnode_reconnect_forced_from_cooldown_total
                    .saturating_add(1);
            }
            generic_cooldown_active
        })
        .unwrap_or(false)
}

fn handle_connection_established(
    inner: &Arc<Mutex<InnerState>>,
    pending_bootnode_dials: &mut HashSet<PeerId>,
    peer_id: &PeerId,
    direction: &str,
) {
    if let Ok(mut guard) = inner.lock() {
        let is_bootnode = is_configured_bootnode_peer(&guard, peer_id);
        if is_bootnode {
            pending_bootnode_dials.remove(peer_id);
            guard.pending_bootnode_dials.remove(&peer_id.to_string());
            guard
                .pending_bootnode_dial_started_at
                .remove(&peer_id.to_string());
            guard.bootnode_next_redial_at.remove(&peer_id.to_string());
            guard
                .bootnode_redial_backoff_secs
                .insert(peer_id.to_string(), 1);
        }
        let peer_key = peer_id.to_string();
        let count = guard
            .active_connections
            .entry(peer_key.clone())
            .or_insert(0);
        *count = count.saturating_add(1);
        let active_count = *count;
        guard.connection_established_total = guard.connection_established_total.saturating_add(1);
        record_peer_lifecycle_event(&mut guard, "connection_established");
        record_peer_lifecycle_event(&mut guard, &format!("{direction}_connected"));
        guard.last_connection_established_peer = Some(peer_key.clone());
        guard.last_peer_state_transition = Some(format!("{peer_key}:connected"));
        guard.peer_connection_final_state.insert(
            peer_key.clone(),
            PeerConnectionFinalState {
                peer_id: peer_key.clone(),
                direction: direction.to_string(),
                state: "connected".to_string(),
                active_connections: active_count,
                last_event_unix: Some(now_unix()),
                last_error: None,
                last_disconnect_reason: None,
            },
        );
        guard.last_bootnode_dial_error = None;
        if is_bootnode {
            guard.bootnode_reconnect_success_total =
                guard.bootnode_reconnect_success_total.saturating_add(1);
        }
        if is_bootnode
            && !guard
                .bootstrap_connected_peer_ids
                .iter()
                .any(|id| id == &peer_key)
        {
            guard.bootstrap_connected_peer_ids.push(peer_key);
        }
    }
    register_peer_result(inner, &peer_id.to_string(), true);
}

fn handle_connection_closed(
    inner: &Arc<Mutex<InnerState>>,
    pending_bootnode_dials: &mut HashSet<PeerId>,
    bootnode_next_redial_at: &mut HashMap<PeerId, u64>,
    bootnode_redial_backoff_secs: &mut HashMap<PeerId, u64>,
    peer_id: &PeerId,
    reason: String,
    direction: &str,
) -> bool {
    let mut should_mark_disconnected = false;
    if let Ok(mut guard) = inner.lock() {
        let peer_key = peer_id.to_string();
        let is_bootnode = is_configured_bootnode_peer(&guard, peer_id);
        let remaining_count = {
            if let Some(entry) = guard.active_connections.get_mut(&peer_key) {
                *entry = entry.saturating_sub(1);
                *entry
            } else {
                0
            }
        };
        guard.last_connection_closed_peer = Some(peer_key.clone());
        guard.connection_closed_total = guard.connection_closed_total.saturating_add(1);
        record_peer_lifecycle_event(&mut guard, "connection_closed");
        record_peer_lifecycle_event(&mut guard, &format!("{direction}_disconnected"));
        *guard
            .disconnect_reason_counts
            .entry(reason.clone())
            .or_insert(0) += 1;
        guard.last_connection_closed_reason = Some(reason.clone());
        guard.last_connection_closed_remaining_count = Some(remaining_count);
        guard.last_disconnect_reason = Some(reason.clone());
        guard.peer_connection_final_state.insert(
            peer_key.clone(),
            PeerConnectionFinalState {
                peer_id: peer_key.clone(),
                direction: direction.to_string(),
                state: if remaining_count == 0 {
                    "disconnected"
                } else {
                    "connected"
                }
                .to_string(),
                active_connections: remaining_count,
                last_event_unix: Some(now_unix()),
                last_error: None,
                last_disconnect_reason: Some(reason),
            },
        );
        if remaining_count == 0 {
            guard.last_peer_state_transition = Some(format!("{peer_key}:disconnected"));
            guard.active_connections.remove(&peer_key);
            should_mark_disconnected = true;

            if is_bootnode {
                pending_bootnode_dials.remove(peer_id);
                guard.pending_bootnode_dials.remove(&peer_key);
                guard.pending_bootnode_dial_started_at.remove(&peer_key);
                let now = now_unix();
                let current = guard
                    .bootnode_redial_backoff_secs
                    .get(&peer_key)
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let next = (current.saturating_mul(2)).min(10);
                let next_at = now.saturating_add(current);
                bootnode_redial_backoff_secs.insert(*peer_id, next);
                bootnode_next_redial_at.insert(*peer_id, next_at);
                guard
                    .bootnode_redial_backoff_secs
                    .insert(peer_key.clone(), next);
                guard.bootnode_next_redial_at.insert(peer_key, next_at);
            }
        }
    }
    should_mark_disconnected
}

fn handle_outgoing_connection_error(
    inner: &Arc<Mutex<InnerState>>,
    pending_bootnode_dials: &mut HashSet<PeerId>,
    peer_id: &PeerId,
    error: &str,
) -> bool {
    if let Ok(mut guard) = inner.lock() {
        let is_bootnode = is_configured_bootnode_peer(&guard, peer_id);
        if is_bootnode {
            pending_bootnode_dials.remove(peer_id);
            guard.pending_bootnode_dials.remove(&peer_id.to_string());
            guard
                .pending_bootnode_dial_started_at
                .remove(&peer_id.to_string());
            let now = now_unix();
            let current = guard
                .bootnode_redial_backoff_secs
                .get(&peer_id.to_string())
                .copied()
                .unwrap_or(1)
                .max(1);
            let next = (current.saturating_mul(2)).min(10);
            guard
                .bootnode_next_redial_at
                .insert(peer_id.to_string(), now.saturating_add(current));
            guard
                .bootnode_redial_backoff_secs
                .insert(peer_id.to_string(), next);
            guard.bootnode_redial_failures = guard.bootnode_redial_failures.saturating_add(1);
            guard.last_bootnode_dial_error = Some(error.to_string());
        }
        let peer_key = peer_id.to_string();
        guard.last_outgoing_connection_error_peer = Some(peer_key.clone());
        guard.last_dial_error = Some(error.to_string());
        record_peer_lifecycle_event(&mut guard, "outgoing_connection_error");
        record_peer_error(
            &mut guard,
            &peer_key,
            "outgoing_connection_error",
            error.to_string(),
            now_unix(),
        );
        let active_connections = guard
            .active_connections
            .get(&peer_key)
            .copied()
            .unwrap_or(0);
        guard.peer_connection_final_state.insert(
            peer_key.clone(),
            PeerConnectionFinalState {
                peer_id: peer_key.clone(),
                direction: "outbound".to_string(),
                state: "error".to_string(),
                active_connections,
                last_event_unix: Some(now_unix()),
                last_error: Some(error.to_string()),
                last_disconnect_reason: None,
            },
        );
    }
    inner
        .lock()
        .ok()
        .and_then(|guard| guard.active_connections.get(&peer_id.to_string()).copied())
        .unwrap_or(0)
        == 0
}

async fn run_libp2p_real_runtime(
    cfg: Libp2pConfig,
    local_key: identity::Keypair,
    topics: Vec<gossipsub::IdentTopic>,
    inner: Arc<Mutex<InnerState>>,
    mut outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    inbound_tx: mpsc::UnboundedSender<InboundEvent>,
) {
    let mut gossip_config = gossipsub::ConfigBuilder::default();
    gossip_config.validation_mode(ValidationMode::Permissive);
    let gossip = match gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key.clone()),
        gossip_config.build().unwrap_or_default(),
    ) {
        Ok(v) => v,
        Err(e) => {
            note_swarm_event(&inner, format!("swarm-init-failed:gossipsub:{e}"));
            return;
        }
    };

    let ping = ping::Behaviour::new(ping::Config::new());

    let mut swarm = match SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        ) {
        Ok(builder) => match builder.with_behaviour(|_| PulseBehaviour {
            gossipsub: gossip,
            ping,
        }) {
            Ok(builder) => builder.build(),
            Err(e) => {
                note_swarm_event(&inner, format!("swarm-init-failed:behaviour:{e}"));
                return;
            }
        },
        Err(e) => {
            note_swarm_event(&inner, format!("swarm-init-failed:transport:{e}"));
            return;
        }
    };

    let listen_addr = match cfg.listen_addr.parse::<Multiaddr>() {
        Ok(addr) => addr,
        Err(e) => {
            note_swarm_event(
                &inner,
                format!(
                    "swarm-init-failed:invalid-listen-addr:{}:{e}",
                    cfg.listen_addr
                ),
            );
            return;
        }
    };
    if let Err(e) = swarm.listen_on(listen_addr.clone()) {
        note_swarm_event(&inner, format!("swarm-init-failed:listen:{e}"));
        return;
    }

    for topic in &topics {
        let _ = swarm.behaviour_mut().gossipsub.subscribe(topic);
    }

    note_swarm_event(&inner, "swarm-real-started");
    note_swarm_event(&inner, format!("listen-attempt:{listen_addr}"));
    let mut pending_bootnode_dials: HashSet<PeerId> = HashSet::new();
    let bootstrap_peers = parse_bootstrap(&cfg.bootstrap);
    for (peer_id, addr) in &bootstrap_peers {
        if let Ok(mut guard) = inner.lock() {
            guard.peer_book.entry(peer_id.to_string()).or_default();
            guard.bootstrap_dial_attempts = guard.bootstrap_dial_attempts.saturating_add(1);
        }
        note_swarm_event(&inner, format!("dial-attempt:bootstrap:{peer_id}:{addr}"));
        if let Err(e) = swarm.dial(addr.clone()) {
            if let Ok(mut guard) = inner.lock() {
                guard.bootstrap_dial_failures = guard.bootstrap_dial_failures.saturating_add(1);
                guard.last_bootnode_dial_error = Some(e.to_string());
            }
            note_swarm_event(
                &inner,
                format!("bootstrap-dial-failed:{peer_id}:{addr}:{e}"),
            );
        } else {
            pending_bootnode_dials.insert(*peer_id);
            if let Ok(mut guard) = inner.lock() {
                guard.pending_bootnode_dials.insert(peer_id.to_string());
                guard
                    .pending_bootnode_dial_started_at
                    .insert(peer_id.to_string(), now_unix());
                guard
                    .bootnode_redial_backoff_secs
                    .insert(peer_id.to_string(), 1);
                guard.bootnode_next_redial_at.remove(&peer_id.to_string());
                guard.bootstrap_dial_successes = guard.bootstrap_dial_successes.saturating_add(1);
            }
            note_swarm_event(&inner, format!("bootstrap-dialing:{peer_id}:{addr}"));
        }
    }
    let mut outbound_queue = OutboundPriorityQueue::default();
    let mut bootnode_next_redial_at: HashMap<PeerId, u64> = HashMap::new();
    let mut bootnode_redial_backoff_secs: HashMap<PeerId, u64> = HashMap::new();
    let mut bootnode_redial_tick = tokio::time::interval(Duration::from_secs(3));
    bootnode_redial_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            Some(msg) = outbound_rx.recv() => {
                enqueue_outbound_message(&inner, &mut outbound_queue, msg);
                drain_outbound_rx_to_priority_queue(&inner, &mut outbound_rx, &mut outbound_queue);
                while let Some(msg) = pop_outbound_message(&inner, &mut outbound_queue) {
                    let (wire, topic_name, message_kind, message_id) = match msg {
                    OutboundMessage::Transaction(tx) => {
                        let topic_name = format!("{}-txs", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
                            chain_id: cfg.chain_id.clone(),
                            transaction: tx.clone(),
                        });
                        let message_id = message_id_for_tx(&tx);
                        (wire, topic_name, "tx", message_id)
                    }
                    OutboundMessage::Block(block) => {
                        let topic_name = format!("{}-blocks", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::NewBlock {
                            chain_id: cfg.chain_id.clone(),
                            block: block.clone(),
                        });
                        let message_id = message_id_for_block(&block);
                        (wire, topic_name, "block", message_id)
                    }
                    OutboundMessage::InvBlock(hashes) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:inv-block:{}", hashes.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::InvBlock { chain_id: cfg.chain_id.clone(), hashes });
                        (wire, topic_name, "inv-block", message_id)
                    }
                    OutboundMessage::GetHeaders { locator, stop_hash, limit } => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let stop_part = stop_hash.as_deref().unwrap_or("none");
                        let message_id = format!("sync:get-headers:{}:{stop_part}:{limit}", locator.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::GetHeaders { chain_id: cfg.chain_id.clone(), locator, stop_hash, limit });
                        (wire, topic_name, "get-headers", message_id)
                    }
                    OutboundMessage::Headers(headers) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let hashes = headers.iter().map(|h| h.hash.as_str()).collect::<Vec<_>>().join(",");
                        let message_id = format!("sync:headers:{hashes}");
                        let wire = serde_json::to_vec(&NetworkMessage::Headers { chain_id: cfg.chain_id.clone(), headers });
                        (wire, topic_name, "headers", message_id)
                    }
                    OutboundMessage::GetTips => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let wire = serde_json::to_vec(&NetworkMessage::GetTips { chain_id: cfg.chain_id.clone() });
                        (wire, topic_name, "get-tips", "sync:get-tips".to_string())
                    }
                    OutboundMessage::Tips(tips) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:tips:{}", tips.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::Tips { chain_id: cfg.chain_id.clone(), tips });
                        (wire, topic_name, "tips", message_id)
                    }
                    OutboundMessage::GetBlockHeaders(hashes) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:get-block-headers:{}", hashes.join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::GetBlockHeaders { chain_id: cfg.chain_id.clone(), hashes });
                        (wire, topic_name, "get-block-headers", message_id)
                    }
                    OutboundMessage::BlockHeaders(headers) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:block-headers:{}", headers.iter().map(|h| h.hash.as_str()).collect::<Vec<_>>().join(","));
                        let wire = serde_json::to_vec(&NetworkMessage::BlockHeaders { chain_id: cfg.chain_id.clone(), headers });
                        (wire, topic_name, "block-headers", message_id)
                    }
                    OutboundMessage::GetBlock(hash) => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = format!("sync:get-block:{hash}");
                        let wire = serde_json::to_vec(&NetworkMessage::GetBlock { chain_id: cfg.chain_id.clone(), hash });
                        (wire, topic_name, "get-block", message_id)
                    }
                    OutboundMessage::BlockData { block, request_hash } => {
                        let topic_name = format!("{}-sync", cfg.chain_id);
                        let message_id = block.as_ref().map(message_id_for_block).unwrap_or_else(|| {
                            request_hash
                                .as_ref()
                                .map(|hash| format!("sync:block-data-missing:{hash}"))
                                .unwrap_or_else(|| "sync:block-data:none".to_string())
                        });
                        let wire = serde_json::to_vec(&NetworkMessage::BlockData { chain_id: cfg.chain_id.clone(), block, request_hash });
                        (wire, topic_name, "block-data", message_id)
                    }
                };

                    note_swarm_event(&inner, format!("publish-attempt:{topic_name}"));
                    record_publish(&inner, &topic_name, message_kind, &message_id);

                    if let Ok(bytes) = wire {
                        let topic = gossipsub::IdentTopic::new(topic_name);
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, bytes) {
                            note_swarm_event(&inner, format!("publish-failed:{e}"));
                        }
                    }
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(PulseBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                        let source_peer = message.source.as_ref().map(|peer| peer.to_string());
                        dispatch_network_message(&cfg.chain_id, &message.data, source_peer.as_deref(), &inner, &inbound_tx);
                    }
                    SwarmEvent::Behaviour(PulseBehaviourEvent::Ping(event)) => {
                        note_swarm_event(&inner, format!("ping:{event:?}"));
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        let direction = connection_direction_from_endpoint_debug(&format!("{endpoint:?}"));
                        note_swarm_event(&inner, format!("peer-connected:{direction}:{peer_id}"));
                        handle_connection_established(&inner, &mut pending_bootnode_dials, &peer_id, direction);
                        let _ = inbound_tx.send(InboundEvent::PeerConnected(peer_id.to_string()));
                    }
                    SwarmEvent::ConnectionClosed { peer_id, endpoint, cause, .. } => {
                        let direction = connection_direction_from_endpoint_debug(&format!("{endpoint:?}"));
                        note_swarm_event(&inner, format!("peer-disconnected:{direction}:{peer_id}"));
                        let reason = format!("swarm-connection-closed:{cause:?}");
                        let should_mark_disconnected = handle_connection_closed(
                            &inner,
                            &mut pending_bootnode_dials,
                            &mut bootnode_next_redial_at,
                            &mut bootnode_redial_backoff_secs,
                            &peer_id,
                            reason,
                            direction,
                        );
                        if should_mark_disconnected {
                            register_peer_result(&inner, &peer_id.to_string(), false);
                        }
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        if let Some(peer_id) = peer_id {
                            let should_mark_failed = handle_outgoing_connection_error(
                                &inner,
                                &mut pending_bootnode_dials,
                                &peer_id,
                                &error.to_string(),
                            );
                            if should_mark_failed {
                                register_peer_result(&inner, &peer_id.to_string(), false);
                            }
                        }
                        note_swarm_event(&inner, format!("outgoing-connection-error:{error}"));
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        note_swarm_event(&inner, format!("listening:{address}"));
                        if let Ok(mut guard) = inner.lock() {
                            if !guard.listening.iter().any(|item| item == &address.to_string()) {
                                guard.listening.push(address.to_string());
                            }
                        }
                    }
                    other => {
                        if let SwarmEvent::IncomingConnectionError { send_back_addr, error, .. } = &other {
                            if let Ok(mut guard) = inner.lock() {
                                let peer_key = send_back_addr.to_string();
                                guard.last_incoming_connection_error_peer = Some(peer_key.clone());
                                guard.last_dial_error = Some(error.to_string());
                                record_peer_lifecycle_event(&mut guard, "incoming_connection_error");
                                record_peer_error(&mut guard, &peer_key, "incoming_connection_error", error.to_string(), now_unix());
                                guard.peer_connection_final_state.insert(
                                    peer_key.clone(),
                                    PeerConnectionFinalState {
                                        peer_id: peer_key.clone(),
                                        direction: "inbound".to_string(),
                                        state: "error".to_string(),
                                        active_connections: 0,
                                        last_event_unix: Some(now_unix()),
                                        last_error: Some(error.to_string()),
                                        last_disconnect_reason: None,
                                    },
                                );
                            }
                        }
                        note_swarm_event(&inner, format!("swarm:{other:?}"));
                    }
                }
            }
            _ = bootnode_redial_tick.tick() => {
                if let Ok(mut guard) = inner.lock() {
                    refresh_connected_peers_from_health(&mut guard);
                    enforce_connectivity_aware_cooldown_floor(&mut guard, now_unix());
                }
                for (peer_id, addr) in &bootstrap_peers {
                    let now = now_unix();
                    if pending_bootnode_dials.contains(peer_id) {
                        let stale_pending = inner
                            .lock()
                            .ok()
                            .and_then(|guard| guard.pending_bootnode_dial_started_at.get(&peer_id.to_string()).copied())
                            .map(|started| now.saturating_sub(started) >= 5)
                            .unwrap_or(false);
                        if stale_pending {
                            pending_bootnode_dials.remove(peer_id);
                            if let Ok(mut guard) = inner.lock() {
                                guard.pending_bootnode_dials.remove(&peer_id.to_string());
                                guard.pending_bootnode_dial_started_at.remove(&peer_id.to_string());
                                guard.last_peer_reconnect_blocked_reason = Some("stale_pending_bootnode_dial_released".to_string());
                            }
                            note_swarm_event(&inner, format!("reconnect-retry:stale-bootnode-dial-pending:{peer_id}"));
                        } else {
                            if let Ok(mut guard) = inner.lock() {
                                guard.last_peer_reconnect_blocked_reason = Some("bootnode_dial_pending".to_string());
                            }
                            note_swarm_event(&inner, format!("reconnect-skipped:bootnode-dial-pending:{peer_id}"));
                            continue;
                        }
                    }
                    let peer_key = peer_id.to_string();
                    let should_force_bootnode_redial = inner
                        .lock()
                        .ok()
                        .map(|guard| should_force_bootnode_redial_for_peer(&guard, &peer_key))
                        .unwrap_or(false);
                    let isolated_without_peers = should_force_bootnode_redial;

                    if should_force_bootnode_redial {
                        bootnode_next_redial_at.insert(*peer_id, now);
                        if let Ok(mut guard) = inner.lock() {
                            guard.bootnode_next_redial_at.insert(peer_id.to_string(), now);
                            guard.bootnode_redial_backoff_secs
                                .entry(peer_id.to_string())
                                .or_insert(1);
                        }
                        note_swarm_event(
                            &inner,
                            format!("reconnect-forced:isolated-bootnode-redial-due:{peer_id}"),
                        );
                    }
                    let active = inner
                        .lock()
                        .ok()
                        .and_then(|guard| guard.active_connections.get(&peer_id.to_string()).copied())
                        .unwrap_or(0);
                    if active > 0 && !isolated_without_peers {
                        bootnode_next_redial_at.remove(peer_id);
                        bootnode_redial_backoff_secs.insert(*peer_id, 1);
                        if let Ok(mut guard) = inner.lock() {
                            guard.bootnode_next_redial_at.remove(&peer_id.to_string());
                            guard.bootnode_redial_backoff_secs.insert(peer_id.to_string(), 1);
                        }
                        continue;
                    }
                    if active > 0 && isolated_without_peers {
                        note_swarm_event(
                            &inner,
                            format!("reconnect-forced:bootnode-stale-active-connection:{peer_id}"),
                        );
                    }
                    let local_next_redial_at = bootnode_next_redial_at.get(peer_id).copied().unwrap_or(0);
                    let exposed_next_redial_at = inner
                        .lock()
                        .ok()
                        .and_then(|guard| guard.bootnode_next_redial_at.get(&peer_id.to_string()).copied())
                        .unwrap_or(0);
                    let redial_due =
                        isolated_without_peers || local_next_redial_at <= now || exposed_next_redial_at <= now;
                    if !redial_due {
                        continue;
                    }
                    record_bootnode_reconnect_schedule(&inner, peer_id, now);
                    note_swarm_event(&inner, format!("reconnect-scheduled:bootnode:{peer_id}:{addr}"));
                    note_swarm_event(&inner, format!("dial-attempt:redial:{peer_id}:{addr}"));
                    if let Err(e) = swarm.dial(addr.clone()) {
                        let current = bootnode_redial_backoff_secs.get(peer_id).copied().unwrap_or(1);
                        let next = (current.saturating_mul(2)).min(10);
                        bootnode_redial_backoff_secs.insert(*peer_id, next);
                        bootnode_next_redial_at.insert(*peer_id, now.saturating_add(current.max(1)));
                        if let Ok(mut guard) = inner.lock() {
                            guard.bootnode_redial_backoff_secs.insert(peer_id.to_string(), next);
                            guard.bootnode_next_redial_at.insert(peer_id.to_string(), now.saturating_add(current.max(1)));
                            guard.bootnode_redial_failures = guard.bootnode_redial_failures.saturating_add(1);
                            guard.last_bootnode_dial_error = Some(e.to_string());
                        }
                        note_swarm_event(&inner, format!("dial-failure:redial:{peer_id}:{e}"));
                    } else if let Ok(mut guard) = inner.lock() {
                        pending_bootnode_dials.insert(*peer_id);
                        bootnode_redial_backoff_secs.insert(*peer_id, 1);
                        bootnode_next_redial_at.remove(peer_id);
                        guard.pending_bootnode_dials.insert(peer_id.to_string());
                        guard.pending_bootnode_dial_started_at.insert(peer_id.to_string(), now);
                        guard.peer_zero_reconnect_attempt_total = guard.peer_zero_reconnect_attempt_total.saturating_add(1);
                        guard.bootnode_redial_backoff_secs.insert(peer_id.to_string(), 1);
                        guard.bootnode_next_redial_at.remove(&peer_id.to_string());
                        guard.bootnode_redial_successes = guard.bootnode_redial_successes.saturating_add(1);
                        note_swarm_event(&inner, format!("dial-success:redial:{peer_id}"));
                    }
                }
            }
            else => break,
        }
    }
}

async fn run_libp2p_skeleton_runtime(
    cfg: Libp2pConfig,
    local_peer_id: PeerId,
    topics: Vec<gossipsub::IdentTopic>,
    inner: Arc<Mutex<InnerState>>,
    outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    inbound_tx: mpsc::UnboundedSender<InboundEvent>,
) {
    run_libp2p_runtime(cfg, local_peer_id, topics, inner, outbound_rx, inbound_tx).await;
}

impl Libp2pHandle {
    fn queue_sync_message(&self, msg: OutboundMessage, kind: &str) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            if queue_backpressure_reject(&mut inner, "outbound_queue_backpressure_sync") {
                return Ok(());
            }
            inner.queued_messages += 1;
            match &msg {
                OutboundMessage::InvBlock(_)
                | OutboundMessage::GetHeaders { .. }
                | OutboundMessage::Headers(_)
                | OutboundMessage::GetBlockHeaders(_)
                | OutboundMessage::BlockHeaders(_)
                | OutboundMessage::GetBlock(_)
                | OutboundMessage::BlockData { .. } => {
                    inner.queued_block_messages += 1;
                }
                _ => inner.queued_non_block_messages += 1,
            }
            inner.last_message_kind = Some(kind.to_string());
            track_queue_depth_on_enqueue(&mut inner);
        }
        self.outbound_tx
            .send(msg)
            .map_err(|e| PulseError::Internal(format!("p2p send failed: {e}")))
    }
}

impl P2pHandle for Libp2pHandle {
    fn broadcast_transaction(&self, tx: &Transaction) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            let tx_id = message_id_for_tx(tx);
            if !should_relay_outbound_tx(&mut inner, &tx_id, now_unix()) {
                inner.last_drop_reason = Some("duplicate_tx_outbound".into());
                return Ok(());
            }
            if !admit_tx_relay_under_budget(&mut inner, &tx_id, tx.fee, now_unix()) {
                inner.last_drop_reason = Some("tx_budget_suppressed".into());
                return Ok(());
            }
            if queue_backpressure_reject(&mut inner, "outbound_queue_backpressure_tx") {
                return Ok(());
            }
            record_outbound_tx_relay(&mut inner, &tx_id, now_unix());
            inner.known_txids.insert(tx.txid.clone());
            inner.queued_messages += 1;
            inner.queued_non_block_messages += 1;
            track_queue_depth_on_enqueue(&mut inner);
        }
        self.outbound_tx
            .send(OutboundMessage::Transaction(tx.clone()))
            .map_err(|e| PulseError::Internal(format!("p2p send failed: {e}")))
    }

    fn broadcast_block(&self, block: &Block) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            let block_id = message_id_for_block(block);
            if !should_relay_outbound_block(&mut inner, &block_id, now_unix()) {
                inner.last_drop_reason = Some("duplicate_block_outbound".into());
                return Ok(());
            }
            inner.known_block_hashes.insert(block.hash.clone());
            inner.queued_messages += 1;
            inner.queued_block_messages += 1;
            track_queue_depth_on_enqueue(&mut inner);
        }
        self.outbound_tx
            .send(OutboundMessage::Block(block.clone()))
            .map_err(|e| PulseError::Internal(format!("p2p send failed: {e}")))
    }

    fn request_tips(&self) -> Result<(), PulseError> {
        self.queue_sync_message(OutboundMessage::GetTips, "get-tips")
    }

    fn send_tips(&self, tips: &[PulseHash]) -> Result<(), PulseError> {
        self.queue_sync_message(OutboundMessage::Tips(tips.to_vec()), "tips")
    }

    fn request_block_headers(&self, hashes: &[PulseHash]) -> Result<(), PulseError> {
        self.queue_sync_message(
            OutboundMessage::GetBlockHeaders(hashes.to_vec()),
            "get-block-headers",
        )
    }

    fn send_block_headers(&self, headers: &[BlockHeaderAnnouncement]) -> Result<(), PulseError> {
        self.queue_sync_message(
            OutboundMessage::BlockHeaders(headers.to_vec()),
            "block-headers",
        )
    }

    fn request_block(&self, hash: &PulseHash) -> Result<(), PulseError> {
        self.queue_sync_message(OutboundMessage::GetBlock(hash.clone()), "get-block")
    }

    fn announce_block_inventory(&self, hashes: &[PulseHash]) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            for hash in hashes {
                inner.known_block_hashes.insert(hash.clone());
            }
            inner.headers_announced = inner.headers_announced.saturating_add(hashes.len() as u64);
        }
        self.queue_sync_message(OutboundMessage::InvBlock(hashes.to_vec()), "inv-block")
    }

    fn request_headers(
        &self,
        locator: &[PulseHash],
        stop_hash: Option<&PulseHash>,
        limit: usize,
    ) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            inner.header_requests_sent = inner.header_requests_sent.saturating_add(1);
        }
        self.queue_sync_message(
            OutboundMessage::GetHeaders {
                locator: locator.to_vec(),
                stop_hash: stop_hash.cloned(),
                limit,
            },
            "get-headers",
        )
    }

    fn send_headers(&self, headers: &[HeaderInventory]) -> Result<(), PulseError> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
            inner.headers_sent = inner.headers_sent.saturating_add(headers.len() as u64);
        }
        self.queue_sync_message(OutboundMessage::Headers(headers.to_vec()), "headers")
    }

    fn send_block_data(
        &self,
        request_hash: Option<&PulseHash>,
        block: Option<&Block>,
    ) -> Result<(), PulseError> {
        self.queue_sync_message(
            OutboundMessage::BlockData {
                block: block.cloned(),
                request_hash: request_hash
                    .cloned()
                    .or_else(|| block.map(|block| block.hash.clone())),
            },
            "block-data",
        )
    }

    fn status(&self) -> Result<P2pStatus, PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        refresh_connected_peers_from_health(&mut inner);
        enforce_connectivity_aware_cooldown_floor(&mut inner, now_unix());
        let (
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_lifecycle_healthy,
            peer_lifecycle_watch,
            peer_lifecycle_degraded,
            peer_lifecycle_cooldown,
            peer_lifecycle_recovering,
            peer_retention_active_total,
            peer_retention_recovering_total,
            peer_retention_cooldown_total,
            peer_sync_eligible_total,
            peer_sync_suppressed_total,
            degraded_mode,
            peer_recovery,
        ) = peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer =
            update_selected_sync_peer(&mut inner, &sync_candidates, now_unix());
        let connected_slots_in_use = inner.connected_peers.len();
        let available_connection_slots = inner
            .connection_slot_budget
            .saturating_sub(connected_slots_in_use);
        let (
            topology_bucket_count,
            topology_distinct_buckets,
            topology_dominant_bucket_share_bps,
            topology_diversity_score_bps,
        ) = topology_stats_for_connected_peers(&inner.connected_peers);
        let (inbound_peer_final_state, outbound_peer_final_state) =
            peer_final_state_snapshots(&inner);
        let peer_target_accounting = peer_target_accounting(&inner, now_unix());
        Ok(P2pStatus {
            chain_id: inner.chain_id.clone(),
            mode: inner.mode.clone(),
            peer_id: inner.peer_id.clone(),
            listening: inner.listening.clone(),
            connected_peers: inner.connected_peers.clone(),
            topics: inner.topics.clone(),
            mdns: inner.mdns,
            kademlia: inner.kademlia,
            broadcasted_messages: inner.broadcasted_messages,
            publish_attempts: inner.publish_attempts,
            seen_message_ids: inner.seen_message_ids.len(),
            queued_messages: inner.queued_messages,
            queued_block_messages: inner.queued_block_messages,
            queued_non_block_messages: inner.queued_non_block_messages,
            queue_max_depth: inner.queue_max_depth,
            dequeued_block_messages: inner.dequeued_block_messages,
            dequeued_non_block_messages: inner.dequeued_non_block_messages,
            queue_block_priority_picks: inner.queue_block_priority_picks,
            queue_priority_tx_lane_picks: inner.queue_priority_tx_lane_picks,
            queue_standard_tx_lane_picks: inner.queue_standard_tx_lane_picks,
            queue_non_block_fair_picks: inner.queue_non_block_fair_picks,
            queue_starvation_relief_picks: inner.queue_starvation_relief_picks,
            queue_backpressure_drops: inner.queue_backpressure_drops,
            inbound_messages: inner.inbound_messages,
            runtime_started: inner.runtime_started,
            runtime_mode_detail: inner.runtime_mode_detail.clone(),
            swarm_events_seen: inner.swarm_events_seen,
            subscriptions_active: inner.subscriptions_active,
            last_message_kind: inner.last_message_kind.clone(),
            last_swarm_event: inner.last_swarm_event.clone(),
            per_topic_publishes: inner.per_topic_publishes.clone(),
            inbound_decode_failed: inner.inbound_decode_failed,
            inbound_chain_mismatch_dropped: inner.inbound_chain_mismatch_dropped,
            inbound_duplicates_suppressed: inner.inbound_duplicates_suppressed,
            outbound_duplicates_suppressed: inner.outbound_duplicates_suppressed,
            inv_blocks_received: inner.inv_blocks_received,
            inv_hashes_known: inner.inv_hashes_known,
            inv_hashes_requested: inner.inv_hashes_requested,
            header_requests_received: inner.header_requests_received,
            header_requests_sent: inner.header_requests_sent,
            headers_received: inner.headers_received,
            headers_sent: inner.headers_sent,
            headers_announced: inner.headers_announced,
            dependency_fetches_scheduled: inner.dependency_fetches_scheduled,
            parent_first_fetches: inner.parent_first_fetches,
            relay_loop_prevented: inner.relay_loop_prevented,
            seen_cache_ttl_secs: MESSAGE_DEDUP_WINDOW_SECS,
            recovery_rebroadcast_ttl_secs: RECOVERY_REBROADCAST_GRACE_SECS,
            max_inventory_length: MAX_INV_BLOCK_HASHES,
            max_request_fanout: MAX_INV_BLOCK_REQUEST_FANOUT,
            tx_inbound_received: inner.tx_inbound_received,
            tx_inbound_accepted: inner.tx_inbound_accepted,
            tx_inbound_duplicate: inner.tx_inbound_duplicate,
            tx_inbound_invalid: inner.tx_inbound_invalid,
            tx_relayed: inner
                .tx_outbound_first_seen_relayed
                .saturating_add(inner.tx_outbound_recovery_relayed),
            tx_relay_suppressed_budget: inner.tx_outbound_budget_suppressed,
            tx_relay_suppressed_duplicate: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_duplicates_suppressed: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_first_seen_relayed: inner.tx_outbound_first_seen_relayed,
            tx_outbound_recovery_relayed: inner.tx_outbound_recovery_relayed,
            tx_outbound_priority_relayed: inner.tx_outbound_priority_relayed,
            tx_outbound_budget_suppressed: inner.tx_outbound_budget_suppressed,
            tx_outbound_recovery_budget_suppressed: inner.tx_outbound_recovery_budget_suppressed,
            block_outbound_duplicates_suppressed: inner.block_outbound_duplicates_suppressed,
            block_outbound_first_seen_relayed: inner.block_outbound_first_seen_relayed,
            block_outbound_recovery_relayed: inner.block_outbound_recovery_relayed,
            last_drop_reason: inner.last_drop_reason.clone(),
            peer_reconnect_attempts: inner.peer_reconnect_attempts,
            peer_recovery_success_count: inner.peer_recovery_success_count,
            last_peer_recovery_unix: inner.last_peer_recovery_unix,
            peer_cooldown_suppressed_count: inner.peer_cooldown_suppressed_count,
            peer_flap_suppressed_count: inner.peer_flap_suppressed_count,
            peer_message_rate_limited_count: inner.peer_message_rate_limited_count,
            peer_effective_count: inner.connected_peers.len().max(peer_sync_eligible_total),
            peer_min_target_missed_total: inner.peer_min_target_missed_total,
            peer_min_target_reconnect_attempt_total: inner.peer_min_target_reconnect_attempt_total,
            peer_min_target_reconnect_success_total: inner.peer_min_target_reconnect_success_total,
            peer_below_target_duration_seconds: inner
                .peer_below_target_since_unix
                .map(|since| now_unix().saturating_sub(since))
                .unwrap_or(0),
            peer_below_target_blocked_reason: inner.peer_below_target_blocked_reason.clone(),
            peer_known_connected_total: peer_target_accounting.connected,
            peer_known_disconnected_total: peer_target_accounting.disconnected,
            peer_known_cooldown_total: peer_target_accounting.cooldown,
            peer_known_rate_limited_total: peer_target_accounting.rate_limited,
            peer_known_dialable_total: peer_target_accounting.dialable,
            peer_recovery_state: peer_recovery_state(&inner),
            peer_cooldown_bypassed_for_connectivity_total: inner
                .peer_cooldown_bypassed_for_connectivity_total,
            peer_rate_limit_recovery_suppressed_total: inner
                .peer_rate_limit_recovery_suppressed_total,
            peer_rate_limit_by_kind_total: inner.peer_rate_limit_by_kind_total.clone(),
            peer_suppressed_dial_count: inner.peer_suppressed_dial_count,
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_lifecycle_healthy,
            peer_lifecycle_watch,
            peer_lifecycle_degraded,
            peer_lifecycle_cooldown,
            peer_lifecycle_recovering,
            peer_retention_active_total,
            peer_retention_recovering_total,
            peer_retention_cooldown_total,
            peer_sync_eligible_total,
            peer_sync_suppressed_total,
            degraded_mode,
            connection_shaping_active: mode_connected_peers_are_real_network(&inner.mode),
            peer_recovery,
            sync_candidates,
            selected_sync_peer,
            connection_slot_budget: inner.connection_slot_budget,
            connected_slots_in_use,
            available_connection_slots,
            sync_selection_sticky_until_unix: (inner.sync_selection_sticky_until_unix > 0)
                .then_some(inner.sync_selection_sticky_until_unix),
            topology_bucket_count,
            topology_distinct_buckets,
            topology_dominant_bucket_share_bps,
            topology_diversity_score_bps,
            blocks_requested: inner.blocks_requested,
            blocks_received: inner.blocks_received,
            invalid_blocks_received: inner.invalid_blocks_received,
            orphan_blocks_received: inner.orphan_blocks_received,
            duplicate_blocks_received: inner.duplicate_blocks_received,
            peer_penalties: inner.peer_penalties,
            active_connections_by_peer: inner.active_connections.clone(),
            active_connection_total: inner.active_connections.values().copied().sum(),
            last_connection_established_peer: inner.last_connection_established_peer.clone(),
            last_connection_closed_peer: inner.last_connection_closed_peer.clone(),
            last_connection_closed_remaining_count: inner.last_connection_closed_remaining_count,
            last_outgoing_connection_error_peer: inner.last_outgoing_connection_error_peer.clone(),
            last_incoming_connection_error_peer: inner.last_incoming_connection_error_peer.clone(),
            last_dial_error: inner.last_dial_error.clone(),
            last_disconnect_reason: inner.last_disconnect_reason.clone(),
            last_peer_state_transition: inner.last_peer_state_transition.clone(),
            bootstrap_dial_attempts: inner.bootstrap_dial_attempts,
            bootstrap_dial_successes: inner.bootstrap_dial_successes,
            bootstrap_dial_failures: inner.bootstrap_dial_failures,
            bootstrap_connected_peer_ids: inner.bootstrap_connected_peer_ids.clone(),
            bootnodes_configured: inner.bootnodes_configured.clone(),
            bootnodes_connected: inner
                .bootnodes_configured
                .iter()
                .filter_map(|addr| parse_bootnode_multiaddr(addr).map(|(peer, _)| peer.to_string()))
                .filter(|peer| inner.active_connections.get(peer).copied().unwrap_or(0) > 0)
                .collect(),
            pending_bootnode_dials: inner.pending_bootnode_dials.iter().cloned().collect(),
            bootnode_redial_attempts: inner.bootnode_redial_attempts,
            bootnode_redial_successes: inner.bootnode_redial_successes,
            bootnode_redial_failures: inner.bootnode_redial_failures,
            bootnode_reconnect_scheduled_total: inner.bootnode_reconnect_scheduled_total,
            bootnode_reconnect_skipped_cooldown_total: inner
                .bootnode_reconnect_skipped_cooldown_total,
            bootnode_reconnect_forced_from_cooldown_total: inner
                .bootnode_reconnect_forced_from_cooldown_total,
            bootnode_reconnect_success_total: inner.bootnode_reconnect_success_total,
            isolated_bootnode_reconnect_active: isolated_bootnode_reconnect_active(&inner),
            peer_zero_count_duration_seconds: inner
                .peer_zero_since_unix
                .map(|since| now_unix().saturating_sub(since))
                .unwrap_or(0),
            peer_zero_reconnect_attempt_total: inner.peer_zero_reconnect_attempt_total,
            peer_zero_reconnect_success_total: inner.peer_zero_reconnect_success_total,
            peer_reconnect_suppressed_by_cooldown_total: inner
                .peer_reconnect_suppressed_by_cooldown_total,
            peer_reconnect_suppressed_by_rate_limit_total: inner
                .peer_reconnect_suppressed_by_rate_limit_total,
            peer_min_target_recovered_total: inner.peer_min_target_recovered_total,
            last_peer_reconnect_blocked_reason: inner.last_peer_reconnect_blocked_reason.clone(),
            bootnode_next_redial_at: inner.bootnode_next_redial_at.clone(),
            bootnode_redial_backoff_secs: inner.bootnode_redial_backoff_secs.clone(),
            last_bootnode_dial_error: inner.last_bootnode_dial_error.clone(),
            gossipsub_peer_count: inner.active_connections.len(),
            subscribed_topics: inner.topics.clone(),
            connection_established_total: inner.connection_established_total,
            connection_closed_total: inner.connection_closed_total,
            last_connection_closed_reason: inner.last_connection_closed_reason.clone(),
            disconnect_reason_counts: inner.disconnect_reason_counts.clone(),
            peer_lifecycle_event_counters: inner.peer_lifecycle_event_counters.clone(),
            last_error_by_peer: inner.last_error_by_peer.clone(),
            inbound_peer_final_state,
            outbound_peer_final_state,
        })
    }
}

pub fn build_p2p_stack(mode: P2pMode) -> Result<P2pStack, PulseError> {
    match mode {
        P2pMode::Memory { chain_id, peers } => {
            let (handle, inbound_rx) = MemoryP2pHandle::new(chain_id, peers);
            Ok(P2pStack {
                handle: Arc::new(handle),
                inbound_rx: Some(inbound_rx),
            })
        }
        P2pMode::Libp2p(cfg) => {
            let (handle, inbound_rx) = Libp2pHandle::new(cfg)?;
            Ok(P2pStack {
                handle: Arc::new(handle),
                inbound_rx: Some(inbound_rx),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    fn peers_for_bucket(bucket: usize, count: usize) -> Vec<String> {
        let mut peers = Vec::new();
        let mut idx = 0usize;
        while peers.len() < count {
            let candidate = format!("bucket-{bucket}-peer-{idx}");
            if topology_bucket_for_peer(&candidate) == bucket {
                peers.push(candidate);
            }
            idx = idx.saturating_add(1);
        }
        peers
    }

    fn sample_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee: 10,
            nonce: 1,
        }
    }

    fn sample_tx_with_fee(txid: &str, fee: u64) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee,
            nonce: 1,
        }
    }

    fn sample_block(hash: &str, height: usize) -> Block {
        Block {
            hash: hash.into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: height as u64,
            },
            transactions: vec![],
        }
    }

    fn relay_outbound_tx_for_test(state: &mut InnerState, id: &str, now: u64) -> bool {
        if !should_relay_outbound_tx(state, id, now) {
            return false;
        }
        record_outbound_tx_relay(state, id, now);
        true
    }

    #[test]
    fn peer_failures_increase_backoff_and_lower_score() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        register_peer_result_at(&state, "peer-a", false, 1_000);
        let first = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();
        register_peer_result_at(&state, "peer-a", false, 1_001);
        let second = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();

        assert!(second.next_retry_unix >= first.next_retry_unix);
        assert!(second.score < first.score);
        assert!(!second.connected);
    }

    #[test]
    fn peer_backoff_is_bounded_for_repeated_failures() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        for attempt in 0..20 {
            register_peer_result_at(&state, "peer-a", false, 10_000 + attempt);
        }

        let health = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();
        let delay = health.next_retry_unix.saturating_sub(10_000 + 19);
        assert!(delay <= BACKOFF_MAX_SECS + 2);
    }

    #[test]
    fn peer_success_recovers_score_and_connectivity() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        register_peer_result(&state, "peer-a", false);
        register_peer_result(&state, "peer-a", true);
        let health = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();

        assert!(health.connected);
        assert_eq!(health.fail_streak, 0);
        assert!(health.score > 80);
    }

    #[test]
    fn sync_candidates_prefer_healthy_peers_without_starving_them() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.peer_book.insert(
            "healthy-peer".to_string(),
            PeerHealth {
                score: 110,
                connected: true,
                fail_streak: 0,
                next_retry_unix: 0,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "noisy-peer".to_string(),
            PeerHealth {
                score: 70,
                connected: false,
                fail_streak: 5,
                next_retry_unix: now_unix().saturating_add(120),
                recent_failures_unix: vec![now_unix().saturating_sub(1); 4],
                ..PeerHealth::default()
            },
        );

        let ranked = sync_candidates_snapshot(&state);
        assert_eq!(
            ranked.first().map(|p| p.peer_id.as_str()),
            Some("healthy-peer")
        );
        assert!(ranked
            .iter()
            .find(|p| p.peer_id == "noisy-peer")
            .and_then(|p| p.excluded_until_unix)
            .is_some());
    }

    #[test]
    fn memory_mode_tracks_publish_metrics_by_topic() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let tx = sample_tx("tx-1");
        let block = Block {
            hash: "block-1".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![tx.clone()],
        };

        handle
            .broadcast_transaction(&tx)
            .expect("memory mode tx broadcast should succeed");
        handle
            .broadcast_block(&block)
            .expect("memory mode block broadcast should succeed");
        let status = handle
            .status()
            .expect("memory mode status should be available");

        assert_eq!(status.mode, P2P_MODE_MEMORY_SIMULATED);
        assert_eq!(status.publish_attempts, 2);
        assert_eq!(status.broadcasted_messages, 2);
        assert_eq!(status.seen_message_ids, 2);
        assert_eq!(status.per_topic_publishes.get("memory-txs"), Some(&1));
        assert_eq!(status.per_topic_publishes.get("memory-blocks"), Some(&1));
        assert_eq!(status.peer_reconnect_attempts, 0);
        assert_eq!(status.peer_recovery_success_count, 0);
    }

    #[test]
    fn chain_id_mismatch_drops_inbound_tx() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "wrongnet".into(),
            transaction: sample_tx("tx-wrong-chain"),
        })
        .expect("serialize tx announcement");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_chain_mismatch_dropped, 1);
        assert_eq!(guard.tx_inbound_received, 0);
    }

    #[test]
    fn oversized_inbound_tx_message_is_rejected() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let mut tx = sample_tx("tx-too-large");
        tx.outputs.push(pulsedag_core::types::TxOutput {
            address: "a".repeat(MAX_TX_MESSAGE_BYTES),
            amount: 1,
        });
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: tx,
        })
        .expect("serialize tx announcement");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.tx_inbound_received, 1);
        assert_eq!(guard.tx_inbound_invalid, 1);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("tx_message_too_large")
        );
    }

    #[test]
    fn inbound_tx_rate_guard_suppresses_spam() {
        let mut state = InnerState::default();
        let now = 1_000;

        for _ in 0..TX_INBOUND_SOFT_MAX_PER_WINDOW {
            assert!(admit_inbound_tx_rate(&mut state, now));
        }
        assert!(!admit_inbound_tx_rate(&mut state, now));
        assert_eq!(
            state.tx_inbound_rate_window_count,
            TX_INBOUND_SOFT_MAX_PER_WINDOW
        );
    }

    #[test]
    fn duplicate_tx_announcements_are_suppressed() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let tx = sample_tx("tx-dup-announcement");
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: tx,
        })
        .expect("serialize tx announcement");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Transaction(_))
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_messages, 1);
        assert_eq!(guard.inbound_duplicates_suppressed, 1);
    }

    #[test]
    fn outbound_tx_first_seen_relay_still_occurs() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let tx = sample_tx("tx-first-seen");

        handle
            .broadcast_transaction(&tx)
            .expect("first-time relay should succeed");

        let status = handle.status().expect("status should be available");
        assert_eq!(status.publish_attempts, 1);
        assert_eq!(status.broadcasted_messages, 1);
        assert_eq!(status.tx_outbound_first_seen_relayed, 1);
        assert_eq!(status.tx_outbound_duplicates_suppressed, 0);
    }

    #[test]
    fn repeated_tx_relay_storm_is_deduped_without_counter_inflation() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let tx = sample_tx("tx-storm");

        for _ in 0..10 {
            handle
                .broadcast_transaction(&tx)
                .expect("duplicate relay should not error");
        }

        let status = handle.status().expect("status should be available");
        assert_eq!(status.publish_attempts, 1);
        assert_eq!(status.broadcasted_messages, 1);
        assert_eq!(status.tx_outbound_first_seen_relayed, 1);
        assert_eq!(status.tx_outbound_duplicates_suppressed, 9);
    }

    #[test]
    fn higher_priority_tx_relay_bypasses_budget_pressure() {
        let mut state = InnerState::default();
        state.queued_messages = TX_BUDGET_LOAD_SHED_QUEUE_DEPTH_THRESHOLD;
        let now = 2_000;

        for idx in 0..TX_RELAY_BUDGET_PER_WINDOW {
            let tx_id = format!("tx-fill-{idx}");
            assert!(should_relay_outbound_tx(&mut state, &tx_id, now));
            assert!(admit_tx_relay_under_budget(&mut state, &tx_id, 1, now));
            record_outbound_tx_relay(&mut state, &tx_id, now);
        }

        let overflow_id = "tx-budget-overflow";
        assert!(should_relay_outbound_tx(&mut state, overflow_id, now));
        assert!(!admit_tx_relay_under_budget(
            &mut state,
            overflow_id,
            1,
            now
        ));

        let priority_id = "tx-priority";
        assert!(should_relay_outbound_tx(&mut state, priority_id, now));
        assert!(admit_tx_relay_under_budget(
            &mut state,
            priority_id,
            TX_PRIORITY_FEE_THRESHOLD,
            now,
        ));
        record_outbound_tx_relay(&mut state, priority_id, now);

        assert_eq!(
            state.tx_outbound_first_seen_relayed,
            TX_RELAY_BUDGET_PER_WINDOW + 1
        );
        assert_eq!(state.tx_outbound_priority_relayed, 1);
        assert_eq!(state.tx_outbound_budget_suppressed, 1);
    }

    #[test]
    fn tx_relay_budget_preserves_normal_traffic_levels() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let normal_traffic_count = TX_RELAY_BUDGET_PER_WINDOW / 4;

        for idx in 0..normal_traffic_count {
            handle
                .broadcast_transaction(&sample_tx_with_fee(&format!("tx-normal-{idx}"), 5))
                .expect("normal relay should not error");
        }

        let status = handle.status().expect("status should be available");
        assert_eq!(status.publish_attempts, normal_traffic_count);
        assert_eq!(status.tx_outbound_budget_suppressed, 0);
        assert_eq!(status.tx_outbound_first_seen_relayed, normal_traffic_count);
    }

    #[test]
    fn budget_suppressed_tx_is_not_marked_as_outbound_duplicate() {
        let mut state = InnerState::default();
        state.tx_budget_window_started_unix = 1_000;
        state.tx_budget_window_relays = TX_RELAY_BUDGET_PER_WINDOW;
        state.queued_messages = TX_BUDGET_LOAD_SHED_QUEUE_DEPTH_THRESHOLD;
        let tx_id = (0..1_000)
            .map(|idx| format!("tx-budget-suppressed-first-attempt-{idx}"))
            .find(|id| !message_id_hash(id).is_multiple_of(TX_RELAY_BUDGET_OVERFLOW_SAMPLE_EVERY))
            .expect("should find a tx id that does not pass overflow sampling");

        assert!(should_relay_outbound_tx(&mut state, &tx_id, 1_000));
        assert!(!admit_tx_relay_under_budget(&mut state, &tx_id, 1, 1_000));
        assert!(should_relay_outbound_tx(&mut state, &tx_id, 1_001));
        assert_eq!(state.tx_outbound_first_seen_relayed, 0);
        assert_eq!(state.tx_outbound_duplicates_suppressed, 0);
    }

    #[test]
    fn repeated_block_relay_storm_is_deduped_without_counter_inflation() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let block = Block {
            hash: "block-storm".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec!["genesis".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![sample_tx("tx-for-block-storm")],
        };

        for _ in 0..10 {
            handle
                .broadcast_block(&block)
                .expect("duplicate block relay should not error");
        }

        let status = handle.status().expect("status should be available");
        assert_eq!(status.publish_attempts, 1);
        assert_eq!(status.broadcasted_messages, 1);
        assert_eq!(status.block_outbound_first_seen_relayed, 1);
        assert_eq!(status.block_outbound_duplicates_suppressed, 9);
        assert_eq!(status.block_outbound_recovery_relayed, 0);
    }

    #[test]
    fn outbound_dedup_is_windowed_and_allows_restart_relay() {
        let mut state = InnerState::default();
        assert!(relay_outbound_tx_for_test(&mut state, "tx-windowed", 1_000));
        assert!(!relay_outbound_tx_for_test(
            &mut state,
            "tx-windowed",
            1_010
        ));
        assert!(relay_outbound_tx_for_test(
            &mut state,
            "tx-windowed",
            1_000 + TX_OUTBOUND_DEDUP_WINDOW_SECS + 1
        ));

        let (handle_a, _inbound_rx_a) =
            MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let tx = sample_tx("tx-after-restart");
        handle_a
            .broadcast_transaction(&tx)
            .expect("first handle relay should succeed");
        let status_a = handle_a.status().expect("status should be available");
        assert_eq!(status_a.publish_attempts, 1);

        let (handle_b, _inbound_rx_b) =
            MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        handle_b
            .broadcast_transaction(&tx)
            .expect("post-restart relay should not be suppressed");
        let status_b = handle_b.status().expect("status should be available");
        assert_eq!(status_b.publish_attempts, 1);
    }

    #[test]
    fn recovery_rebroadcast_allows_one_duplicate_per_rejoin_event() {
        let mut state = InnerState::default();
        assert!(relay_outbound_tx_for_test(&mut state, "tx-recovery", 1_000));
        assert!(should_relay_outbound_block(
            &mut state,
            "block-recovery",
            1_000
        ));
        assert!(!relay_outbound_tx_for_test(
            &mut state,
            "tx-recovery",
            1_001
        ));
        assert!(!should_relay_outbound_block(
            &mut state,
            "block-recovery",
            1_001
        ));

        let shared = Arc::new(Mutex::new(state));
        register_peer_result_at(&shared, "peer-a", true, 1_002);

        let mut guard = shared.lock().unwrap();
        assert!(relay_outbound_tx_for_test(&mut guard, "tx-recovery", 1_003));
        assert!(should_relay_outbound_block(
            &mut guard,
            "block-recovery",
            1_003
        ));
        assert!(!relay_outbound_tx_for_test(
            &mut guard,
            "tx-recovery",
            1_004
        ));
        assert!(!should_relay_outbound_block(
            &mut guard,
            "block-recovery",
            1_004
        ));
        assert_eq!(guard.tx_outbound_recovery_relayed, 1);
        assert_eq!(guard.block_outbound_recovery_relayed, 1);
    }

    #[test]
    fn churn_rejoin_recovery_relay_rearms_and_then_reconverges() {
        let shared = Arc::new(Mutex::new(InnerState::default()));
        {
            let mut guard = shared.lock().unwrap();
            assert!(relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_000));
            assert!(!relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_001));
        }

        register_peer_result_at(&shared, "peer-a", false, 2_010);
        register_peer_result_at(&shared, "peer-a", true, 2_020);
        {
            let mut guard = shared.lock().unwrap();
            assert!(relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_021));
            assert!(!relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_022));
        }

        register_peer_result_at(&shared, "peer-a", false, 2_030);
        register_peer_result_at(&shared, "peer-a", true, 2_040);
        let mut guard = shared.lock().unwrap();
        assert!(relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_041));
        assert!(!relay_outbound_tx_for_test(&mut guard, "tx-churn", 2_042));
        assert_eq!(guard.tx_outbound_recovery_relayed, 2);
        assert!(guard.tx_outbound_duplicates_suppressed >= 3);
    }

    #[test]
    fn connected_peers_truth_flag_is_mode_dependent() {
        assert!(!mode_connected_peers_are_real_network(
            P2P_MODE_MEMORY_SIMULATED
        ));
        assert!(!mode_connected_peers_are_real_network(
            P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON
        ));
        assert!(mode_connected_peers_are_real_network(P2P_MODE_LIBP2P_REAL));
    }

    #[test]
    fn connected_peer_semantics_label_is_mode_dependent() {
        assert_eq!(
            connected_peers_semantics(P2P_MODE_MEMORY_SIMULATED),
            "simulated-or-internal-peer-observations"
        );
        assert_eq!(
            connected_peers_semantics(P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON),
            "simulated-or-internal-peer-observations"
        );
        assert_eq!(
            connected_peers_semantics(P2P_MODE_LIBP2P_REAL),
            "real-network-connected-peers"
        );
    }

    #[test]
    fn block_messages_are_prioritized_over_non_block_messages() {
        let inner = Arc::new(Mutex::new(InnerState {
            queued_messages: 4,
            queued_block_messages: 1,
            queued_non_block_messages: 3,
            ..Default::default()
        }));
        let mut queue = OutboundPriorityQueue::default();
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx("tx-a")),
        );
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx("tx-b")),
        );
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Block(Block {
                hash: "block-priority".into(),
                header: pulsedag_core::types::BlockHeader {
                    version: 1,
                    parents: vec![],
                    timestamp: 1,
                    difficulty: 1,
                    nonce: 1,
                    merkle_root: "mr".into(),
                    state_root: "sr".into(),
                    blue_score: 1,
                    height: 1,
                },
                transactions: vec![],
            }),
        );
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx("tx-c")),
        );

        let first = pop_outbound_message(&inner, &mut queue).expect("must pop");
        assert!(matches!(first, OutboundMessage::Block(_)));
    }

    #[test]
    fn mixed_bursty_queue_avoids_non_block_starvation() {
        let inner = Arc::new(Mutex::new(InnerState {
            queued_messages: 18,
            queued_block_messages: 16,
            queued_non_block_messages: 2,
            ..Default::default()
        }));
        let mut queue = OutboundPriorityQueue::default();
        for idx in 0..16 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Block(Block {
                    hash: format!("block-{idx}"),
                    header: pulsedag_core::types::BlockHeader {
                        version: 1,
                        parents: vec![],
                        timestamp: 1,
                        difficulty: 1,
                        nonce: 1,
                        merkle_root: "mr".into(),
                        state_root: "sr".into(),
                        blue_score: 1,
                        height: idx + 1,
                    },
                    transactions: vec![],
                }),
            );
        }
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx("tx-1")),
        );
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx("tx-2")),
        );

        let mut tx_seen = 0;
        for _ in 0..10 {
            let msg = pop_outbound_message(&inner, &mut queue).expect("message exists");
            if matches!(msg, OutboundMessage::Transaction(_)) {
                tx_seen += 1;
            }
        }
        assert!(tx_seen >= 1);
        let status = inner.lock().unwrap();
        assert!(status.queue_starvation_relief_picks >= 1);
    }

    #[test]
    fn relay_lanes_keep_priority_txs_moving_without_starving_standard_lane() {
        let inner = Arc::new(Mutex::new(InnerState {
            queued_messages: 12,
            queued_block_messages: 8,
            queued_non_block_messages: 4,
            ..Default::default()
        }));
        let mut queue = OutboundPriorityQueue::default();
        for idx in 0..8 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Block(Block {
                    hash: format!("lane-block-{idx}"),
                    header: pulsedag_core::types::BlockHeader {
                        version: 1,
                        parents: vec![],
                        timestamp: 1,
                        difficulty: 1,
                        nonce: 1,
                        merkle_root: "mr".into(),
                        state_root: "sr".into(),
                        blue_score: 1,
                        height: idx + 1,
                    },
                    transactions: vec![],
                }),
            );
        }
        for idx in 0..3 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Transaction(sample_tx_with_fee(
                    &format!("lane-priority-{idx}"),
                    TX_PRIORITY_FEE_THRESHOLD,
                )),
            );
        }
        enqueue_outbound_message(
            &inner,
            &mut queue,
            OutboundMessage::Transaction(sample_tx_with_fee("lane-standard", 1)),
        );

        let mut saw_standard = false;
        let mut priority_picks = 0;
        for _ in 0..12 {
            if let Some(OutboundMessage::Transaction(tx)) = pop_outbound_message(&inner, &mut queue)
            {
                if tx.fee >= TX_PRIORITY_FEE_THRESHOLD {
                    priority_picks += 1;
                } else {
                    saw_standard = true;
                }
            }
        }
        assert!(saw_standard);
        assert!(priority_picks >= 2);
        let status = inner.lock().unwrap();
        assert!(status.queue_priority_tx_lane_picks >= 2);
        assert!(status.queue_standard_tx_lane_picks >= 1);
    }

    #[test]
    fn recovery_rebroadcast_budget_is_bounded_during_rejoin_storms() {
        let mut state = InnerState::default();
        state.recovery_rebroadcast_generation = 1;
        state.recovery_rebroadcast_until_unix = 1_200;
        state.recovery_rebroadcast_budget_window_started_unix = 1_000;
        state.recovery_rebroadcast_budget_used = RECOVERY_REBROADCAST_BUDGET_PER_WINDOW;
        state
            .outbound_tx_seen_at_unix
            .insert("tx-recovery-budget".into(), 1_005);

        assert!(!relay_outbound_tx_for_test(
            &mut state,
            "tx-recovery-budget",
            1_006
        ));
        assert_eq!(state.tx_outbound_recovery_budget_suppressed, 1);
        assert_eq!(state.tx_outbound_recovery_relayed, 0);
    }

    #[test]
    fn anti_spam_budget_only_activates_under_load_and_preserves_healthy_flow() {
        let mut state = InnerState::default();
        state.tx_budget_window_started_unix = 1_000;
        state.tx_budget_window_relays = TX_RELAY_BUDGET_PER_WINDOW;

        assert!(admit_tx_relay_under_budget(
            &mut state,
            "tx-healthy-not-loaded",
            1,
            1_000
        ));
        assert_eq!(state.tx_outbound_budget_suppressed, 0);

        state.queued_messages = TX_BUDGET_LOAD_SHED_QUEUE_DEPTH_THRESHOLD;
        let blocked_id = (0..1_000)
            .map(|idx| format!("tx-loaded-budget-suppressed-{idx}"))
            .find(|id| !message_id_hash(id).is_multiple_of(TX_RELAY_BUDGET_OVERFLOW_SAMPLE_EVERY))
            .expect("need deterministic blocked id");
        assert!(!admit_tx_relay_under_budget(
            &mut state,
            &blocked_id,
            1,
            1_000
        ));
        assert_eq!(state.tx_outbound_budget_suppressed, 1);
    }

    #[test]
    fn outbound_backpressure_is_bounded_and_explicit() {
        let mut state = InnerState {
            queued_messages: OUTBOUND_QUEUE_SOFT_CAP,
            ..Default::default()
        };
        assert!(queue_backpressure_reject(
            &mut state,
            "outbound_queue_backpressure_tx"
        ));
        assert_eq!(state.queue_backpressure_drops, 1);
        assert_eq!(
            state.last_drop_reason.as_deref(),
            Some("outbound_queue_backpressure_tx")
        );
    }

    #[test]
    fn burst_heavy_mix_preserves_standard_lane_fairness() {
        let inner = Arc::new(Mutex::new(InnerState {
            queued_messages: 40,
            queued_block_messages: 30,
            queued_non_block_messages: 10,
            ..Default::default()
        }));
        let mut queue = OutboundPriorityQueue::default();
        for idx in 0..30 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Block(sample_block(&format!("burst-block-{idx}"), idx + 1)),
            );
        }
        for idx in 0..8 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Transaction(sample_tx_with_fee(
                    &format!("burst-prio-{idx}"),
                    TX_PRIORITY_FEE_THRESHOLD,
                )),
            );
        }
        for idx in 0..2 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Transaction(sample_tx_with_fee(&format!("burst-std-{idx}"), 1)),
            );
        }

        let mut std_picks = 0;
        for _ in 0..40 {
            if let Some(OutboundMessage::Transaction(tx)) = pop_outbound_message(&inner, &mut queue)
            {
                if tx.fee < TX_PRIORITY_FEE_THRESHOLD {
                    std_picks += 1;
                }
            }
        }
        assert!(std_picks >= 1);
    }

    #[test]
    fn tx_recovery_credit_drains_after_burst_and_reconverges() {
        let inner = Arc::new(Mutex::new(InnerState {
            queued_messages: 12,
            queued_block_messages: 8,
            queued_non_block_messages: 4,
            ..Default::default()
        }));
        let mut queue = OutboundPriorityQueue::default();
        for idx in 0..8 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Block(sample_block(&format!("recovery-block-{idx}"), idx + 1)),
            );
        }
        for idx in 0..4 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Transaction(sample_tx_with_fee(&format!("recovery-tx-{idx}"), 1)),
            );
        }
        for _ in 0..12 {
            let _ = pop_outbound_message(&inner, &mut queue);
        }
        assert_eq!(queue.tx_recovery_credit, 0);
    }

    #[test]
    fn queue_counters_remain_coherent_through_drain() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        {
            let mut guard = inner.lock().unwrap();
            guard.queued_messages = 5;
            guard.queued_block_messages = 2;
            guard.queued_non_block_messages = 3;
            guard.queue_max_depth = 5;
        }

        let mut queue = OutboundPriorityQueue::default();
        for idx in 0..2 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Block(Block {
                    hash: format!("block-c-{idx}"),
                    header: pulsedag_core::types::BlockHeader {
                        version: 1,
                        parents: vec![],
                        timestamp: 1,
                        difficulty: 1,
                        nonce: 1,
                        merkle_root: "mr".into(),
                        state_root: "sr".into(),
                        blue_score: 1,
                        height: idx + 1,
                    },
                    transactions: vec![],
                }),
            );
        }
        for idx in 0..3 {
            enqueue_outbound_message(
                &inner,
                &mut queue,
                OutboundMessage::Transaction(sample_tx(&format!("tx-c-{idx}"))),
            );
        }

        for _ in 0..5 {
            let _ = pop_outbound_message(&inner, &mut queue);
        }
        let status = inner.lock().unwrap();
        assert_eq!(status.queued_messages, 0);
        assert_eq!(status.queued_block_messages, 0);
        assert_eq!(status.queued_non_block_messages, 0);
        assert_eq!(
            status.dequeued_block_messages + status.dequeued_non_block_messages,
            5
        );
    }

    fn peer_state_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("peer state env lock poisoned")
    }

    fn unique_peer_state_path(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "pulsedag-peer-state-{prefix}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn restart_rehydrates_peer_health_without_claiming_real_connectivity() {
        let _env_lock = peer_state_env_lock();
        let path = unique_peer_state_path("rehydrate");
        std::env::set_var("PULSEDAG_P2P_PEER_STATE_PATH", &path);

        let now = now_unix();
        let persisted = HashMap::from([(
            "peer-rejoin".to_string(),
            PeerHealth {
                score: 145,
                fail_streak: 0,
                next_retry_unix: now,
                connected: true,
                last_seen_unix: Some(now),
                last_successful_connect_unix: Some(now),
                last_recovery_unix: Some(now),
                recent_failures_unix: vec![now.saturating_sub(10)],
                ..PeerHealth::default()
            },
        )]);
        persist_peer_book(&path, &persisted);

        let bootstrap_key = identity::Keypair::generate_ed25519();
        let bootstrap_peer = PeerId::from(bootstrap_key.public());
        let bootstrap_addr = format!("/ip4/127.0.0.1/tcp/19080/p2p/{bootstrap_peer}");
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/30333".into(),
            bootstrap: vec![bootstrap_addr],
            enable_mdns: false,
            enable_kademlia: false,
            connection_slot_budget: 8,
            sync_selection_stickiness_secs: 30,
            runtime: Libp2pRuntimeMode::DevLoopbackSkeleton,
        };
        let (handle, _rx) = Libp2pHandle::new(cfg).expect("libp2p handle should init");
        let status = handle.status().expect("status should work");

        assert_eq!(status.mode, P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON);
        assert!(status.connected_peers.is_empty());
        assert!(!mode_connected_peers_are_real_network(&status.mode));
        assert_eq!(status.peer_recovery.len(), 2);
        let rejoin = status
            .peer_recovery
            .iter()
            .find(|peer| peer.peer_id == "peer-rejoin")
            .cloned()
            .expect("persisted peer should be surfaced");
        assert_eq!(rejoin.last_seen_unix, Some(now));
        assert_eq!(rejoin.last_successful_connect_unix, Some(now));
        assert_eq!(rejoin.recent_failures_unix.len(), 1);

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn corrupt_peer_metadata_fails_safe_on_startup() {
        let _env_lock = peer_state_env_lock();
        let path = unique_peer_state_path("corrupt");
        std::env::set_var("PULSEDAG_P2P_PEER_STATE_PATH", &path);
        fs::write(&path, b"{ definitely-not-json").expect("write corrupt peer snapshot");

        let bootstrap_key = identity::Keypair::generate_ed25519();
        let bootstrap_peer = PeerId::from(bootstrap_key.public());
        let bootstrap_addr = format!("/ip4/127.0.0.1/tcp/19080/p2p/{bootstrap_peer}");
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/30334".into(),
            bootstrap: vec![bootstrap_addr],
            enable_mdns: false,
            enable_kademlia: false,
            connection_slot_budget: 8,
            sync_selection_stickiness_secs: 30,
            runtime: Libp2pRuntimeMode::DevLoopbackSkeleton,
        };
        let (handle, _rx) = Libp2pHandle::new(cfg).expect("libp2p handle should init");
        let status = handle.status().expect("status should work");
        assert!(status
            .peer_recovery
            .iter()
            .any(|peer| peer.peer_id == bootstrap_peer.to_string()));

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn stale_peer_records_are_dropped_during_load() {
        let now = now_unix();
        let stale = now
            .saturating_sub(PEER_RECORD_MAX_AGE_SECS)
            .saturating_sub(100);
        let fresh = now.saturating_sub(60);
        let loaded = sanitize_loaded_peer_book(
            HashMap::from([
                (
                    "peer-stale".to_string(),
                    PeerHealth {
                        last_seen_unix: Some(stale),
                        ..PeerHealth::default()
                    },
                ),
                (
                    "peer-fresh".to_string(),
                    PeerHealth {
                        last_seen_unix: Some(fresh),
                        connected: true,
                        ..PeerHealth::default()
                    },
                ),
            ]),
            now,
        );
        assert!(!loaded.contains_key("peer-stale"));
        let fresh_peer = loaded.get("peer-fresh").expect("fresh peer should survive");
        assert!(!fresh_peer.connected);
    }

    #[test]
    fn reconnect_uses_loaded_history_and_respects_cooldown() {
        let now = now_unix();
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: sanitize_loaded_peer_book(
                HashMap::from([(
                    "peer-a".to_string(),
                    PeerHealth {
                        next_retry_unix: now.saturating_add(120),
                        last_seen_unix: Some(now.saturating_sub(10)),
                        ..PeerHealth::default()
                    },
                )]),
                now,
            ),
            ..Default::default()
        }));

        register_peer_result_at(&state, "peer-a", false, now.saturating_add(5));
        let guard = state.lock().unwrap();
        let health = guard.peer_book.get("peer-a").cloned().unwrap();
        assert!(health.next_retry_unix >= now.saturating_add(120));
        assert!(health.cooldown_suppressed_count >= 1);
        assert_eq!(health.last_seen_unix, Some(now.saturating_add(5)));
    }

    #[test]
    fn peer_recovery_reduces_backoff_and_increments_metrics() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        register_peer_result_at(&state, "peer-a", false, 1_000);
        let failed = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();

        register_peer_result_at(&state, "peer-a", true, 1_020);
        let guard = state.lock().unwrap();
        let recovered = guard.peer_book.get("peer-a").cloned().unwrap();

        assert!(failed.next_retry_unix > 1_000);
        assert_eq!(recovered.next_retry_unix, 1_020);
        assert_eq!(recovered.recovery_success_count, 1);
        assert_eq!(guard.peer_recovery_success_count, 1);
        assert_eq!(guard.last_peer_recovery_unix, Some(1_020));
    }

    #[test]
    fn repeated_failures_flap_and_cooldown_are_suppressed() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        register_peer_result_at(&state, "peer-a", false, 2_000);
        register_peer_result_at(&state, "peer-a", false, 2_001);
        register_peer_result_at(&state, "peer-a", true, 2_010);
        register_peer_result_at(&state, "peer-a", false, 2_020);
        register_peer_result_at(&state, "peer-a", true, 2_030);
        register_peer_result_at(&state, "peer-a", false, 2_040);

        let guard = state.lock().unwrap();
        let health = guard.peer_book.get("peer-a").cloned().unwrap();
        assert!(guard.peer_cooldown_suppressed_count >= 1);
        assert!(guard.peer_flap_suppressed_count >= 1);
        assert!(health.flap_suppressed_count >= 1);
        assert!(health.suppressed_until_unix >= health.next_retry_unix);
    }

    #[test]
    fn peer_recovery_snapshot_is_sorted_and_stable() {
        let mut state = InnerState::default();
        state.peer_book.insert(
            "peer-b".into(),
            PeerHealth {
                connected: false,
                next_retry_unix: 5_000,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-a".into(),
            PeerHealth {
                connected: true,
                recovery_success_count: 2,
                ..PeerHealth::default()
            },
        );

        let (_cooldown, _flap, _, _, _, _, _, _, _, _, _, _, _, snapshot) =
            peer_recovery_snapshot(&state);
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].peer_id, "peer-a");
        assert_eq!(snapshot[1].peer_id, "peer-b");
        assert_eq!(snapshot[0].recovery_success_count, 2);
    }

    #[test]
    fn peers_transition_coherently_across_lifecycle_tiers() {
        let state = Arc::new(Mutex::new(InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));
        register_peer_result_at(&state, "peer-a", false, 10_000);
        register_peer_result_at(&state, "peer-a", false, 10_010);
        {
            let guard = state.lock().unwrap();
            let health = guard.peer_book.get("peer-a").unwrap();
            assert_eq!(peer_lifecycle_tier(health, 10_011), "cooldown");
            assert_eq!(peer_recovery_tier(health, 10_011), "assisted");
        }
        register_peer_result_at(&state, "peer-a", true, 10_200);
        let guard = state.lock().unwrap();
        let health = guard.peer_book.get("peer-a").unwrap();
        assert_eq!(peer_lifecycle_tier(health, 10_201), "recovering");
        assert_eq!(peer_recovery_tier(health, 10_201), "recovering");
    }

    #[test]
    fn recovering_peer_remains_visible_in_status() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        state.active_connections.insert("peer-recovering".into(), 1);
        state.peer_book.insert(
            "peer-recovering".into(),
            PeerHealth {
                connected: true,
                score: 82,
                fail_streak: 1,
                recent_failures_unix: vec![now_unix().saturating_sub(1)],
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);
        let status = peer_recovery_snapshot(&state).13;
        let peer = status
            .iter()
            .find(|peer| peer.peer_id == "peer-recovering")
            .expect("recovering peer remains in diagnostics");

        assert!(state
            .connected_peers
            .contains(&"peer-recovering".to_string()));
        assert_eq!(peer.lifecycle_tier, "recovering");
        assert!(peer.health_states.iter().any(|state| state == "recovering"));
        assert!(peer.health_states.iter().any(|state| state == "active"));
    }

    #[test]
    fn cooldown_does_not_zero_peer_count_when_connections_exist() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 1;
        state.active_connections.insert("peer-cooling".into(), 1);
        state.peer_book.insert(
            "peer-cooling".into(),
            PeerHealth {
                connected: true,
                score: 25,
                fail_streak: 5,
                next_retry_unix: now_unix().saturating_add(120),
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);

        assert_eq!(state.connected_peers, vec!["peer-cooling".to_string()]);
        assert_eq!(peer_recovery_snapshot(&state).7, 1);
    }

    #[test]
    fn sync_peer_selection_has_fallback_peers() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        let now = now_unix();
        for (peer, retry_offset) in [("peer-a", 60), ("peer-b", 120)] {
            state.active_connections.insert(peer.into(), 1);
            state.peer_book.insert(
                peer.into(),
                PeerHealth {
                    connected: true,
                    score: 70,
                    fail_streak: 2,
                    next_retry_unix: now.saturating_add(retry_offset),
                    ..PeerHealth::default()
                },
            );
        }

        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        let selected = update_selected_sync_peer(&mut state, &ranked, now);

        assert_eq!(state.connected_peers.len(), 2);
        assert!(selected.is_some());
        assert_eq!(peer_recovery_snapshot(&state).10, 2);
    }

    #[test]
    fn degraded_peers_are_cooled_down_without_starving_healthy_peers() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 3;
        state.peer_book.insert(
            "peer-healthy".into(),
            PeerHealth {
                connected: true,
                score: 120,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-degraded-a".into(),
            PeerHealth {
                connected: true,
                score: 40,
                fail_streak: 4,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-degraded-b".into(),
            PeerHealth {
                connected: true,
                score: 30,
                fail_streak: 5,
                ..PeerHealth::default()
            },
        );
        refresh_connected_peers_from_health(&mut state);
        assert_eq!(
            state.connected_peers.first().map(String::as_str),
            Some("peer-healthy")
        );
        assert_eq!(state.connected_peers.len(), 2);
    }

    #[test]
    fn connection_shaping_reduces_churn_loops_under_stress() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        state.sync_selection_stickiness_secs = 30;
        for (peer, score) in [("peer-a", 110), ("peer-b", 108), ("peer-c", 80)] {
            state.peer_book.insert(
                peer.into(),
                PeerHealth {
                    connected: true,
                    score,
                    ..PeerHealth::default()
                },
            );
        }
        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        let first = update_selected_sync_peer(&mut state, &ranked, 20_000).unwrap();
        state.connected_peers = vec!["peer-b".into()];
        let second = update_selected_sync_peer(&mut state, &ranked, 20_010).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn adaptive_budgeting_is_deterministic_within_bounded_conditions() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 6;
        for peer in ["peer-h1", "peer-h2", "peer-h3"] {
            state.peer_book.insert(
                peer.into(),
                PeerHealth {
                    connected: true,
                    score: 100,
                    ..PeerHealth::default()
                },
            );
        }
        for peer in ["peer-d1", "peer-d2", "peer-d3"] {
            state.peer_book.insert(
                peer.into(),
                PeerHealth {
                    connected: true,
                    score: 20,
                    fail_streak: 4,
                    ..PeerHealth::default()
                },
            );
        }
        let first = adaptive_connection_slot_budget(&state, 42_000);
        let second = adaptive_connection_slot_budget(&state, 42_000);
        assert_eq!(first, second);
        assert!((state.connection_slot_budget / 2..=state.connection_slot_budget).contains(&first));
    }

    #[test]
    fn adaptive_pressure_controls_reduce_reconnect_churn_when_degraded() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 8;
        for idx in 0..8 {
            state.peer_book.insert(
                format!("peer-{idx}"),
                PeerHealth {
                    connected: true,
                    score: 30,
                    fail_streak: 5,
                    ..PeerHealth::default()
                },
            );
        }
        let reduced = adaptive_connection_slot_budget(&state, 99_000);
        assert!(reduced < state.connection_slot_budget);
        refresh_connected_peers_from_health(&mut state);
        assert!(state.connected_peers.len() <= reduced);
    }

    #[test]
    fn sync_candidate_selection_deprioritizes_slow_or_degraded_peers() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.peer_book.insert(
            "peer-fast".into(),
            PeerHealth {
                connected: true,
                score: 130,
                recovery_success_count: 2,
                next_retry_unix: 0,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-slow".into(),
            PeerHealth {
                connected: true,
                score: 140,
                fail_streak: 3,
                next_retry_unix: now_unix().saturating_add(120),
                recent_failures_unix: vec![now_unix()],
                ..PeerHealth::default()
            },
        );
        refresh_connected_peers_from_health(&mut state);
        assert_eq!(
            state.connected_peers.first().map(String::as_str),
            Some("peer-fast")
        );
    }

    #[test]
    fn refresh_connected_peers_excludes_disconnected_ranked_candidates() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.peer_book.insert(
            "peer-live".into(),
            PeerHealth {
                connected: true,
                score: 95,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-offline".into(),
            PeerHealth {
                connected: false,
                score: 150,
                next_retry_unix: 0,
                suppressed_until_unix: 0,
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);

        assert_eq!(state.connected_peers, vec!["peer-live".to_string()]);
    }

    #[test]
    fn connection_budget_caps_connected_peer_surface() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        for (id, score) in [("peer-a", 120), ("peer-b", 115), ("peer-c", 110)] {
            state.peer_book.insert(
                id.into(),
                PeerHealth {
                    connected: true,
                    score,
                    ..PeerHealth::default()
                },
            );
        }

        refresh_connected_peers_from_health(&mut state);
        assert_eq!(state.connected_peers.len(), 2);
        assert_eq!(
            state.connected_peers,
            vec!["peer-a".to_string(), "peer-b".to_string()]
        );
    }

    #[test]
    fn topology_diversity_prevents_slot_collapse_when_alternatives_exist() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 4;
        let peers_bucket_0 = peers_for_bucket(0, 6);
        let peers_bucket_1 = peers_for_bucket(1, 3);
        for peer in peers_bucket_0.iter().chain(peers_bucket_1.iter()) {
            state.peer_book.insert(
                peer.clone(),
                PeerHealth {
                    connected: true,
                    score: 120,
                    ..PeerHealth::default()
                },
            );
        }

        refresh_connected_peers_from_health(&mut state);

        let bucket_0_selected = state
            .connected_peers
            .iter()
            .filter(|peer| topology_bucket_for_peer(peer) == 0)
            .count();
        let bucket_1_selected = state
            .connected_peers
            .iter()
            .filter(|peer| topology_bucket_for_peer(peer) == 1)
            .count();
        assert_eq!(state.connected_peers.len(), 4);
        assert!(bucket_0_selected <= 2);
        assert!(bucket_1_selected >= 1);
    }

    #[test]
    fn topology_diversity_metrics_are_bounded_and_deterministic() {
        let peers = vec![
            "peer-alpha".to_string(),
            "peer-beta".to_string(),
            "peer-gamma".to_string(),
            "peer-delta".to_string(),
        ];

        let first = topology_stats_for_connected_peers(&peers);
        let second = topology_stats_for_connected_peers(&peers);

        assert_eq!(first, second);
        assert_eq!(first.0, TOPOLOGY_BUCKET_COUNT);
        assert!(first.1 <= TOPOLOGY_BUCKET_COUNT);
        assert!(first.2 <= 10_000);
        assert!(first.3 <= 10_000);
    }

    #[test]
    fn topology_aware_shaping_still_respects_health_and_budget_constraints() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        state.peer_book.insert(
            "healthy-a".into(),
            PeerHealth {
                connected: true,
                score: 130,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "healthy-b".into(),
            PeerHealth {
                connected: true,
                score: 120,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "suppressed-high-score".into(),
            PeerHealth {
                connected: true,
                score: 200,
                suppressed_until_unix: u64::MAX,
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        let selected = update_selected_sync_peer(&mut state, &ranked, 42).unwrap();

        assert_eq!(state.connected_peers.len(), 2);
        assert!(!state
            .connected_peers
            .iter()
            .any(|p| p == "suppressed-high-score"));
        assert!(state.connected_peers.iter().any(|p| p == &selected));
    }

    #[test]
    fn sticky_sync_peer_selection_avoids_churn_loops() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.sync_selection_stickiness_secs = 30;
        state.connection_slot_budget = 1;
        state.selected_sync_peer = Some("peer-a".into());
        state.sync_selection_sticky_until_unix = 10_020;
        state.connected_peers = vec!["peer-a".into()];

        let first = update_selected_sync_peer(&mut state, &[], 10_000);
        assert_eq!(first.as_deref(), Some("peer-a"));

        state.connected_peers = vec!["peer-b".into()];
        let second = update_selected_sync_peer(&mut state, &[], 10_005);
        assert_eq!(second.as_deref(), Some("peer-b"));
        assert!(state.sync_selection_sticky_until_unix >= 10_035);
    }

    #[test]
    fn constrained_slots_keep_selection_coherent_with_connected_set() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 1;
        state.sync_selection_stickiness_secs = 0;
        state.peer_book.insert(
            "peer-primary".into(),
            PeerHealth {
                connected: true,
                score: 140,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-secondary".into(),
            PeerHealth {
                connected: true,
                score: 130,
                ..PeerHealth::default()
            },
        );
        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        let selected = update_selected_sync_peer(&mut state, &ranked, 1_000).unwrap();
        assert_eq!(state.connected_peers, vec!["peer-primary".to_string()]);
        assert_eq!(selected, "peer-primary".to_string());
    }

    #[test]
    fn selected_sync_peer_does_not_flap_on_small_rank_advantage_during_churn() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.sync_selection_stickiness_secs = 0;
        state.selected_sync_peer = Some("peer-a".into());
        state.connected_peers = vec!["peer-b".into()];
        let ranked = vec![
            RankedSyncPeer {
                peer_id: "peer-b".into(),
                rank_score: 115,
                excluded_until_unix: None,
            },
            RankedSyncPeer {
                peer_id: "peer-a".into(),
                rank_score: 108,
                excluded_until_unix: None,
            },
        ];

        let selected = update_selected_sync_peer(&mut state, &ranked, 100).unwrap();
        assert_eq!(selected, "peer-a");
    }

    #[test]
    fn rejoin_convergence_switches_deterministically_after_sticky_window() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.sync_selection_stickiness_secs = 20;
        state.selected_sync_peer = Some("peer-a".into());
        state.connected_peers = vec!["peer-b".into()];

        let ranked = vec![
            RankedSyncPeer {
                peer_id: "peer-b".into(),
                rank_score: 180,
                excluded_until_unix: None,
            },
            RankedSyncPeer {
                peer_id: "peer-a".into(),
                rank_score: 110,
                excluded_until_unix: None,
            },
        ];
        let during_churn = update_selected_sync_peer(&mut state, &ranked, 1_000).unwrap();
        assert_eq!(during_churn, "peer-b");
        let sticky_until = state.sync_selection_sticky_until_unix;

        state.connected_peers = vec!["peer-a".into()];
        let rejoined_ranked = vec![
            RankedSyncPeer {
                peer_id: "peer-a".into(),
                rank_score: 190,
                excluded_until_unix: None,
            },
            RankedSyncPeer {
                peer_id: "peer-b".into(),
                rank_score: 120,
                excluded_until_unix: None,
            },
        ];
        let still_sticky =
            update_selected_sync_peer(&mut state, &rejoined_ranked, sticky_until - 1).unwrap();
        assert_eq!(still_sticky, "peer-b");

        let converged =
            update_selected_sync_peer(&mut state, &rejoined_ranked, sticky_until + 1).unwrap();
        assert_eq!(converged, "peer-a");
    }

    #[test]
    fn selected_sync_peer_prefers_highest_ranked_candidate_over_connected_order() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connected_peers = vec!["peer-z".into(), "peer-a".into()];
        let ranked = vec![
            RankedSyncPeer {
                peer_id: "peer-a".into(),
                rank_score: 200,
                excluded_until_unix: None,
            },
            RankedSyncPeer {
                peer_id: "peer-z".into(),
                rank_score: 90,
                excluded_until_unix: None,
            },
        ];

        let selected = update_selected_sync_peer(&mut state, &ranked, 42).unwrap();
        assert_eq!(selected, "peer-a");
    }

    #[test]
    fn selected_sync_peer_tie_break_is_lexicographically_stable() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        let ranked = vec![
            RankedSyncPeer {
                peer_id: "peer-b".into(),
                rank_score: 120,
                excluded_until_unix: None,
            },
            RankedSyncPeer {
                peer_id: "peer-a".into(),
                rank_score: 120,
                excluded_until_unix: None,
            },
        ];
        let selected_first = update_selected_sync_peer(&mut state, &ranked, 100).unwrap();
        let selected_second = update_selected_sync_peer(&mut state, &ranked, 101).unwrap();
        assert_eq!(selected_first, "peer-a");
        assert_eq!(selected_second, "peer-a");
    }

    #[test]
    fn sync_candidates_reject_full_multiaddr_peer_ids() {
        let mut state = InnerState::default();
        state.peer_book.insert(
            "/ip4/127.0.0.1/tcp/19080/p2p/12D3KooWBad".into(),
            PeerHealth::default(),
        );
        let ranked = sync_candidates_snapshot(&state);
        assert!(ranked.is_empty());
    }

    #[test]
    fn selection_respects_health_and_budget_constraints_under_hysteresis() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 1;
        state.sync_selection_stickiness_secs = 0;
        state.peer_book.insert(
            "peer-healthy".into(),
            PeerHealth {
                connected: true,
                score: 90,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-suppressed".into(),
            PeerHealth {
                connected: true,
                score: 200,
                suppressed_until_unix: u64::MAX,
                ..PeerHealth::default()
            },
        );
        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);

        let selected = update_selected_sync_peer(&mut state, &ranked, 10_000).unwrap();
        assert_eq!(state.connected_peers, vec!["peer-healthy".to_string()]);
        assert_eq!(selected, "peer-healthy");
    }

    #[tokio::test]
    async fn real_runtime_mode_initializes_without_loopback_labeling() {
        let bootstrap_key = identity::Keypair::generate_ed25519();
        let bootstrap_peer = PeerId::from(bootstrap_key.public());
        let bootstrap_addr = format!("/ip4/127.0.0.1/tcp/19080/p2p/{bootstrap_peer}");
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/0".into(),
            bootstrap: vec![bootstrap_addr],
            enable_mdns: false,
            enable_kademlia: false,
            connection_slot_budget: 8,
            sync_selection_stickiness_secs: 30,
            runtime: Libp2pRuntimeMode::RealSwarm,
        };

        let (handle, _rx) = Libp2pHandle::new(cfg).expect("real swarm handle should init");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let status = handle.status().expect("status should be available");

        assert_eq!(status.mode, P2P_MODE_LIBP2P_REAL);
        assert!(mode_connected_peers_are_real_network(&status.mode));
        assert_eq!(status.runtime_mode_detail, "swarm-poll-loop-real");
        assert!(status.connected_peers.is_empty());
        let guard = handle.inner.lock().unwrap();
        assert_eq!(
            guard
                .peer_book
                .get(&bootstrap_peer.to_string())
                .map(|h| h.connected),
            Some(false)
        );
    }

    #[tokio::test]
    async fn real_runtime_clears_persisted_connected_flags_on_startup() {
        let path = unique_peer_state_path("real-runtime");
        let (handle, _rx) = {
            let _env_lock = peer_state_env_lock();
            std::env::set_var("PULSEDAG_P2P_PEER_STATE_PATH", &path);

            let persisted = HashMap::from([(
                "persisted-peer".to_string(),
                PeerHealth {
                    connected: true,
                    ..PeerHealth::default()
                },
            )]);
            persist_peer_book(&path, &persisted);

            let cfg = Libp2pConfig {
                chain_id: "testnet".into(),
                listen_addr: "/ip4/127.0.0.1/tcp/0".into(),
                bootstrap: vec![],
                enable_mdns: false,
                enable_kademlia: false,
                connection_slot_budget: 8,
                sync_selection_stickiness_secs: 30,
                runtime: Libp2pRuntimeMode::RealSwarm,
            };

            let handle = Libp2pHandle::new(cfg).expect("real swarm handle should init");
            std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
            handle
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        let status = handle.status().expect("status should be available");

        assert!(status.connected_peers.is_empty());
        let guard = handle.inner.lock().unwrap();
        assert_ne!(
            guard.peer_book.get("persisted-peer").map(|h| h.connected),
            Some(true)
        );

        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod inventory_tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    fn sample_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee: 10,
            nonce: 1,
        }
    }

    fn sample_block(hash: &str) -> Block {
        Block {
            hash: hash.into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        }
    }

    #[test]
    fn block_inventory_messages_roundtrip() {
        let msg = NetworkMessage::InvBlock {
            chain_id: "testnet".into(),
            hashes: vec!["h1".into(), "h2".into()],
        };
        let bytes = serde_json::to_vec(&msg).expect("serialize inventory message");
        let decoded: NetworkMessage =
            serde_json::from_slice(&bytes).expect("deserialize inventory message");
        match decoded {
            NetworkMessage::InvBlock { chain_id, hashes } => {
                assert_eq!(chain_id, "testnet");
                assert_eq!(hashes, vec!["h1".to_string(), "h2".to_string()]);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn get_block_message_roundtrip() {
        let msg = NetworkMessage::GetBlock {
            chain_id: "testnet".into(),
            hash: "h-request".into(),
        };
        let bytes = serde_json::to_vec(&msg).expect("serialize get_block message");
        let decoded: NetworkMessage =
            serde_json::from_slice(&bytes).expect("deserialize get_block message");
        match decoded {
            NetworkMessage::GetBlock { chain_id, hash } => {
                assert_eq!(chain_id, "testnet");
                assert_eq!(hash, "h-request");
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn same_block_from_two_peers_is_accepted_once() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let block = sample_block("dedupe-block");
        let wire = serde_json::to_vec(&NetworkMessage::NewBlock {
            chain_id: "testnet".into(),
            block: block.clone(),
        })
        .expect("serialize block");

        dispatch_network_message("testnet", &wire, Some("peer-a"), &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, Some("peer-b"), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Block(received)) if received.hash == block.hash
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_duplicates_suppressed, 1);
        let entry = guard.inbound_seen_cache.get("block:dedupe-block").unwrap();
        assert_eq!(entry.block_hash.as_deref(), Some("dedupe-block"));
        assert_eq!(entry.peer_source.as_deref(), Some("peer-a"));
    }

    #[test]
    fn same_tx_from_two_peers_is_accepted_once() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let tx = sample_tx("dedupe-tx");
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: tx.clone(),
        })
        .expect("serialize tx");

        dispatch_network_message("testnet", &wire, Some("peer-a"), &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, Some("peer-b"), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Transaction(received)) if received.txid == tx.txid
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.tx_inbound_accepted, 1);
        assert_eq!(guard.tx_inbound_duplicate, 1);
        let entry = guard.inbound_seen_cache.get("tx:dedupe-tx").unwrap();
        assert_eq!(entry.txid.as_deref(), Some("dedupe-tx"));
    }

    #[test]
    fn rebroadcast_does_not_loop_forever() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let block = sample_block("loop-block");

        for _ in 0..32 {
            handle
                .broadcast_block(&block)
                .expect("broadcast should not fail");
        }

        let status = handle.status().expect("status should be available");
        assert_eq!(status.broadcasted_messages, 1);
        assert_eq!(status.outbound_duplicates_suppressed, 31);
        assert_eq!(status.relay_loop_prevented, 31);
    }

    #[test]
    fn inventory_requests_only_unknown_hashes() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        {
            let mut guard = inner.lock().unwrap();
            guard.known_block_hashes.insert("known-block".into());
        }
        let wire = serde_json::to_vec(&NetworkMessage::InvBlock {
            chain_id: "testnet".into(),
            hashes: vec![
                "known-block".into(),
                "new-block".into(),
                "new-block-2".into(),
            ],
        })
        .expect("serialize inventory");

        dispatch_network_message("testnet", &wire, Some("peer-inv"), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::BlockInventory { hashes })
                if hashes == vec!["new-block".to_string(), "new-block-2".to_string()]
        ));
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inv_blocks_received, 1);
        assert_eq!(guard.inv_hashes_known, 1);
        assert_eq!(guard.inv_hashes_requested, 2);
    }

    #[test]
    fn oversized_inventory_is_capped() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let hashes = (0..MAX_INV_BLOCK_HASHES + 10)
            .map(|idx| format!("inv-{idx}"))
            .collect::<Vec<_>>();
        let wire = serde_json::to_vec(&NetworkMessage::InvBlock {
            chain_id: "testnet".into(),
            hashes,
        })
        .expect("serialize inventory");

        dispatch_network_message("testnet", &wire, Some("peer-inv"), &inner, &inbound_tx);

        let requested = match inbound_rx.try_recv() {
            Ok(InboundEvent::BlockInventory { hashes }) => hashes.len(),
            other => panic!("expected block inventory event, got {other:?}"),
        };
        assert_eq!(requested, MAX_INV_BLOCK_REQUEST_FANOUT);
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inv_hashes_requested, MAX_INV_BLOCK_REQUEST_FANOUT);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("inv_block_request_fanout_capped")
        );
    }

    #[test]
    fn peer_score_decreases_on_invalid_message() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, _inbound_rx) = mpsc::unbounded_channel();

        dispatch_network_message(
            "testnet",
            b"not-json",
            Some("peer-invalid"),
            &inner,
            &inbound_tx,
        );

        let guard = inner.lock().unwrap();
        let health = guard.peer_book.get("peer-invalid").expect("peer health");
        assert!(health.score < 100);
        assert_eq!(health.fail_streak, 1);
        assert_eq!(guard.inbound_decode_failed, 1);
    }

    #[test]
    fn peer_score_recovers_after_good_behavior() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, _inbound_rx) = mpsc::unbounded_channel();
        dispatch_network_message(
            "testnet",
            b"not-json",
            Some("peer-recover"),
            &inner,
            &inbound_tx,
        );
        let after_bad = inner
            .lock()
            .unwrap()
            .peer_book
            .get("peer-recover")
            .unwrap()
            .score;
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: Transaction {
                txid: "recover-tx".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 10,
                nonce: 1,
            },
        })
        .expect("serialize tx");

        dispatch_network_message("testnet", &wire, Some("peer-recover"), &inner, &inbound_tx);

        let guard = inner.lock().unwrap();
        let health = guard.peer_book.get("peer-recover").expect("peer health");
        assert!(health.score > after_bad);
        assert_eq!(guard.tx_inbound_accepted, 1);
    }

    #[test]
    fn repeated_chain_id_mismatch_suppresses_peer() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, _inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "wrongnet".into(),
            transaction: Transaction {
                txid: "wrong-chain-tx".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 10,
                nonce: 1,
            },
        })
        .expect("serialize tx");

        for _ in 0..3 {
            dispatch_network_message(
                "testnet",
                &wire,
                Some("peer-wrong-chain"),
                &inner,
                &inbound_tx,
            );
        }

        let guard = inner.lock().unwrap();
        let health = guard
            .peer_book
            .get("peer-wrong-chain")
            .expect("peer health");
        assert!(health.score < 100);
        assert!(!health.connected);
        assert!(!health.chain_id_compatible);
        assert!(health.suppressed_until_unix > now_unix());
        assert_eq!(health.chain_mismatch_streak, 3);
    }

    #[test]
    fn compatible_connected_peers_exclude_chain_mismatch_peers() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            chain_id: "testnet".into(),
            connection_slot_budget: 8,
            ..InnerState::default()
        };
        state.peer_book.insert(
            "peer-compatible".into(),
            PeerHealth {
                connected: true,
                remote_chain_id: Some("testnet".into()),
                chain_id_compatible: true,
                last_seen_unix: Some(now_unix()),
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-wrong-chain".into(),
            PeerHealth {
                connected: true,
                remote_chain_id: Some("wrongnet".into()),
                chain_id_compatible: false,
                last_seen_unix: Some(now_unix()),
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);

        assert_eq!(state.connected_peers, vec!["peer-compatible".to_string()]);
    }

    #[test]
    fn per_peer_inbound_message_budget_rate_limits_noisy_peer() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, _inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::GetTips {
            chain_id: "testnet".into(),
        })
        .expect("serialize get tips");

        for _ in 0..=PEER_MAX_INBOUND_MESSAGES_PER_WINDOW {
            dispatch_network_message("testnet", &wire, Some("peer-noisy"), &inner, &inbound_tx);
        }

        let guard = inner.lock().unwrap();
        assert!(guard.peer_message_rate_limited_count >= 1);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("peer_inbound_rate_limited")
        );
        assert!(guard.peer_book.get("peer-noisy").unwrap().score < PEER_SCORE_MAX);
    }

    #[test]
    fn duplicate_block_announcement_is_ignored() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::NewBlockHash {
            chain_id: "testnet".into(),
            hash: "block-x".into(),
        })
        .expect("serialize block announcement");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::BlockAnnouncement { .. })
        ));
        assert!(inbound_rx.try_recv().is_err());
    }

    #[test]
    fn unknown_block_announcement_can_be_observed_for_get_block_flow() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::BlockAnnounce {
            chain_id: "testnet".into(),
            hash: "unknown-block".into(),
        })
        .expect("serialize block announce");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::BlockAnnouncement { hash }) if hash == "unknown-block"
        ));
    }

    #[test]
    fn block_data_message_delivers_full_block_to_acceptance_path() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let block = Block {
            hash: "block-data-1".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        };
        let wire = serde_json::to_vec(&NetworkMessage::BlockData {
            chain_id: "testnet".into(),
            block: Some(block.clone()),
            request_hash: Some(block.hash.clone()),
        })
        .expect("serialize block data");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Block(received)) if received.hash == block.hash
        ));
    }

    #[test]
    fn block_data_chain_id_mismatch_is_dropped_before_node_acceptance() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let block = Block {
            hash: "wrong-chain-block".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        };
        let wire = serde_json::to_vec(&NetworkMessage::BlockData {
            chain_id: "wrongnet".into(),
            block: Some(block),
            request_hash: None,
        })
        .expect("serialize block data");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_chain_mismatch_dropped, 1);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("chain_mismatch_block_data")
        );
    }

    #[test]
    fn v2_2_12_block_chain_id_mismatches_are_dropped_and_counted() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let block = Block {
            hash: "foreign-block".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec!["genesis".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        };
        let messages = [
            serde_json::to_vec(&NetworkMessage::NewBlock {
                chain_id: "wrongnet".into(),
                block: block.clone(),
            })
            .expect("serialize wrong-chain block"),
            serde_json::to_vec(&NetworkMessage::BlockAnnounce {
                chain_id: "wrongnet".into(),
                hash: block.hash.clone(),
            })
            .expect("serialize wrong-chain announce"),
            serde_json::to_vec(&NetworkMessage::BlockData {
                chain_id: "wrongnet".into(),
                block: Some(block),
                request_hash: None,
            })
            .expect("serialize wrong-chain blockdata"),
        ];

        for wire in messages {
            dispatch_network_message("testnet", &wire, Some("peer-wrong"), &inner, &inbound_tx);
        }

        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_chain_mismatch_dropped, 3);
        assert_eq!(guard.inbound_messages, 0);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("chain_mismatch_block_data")
        );
    }

    #[test]
    fn v2_2_12_duplicate_blockdata_is_delivered_once_and_counted() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let block = Block {
            hash: "duplicate-blockdata".into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec!["genesis".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        };
        let wire = serde_json::to_vec(&NetworkMessage::BlockData {
            chain_id: "testnet".into(),
            block: Some(block.clone()),
            request_hash: Some(block.hash.clone()),
        })
        .expect("serialize block data");

        dispatch_network_message("testnet", &wire, Some("peer-a"), &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, Some("peer-b"), &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Block(received)) if received.hash == block.hash
        ));
        assert!(inbound_rx.try_recv().is_err());
        let guard = inner.lock().unwrap();
        assert_eq!(guard.inbound_messages, 1);
        assert_eq!(guard.inbound_duplicates_suppressed, 1);
        assert_eq!(
            guard.last_drop_reason.as_deref(),
            Some("duplicate_block_data")
        );
    }

    #[test]
    fn tips_message_delivers_remote_tips_for_catchup_requests() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::Tips {
            chain_id: "testnet".into(),
            tips: vec!["remote-tip-1".into(), "remote-tip-2".into()],
        })
        .expect("serialize tips");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::Tips { tips }) if tips == vec!["remote-tip-1".to_string(), "remote-tip-2".to_string()]
        ));
    }

    #[test]
    fn getblock_message_delivers_missing_block_request() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::GetBlock {
            chain_id: "testnet".into(),
            hash: "missing-block".into(),
        })
        .expect("serialize getblock");

        dispatch_network_message("testnet", &wire, None, &inner, &inbound_tx);

        assert!(matches!(
            inbound_rx.try_recv(),
            Ok(InboundEvent::GetBlock { hash }) if hash == "missing-block"
        ));
    }
}

#[cfg(test)]
mod deterministic_p2p_sync_coverage_tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    fn sample_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee: 10,
            nonce: 1,
        }
    }

    fn sample_block(hash: &str, parent: &str, height: u64) -> Block {
        Block {
            hash: hash.into(),
            header: pulsedag_core::types::BlockHeader {
                version: 1,
                parents: vec![parent.into()],
                timestamp: height.max(1),
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: height,
                height,
            },
            transactions: vec![],
        }
    }

    #[test]
    fn getblock_known_and_unknown_drive_deterministic_blockdata_responses() {
        let known = sample_block("known-block-data", "genesis", 1);
        let (handle, _rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-b".into()]);
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();

        for requested_hash in [&known.hash, "missing-block-data"] {
            let wire = serde_json::to_vec(&NetworkMessage::GetBlock {
                chain_id: "testnet".into(),
                hash: requested_hash.to_string(),
            })
            .expect("serialize getblock");
            dispatch_network_message("testnet", &wire, Some("peer-b"), &inner, &inbound_tx);
            match inbound_rx.try_recv() {
                Ok(InboundEvent::GetBlock { hash }) if hash == known.hash => handle
                    .send_block_data(Some(&known.hash), Some(&known))
                    .expect("known block response is sent"),
                Ok(InboundEvent::GetBlock { hash }) if hash == "missing-block-data" => handle
                    .send_block_data(Some(&"missing-block-data".to_string()), None)
                    .expect("missing block response is sent"),
                other => panic!("unexpected getblock flow event: {other:?}"),
            }
        }

        let status = handle.status().expect("status");
        assert_eq!(status.publish_attempts, 2);
        assert_eq!(status.broadcasted_messages, 2);
        assert_eq!(status.last_message_kind.as_deref(), Some("block-data"));
    }

    #[test]
    fn tips_exchange_requests_only_unknown_remote_tips() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        {
            inner
                .lock()
                .unwrap()
                .known_block_hashes
                .insert("local-tip".into());
        }
        let (handle, _rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-sync".into()]);
        handle.request_tips().expect("gettips broadcast");
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let wire = serde_json::to_vec(&NetworkMessage::Tips {
            chain_id: "testnet".into(),
            tips: vec!["local-tip".into(), "remote-tip".into()],
        })
        .expect("serialize tips");

        dispatch_network_message("testnet", &wire, Some("peer-sync"), &inner, &inbound_tx);

        let mut requested = Vec::new();
        if let Ok(InboundEvent::Tips { tips }) = inbound_rx.try_recv() {
            let known = inner.lock().unwrap().known_block_hashes.clone();
            for tip in tips {
                if !known.contains(&tip) {
                    handle.request_block(&tip).expect("request unknown tip");
                    requested.push(tip);
                }
            }
        }

        assert_eq!(requested, vec!["remote-tip".to_string()]);
        let status = handle.status().expect("status");
        assert_eq!(status.publish_attempts, 2);
        assert_eq!(status.last_message_kind.as_deref(), Some("get-block"));
    }

    #[test]
    fn inbound_transaction_is_relayed_once_after_acceptance_and_duplicates_are_suppressed() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (handle, _rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let tx = sample_tx("relay-once-tx");
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: tx.clone(),
        })
        .expect("serialize tx");

        dispatch_network_message("testnet", &wire, Some("peer-a"), &inner, &inbound_tx);
        if let Ok(InboundEvent::Transaction(accepted)) = inbound_rx.try_recv() {
            handle
                .broadcast_transaction(&accepted)
                .expect("accepted transaction relays");
        } else {
            panic!("valid transaction was not accepted by p2p dispatch");
        }
        dispatch_network_message("testnet", &wire, Some("peer-b"), &inner, &inbound_tx);

        assert!(inbound_rx.try_recv().is_err());
        let inbound_status = inner.lock().unwrap();
        assert_eq!(inbound_status.tx_inbound_accepted, 1);
        assert_eq!(inbound_status.tx_inbound_duplicate, 1);
        assert_eq!(inbound_status.inbound_duplicates_suppressed, 1);
        drop(inbound_status);

        let status = handle.status().expect("status");
        assert_eq!(status.tx_relayed, 1);
        assert_eq!(status.tx_outbound_duplicates_suppressed, 0);
    }

    #[test]
    fn malformed_and_wrong_chain_messages_are_rejected_before_flow_events() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        dispatch_network_message(
            "testnet",
            b"not-json",
            Some("peer-bad"),
            &inner,
            &inbound_tx,
        );
        let wrong_chain = serde_json::to_vec(&NetworkMessage::BlockAnnounce {
            chain_id: "wrongnet".into(),
            hash: "foreign-block".into(),
        })
        .expect("serialize wrong chain block announce");
        dispatch_network_message(
            "testnet",
            &wrong_chain,
            Some("peer-bad"),
            &inner,
            &inbound_tx,
        );

        assert!(inbound_rx.try_recv().is_err());
        let status = inner.lock().unwrap();
        assert_eq!(status.inbound_decode_failed, 1);
        assert_eq!(status.inbound_chain_mismatch_dropped, 1);
        assert_eq!(
            status.last_drop_reason.as_deref(),
            Some("chain_mismatch_block_announce")
        );
        assert!(status.peer_book.get("peer-bad").expect("peer health").score < PEER_SCORE_MAX);
    }

    #[test]
    fn connection_slot_budget_snapshot_is_deterministic() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.connection_slot_budget = 2;
        state.peer_book.insert(
            "peer-a".into(),
            PeerHealth {
                connected: true,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-b".into(),
            PeerHealth {
                connected: true,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            "peer-c".into(),
            PeerHealth {
                connected: true,
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);
        assert_eq!(state.connected_peers.len(), 2);
        assert!(state
            .connected_peers
            .iter()
            .all(|peer| { ["peer-a", "peer-b", "peer-c"].contains(&peer.as_str()) }));
    }

    #[test]
    fn connection_established_tracks_active_peer_and_pending_dial() {
        let inner = Arc::new(Mutex::new(InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec![],
            ..InnerState::default()
        }));
        let mut pending = HashSet::new();
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        {
            let mut guard = inner.lock().unwrap();
            guard.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        }
        pending.insert(peer);
        handle_connection_established(&inner, &mut pending, &peer, "outbound");
        let guard = inner.lock().unwrap();
        assert!(!pending.contains(&peer));
        assert_eq!(
            guard.active_connections.get(&peer.to_string()).copied(),
            Some(1)
        );
        assert_eq!(guard.connection_established_total, 1);
        assert_eq!(
            guard
                .peer_lifecycle_event_counters
                .get("outbound_connected")
                .copied(),
            Some(1)
        );
        assert_eq!(
            guard
                .peer_connection_final_state
                .get(&peer.to_string())
                .map(|state| state.state.as_str()),
            Some("connected")
        );
    }

    #[test]
    fn connection_closed_decrements_only_target_peer() {
        let key_a = identity::Keypair::generate_ed25519();
        let key_b = identity::Keypair::generate_ed25519();
        let peer_a = PeerId::from(key_a.public());
        let peer_b = PeerId::from(key_b.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.active_connections.insert(peer_a.to_string(), 2);
        state.active_connections.insert(peer_b.to_string(), 1);
        let inner = Arc::new(Mutex::new(state));

        let mut pending = HashSet::new();
        let mut next_redial = HashMap::new();
        let mut backoff = HashMap::new();
        assert!(!handle_connection_closed(
            &inner,
            &mut pending,
            &mut next_redial,
            &mut backoff,
            &peer_a,
            "closed".into(),
            "outbound"
        ));
        {
            let guard = inner.lock().unwrap();
            assert_eq!(
                guard.active_connections.get(&peer_a.to_string()).copied(),
                Some(1)
            );
            assert_eq!(
                guard.active_connections.get(&peer_b.to_string()).copied(),
                Some(1)
            );
        }
        let mut pending = HashSet::new();
        let mut next_redial = HashMap::new();
        let mut backoff = HashMap::new();
        assert!(handle_connection_closed(
            &inner,
            &mut pending,
            &mut next_redial,
            &mut backoff,
            &peer_a,
            "closed".into(),
            "outbound"
        ));
        let guard = inner.lock().unwrap();
        assert!(!guard.active_connections.contains_key(&peer_a.to_string()));
        assert_eq!(
            guard.active_connections.get(&peer_b.to_string()).copied(),
            Some(1)
        );
        assert_eq!(
            guard.disconnect_reason_counts.get("closed").copied(),
            Some(2)
        );
        assert_eq!(
            guard
                .peer_connection_final_state
                .get(&peer_a.to_string())
                .map(|state| state.state.as_str()),
            Some("disconnected")
        );
    }

    #[test]
    fn bootnode_reconnect_schedule_bypasses_generic_cooldown() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        state.peer_book.insert(
            peer.to_string(),
            PeerHealth {
                next_retry_unix: 10_100,
                suppressed_until_unix: 10_050,
                ..PeerHealth::default()
            },
        );
        let inner = Arc::new(Mutex::new(state));

        assert!(record_bootnode_reconnect_schedule(&inner, &peer, 10_000));

        let guard = inner.lock().unwrap();
        assert_eq!(guard.bootnode_reconnect_scheduled_total, 1);
        assert_eq!(guard.bootnode_reconnect_skipped_cooldown_total, 1);
        assert_eq!(guard.bootnode_reconnect_forced_from_cooldown_total, 1);
        assert_eq!(guard.bootnode_redial_attempts, 1);
    }

    #[test]
    fn bootnode_cooldown_does_not_suppress_all_reconnect_attempts() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        state.peer_book.insert(
            peer.to_string(),
            PeerHealth {
                next_retry_unix: 20_000,
                ..PeerHealth::default()
            },
        );
        let inner = Arc::new(Mutex::new(state));

        for now in [10_000, 10_003, 10_006] {
            assert!(record_bootnode_reconnect_schedule(&inner, &peer, now));
        }

        let guard = inner.lock().unwrap();
        assert_eq!(guard.bootnode_reconnect_scheduled_total, 3);
        assert_eq!(guard.bootnode_reconnect_forced_from_cooldown_total, 3);
        assert_eq!(guard.bootnode_reconnect_skipped_cooldown_total, 3);
    }

    #[test]
    fn configured_bootnode_reconnect_remains_scheduled_after_disconnect() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.active_connections.insert(peer.to_string(), 1);
        state.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        state.pending_bootnode_dials.insert(peer.to_string());
        state
            .bootnode_redial_backoff_secs
            .insert(peer.to_string(), 4);
        let inner = Arc::new(Mutex::new(state));
        let mut pending = HashSet::new();
        pending.insert(peer);
        let mut next_redial = HashMap::new();
        let mut backoff = HashMap::new();

        assert!(handle_connection_closed(
            &inner,
            &mut pending,
            &mut next_redial,
            &mut backoff,
            &peer,
            "closed-by-peer".into(),
            "outbound"
        ));

        let guard = inner.lock().unwrap();
        assert!(!pending.contains(&peer));
        assert!(!guard.pending_bootnode_dials.contains(&peer.to_string()));
        assert!(guard
            .bootnode_next_redial_at
            .contains_key(&peer.to_string()));
        assert!(next_redial.contains_key(&peer));
        assert_eq!(
            guard.bootnode_redial_backoff_secs.get(&peer.to_string()),
            Some(&8)
        );
        assert_eq!(backoff.get(&peer), Some(&8));
        assert_eq!(
            guard
                .peer_connection_final_state
                .get(&peer.to_string())
                .and_then(|state| state.last_disconnect_reason.as_deref()),
            Some("closed-by-peer")
        );
    }

    #[test]
    fn outgoing_connection_error_clears_pending_without_removing_live_peer() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.active_connections.insert(peer.to_string(), 1);
        state.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        let inner = Arc::new(Mutex::new(state));
        let mut pending = HashSet::new();
        pending.insert(peer);

        let should_mark_failed =
            handle_outgoing_connection_error(&inner, &mut pending, &peer, "boom");
        assert!(!should_mark_failed);
        let guard = inner.lock().unwrap();
        assert_eq!(
            guard
                .last_error_by_peer
                .get(&peer.to_string())
                .map(String::as_str),
            Some("boom")
        );
        assert_eq!(
            guard
                .peer_connection_final_state
                .get(&peer.to_string())
                .map(|state| state.state.as_str()),
            Some("error")
        );
        assert!(!pending.contains(&peer));
        assert_eq!(guard.bootnode_redial_failures, 1);
        assert_eq!(
            guard.active_connections.get(&peer.to_string()).copied(),
            Some(1)
        );
    }

    #[test]
    fn peer_recovery_success_count_does_not_imply_connected_peers() {
        let mut state = InnerState::default();
        state.mode = P2P_MODE_LIBP2P_REAL.into();
        state.peer_recovery_success_count = 5;
        refresh_connected_peers_from_health(&mut state);
        assert!(state.connected_peers.is_empty());
        assert_eq!(state.active_connections.values().copied().sum::<usize>(), 0);
    }

    #[test]
    fn active_chain_compatible_connections_remain_visible_during_recovery() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.active_connections.insert("peer-recovering".into(), 1);
        state.peer_book.insert(
            "peer-recovering".into(),
            PeerHealth {
                connected: false,
                fail_streak: 2,
                next_retry_unix: now_unix().saturating_add(60),
                chain_id_compatible: true,
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);

        assert_eq!(state.connected_peers, vec!["peer-recovering".to_string()]);
    }

    #[test]
    fn stale_unusable_bootnode_active_slot_forces_redial() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")],
            ..InnerState::default()
        };
        state.active_connections.insert(peer.to_string(), 1);
        state.peer_book.insert(
            peer.to_string(),
            PeerHealth {
                connected: false,
                chain_id_compatible: false,
                last_error: Some("chain_mismatch".into()),
                ..PeerHealth::default()
            },
        );

        refresh_connected_peers_from_health(&mut state);

        assert!(state.connected_peers.is_empty());
        assert!(should_force_bootnode_redial_for_peer(
            &state,
            &peer.to_string()
        ));
    }

    #[test]
    fn parse_bootnode_multiaddr_extracts_peer_id() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let addr = format!("/ip4/127.0.0.1/tcp/19080/p2p/{peer}");
        let parsed = parse_bootnode_multiaddr(&addr).expect("parse bootnode");
        assert_eq!(parsed.0, peer);
        assert_eq!(parsed.1.to_string(), addr);
    }

    #[test]
    fn parse_bootnode_multiaddr_rejects_missing_peer_id() {
        assert!(parse_bootnode_multiaddr("/ip4/127.0.0.1/tcp/19080").is_none());
    }

    #[test]
    fn connection_established_clears_cooldown_and_marks_connected() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        let mut health = PeerHealth::default();
        health.connected = false;
        health.fail_streak = 3;
        health.next_retry_unix = now_unix().saturating_add(99);
        state.peer_book.insert(peer.to_string(), health);
        let inner = Arc::new(Mutex::new(state));
        let mut pending = HashSet::new();
        pending.insert(peer);

        handle_connection_established(&inner, &mut pending, &peer, "outbound");

        let guard = inner.lock().unwrap();
        let health = guard.peer_book.get(&peer.to_string()).unwrap();
        assert!(health.connected);
        assert_eq!(health.fail_streak, 0);
        assert!(health.next_retry_unix > 0);
        assert!(health.next_retry_unix <= now_unix());
    }

    #[test]
    fn bootnode_redial_backoff_resets_after_success() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        state
            .bootnode_redial_backoff_secs
            .insert(peer.to_string(), 8);
        state
            .bootnode_next_redial_at
            .insert(peer.to_string(), now_unix().saturating_add(10));
        state.pending_bootnode_dials.insert(peer.to_string());
        state.bootnodes_configured = vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")];
        let inner = Arc::new(Mutex::new(state));
        let mut pending = HashSet::new();
        pending.insert(peer);

        handle_connection_established(&inner, &mut pending, &peer, "outbound");

        let guard = inner.lock().unwrap();
        assert_eq!(
            guard
                .bootnode_redial_backoff_secs
                .get(&peer.to_string())
                .copied(),
            Some(1)
        );
        assert!(!guard
            .bootnode_next_redial_at
            .contains_key(&peer.to_string()));
        assert!(!guard.pending_bootnode_dials.contains(&peer.to_string()));
    }

    #[test]
    fn non_bootnode_connection_established_does_not_mutate_bootnode_backoff_state() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        state.bootnodes_configured = vec![];
        let inner = Arc::new(Mutex::new(state));
        let mut pending = HashSet::new();
        pending.insert(peer);

        handle_connection_established(&inner, &mut pending, &peer, "outbound");

        let guard = inner.lock().unwrap();
        assert!(pending.contains(&peer));
        assert!(!guard.pending_bootnode_dials.contains(&peer.to_string()));
        assert!(!guard
            .bootnode_redial_backoff_secs
            .contains_key(&peer.to_string()));
        assert!(!guard
            .bootnode_next_redial_at
            .contains_key(&peer.to_string()));
    }

    #[test]
    fn peer_zero_with_configured_bootnode_marks_immediate_reconnect_need() {
        let key = identity::Keypair::generate_ed25519();
        let peer = PeerId::from(key.public()).to_string();
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec![format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}")],
            ..InnerState::default()
        };
        state.peer_book.insert(
            peer.clone(),
            PeerHealth {
                connected: false,
                next_retry_unix: 10_100,
                suppressed_until_unix: 10_200,
                ..PeerHealth::default()
            },
        );

        enforce_connectivity_aware_cooldown_floor(&mut state, 10_000);

        let health = state.peer_book.get(&peer).unwrap();
        assert_eq!(health.next_retry_unix, 10_000);
        assert_eq!(health.suppressed_until_unix, 10_000);
        assert_eq!(state.peer_zero_since_unix, Some(10_000));
        assert_eq!(state.peer_min_target_missed_total, 1);
        assert_eq!(state.peer_cooldown_bypassed_for_connectivity_total, 1);
    }

    #[test]
    fn cooldown_floor_preserves_minimum_useful_private_topology() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec!["/ip4/127.0.0.1/tcp/1/p2p/peer-a".into()],
            ..InnerState::default()
        };
        for peer in ["peer-a", "peer-b", "peer-c"] {
            state.peer_book.insert(
                peer.into(),
                PeerHealth {
                    connected: false,
                    next_retry_unix: 5_100,
                    suppressed_until_unix: 5_200,
                    ..PeerHealth::default()
                },
            );
        }

        enforce_connectivity_aware_cooldown_floor(&mut state, 5_000);

        let released = state
            .peer_book
            .values()
            .filter(|health| {
                health.next_retry_unix == 5_000 && health.suppressed_until_unix == 5_000
            })
            .count();
        assert_eq!(released, 2);
        assert_eq!(state.peer_cooldown_bypassed_for_connectivity_total, 2);
    }

    #[test]
    fn rate_limit_flag_does_not_permanently_block_first_peer_reconnect() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec!["/ip4/127.0.0.1/tcp/1/p2p/peer-a".into()],
            ..InnerState::default()
        };
        state.peer_book.insert(
            "peer-a".into(),
            PeerHealth {
                connected: false,
                last_rate_limited_unix: Some(9_999),
                next_retry_unix: 10_050,
                ..PeerHealth::default()
            },
        );

        enforce_connectivity_aware_cooldown_floor(&mut state, 10_000);

        let health = state.peer_book.get("peer-a").unwrap();
        assert_eq!(health.next_retry_unix, 10_000);
        assert_eq!(health.last_rate_limited_unix, None);
        assert_eq!(state.peer_reconnect_suppressed_by_rate_limit_total, 1);
    }

    #[test]
    fn private_rehearsal_recovers_from_peer_zero_after_connection() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec!["/ip4/127.0.0.1/tcp/1/p2p/peer-a".into()],
            ..InnerState::default()
        };
        state.peer_book.insert(
            "peer-a".into(),
            PeerHealth {
                connected: false,
                ..PeerHealth::default()
            },
        );
        enforce_connectivity_aware_cooldown_floor(&mut state, 1_000);
        assert!(state.peer_zero_since_unix.is_some());
        state.active_connections.insert("peer-a".into(), 1);
        state.peer_book.get_mut("peer-a").unwrap().connected = true;
        enforce_connectivity_aware_cooldown_floor(&mut state, 1_010);
        assert_eq!(state.peer_zero_since_unix, None);
        assert_eq!(state.peer_zero_reconnect_success_total, 1);
    }

    #[test]
    fn peer_below_target_schedules_bootnode_reconnect_above_one_peer() {
        let key_a = identity::Keypair::generate_ed25519();
        let key_b = identity::Keypair::generate_ed25519();
        let peer_a = PeerId::from(key_a.public());
        let peer_b = PeerId::from(key_b.public());
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            bootnodes_configured: vec![
                format!("/ip4/127.0.0.1/tcp/1/p2p/{peer_a}"),
                format!("/ip4/127.0.0.1/tcp/2/p2p/{peer_b}"),
            ],
            ..InnerState::default()
        };
        state.active_connections.insert(peer_a.to_string(), 1);
        state.connected_peers = vec![peer_a.to_string()];
        state.peer_book.insert(
            peer_a.to_string(),
            PeerHealth {
                connected: true,
                chain_id_compatible: true,
                ..PeerHealth::default()
            },
        );
        state.peer_book.insert(
            peer_b.to_string(),
            PeerHealth {
                connected: false,
                next_retry_unix: 20_000,
                suppressed_until_unix: 20_000,
                ..PeerHealth::default()
            },
        );

        enforce_connectivity_aware_cooldown_floor(&mut state, 10_000);

        assert_eq!(private_rehearsal_min_useful_peer_target(&state), 2);
        assert!(state
            .bootnode_next_redial_at
            .contains_key(&peer_b.to_string()));
        assert_eq!(state.peer_min_target_reconnect_attempt_total, 1);
        assert_eq!(
            state.peer_below_target_blocked_reason.as_deref(),
            Some("bootnode_redial_scheduled")
        );
    }

    #[test]
    fn five_node_two_miner_style_topology_floor_does_not_leave_two_nodes_isolated() {
        let mut nodes = Vec::new();
        for node_idx in 0..5 {
            let mut state = InnerState {
                mode: P2P_MODE_LIBP2P_REAL.into(),
                bootnodes_configured: vec!["/ip4/127.0.0.1/tcp/1/p2p/peer-0".into()],
                ..InnerState::default()
            };
            for peer_idx in 0..5 {
                if peer_idx != node_idx {
                    state.peer_book.insert(
                        format!("peer-{peer_idx}"),
                        PeerHealth {
                            connected: false,
                            next_retry_unix: 50_300,
                            suppressed_until_unix: 50_300,
                            ..PeerHealth::default()
                        },
                    );
                }
            }
            enforce_connectivity_aware_cooldown_floor(&mut state, 50_000);
            nodes.push(state);
        }

        assert!(nodes.iter().all(|state| {
            state
                .peer_book
                .values()
                .filter(|health| health.next_retry_unix <= 50_000)
                .count()
                >= 2
        }));
    }

    #[test]
    fn is_valid_peer_id_rejects_multiaddr_values() {
        assert!(!is_valid_peer_id("/ip4/127.0.0.1/tcp/19080/p2p/12D3KooW"));
    }
}
