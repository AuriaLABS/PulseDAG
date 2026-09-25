use std::collections::{HashMap, HashSet};

use pulsedag_core::{
    types::{Hash, Transaction},
    ChainState,
};
use pulsedag_p2p::messages::{
    CompactRelayCapabilitiesV1, CompactRelayControllerActionV1, CompactRelayControllerErrorV1,
    CompactRelayControllerTelemetryV1, CompactRelayControllerV1, CompactRelayRuntimeSessionBookV1,
    CompactRelayRuntimeSessionErrorV1, CompactRelayWireV1,
};

pub const COMPACT_RELAY_CAPABILITY_PROBE_RETRY_SECS_V1: u64 = 30;

#[derive(Debug, Default)]
pub struct CompactRelayProbeScheduleV1 {
    last_attempt_unix_by_peer: HashMap<String, u64>,
}

impl CompactRelayProbeScheduleV1 {
    pub fn probe_targets(
        &mut self,
        protocol_eligible_peers: &[String],
        compact_eligible_peers: &[String],
        now_unix: u64,
    ) -> Vec<String> {
        let protocol_eligible = protocol_eligible_peers
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let compact_eligible = compact_eligible_peers
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        self.last_attempt_unix_by_peer.retain(|peer_id, _| {
            protocol_eligible.contains(peer_id) && !compact_eligible.contains(peer_id)
        });

        let mut candidates = protocol_eligible_peers
            .iter()
            .filter(|peer_id| !compact_eligible.contains(*peer_id))
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();

        candidates
            .into_iter()
            .filter(|peer_id| {
                let ready = self
                    .last_attempt_unix_by_peer
                    .get(peer_id)
                    .map(|last| {
                        now_unix.saturating_sub(*last)
                            >= COMPACT_RELAY_CAPABILITY_PROBE_RETRY_SECS_V1
                    })
                    .unwrap_or(true);
                if ready {
                    self.last_attempt_unix_by_peer
                        .insert(peer_id.clone(), now_unix);
                }
                ready
            })
            .collect()
    }

    pub fn note_send_failure(&mut self, peer_id: &str) {
        self.last_attempt_unix_by_peer.remove(peer_id);
    }

    pub fn note_peer_connected(&mut self, peer_id: &str) {
        self.last_attempt_unix_by_peer.remove(peer_id);
    }
}

pub struct CompactRelayDaemonRuntimeV1 {
    controller: CompactRelayControllerV1,
    sessions: CompactRelayRuntimeSessionBookV1,
}

impl CompactRelayDaemonRuntimeV1 {
    pub fn new(chain_id: &str) -> Result<Self, CompactRelayRuntimeSessionErrorV1> {
        let mut sessions = CompactRelayRuntimeSessionBookV1::default();
        sessions.configure_local(chain_id, CompactRelayCapabilitiesV1::canonical(chain_id))?;
        Ok(Self {
            controller: CompactRelayControllerV1::default(),
            sessions,
        })
    }

    pub fn handle_inbound(
        &mut self,
        peer_id: &str,
        wire: &CompactRelayWireV1,
        chain: &ChainState,
    ) -> Result<Vec<CompactRelayControllerActionV1>, CompactRelayControllerErrorV1> {
        if matches!(wire, CompactRelayWireV1::Capabilities(_)) {
            self.peer_disconnected(peer_id);
        }
        let known_transactions = known_transactions_for_wire(chain, wire);
        self.controller
            .handle_wire(&mut self.sessions, peer_id, wire, &known_transactions)
    }

    pub fn peer_disconnected(&mut self, peer_id: &str) {
        self.controller
            .peer_disconnected(&mut self.sessions, peer_id);
    }

    pub fn handle_send_failure(
        &mut self,
        peer_id: &str,
        wire: &CompactRelayWireV1,
    ) -> Vec<CompactRelayControllerActionV1> {
        self.controller
            .handle_send_failure(&mut self.sessions, peer_id, wire)
    }

    pub fn telemetry(&self) -> CompactRelayControllerTelemetryV1 {
        self.controller.telemetry()
    }
}

fn known_transactions_for_wire(
    chain: &ChainState,
    wire: &CompactRelayWireV1,
) -> HashMap<Hash, Transaction> {
    let (retained_block_hash, txids) = match wire {
        CompactRelayWireV1::Announce(announcement) => (None, announcement.txids.as_slice()),
        CompactRelayWireV1::GetTransactions(request) => {
            (Some(&request.block_hash), request.txids.as_slice())
        }
        _ => return HashMap::new(),
    };

    let requested = txids.iter().collect::<HashSet<_>>();
    let mut known = HashMap::with_capacity(txids.len());

    if let Some(block) = retained_block_hash.and_then(|hash| chain.dag.blocks.get(hash)) {
        for transaction in &block.transactions {
            if requested.contains(&transaction.txid) {
                known.insert(transaction.txid.clone(), transaction.clone());
            }
        }
    }

    for txid in txids {
        if known.contains_key(txid) {
            continue;
        }
        if let Some(transaction) = chain.mempool.transactions.get(txid) {
            known.insert(txid.clone(), transaction.clone());
        }
    }

    known
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state,
        types::{compute_block_hash, compute_merkle_root, Block, BlockHeader, TxOutput},
    };
    use pulsedag_p2p::messages::{
        build_compact_block_announcement_v1, CompactTransactionRequestV1,
        COMPACT_DAG_RELAY_VERSION_V1,
    };

    fn transaction(txid: &str) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".to_string(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn block() -> Block {
        let transactions = vec![
            transaction("coinbase"),
            transaction("body-a"),
            transaction("body-b"),
        ];
        let header = BlockHeader {
            version: 1,
            parents: vec!["parent-a".to_string()],
            timestamp: 1,
            difficulty: 1,
            nonce: 1,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "state".to_string(),
            blue_score: 1,
            height: 1,
        };
        Block {
            hash: compute_block_hash(&header),
            header,
            transactions,
        }
    }

    #[test]
    fn compact_probe_schedule_is_sorted_deduplicated_and_bounded_by_cooldown() {
        let mut schedule = CompactRelayProbeScheduleV1::default();
        let protocol = vec![
            "peer-b".to_string(),
            "peer-a".to_string(),
            "peer-a".to_string(),
        ];
        let compact = vec!["peer-b".to_string()];

        assert_eq!(
            schedule.probe_targets(&protocol, &compact, 100),
            vec!["peer-a".to_string()]
        );
        assert!(schedule.probe_targets(&protocol, &compact, 129).is_empty());
        assert_eq!(
            schedule.probe_targets(&protocol, &compact, 130),
            vec!["peer-a".to_string()]
        );
    }

    #[test]
    fn compact_probe_schedule_retries_send_failure_and_reconnect_immediately() {
        let mut schedule = CompactRelayProbeScheduleV1::default();
        let protocol = vec!["peer-a".to_string()];
        let compact = Vec::<String>::new();

        assert_eq!(
            schedule.probe_targets(&protocol, &compact, 100),
            vec!["peer-a".to_string()]
        );
        schedule.note_send_failure("peer-a");
        assert_eq!(
            schedule.probe_targets(&protocol, &compact, 101),
            vec!["peer-a".to_string()]
        );

        schedule.note_peer_connected("peer-a");
        assert_eq!(
            schedule.probe_targets(&protocol, &compact, 102),
            vec!["peer-a".to_string()]
        );
    }

    #[test]
    fn compact_probe_schedule_stops_after_compact_authorization() {
        let mut schedule = CompactRelayProbeScheduleV1::default();
        let protocol = vec!["peer-a".to_string()];
        assert_eq!(
            schedule.probe_targets(&protocol, &[], 100),
            vec!["peer-a".to_string()]
        );

        let compact = vec!["peer-a".to_string()];
        assert!(schedule.probe_targets(&protocol, &compact, 200).is_empty());
        assert!(schedule.last_attempt_unix_by_peer.is_empty());
    }

    #[test]
    fn lookup_is_bounded_to_wire_referenced_transactions() {
        let mut chain = init_chain_state("compact-daemon-test".to_string());
        chain
            .mempool
            .transactions
            .insert("body-a".to_string(), transaction("body-a"));
        chain
            .mempool
            .transactions
            .insert("unrelated".to_string(), transaction("unrelated"));

        let request = CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "block-a".to_string(),
            txids: vec!["body-a".to_string(), "missing".to_string()],
        });
        let known = known_transactions_for_wire(&chain, &request);

        assert_eq!(known.len(), 1);
        assert!(known.contains_key("body-a"));
        assert!(!known.contains_key("unrelated"));
    }

    #[test]
    fn body_service_can_use_retained_block_after_mempool_eviction() {
        let mut chain = init_chain_state("compact-daemon-test".to_string());
        let block = block();
        chain.dag.blocks.insert(block.hash.clone(), block.clone());

        let request = CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            txids: vec!["body-b".to_string()],
        });
        let known = known_transactions_for_wire(&chain, &request);

        let observed = known.get("body-b").expect("retained block transaction");
        assert_eq!(
            serde_json::to_vec(observed).unwrap(),
            serde_json::to_vec(&transaction("body-b")).unwrap()
        );
    }

    #[test]
    fn fresh_capabilities_clear_stale_pending_reconstruction() {
        let chain_id = "compact-daemon-test";
        let peer = "peer-a";
        let mut runtime = CompactRelayDaemonRuntimeV1::new(chain_id).unwrap();
        let mut chain = init_chain_state(chain_id.to_string());
        chain
            .mempool
            .transactions
            .insert("coinbase".to_string(), transaction("coinbase"));

        let capabilities =
            CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(chain_id));
        runtime.handle_inbound(peer, &capabilities, &chain).unwrap();

        let announcement = build_compact_block_announcement_v1(&block()).unwrap();
        let actions = runtime
            .handle_inbound(peer, &CompactRelayWireV1::Announce(announcement), &chain)
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(_),
                ..
            }]
        ));
        assert_eq!(runtime.telemetry().pending_announcements_current, 1);

        runtime.handle_inbound(peer, &capabilities, &chain).unwrap();
        assert_eq!(runtime.telemetry().pending_announcements_current, 0);
    }

    #[test]
    fn failed_body_request_send_abandons_pending_and_requests_full_block() {
        let chain_id = "compact-daemon-test";
        let peer = "peer-a";
        let mut runtime = CompactRelayDaemonRuntimeV1::new(chain_id).unwrap();
        let mut chain = init_chain_state(chain_id.to_string());
        chain
            .mempool
            .transactions
            .insert("coinbase".to_string(), transaction("coinbase"));

        let capabilities =
            CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(chain_id));
        runtime.handle_inbound(peer, &capabilities, &chain).unwrap();

        let source_block = block();
        let announcement = build_compact_block_announcement_v1(&source_block).unwrap();
        let actions = runtime
            .handle_inbound(peer, &CompactRelayWireV1::Announce(announcement), &chain)
            .unwrap();
        let failed_wire = match actions.as_slice() {
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(request),
                ..
            }] => CompactRelayWireV1::GetTransactions(request.clone()),
            other => panic!("unexpected actions: {other:?}"),
        };
        assert_eq!(runtime.telemetry().pending_announcements_current, 1);

        let recovery = runtime.handle_send_failure(peer, &failed_wire);
        assert!(matches!(
            recovery.as_slice(),
            [CompactRelayControllerActionV1::RequestFullBlock {
                peer_id,
                block_hash,
            }] if peer_id == peer && block_hash == &source_block.hash
        ));
        let telemetry = runtime.telemetry();
        assert_eq!(telemetry.pending_announcements_current, 0);
        assert_eq!(telemetry.full_block_requests_total, 1);
    }

    #[test]
    fn failed_body_response_send_falls_back_to_full_block_service() {
        let chain_id = "compact-daemon-test";
        let peer = "peer-a";
        let mut runtime = CompactRelayDaemonRuntimeV1::new(chain_id).unwrap();
        let mut chain = init_chain_state(chain_id.to_string());
        let source_block = block();
        chain
            .dag
            .blocks
            .insert(source_block.hash.clone(), source_block.clone());

        let capabilities =
            CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(chain_id));
        runtime.handle_inbound(peer, &capabilities, &chain).unwrap();

        let request = CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: source_block.hash.clone(),
            txids: vec!["body-b".to_string()],
        });
        let actions = runtime.handle_inbound(peer, &request, &chain).unwrap();
        let failed_wire = match actions.as_slice() {
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::Transactions(response),
                ..
            }] => CompactRelayWireV1::Transactions(response.clone()),
            other => panic!("unexpected actions: {other:?}"),
        };

        let recovery = runtime.handle_send_failure(peer, &failed_wire);
        assert!(matches!(
            recovery.as_slice(),
            [CompactRelayControllerActionV1::ServeFullBlock {
                peer_id,
                block_hash,
            }] if peer_id == peer && block_hash == &source_block.hash
        ));
        assert_eq!(runtime.telemetry().full_block_service_fallback_total, 1);
    }

    #[test]
    fn peer_disconnect_clears_stale_pending_reconstruction() {
        let chain_id = "compact-daemon-test";
        let peer = "peer-a";
        let mut runtime = CompactRelayDaemonRuntimeV1::new(chain_id).unwrap();
        let mut chain = init_chain_state(chain_id.to_string());
        chain
            .mempool
            .transactions
            .insert("coinbase".to_string(), transaction("coinbase"));

        let capabilities =
            CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(chain_id));
        runtime.handle_inbound(peer, &capabilities, &chain).unwrap();

        let announcement = build_compact_block_announcement_v1(&block()).unwrap();
        let actions = runtime
            .handle_inbound(peer, &CompactRelayWireV1::Announce(announcement), &chain)
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(_),
                ..
            }]
        ));
        assert_eq!(runtime.telemetry().pending_announcements_current, 1);

        runtime.peer_disconnected(peer);
        assert_eq!(runtime.telemetry().pending_announcements_current, 0);
    }

    #[test]
    fn announcement_lookup_never_clones_unreferenced_mempool_entries() {
        let mut chain = init_chain_state("compact-daemon-test".to_string());
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        for transaction in &block.transactions {
            chain
                .mempool
                .transactions
                .insert(transaction.txid.clone(), transaction.clone());
        }
        chain
            .mempool
            .transactions
            .insert("unrelated".to_string(), transaction("unrelated"));

        let known = known_transactions_for_wire(
            &chain,
            &CompactRelayWireV1::Announce(announcement.clone()),
        );

        assert_eq!(known.len(), announcement.txids.len());
        assert!(!known.contains_key("unrelated"));
    }
}
