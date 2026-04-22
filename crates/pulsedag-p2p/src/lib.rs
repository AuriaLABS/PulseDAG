pub mod messages;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::{gossipsub, identity, PeerId};
use pulsedag_core::{
    errors::PulseError,
    types::{Block, Transaction},
};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::messages::{message_id_for_block, message_id_for_tx, topic_names, NetworkMessage};

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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PeerHealth {
    score: i32,
    fail_streak: u32,
    next_retry_unix: u64,
    connected: bool,
}

impl Default for PeerHealth {
    fn default() -> Self {
        Self {
            score: 100,
            fail_streak: 0,
            next_retry_unix: 0,
            connected: true,
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
        state.mode = "memory-simulated".into();
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
        let mut state = InnerState {
            mode: "libp2p-swarm-skeleton".into(),
            runtime_mode_detail: "swarm-poll-loop-skeleton".into(),
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
        let mut candidates = state
            .peer_book
            .iter()
            .filter(|(_, v)| v.connected)
            .map(|(k, v)| (k.clone(), v.score))
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        state.connected_peers = candidates.into_iter().map(|(peer, _)| peer).collect();
        state.topics = topics.clone();
        state.subscriptions_active = topics.len();
        state.mdns = cfg.enable_mdns;
        state.kademlia = cfg.enable_kademlia;
        state.runtime_started = true;
        let inner = Arc::new(Mutex::new(state));

        tokio::spawn(run_libp2p_runtime(
            cfg,
            peer_id,
            topic_objs,
            inner.clone(),
            outbound_rx,
            inbound_tx,
        ));

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
    let now = now_unix();
    if let Ok(mut guard) = inner.lock() {
        let health = guard.peer_book.entry(peer.to_string()).or_default();
        if success {
            health.connected = true;
            health.fail_streak = 0;
            health.next_retry_unix = now;
            health.score = (health.score + 8).min(200);
        } else {
            health.connected = false;
            health.fail_streak = health.fail_streak.saturating_add(1);
            health.score = (health.score - 16).max(-200);
            let exp = health.fail_streak.min(6);
            let backoff = 2u64.pow(exp);
            health.next_retry_unix = now.saturating_add(backoff + peer_jitter(peer));
        }

        let mut candidates = guard
            .peer_book
            .iter()
            .filter(|(_, v)| v.connected)
            .map(|(k, v)| (k.clone(), v.score))
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        guard.connected_peers = candidates.into_iter().map(|(peer, _)| peer).collect();
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

    #[tokio::test]
    async fn restart_rehydrates_peer_health_from_persisted_state() {
        let path = std::env::temp_dir().join(format!("pulsedag-peer-state-{}.json", now_unix()));
        std::env::set_var("PULSEDAG_P2P_PEER_STATE_PATH", &path);

        let persisted = HashMap::from([(
            "peer-rejoin".to_string(),
            PeerHealth {
                score: 145,
                fail_streak: 0,
                next_retry_unix: now_unix(),
                connected: true,
            },
        )]);
        persist_peer_book(&path, &persisted);

        let cfg = Libp2pConfig {
            chain_id: "testnet".into(),
            listen_addr: "/ip4/127.0.0.1/tcp/30333".into(),
            bootstrap: vec!["peer-bootstrap".into()],
            enable_mdns: false,
            enable_kademlia: false,
        };
        let (handle, _rx) = Libp2pHandle::new(cfg).expect("libp2p handle should init");
        let status = handle.status().expect("status should work");

        assert!(status.connected_peers.iter().any(|p| p == "peer-rejoin"));
        assert!(status.connected_peers.iter().any(|p| p == "peer-bootstrap"));

        std::env::remove_var("PULSEDAG_P2P_PEER_STATE_PATH");
        let _ = fs::remove_file(path);
    }
}
