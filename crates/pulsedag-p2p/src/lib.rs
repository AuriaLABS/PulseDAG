pub mod messages;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identity, Multiaddr, PeerId, SwarmBuilder};
use pulsedag_core::{
    errors::PulseError,
    rank_sync_candidates,
    types::{Block, Transaction},
    RankedSyncPeer, SyncPeerCandidate,
};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::time::{sleep, Duration};

use crate::messages::{message_id_for_block, message_id_for_tx, topic_names, NetworkMessage};

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

#[derive(Debug, Clone)]
pub struct PeerRecoveryStatus {
    pub peer_id: String,
    pub score: i32,
    pub fail_streak: u32,
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
}

#[derive(Debug, Clone)]
pub struct P2pStatus {
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
    pub queue_non_block_fair_picks: usize,
    pub queue_starvation_relief_picks: usize,
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
    pub tx_outbound_duplicates_suppressed: usize,
    pub tx_outbound_first_seen_relayed: usize,
    pub tx_outbound_recovery_relayed: usize,
    pub block_outbound_duplicates_suppressed: usize,
    pub block_outbound_first_seen_relayed: usize,
    pub block_outbound_recovery_relayed: usize,
    pub last_drop_reason: Option<String>,
    pub peer_reconnect_attempts: u64,
    pub peer_recovery_success_count: u64,
    pub last_peer_recovery_unix: Option<u64>,
    pub peer_cooldown_suppressed_count: u64,
    pub peer_flap_suppressed_count: u64,
    pub peers_under_cooldown: usize,
    pub peers_under_flap_guard: usize,
    pub peer_recovery: Vec<PeerRecoveryStatus>,
    pub sync_candidates: Vec<RankedSyncPeer>,
    pub selected_sync_peer: Option<String>,
    pub connection_slot_budget: usize,
    pub connected_slots_in_use: usize,
    pub available_connection_slots: usize,
    pub sync_selection_sticky_until_unix: Option<u64>,
}

pub trait P2pHandle: Send + Sync {
    fn broadcast_transaction(&self, tx: &Transaction) -> Result<(), PulseError>;
    fn broadcast_block(&self, block: &Block) -> Result<(), PulseError>;
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
    PeerConnected(String),
}

#[derive(Debug, Clone)]
enum OutboundMessage {
    Transaction(Transaction),
    Block(Block),
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
    connected_peers: Vec<String>,
    seen_message_ids: HashSet<String>,
    queued_messages: usize,
    queued_block_messages: usize,
    queued_non_block_messages: usize,
    queue_max_depth: usize,
    dequeued_block_messages: usize,
    dequeued_non_block_messages: usize,
    queue_block_priority_picks: usize,
    queue_non_block_fair_picks: usize,
    queue_starvation_relief_picks: usize,
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
    tx_outbound_duplicates_suppressed: usize,
    tx_outbound_first_seen_relayed: usize,
    tx_outbound_recovery_relayed: usize,
    block_outbound_duplicates_suppressed: usize,
    block_outbound_first_seen_relayed: usize,
    block_outbound_recovery_relayed: usize,
    last_drop_reason: Option<String>,
    inbound_seen_at_unix: HashMap<String, u64>,
    outbound_tx_seen_at_unix: HashMap<String, u64>,
    outbound_block_seen_at_unix: HashMap<String, u64>,
    outbound_tx_recovery_relay_generation: HashMap<String, u64>,
    outbound_block_recovery_relay_generation: HashMap<String, u64>,
    recovery_rebroadcast_generation: u64,
    recovery_rebroadcast_until_unix: u64,
    peer_book: HashMap<String, PeerHealth>,
    peer_state_path: Option<PathBuf>,
    peer_reconnect_attempts: u64,
    peer_recovery_success_count: u64,
    last_peer_recovery_unix: Option<u64>,
    peer_cooldown_suppressed_count: u64,
    peer_flap_suppressed_count: u64,
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
}

impl Default for PeerHealth {
    fn default() -> Self {
        Self {
            score: 100,
            fail_streak: 0,
            next_retry_unix: 0,
            connected: true,
            last_seen_unix: None,
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

    fn status(&self) -> Result<P2pStatus, PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        let (peers_under_cooldown, peers_under_flap_guard, peer_recovery) =
            peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer =
            update_selected_sync_peer(&mut inner, &sync_candidates, now_unix());
        let connected_slots_in_use = inner.connected_peers.len();
        let available_connection_slots = inner
            .connection_slot_budget
            .saturating_sub(connected_slots_in_use);
        Ok(P2pStatus {
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
            queue_non_block_fair_picks: inner.queue_non_block_fair_picks,
            queue_starvation_relief_picks: inner.queue_starvation_relief_picks,
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
            tx_outbound_duplicates_suppressed: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_first_seen_relayed: inner.tx_outbound_first_seen_relayed,
            tx_outbound_recovery_relayed: inner.tx_outbound_recovery_relayed,
            block_outbound_duplicates_suppressed: inner.block_outbound_duplicates_suppressed,
            block_outbound_first_seen_relayed: inner.block_outbound_first_seen_relayed,
            block_outbound_recovery_relayed: inner.block_outbound_recovery_relayed,
            last_drop_reason: inner.last_drop_reason.clone(),
            peer_reconnect_attempts: inner.peer_reconnect_attempts,
            peer_recovery_success_count: inner.peer_recovery_success_count,
            last_peer_recovery_unix: inner.last_peer_recovery_unix,
            peer_cooldown_suppressed_count: inner.peer_cooldown_suppressed_count,
            peer_flap_suppressed_count: inner.peer_flap_suppressed_count,
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_recovery,
            sync_candidates,
            selected_sync_peer,
            connection_slot_budget: inner.connection_slot_budget,
            connected_slots_in_use,
            available_connection_slots,
            sync_selection_sticky_until_unix: (inner.sync_selection_sticky_until_unix > 0)
                .then_some(inner.sync_selection_sticky_until_unix),
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
        let peer_book = cfg
            .bootstrap
            .iter()
            .map(|peer| {
                let mut health = PeerHealth::default();
                if real_network_connectivity {
                    health.connected = false;
                }
                (peer.clone(), health)
            })
            .collect();
        let mut state = InnerState {
            mode,
            runtime_mode_detail,
            peer_id: peer_id.to_string(),
            listening: vec![cfg.listen_addr.clone()],
            peer_book,
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
const TX_OUTBOUND_DEDUP_WINDOW_SECS: u64 = 30;
const BLOCK_OUTBOUND_DEDUP_WINDOW_SECS: u64 = 30;
const RECOVERY_REBROADCAST_GRACE_SECS: u64 = 8;
const MAX_DEDUP_TRACKED_IDS: usize = 16_384;
const BLOCK_PRIORITY_BURST_LIMIT: usize = 8;

fn peer_recovery_snapshot(state: &InnerState) -> (usize, usize, Vec<PeerRecoveryStatus>) {
    let now = now_unix();
    let mut peer_recovery = state
        .peer_book
        .iter()
        .map(|(peer_id, health)| PeerRecoveryStatus {
            peer_id: peer_id.clone(),
            score: health.score,
            fail_streak: health.fail_streak,
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
    (peers_under_cooldown, peers_under_flap_guard, peer_recovery)
}

fn sync_candidates_snapshot(state: &InnerState) -> Vec<RankedSyncPeer> {
    let now = now_unix();
    let candidates = state
        .peer_book
        .iter()
        .map(|(peer_id, health)| SyncPeerCandidate {
            peer_id: peer_id.clone(),
            score: health.score,
            fail_streak: health.fail_streak,
            connected: health.connected,
            next_retry_unix: health.next_retry_unix,
            suppressed_until_unix: health.suppressed_until_unix,
            recovery_success_count: health.recovery_success_count,
            recent_failures: health.recent_failures_unix.len(),
        })
        .collect::<Vec<_>>();
    rank_sync_candidates(&candidates, now)
}

fn refresh_connected_peers_from_health(state: &mut InnerState) {
    if mode_connected_peers_are_real_network(&state.mode) {
        let budget = if state.connection_slot_budget == 0 {
            usize::MAX
        } else {
            state.connection_slot_budget
        };
        state.connected_peers = sync_candidates_snapshot(state)
            .into_iter()
            .filter(|peer| {
                peer.excluded_until_unix.is_none()
                    && state
                        .peer_book
                        .get(&peer.peer_id)
                        .map(|health| health.connected)
                        .unwrap_or(false)
            })
            .map(|peer| peer.peer_id)
            .take(budget)
            .collect();
    } else {
        state.connected_peers.clear();
    }
}

fn update_selected_sync_peer(
    state: &mut InnerState,
    sync_candidates: &[RankedSyncPeer],
    now: u64,
) -> Option<String> {
    let preferred = state
        .connected_peers
        .first()
        .cloned()
        .or_else(|| sync_candidates.first().map(|peer| peer.peer_id.clone()));
    let current_is_eligible = state
        .selected_sync_peer
        .as_ref()
        .map(|peer| state.connected_peers.contains(peer))
        .unwrap_or(false);
    let sticky_active = state.sync_selection_sticky_until_unix > now;

    if sticky_active && current_is_eligible {
        return state.selected_sync_peer.clone();
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
    non_blocks: std::collections::VecDeque<OutboundMessage>,
    consecutive_block_picks: usize,
}

fn track_queue_depth_on_enqueue(state: &mut InnerState) {
    state.queue_max_depth = state.queue_max_depth.max(state.queued_messages);
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
        OutboundMessage::Transaction(tx) => {
            queue.non_blocks.push_back(OutboundMessage::Transaction(tx));
        }
    }
}

fn pop_outbound_message(
    inner: &Arc<Mutex<InnerState>>,
    queue: &mut OutboundPriorityQueue,
) -> Option<OutboundMessage> {
    let blocks_waiting = !queue.blocks.is_empty();
    let non_blocks_waiting = !queue.non_blocks.is_empty();
    if !blocks_waiting && !non_blocks_waiting {
        return None;
    }
    let take_non_block_for_fairness = blocks_waiting
        && non_blocks_waiting
        && queue.consecutive_block_picks >= BLOCK_PRIORITY_BURST_LIMIT;
    let picked = if take_non_block_for_fairness {
        queue.consecutive_block_picks = 0;
        queue.non_blocks.pop_front()
    } else if blocks_waiting {
        queue.consecutive_block_picks = queue.consecutive_block_picks.saturating_add(1);
        queue.blocks.pop_front()
    } else {
        queue.consecutive_block_picks = 0;
        queue.non_blocks.pop_front()
    };
    if let (Some(msg), Ok(mut guard)) = (picked.as_ref(), inner.lock()) {
        guard.queued_messages = guard.queued_messages.saturating_sub(1);
        match msg {
            OutboundMessage::Block(_) => {
                guard.queued_block_messages = guard.queued_block_messages.saturating_sub(1);
                guard.dequeued_block_messages = guard.dequeued_block_messages.saturating_add(1);
                guard.queue_block_priority_picks =
                    guard.queue_block_priority_picks.saturating_add(1);
            }
            OutboundMessage::Transaction(_) => {
                guard.queued_non_block_messages = guard.queued_non_block_messages.saturating_sub(1);
                guard.dequeued_non_block_messages =
                    guard.dequeued_non_block_messages.saturating_add(1);
                guard.queue_non_block_fair_picks =
                    guard.queue_non_block_fair_picks.saturating_add(1);
                if blocks_waiting {
                    guard.queue_starvation_relief_picks =
                        guard.queue_starvation_relief_picks.saturating_add(1);
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
    loop {
        match outbound_rx.try_recv() {
            Ok(msg) => enqueue_outbound_message(inner, queue, msg),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
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
        {
            let health = guard.peer_book.entry(peer.to_string()).or_default();
            health.reconnect_attempts = health.reconnect_attempts.saturating_add(1);
            health.last_seen_unix = Some(now);
            if success {
                health.connected = true;
                health.fail_streak = 0;
                health.next_retry_unix = now;
                trigger_rebroadcast_window = true;
                let success_bonus = if health.score < 0 {
                    PEER_SUCCESS_BASE_BONUS + 4
                } else {
                    PEER_SUCCESS_BASE_BONUS
                };
                health.score = (health.score + success_bonus).clamp(PEER_SCORE_MIN, PEER_SCORE_MAX);
                health.recovery_success_count = health.recovery_success_count.saturating_add(1);
                health.last_recovery_unix = Some(now);
                health.last_successful_connect_unix = Some(now);
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
                let mut next_retry_unix = now.saturating_add(bounded_backoff + peer_jitter(peer));
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
                    health.flap_suppressed_count = health.flap_suppressed_count.saturating_add(1);
                    counted_flap_suppression = true;
                }
                health.last_failure_unix = Some(now);
                health.recent_failures_unix.push(now);
                if health.recent_failures_unix.len() > RECENT_FAILURES_KEEP {
                    let keep_from = health.recent_failures_unix.len() - RECENT_FAILURES_KEEP;
                    health.recent_failures_unix = health.recent_failures_unix.split_off(keep_from);
                }
                health.next_retry_unix = next_retry_unix.max(previous_next_retry_unix);
            }
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

fn mark_inbound_id_seen(state: &mut InnerState, id: String, now: u64) -> bool {
    trim_old_entries(
        &mut state.inbound_seen_at_unix,
        now,
        MESSAGE_DEDUP_WINDOW_SECS,
    );
    match state.inbound_seen_at_unix.get(&id) {
        Some(last_seen) if now.saturating_sub(*last_seen) <= MESSAGE_DEDUP_WINDOW_SECS => false,
        _ => {
            state.inbound_seen_at_unix.insert(id.clone(), now);
            state.seen_message_ids.insert(id);
            true
        }
    }
}

fn should_relay_outbound_tx(state: &mut InnerState, id: &str, now: u64) -> bool {
    trim_old_entries(
        &mut state.outbound_tx_seen_at_unix,
        now,
        TX_OUTBOUND_DEDUP_WINDOW_SECS,
    );
    match state.outbound_tx_seen_at_unix.get(id) {
        Some(last_seen) if now.saturating_sub(*last_seen) <= TX_OUTBOUND_DEDUP_WINDOW_SECS => {
            if should_allow_recovery_rebroadcast(
                &mut state.outbound_tx_recovery_relay_generation,
                state.recovery_rebroadcast_generation,
                state.recovery_rebroadcast_until_unix,
                id,
                now,
            ) {
                state.outbound_tx_seen_at_unix.insert(id.to_string(), now);
                state.tx_outbound_recovery_relayed =
                    state.tx_outbound_recovery_relayed.saturating_add(1);
                return true;
            }
            state.tx_outbound_duplicates_suppressed =
                state.tx_outbound_duplicates_suppressed.saturating_add(1);
            false
        }
        _ => {
            state.outbound_tx_seen_at_unix.insert(id.to_string(), now);
            state.tx_outbound_first_seen_relayed =
                state.tx_outbound_first_seen_relayed.saturating_add(1);
            true
        }
    }
}

fn should_relay_outbound_block(state: &mut InnerState, id: &str, now: u64) -> bool {
    trim_old_entries(
        &mut state.outbound_block_seen_at_unix,
        now,
        BLOCK_OUTBOUND_DEDUP_WINDOW_SECS,
    );
    match state.outbound_block_seen_at_unix.get(id) {
        Some(last_seen) if now.saturating_sub(*last_seen) <= BLOCK_OUTBOUND_DEDUP_WINDOW_SECS => {
            if should_allow_recovery_rebroadcast(
                &mut state.outbound_block_recovery_relay_generation,
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
                return true;
            }
            state.block_outbound_duplicates_suppressed =
                state.block_outbound_duplicates_suppressed.saturating_add(1);
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
    recovery_relays: &mut HashMap<String, u64>,
    current_generation: u64,
    recovery_until_unix: u64,
    id: &str,
    now: u64,
) -> bool {
    if current_generation == 0 || now > recovery_until_unix {
        return false;
    }
    if recovery_relays.len() > MAX_DEDUP_TRACKED_IDS {
        recovery_relays.retain(|_, generation| *generation == current_generation);
    }
    match recovery_relays.get(id) {
        Some(previous_generation) if *previous_generation == current_generation => false,
        _ => {
            recovery_relays.insert(id.to_string(), current_generation);
            true
        }
    }
}

fn note_swarm_event(inner: &Arc<Mutex<InnerState>>, label: impl Into<String>) {
    if let Ok(mut guard) = inner.lock() {
        guard.swarm_events_seen += 1;
        guard.last_swarm_event = Some(label.into());
    }
}

fn dispatch_network_message(
    expected_chain_id: &str,
    bytes: &[u8],
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
            }
            return;
        }
    };

    match msg {
        NetworkMessage::NewTransaction {
            chain_id,
            transaction,
        } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_tx".into());
                }
                return;
            }
            let id = message_id_for_tx(&transaction);
            if let Ok(mut guard) = inner.lock() {
                if !mark_inbound_id_seen(&mut guard, id, now_unix()) {
                    guard.inbound_duplicates_suppressed += 1;
                    guard.last_drop_reason = Some("duplicate_tx".into());
                    return;
                }
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("tx-inbound".into());
            }
            let _ = inbound_tx.send(InboundEvent::Transaction(transaction));
        }
        NetworkMessage::NewBlock { chain_id, block } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block".into());
                }
                return;
            }
            let id = message_id_for_block(&block);
            if let Ok(mut guard) = inner.lock() {
                if !mark_inbound_id_seen(&mut guard, id, now_unix()) {
                    guard.inbound_duplicates_suppressed += 1;
                    guard.last_drop_reason = Some("duplicate_block".into());
                    return;
                }
                guard.inbound_messages += 1;
                guard.last_message_kind = Some("block-inbound".into());
            }
            let _ = inbound_tx.send(InboundEvent::Block(block));
        }
        NetworkMessage::GetTips { chain_id } => {
            if chain_id == expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("get-tips".into());
                }
            }
        }
        NetworkMessage::Tips { chain_id, .. } => {
            if chain_id == expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("tips".into());
                }
            }
        }
        NetworkMessage::GetBlock { chain_id, .. } => {
            if chain_id == expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("get-block".into());
                }
            }
        }
        NetworkMessage::BlockData { chain_id, block } => {
            if chain_id != expected_chain_id {
                if let Ok(mut guard) = inner.lock() {
                    guard.inbound_chain_mismatch_dropped += 1;
                    guard.last_drop_reason = Some("chain_mismatch_block_data".into());
                }
                return;
            }
            if let Some(block) = block {
                let id = message_id_for_block(&block);
                if let Ok(mut guard) = inner.lock() {
                    if !mark_inbound_id_seen(&mut guard, id, now_unix()) {
                        guard.inbound_duplicates_suppressed += 1;
                        guard.last_drop_reason = Some("duplicate_block_data".into());
                        return;
                    }
                    guard.inbound_messages += 1;
                    guard.last_message_kind = Some("block-data".into());
                }
                let _ = inbound_tx.send(InboundEvent::Block(block));
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
                };

                    note_swarm_event(&inner, format!("publish-attempt:{topic_name}"));
                    record_publish(&inner, &topic_name, message_kind, &message_id);

                    if let Ok(bytes) = wire {
                        // v0.6.9 keeps one canonical wire path while the actual Swarm
                        // publish/poll code is staged. The bytes are decoded through the
                        // same dispatcher that the live Swarm loop will use.
                        dispatch_network_message(&cfg.chain_id, &bytes, &inner, &inbound_tx);
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
                    dispatch_network_message(&cfg.chain_id, &bytes, &inner, &inbound_tx);
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
}

fn parse_bootstrap(bootstrap: &[String]) -> Vec<Multiaddr> {
    bootstrap
        .iter()
        .filter_map(|addr| addr.parse::<Multiaddr>().ok())
        .collect()
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

    let mut swarm = match SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        ) {
        Ok(builder) => match builder.with_behaviour(|_| PulseBehaviour { gossipsub: gossip }) {
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
    for addr in parse_bootstrap(&cfg.bootstrap) {
        if let Err(e) = swarm.dial(addr.clone()) {
            note_swarm_event(&inner, format!("bootstrap-dial-failed:{addr}:{e}"));
        } else {
            note_swarm_event(&inner, format!("bootstrap-dialing:{addr}"));
        }
    }
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
                        dispatch_network_message(&cfg.chain_id, &message.data, &inner, &inbound_tx);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        note_swarm_event(&inner, format!("peer-connected:{peer_id}"));
                        register_peer_result(&inner, &peer_id.to_string(), true);
                        let _ = inbound_tx.send(InboundEvent::PeerConnected(peer_id.to_string()));
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        note_swarm_event(&inner, format!("peer-disconnected:{peer_id}"));
                        register_peer_result(&inner, &peer_id.to_string(), false);
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        if let Some(peer_id) = peer_id {
                            register_peer_result(&inner, &peer_id.to_string(), false);
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
                        note_swarm_event(&inner, format!("swarm:{other:?}"));
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
            inner.queued_messages += 1;
            inner.queued_block_messages += 1;
            track_queue_depth_on_enqueue(&mut inner);
        }
        self.outbound_tx
            .send(OutboundMessage::Block(block.clone()))
            .map_err(|e| PulseError::Internal(format!("p2p send failed: {e}")))
    }

    fn status(&self) -> Result<P2pStatus, PulseError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        refresh_connected_peers_from_health(&mut inner);
        let (peers_under_cooldown, peers_under_flap_guard, peer_recovery) =
            peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer =
            update_selected_sync_peer(&mut inner, &sync_candidates, now_unix());
        let connected_slots_in_use = inner.connected_peers.len();
        let available_connection_slots = inner
            .connection_slot_budget
            .saturating_sub(connected_slots_in_use);
        Ok(P2pStatus {
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
            queue_non_block_fair_picks: inner.queue_non_block_fair_picks,
            queue_starvation_relief_picks: inner.queue_starvation_relief_picks,
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
            tx_outbound_duplicates_suppressed: inner.tx_outbound_duplicates_suppressed,
            tx_outbound_first_seen_relayed: inner.tx_outbound_first_seen_relayed,
            tx_outbound_recovery_relayed: inner.tx_outbound_recovery_relayed,
            block_outbound_duplicates_suppressed: inner.block_outbound_duplicates_suppressed,
            block_outbound_first_seen_relayed: inner.block_outbound_first_seen_relayed,
            block_outbound_recovery_relayed: inner.block_outbound_recovery_relayed,
            last_drop_reason: inner.last_drop_reason.clone(),
            peer_reconnect_attempts: inner.peer_reconnect_attempts,
            peer_recovery_success_count: inner.peer_recovery_success_count,
            last_peer_recovery_unix: inner.last_peer_recovery_unix,
            peer_cooldown_suppressed_count: inner.peer_cooldown_suppressed_count,
            peer_flap_suppressed_count: inner.peer_flap_suppressed_count,
            peers_under_cooldown,
            peers_under_flap_guard,
            peer_recovery,
            sync_candidates,
            selected_sync_peer,
            connection_slot_budget: inner.connection_slot_budget,
            connected_slots_in_use,
            available_connection_slots,
            sync_selection_sticky_until_unix: (inner.sync_selection_sticky_until_unix > 0)
                .then_some(inner.sync_selection_sticky_until_unix),
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
    fn duplicate_tx_announcements_are_suppressed() {
        let inner = Arc::new(Mutex::new(InnerState::default()));
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel();
        let tx = sample_tx("tx-dup-announcement");
        let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
            chain_id: "testnet".into(),
            transaction: tx,
        })
        .expect("serialize tx announcement");

        dispatch_network_message("testnet", &wire, &inner, &inbound_tx);
        dispatch_network_message("testnet", &wire, &inner, &inbound_tx);

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
        assert!(should_relay_outbound_tx(&mut state, "tx-windowed", 1_000));
        assert!(!should_relay_outbound_tx(&mut state, "tx-windowed", 1_010));
        assert!(should_relay_outbound_tx(
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
        assert!(should_relay_outbound_tx(&mut state, "tx-recovery", 1_000));
        assert!(should_relay_outbound_block(
            &mut state,
            "block-recovery",
            1_000
        ));
        assert!(!should_relay_outbound_tx(&mut state, "tx-recovery", 1_001));
        assert!(!should_relay_outbound_block(
            &mut state,
            "block-recovery",
            1_001
        ));

        let shared = Arc::new(Mutex::new(state));
        register_peer_result_at(&shared, "peer-a", true, 1_002);

        let mut guard = shared.lock().unwrap();
        assert!(should_relay_outbound_tx(&mut guard, "tx-recovery", 1_003));
        assert!(should_relay_outbound_block(
            &mut guard,
            "block-recovery",
            1_003
        ));
        assert!(!should_relay_outbound_tx(&mut guard, "tx-recovery", 1_004));
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
            assert!(should_relay_outbound_tx(&mut guard, "tx-churn", 2_000));
            assert!(!should_relay_outbound_tx(&mut guard, "tx-churn", 2_001));
        }

        register_peer_result_at(&shared, "peer-a", false, 2_010);
        register_peer_result_at(&shared, "peer-a", true, 2_020);
        {
            let mut guard = shared.lock().unwrap();
            assert!(should_relay_outbound_tx(&mut guard, "tx-churn", 2_021));
            assert!(!should_relay_outbound_tx(&mut guard, "tx-churn", 2_022));
        }

        register_peer_result_at(&shared, "peer-a", false, 2_030);
        register_peer_result_at(&shared, "peer-a", true, 2_040);
        let mut guard = shared.lock().unwrap();
        assert!(should_relay_outbound_tx(&mut guard, "tx-churn", 2_041));
        assert!(!should_relay_outbound_tx(&mut guard, "tx-churn", 2_042));
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

    #[tokio::test]
    async fn restart_rehydrates_peer_health_without_claiming_real_connectivity() {
        let path = std::env::temp_dir().join(format!("pulsedag-peer-state-{}.json", now_unix()));
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

        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/30333".into(),
            bootstrap: vec!["peer-bootstrap".into()],
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
        let path =
            std::env::temp_dir().join(format!("pulsedag-peer-state-corrupt-{}.json", now_unix()));
        std::env::set_var("PULSEDAG_P2P_PEER_STATE_PATH", &path);
        fs::write(&path, b"{ definitely-not-json").expect("write corrupt peer snapshot");

        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/30334".into(),
            bootstrap: vec!["peer-bootstrap".into()],
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
            .any(|peer| peer.peer_id == "peer-bootstrap"));

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

        let (_cooldown, _flap, snapshot) = peer_recovery_snapshot(&state);
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].peer_id, "peer-a");
        assert_eq!(snapshot[1].peer_id, "peer-b");
        assert_eq!(snapshot[0].recovery_success_count, 2);
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

    #[tokio::test]
    async fn real_runtime_mode_initializes_without_loopback_labeling() {
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/0".into(),
            bootstrap: vec!["bootstrap-peer".into()],
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
            guard.peer_book.get("bootstrap-peer").map(|h| h.connected),
            Some(false)
        );
    }

    #[tokio::test]
    async fn real_runtime_clears_persisted_connected_flags_on_startup() {
        let path = std::env::temp_dir().join(format!(
            "pulsedag-peer-state-real-runtime-{}.json",
            now_unix()
        ));
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
            bootstrap: vec!["bootstrap-peer".into()],
            enable_mdns: false,
            enable_kademlia: false,
            connection_slot_budget: 8,
            sync_selection_stickiness_secs: 30,
            runtime: Libp2pRuntimeMode::RealSwarm,
        };

        let (handle, _rx) = Libp2pHandle::new(cfg).expect("real swarm handle should init");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let status = handle.status().expect("status should be available");

        assert!(status.connected_peers.is_empty());
        let guard = handle.inner.lock().unwrap();
        assert_ne!(
            guard.peer_book.get("persisted-peer").map(|h| h.connected),
            Some(true)
        );

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
    }
}
