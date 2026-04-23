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
    types::{Block, Transaction},
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
    pub next_retry_unix: u64,
    pub reconnect_attempts: u64,
    pub recovery_success_count: u64,
    pub last_recovery_unix: Option<u64>,
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
    reconnect_attempts: u64,
    recovery_success_count: u64,
    last_recovery_unix: Option<u64>,
    last_failure_unix: Option<u64>,
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
            reconnect_attempts: 0,
            recovery_success_count: 0,
            last_recovery_unix: None,
            last_failure_unix: None,
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

        let peer_book = cfg
            .bootstrap
            .iter()
            .map(|peer| (peer.clone(), PeerHealth::default()))
            .collect();
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
        if mode_connected_peers_are_real_network(&state.mode) {
            let mut candidates = state
                .peer_book
                .iter()
                .filter(|(_, v)| v.connected)
                .map(|(k, v)| (k.clone(), v.score))
                .collect::<Vec<_>>();
            candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            state.connected_peers = candidates.into_iter().map(|(peer, _)| peer).collect();
        } else {
            state.connected_peers.clear();
        }
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
            next_retry_unix: health.next_retry_unix,
            reconnect_attempts: health.reconnect_attempts,
            recovery_success_count: health.recovery_success_count,
            last_recovery_unix: health.last_recovery_unix,
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PeerBookSnapshot {
    peer_book: HashMap<String, PeerHealth>,
}

fn peer_state_path() -> Option<PathBuf> {
    std::env::var("PULSEDAG_P2P_PEER_STATE_PATH")
        .ok()
        .map(PathBuf::from)
}

fn load_peer_book(path: &PathBuf) -> HashMap<String, PeerHealth> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PeerBookSnapshot>(&bytes).ok())
        .map(|snapshot| snapshot.peer_book)
        .unwrap_or_default()
}

fn persist_peer_book(path: &PathBuf, peer_book: &HashMap<String, PeerHealth>) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let snapshot = PeerBookSnapshot {
        peer_book: peer_book.clone(),
    };
    if let Ok(bytes) = serde_json::to_vec(&snapshot) {
        let _ = fs::write(path, bytes);
    }
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
            if success {
                health.connected = true;
                health.fail_streak = 0;
                health.next_retry_unix = now;
                health.score = (health.score + 8).min(200);
                health.recovery_success_count = health.recovery_success_count.saturating_add(1);
                health.last_recovery_unix = Some(now);
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
                health.next_retry_unix = next_retry_unix;
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

        if mode_connected_peers_are_real_network(&guard.mode) {
            let mut candidates = guard
                .peer_book
                .iter()
                .filter(|(_, v)| v.connected)
                .map(|(k, v)| (k.clone(), v.score))
                .collect::<Vec<_>>();
            candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            guard.connected_peers = candidates.into_iter().map(|(peer, _)| peer).collect();
        } else {
            guard.connected_peers.clear();
        }
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

        let persisted = HashMap::from([(
            "peer-rejoin".to_string(),
            PeerHealth {
                score: 145,
                fail_streak: 0,
                next_retry_unix: now_unix(),
                connected: true,
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

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
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

    #[tokio::test]
    async fn real_runtime_mode_initializes_without_loopback_labeling() {
        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/0".into(),
            bootstrap: vec![],
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
    }
}
