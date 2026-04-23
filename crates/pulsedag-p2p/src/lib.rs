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
use tokio::time::{sleep, Duration};

use crate::messages::{message_id_for_block, message_id_for_tx, topic_names, NetworkMessage};

pub const P2P_MODE_MEMORY_SIMULATED: &str = "memory-simulated";
pub const P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON: &str = "libp2p-dev-loopback-skeleton";
pub const P2P_MODE_LIBP2P_REAL: &str = "libp2p-real";

pub fn mode_connected_peers_are_real_network(mode: &str) -> bool {
    mode == P2P_MODE_LIBP2P_REAL
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
    last_drop_reason: Option<String>,
    peer_book: HashMap<String, PeerHealth>,
    peer_state_path: Option<PathBuf>,
    peer_reconnect_attempts: u64,
    peer_recovery_success_count: u64,
    last_peer_recovery_unix: Option<u64>,
    peer_cooldown_suppressed_count: u64,
    peer_flap_suppressed_count: u64,
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
        inner.seen_message_ids.insert(message_id_for_tx(tx));
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
        inner.seen_message_ids.insert(message_id_for_block(block));
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
        let inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        let (peers_under_cooldown, peers_under_flap_guard, peer_recovery) =
            peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer = sync_candidates.first().map(|peer| peer.peer_id.clone());
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
            .collect();
    } else {
        state.connected_peers.clear();
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
        {
            let health = guard.peer_book.entry(peer.to_string()).or_default();
            health.reconnect_attempts = health.reconnect_attempts.saturating_add(1);
            health.last_seen_unix = Some(now);
            if success {
                health.connected = true;
                health.fail_streak = 0;
                health.next_retry_unix = now;
                health.score = (health.score + 8).min(200);
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
                health.score = (health.score - 16).max(-200);
                let exp = health.fail_streak.min(6);
                let backoff = 2u64.pow(exp);
                let mut next_retry_unix = now.saturating_add(backoff + peer_jitter(peer));
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
                if !guard.seen_message_ids.insert(id) {
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
                if !guard.seen_message_ids.insert(id) {
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
                    if !guard.seen_message_ids.insert(id) {
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

    loop {
        tokio::select! {
            Some(msg) = outbound_rx.recv() => {
                if let Ok(mut guard) = inner.lock() {
                    guard.queued_messages = guard.queued_messages.saturating_sub(1);
                }

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

    loop {
        tokio::select! {
            Some(msg) = outbound_rx.recv() => {
                if let Ok(mut guard) = inner.lock() {
                    guard.queued_messages = guard.queued_messages.saturating_sub(1);
                }

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
            inner.queued_messages += 1;
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
            inner.queued_messages += 1;
        }
        self.outbound_tx
            .send(OutboundMessage::Block(block.clone()))
            .map_err(|e| PulseError::Internal(format!("p2p send failed: {e}")))
    }

    fn status(&self) -> Result<P2pStatus, PulseError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        let (peers_under_cooldown, peers_under_flap_guard, peer_recovery) =
            peer_recovery_snapshot(&inner);
        let sync_candidates = sync_candidates_snapshot(&inner);
        let selected_sync_peer = sync_candidates.first().map(|peer| peer.peer_id.clone());
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

    #[test]
    fn peer_failures_increase_backoff_and_lower_score() {
        let state = Arc::new(Mutex::new(InnerState {
            peer_book: HashMap::from([("peer-a".to_string(), PeerHealth::default())]),
            ..Default::default()
        }));

        register_peer_result(&state, "peer-a", false);
        let first = state
            .lock()
            .unwrap()
            .peer_book
            .get("peer-a")
            .cloned()
            .unwrap();
        register_peer_result(&state, "peer-a", false);
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
    fn memory_mode_tracks_publish_metrics_by_topic() {
        let (handle, _inbound_rx) = MemoryP2pHandle::new("testnet".into(), vec!["peer-a".into()]);
        let tx = Transaction {
            txid: "tx-1".into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee: 10,
            nonce: 1,
        };
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
    fn connected_peers_truth_flag_is_mode_dependent() {
        assert!(!mode_connected_peers_are_real_network(
            P2P_MODE_MEMORY_SIMULATED
        ));
        assert!(!mode_connected_peers_are_real_network(
            P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON
        ));
        assert!(mode_connected_peers_are_real_network(P2P_MODE_LIBP2P_REAL));
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

    #[tokio::test]
    async fn real_runtime_mode_initializes_without_loopback_labeling() {
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/0".into(),
            bootstrap: vec!["bootstrap-peer".into()],
            enable_mdns: false,
            enable_kademlia: false,
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
            runtime: Libp2pRuntimeMode::RealSwarm,
        };

        let (handle, _rx) = Libp2pHandle::new(cfg).expect("real swarm handle should init");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let status = handle.status().expect("status should be available");

        assert!(status.connected_peers.is_empty());
        let guard = handle.inner.lock().unwrap();
        assert_eq!(
            guard.peer_book.get("persisted-peer").map(|h| h.connected),
            Some(false)
        );

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
    }
}
