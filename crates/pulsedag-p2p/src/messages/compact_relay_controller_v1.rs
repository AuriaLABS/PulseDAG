use std::collections::{BTreeMap, HashMap};

use pulsedag_core::types::{Block, Hash, Transaction};

use super::compact_relay_carrier_v1::CompactRelayWireV1;
use super::compact_relay_runtime_v1::{
    CompactRelayRuntimeSessionBookV1, CompactRelayRuntimeSessionErrorV1,
};
use super::compact_relay_v1::{
    build_compact_transaction_response_v1, plan_compact_block_reconstruction_for_chain_v1,
    CompactBlockAnnouncementV1, CompactBlockReconstructionPlanV1, CompactRelayErrorV1,
};

#[derive(Debug, Clone)]
pub enum CompactRelayControllerActionV1 {
    Send {
        peer_id: String,
        wire: CompactRelayWireV1,
    },
    ReconstructedBlockReady {
        peer_id: String,
        block: Block,
    },
    RequestFullBlock {
        peer_id: String,
        block_hash: Hash,
    },
    ServeFullBlock {
        peer_id: String,
        block_hash: Hash,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayControllerErrorV1 {
    Runtime(CompactRelayRuntimeSessionErrorV1),
    Compact(CompactRelayErrorV1),
    PendingAnnouncementMissing { peer_id: String, block_hash: Hash },
}

impl From<CompactRelayRuntimeSessionErrorV1> for CompactRelayControllerErrorV1 {
    fn from(value: CompactRelayRuntimeSessionErrorV1) -> Self {
        Self::Runtime(value)
    }
}

impl From<CompactRelayErrorV1> for CompactRelayControllerErrorV1 {
    fn from(value: CompactRelayErrorV1) -> Self {
        Self::Compact(value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompactRelayControllerV1 {
    pending_announcements: BTreeMap<(String, Hash), CompactBlockAnnouncementV1>,
}

impl CompactRelayControllerV1 {
    pub fn pending_count(&self, peer_id: &str) -> usize {
        self.pending_announcements
            .keys()
            .filter(|(owner, _)| owner == peer_id)
            .count()
    }

    pub fn peer_disconnected(
        &mut self,
        sessions: &mut CompactRelayRuntimeSessionBookV1,
        peer_id: &str,
    ) {
        sessions.peer_disconnected(peer_id);
        self.pending_announcements
            .retain(|(owner, _), _| owner != peer_id);
    }

    pub fn handle_wire(
        &mut self,
        sessions: &mut CompactRelayRuntimeSessionBookV1,
        peer_id: &str,
        wire: &CompactRelayWireV1,
        known_transactions: &HashMap<Hash, Transaction>,
    ) -> Result<Vec<CompactRelayControllerActionV1>, CompactRelayControllerErrorV1> {
        sessions.note_inbound(peer_id, wire)?;

        match wire {
            CompactRelayWireV1::CapabilityProbe => {
                let capabilities = sessions
                    .local_capabilities()
                    .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?
                    .clone();
                Ok(vec![CompactRelayControllerActionV1::Send {
                    peer_id: peer_id.to_string(),
                    wire: CompactRelayWireV1::Capabilities(capabilities),
                }])
            }
            CompactRelayWireV1::Capabilities(_) => Ok(Vec::new()),
            CompactRelayWireV1::Announce(announcement) => {
                let chain_id = sessions
                    .local_capabilities()
                    .ok_or(CompactRelayRuntimeSessionErrorV1::LocalCapabilitiesMissing)?
                    .chain_id
                    .clone();
                match plan_compact_block_reconstruction_for_chain_v1(
                    announcement,
                    known_transactions,
                    &chain_id,
                )? {
                    CompactBlockReconstructionPlanV1::Complete(block) => {
                        sessions.abandon_in_flight(peer_id, &announcement.block_hash);
                        self.pending_announcements
                            .remove(&(peer_id.to_string(), announcement.block_hash.clone()));
                        Ok(vec![
                            CompactRelayControllerActionV1::ReconstructedBlockReady {
                                peer_id: peer_id.to_string(),
                                block,
                            },
                        ])
                    }
                    CompactBlockReconstructionPlanV1::RequestTransactions(state) => {
                        let request = state.request.clone();
                        sessions.register_in_flight(peer_id, state)?;
                        self.pending_announcements.insert(
                            (peer_id.to_string(), announcement.block_hash.clone()),
                            announcement.clone(),
                        );
                        Ok(vec![CompactRelayControllerActionV1::Send {
                            peer_id: peer_id.to_string(),
                            wire: CompactRelayWireV1::GetTransactions(request),
                        }])
                    }
                    CompactBlockReconstructionPlanV1::FullBlockFallback { block_hash, .. } => {
                        sessions.abandon_in_flight(peer_id, &block_hash);
                        self.pending_announcements
                            .remove(&(peer_id.to_string(), block_hash.clone()));
                        Ok(vec![CompactRelayControllerActionV1::RequestFullBlock {
                            peer_id: peer_id.to_string(),
                            block_hash,
                        }])
                    }
                }
            }
            CompactRelayWireV1::GetTransactions(request) => {
                match build_compact_transaction_response_v1(request, known_transactions)? {
                    Some(response) => Ok(vec![CompactRelayControllerActionV1::Send {
                        peer_id: peer_id.to_string(),
                        wire: CompactRelayWireV1::Transactions(response),
                    }]),
                    None => Ok(vec![CompactRelayControllerActionV1::ServeFullBlock {
                        peer_id: peer_id.to_string(),
                        block_hash: request.block_hash.clone(),
                    }]),
                }
            }
            CompactRelayWireV1::Transactions(response) => {
                let key = (peer_id.to_string(), response.block_hash.clone());
                let announcement =
                    self.pending_announcements
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| {
                            CompactRelayControllerErrorV1::PendingAnnouncementMissing {
                                peer_id: peer_id.to_string(),
                                block_hash: response.block_hash.clone(),
                            }
                        })?;

                let result = sessions.complete_in_flight(peer_id, &announcement, response);
                match result {
                    Ok(CompactBlockReconstructionPlanV1::Complete(block)) => {
                        self.pending_announcements.remove(&key);
                        Ok(vec![
                            CompactRelayControllerActionV1::ReconstructedBlockReady {
                                peer_id: peer_id.to_string(),
                                block,
                            },
                        ])
                    }
                    Ok(CompactBlockReconstructionPlanV1::RequestTransactions(state)) => {
                        let request = state.request.clone();
                        sessions.register_in_flight(peer_id, state)?;
                        Ok(vec![CompactRelayControllerActionV1::Send {
                            peer_id: peer_id.to_string(),
                            wire: CompactRelayWireV1::GetTransactions(request),
                        }])
                    }
                    Ok(CompactBlockReconstructionPlanV1::FullBlockFallback {
                        block_hash, ..
                    }) => {
                        self.pending_announcements.remove(&key);
                        Ok(vec![CompactRelayControllerActionV1::RequestFullBlock {
                            peer_id: peer_id.to_string(),
                            block_hash,
                        }])
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::compact_relay_carrier_v1::CompactRelayCapabilitiesV1;
    use super::super::compact_relay_v1::{
        build_compact_block_announcement_v1, COMPACT_DAG_RELAY_VERSION_V1,
    };
    use super::*;
    use pulsedag_core::types::{compute_block_hash, compute_merkle_root, BlockHeader, TxOutput};

    const CHAIN_ID: &str = "compact-relay-controller-testnet";
    const PEER: &str = "peer-controller";

    fn same_block(left: &Block, right: &Block) -> bool {
        serde_json::to_vec(left).unwrap() == serde_json::to_vec(right).unwrap()
    }

    fn transaction(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn block() -> Block {
        let transactions = vec![
            transaction("coinbase"),
            transaction("tx-a"),
            transaction("tx-b"),
        ];
        let header = BlockHeader {
            version: 1,
            parents: vec!["parent-a".into(), "parent-b".into()],
            timestamp: 1,
            difficulty: 1,
            nonce: 1,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "state".into(),
            blue_score: 2,
            height: 2,
        };
        Block {
            hash: compute_block_hash(&header),
            header,
            transactions,
        }
    }

    fn configured() -> (CompactRelayControllerV1, CompactRelayRuntimeSessionBookV1) {
        let controller = CompactRelayControllerV1::default();
        let mut sessions = CompactRelayRuntimeSessionBookV1::default();
        sessions
            .configure_local(CHAIN_ID, CompactRelayCapabilitiesV1::canonical(CHAIN_ID))
            .unwrap();
        (controller, sessions)
    }

    fn authorize(
        controller: &mut CompactRelayControllerV1,
        sessions: &mut CompactRelayRuntimeSessionBookV1,
    ) {
        controller
            .handle_wire(
                sessions,
                PEER,
                &CompactRelayWireV1::Capabilities(CompactRelayCapabilitiesV1::canonical(CHAIN_ID)),
                &HashMap::new(),
            )
            .unwrap();
    }

    #[test]
    fn probe_returns_exact_local_capabilities() {
        let (mut controller, mut sessions) = configured();
        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::CapabilityProbe,
                &HashMap::new(),
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::Send {
                peer_id,
                wire: CompactRelayWireV1::Capabilities(capabilities),
            }] if peer_id == PEER
                && capabilities == &CompactRelayCapabilitiesV1::canonical(CHAIN_ID)
        ));
    }

    #[test]
    fn complete_local_announcement_submits_one_canonical_block() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = block
            .transactions
            .iter()
            .cloned()
            .map(|transaction| (transaction.txid.clone(), transaction))
            .collect();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &known,
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ReconstructedBlockReady { peer_id, block: rebuilt }]
                if peer_id == PEER && same_block(rebuilt, &block)
        ));
    }

    #[test]
    fn missing_bodies_roundtrip_to_canonical_submission() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = [(
            block.transactions[0].txid.clone(),
            block.transactions[0].clone(),
        )]
        .into_iter()
        .collect();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &known,
            )
            .unwrap();
        let request = match actions.as_slice() {
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(request),
                ..
            }] => request.clone(),
            other => panic!("unexpected actions: {other:?}"),
        };
        assert_eq!(controller.pending_count(PEER), 1);

        let transactions = request
            .txids
            .iter()
            .map(|txid| {
                block
                    .transactions
                    .iter()
                    .find(|transaction| &transaction.txid == txid)
                    .unwrap()
                    .clone()
            })
            .collect();
        let response = super::super::compact_relay_v1::CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            transactions,
        };

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Transactions(response),
                &known,
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ReconstructedBlockReady { block: rebuilt, .. }]
                if same_block(rebuilt, &block)
        ));
        assert_eq!(controller.pending_count(PEER), 0);
        assert_eq!(sessions.in_flight_count(PEER), 0);
    }

    #[test]
    fn repeated_complete_announcement_clears_stale_request_and_late_response() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let initially_known = [(
            block.transactions[0].txid.clone(),
            block.transactions[0].clone(),
        )]
        .into_iter()
        .collect();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement.clone()),
                &initially_known,
            )
            .unwrap();
        let request = match actions.as_slice() {
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(request),
                ..
            }] => request.clone(),
            other => panic!("unexpected actions: {other:?}"),
        };
        assert_eq!(controller.pending_count(PEER), 1);
        assert_eq!(sessions.in_flight_count(PEER), 1);

        let all_known = block
            .transactions
            .iter()
            .cloned()
            .map(|transaction| (transaction.txid.clone(), transaction))
            .collect();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &all_known,
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ReconstructedBlockReady { block: rebuilt, .. }]
                if same_block(rebuilt, &block)
        ));
        assert_eq!(controller.pending_count(PEER), 0);
        assert_eq!(sessions.in_flight_count(PEER), 0);

        let transactions = request
            .txids
            .iter()
            .map(|txid| all_known.get(txid).unwrap().clone())
            .collect();
        let late_response = super::super::compact_relay_v1::CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            transactions,
        };

        let error = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Transactions(late_response),
                &all_known,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            CompactRelayControllerErrorV1::PendingAnnouncementMissing {
                peer_id,
                block_hash,
            } if peer_id == PEER && block_hash == block.hash
        ));
    }

    #[test]
    fn commitment_mismatch_requests_full_block_without_body_request() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let mut announcement = build_compact_block_announcement_v1(&block).unwrap();
        announcement.header.merkle_root = "forged".into();
        announcement.block_hash = compute_block_hash(&announcement.header);
        let expected_hash = announcement.block_hash.clone();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &HashMap::new(),
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::RequestFullBlock {
                peer_id,
                block_hash,
            }] if peer_id == PEER && block_hash == &expected_hash
        ));
    }

    #[test]
    fn oversized_transaction_response_uses_full_block_service() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);

        let txid = "tx-large".to_string();
        let mut large = transaction(&txid);
        large.outputs[0].address = "x"
            .repeat(super::super::compact_relay_carrier_v1::COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);
        let known = [(txid.clone(), large)].into_iter().collect();
        let request = super::super::compact_relay_v1::CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "oversized-response-block".into(),
            txids: vec![txid],
        };

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::GetTransactions(request),
                &known,
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ServeFullBlock {
                peer_id,
                block_hash,
            }] if peer_id == PEER && block_hash == "oversized-response-block"
        ));
    }

    #[test]
    fn unavailable_requested_body_requires_full_block_service() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let request = super::super::compact_relay_v1::CompactTransactionRequestV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "missing-body-block".into(),
            txids: vec!["tx-missing".into()],
        };

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::GetTransactions(request),
                &HashMap::new(),
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ServeFullBlock {
                peer_id,
                block_hash,
            }] if peer_id == PEER && block_hash == "missing-body-block"
        ));
    }

    #[test]
    fn invalid_response_preserves_controller_and_runtime_state_for_retry() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = [(
            block.transactions[0].txid.clone(),
            block.transactions[0].clone(),
        )]
        .into_iter()
        .collect();

        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &known,
            )
            .unwrap();
        let request = match actions.as_slice() {
            [CompactRelayControllerActionV1::Send {
                wire: CompactRelayWireV1::GetTransactions(request),
                ..
            }] => request.clone(),
            other => panic!("unexpected actions: {other:?}"),
        };

        let invalid = super::super::compact_relay_v1::CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            transactions: vec![transaction("wrong-response")],
        };
        assert!(controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Transactions(invalid),
                &known,
            )
            .is_err());
        assert_eq!(controller.pending_count(PEER), 1);
        assert_eq!(sessions.in_flight_count(PEER), 1);

        let transactions = request
            .txids
            .iter()
            .map(|txid| {
                block
                    .transactions
                    .iter()
                    .find(|transaction| &transaction.txid == txid)
                    .unwrap()
                    .clone()
            })
            .collect();
        let valid = super::super::compact_relay_v1::CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            transactions,
        };
        let actions = controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Transactions(valid),
                &known,
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [CompactRelayControllerActionV1::ReconstructedBlockReady { block: rebuilt, .. }]
                if same_block(rebuilt, &block)
        ));
        assert_eq!(controller.pending_count(PEER), 0);
        assert_eq!(sessions.in_flight_count(PEER), 0);
    }

    #[test]
    fn disconnect_clears_controller_and_runtime_pending_state() {
        let (mut controller, mut sessions) = configured();
        authorize(&mut controller, &mut sessions);
        let block = block();
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = [(
            block.transactions[0].txid.clone(),
            block.transactions[0].clone(),
        )]
        .into_iter()
        .collect();
        controller
            .handle_wire(
                &mut sessions,
                PEER,
                &CompactRelayWireV1::Announce(announcement),
                &known,
            )
            .unwrap();
        assert_eq!(controller.pending_count(PEER), 1);
        assert_eq!(sessions.in_flight_count(PEER), 1);

        controller.peer_disconnected(&mut sessions, PEER);
        assert_eq!(controller.pending_count(PEER), 0);
        assert_eq!(sessions.in_flight_count(PEER), 0);
        assert!(!sessions.peer_session_authorized(PEER));
    }
}
