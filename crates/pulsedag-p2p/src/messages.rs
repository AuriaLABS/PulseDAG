pub mod capability_carrier_v1;
pub mod dag_sync_v2;
pub mod fast_sync_carrier_v1;
pub mod fast_sync_runtime_v1;
pub mod frontier_reconcile_v1;
pub mod frontier_response_v1;
mod network_message_decode_v1;
pub mod protocol_v2;
pub mod recovery_progress_v1;
pub mod selected_locator_v1;
pub mod sync_carrier_v1;
pub mod sync_wire_v2;
mod wire_limits_v1;

use serde::{Deserialize, Deserializer, Serialize};

use pulsedag_core::{
    types::{Block, BlockHeader, Hash, Transaction},
    BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
};

pub use capability_carrier_v1::{
    decode_network_message_with_capabilities_v1, encode_network_message_with_capabilities_v1,
    DecodedNetworkMessageWithCapabilitiesV1, ProtocolCapabilityCarrierErrorV1,
    PROTOCOL_CAPABILITY_EXTENSION_FIELD_V1,
};
pub use dag_sync_v2::{
    DagFrontierEntryV1, DagFrontierResponseV1, DagSyncContractError, SelectedChainLocatorV1,
    MAX_DAG_FRONTIER_ENTRIES, MAX_DAG_FRONTIER_PARENTS, MAX_DAG_FRONTIER_REQUIRED_CONTEXT,
    MAX_SELECTED_CHAIN_LOCATOR_HASHES, MAX_SELECTED_CHAIN_SUFFIX_HASHES,
    P2P_DAG_SYNC_CONTRACT_VERSION,
};
pub use frontier_reconcile_v1::{
    plan_dag_frontier_reconciliation_v1, DagFrontierReconcileError, DagFrontierReconcilePlanV1,
};
pub use frontier_response_v1::{build_dag_frontier_response_v1, DagFrontierBuildErrorV1};
pub use protocol_v2::{
    require_protocol_compatibility_v1, ProtocolCapabilitiesV1, ProtocolCapabilityHandshakeV1,
    ProtocolCompatibilityError, ProtocolMessageClassV1, ProtocolPeerCompatibilityV1,
    ProtocolPeerRouteActionV1, ProtocolPeerRouteDecisionV1, ProtocolPeerRouterV1,
    ProtocolPeerStateV1, P2P_PROTOCOL_CAPABILITIES_VERSION,
};
pub use recovery_progress_v1::{
    RecoveryProgressDecisionV1, RecoveryProgressObservationV1, RecoveryProgressReasonV1,
    RecoveryProgressTrackerV1,
};
pub use selected_locator_v1::{
    build_selected_chain_locator_v1, resolve_selected_common_ancestor_v1, SelectedCommonAncestorV1,
    SelectedLocatorError, SELECTED_LOCATOR_LINEAR_WINDOW,
};
pub use sync_carrier_v1::{
    attach_protocol_sync_carrier_v1, decode_network_message_with_protocol_sync_for_peer_v1,
    decode_network_message_with_protocol_sync_v1, encode_network_message_with_protocol_sync_v1,
    DecodedNetworkMessageWithProtocolSyncV1, ProtocolSyncCarrierErrorV1, ProtocolSyncCarrierV1,
    PROTOCOL_SYNC_EXTENSION_FIELD_V1,
};
pub use sync_wire_v2::{
    plan_protocol_sync_dispatch_v1, ProtocolSyncDispatchActionV1, ProtocolSyncDispatchPlanV1,
    ProtocolSyncWireError, ProtocolSyncWireV1,
};
pub use wire_limits_v1::{P2P_WIRE_MAX_INVENTORY_ITEMS_V1, P2P_WIRE_MAX_REQUEST_ITEMS_V1};

fn supported_header_version(version: u32) -> bool {
    matches!(version, BLOCK_HEADER_VERSION_V1 | BLOCK_HEADER_VERSION_V2)
}

fn deserialize_supported_block_header<'de, D>(deserializer: D) -> Result<BlockHeader, D::Error>
where
    D: Deserializer<'de>,
{
    let header = BlockHeader::deserialize(deserializer)?;
    if supported_header_version(header.version) {
        Ok(header)
    } else {
        Err(serde::de::Error::custom(format!(
            "unsupported block header version {}",
            header.version
        )))
    }
}

fn deserialize_supported_optional_block<'de, D>(deserializer: D) -> Result<Option<Block>, D::Error>
where
    D: Deserializer<'de>,
{
    let block = Option::<Block>::deserialize(deserializer)?;
    if let Some(block) = &block {
        if !supported_header_version(block.header.version) {
            return Err(serde::de::Error::custom(format!(
                "unsupported block header version {}",
                block.header.version
            )));
        }
    }
    Ok(block)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderInventory {
    pub hash: Hash,
    #[serde(deserialize_with = "deserialize_supported_block_header")]
    pub header: BlockHeader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeaderAnnouncement {
    pub hash: Hash,
    #[serde(deserialize_with = "deserialize_supported_block_header")]
    pub header: BlockHeader,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TipInventoryStatus {
    pub chain_id: String,
    pub selected_tip: Option<Hash>,
    pub selected_height: Option<u64>,
    pub selected_blue_score: Option<u64>,
    pub ordered_dag_tip: Option<Hash>,
    pub state_root_digest: Option<String>,
    pub observed_at_unix: u64,
    pub inventory_generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NetworkMessage {
    NewTransaction {
        chain_id: String,
        transaction: Transaction,
    },
    NewBlock {
        chain_id: String,
        block: Block,
    },
    BlockAnnounce {
        chain_id: String,
        hash: Hash,
    },
    NewBlockHash {
        chain_id: String,
        hash: Hash,
    },
    InvBlock {
        chain_id: String,
        hashes: Vec<Hash>,
    },
    GetHeaders {
        chain_id: String,
        locator: Vec<Hash>,
        stop_hash: Option<Hash>,
        limit: usize,
    },
    Headers {
        chain_id: String,
        headers: Vec<HeaderInventory>,
    },
    GetTips {
        chain_id: String,
        inventory: Option<TipInventoryStatus>,
    },
    Tips {
        chain_id: String,
        #[serde(serialize_with = "wire_limits_v1::serialize_bounded_tips")]
        tips: Vec<Hash>,
        inventory: Option<TipInventoryStatus>,
    },
    GetBlockHeaders {
        chain_id: String,
        #[serde(serialize_with = "wire_limits_v1::serialize_bounded_block_header_request_hashes")]
        hashes: Vec<Hash>,
    },
    BlockHeaders {
        chain_id: String,
        headers: Vec<BlockHeaderAnnouncement>,
    },
    GetBlock {
        chain_id: String,
        hash: Hash,
        request_id: Option<String>,
        requesting_peer_id: Option<String>,
        requested_peer_id: Option<String>,
        request_kind: Option<String>,
    },
    BlockData {
        chain_id: String,
        block: Option<Block>,
        request_id: Option<String>,
        request_hash: Option<Hash>,
    },
    Block {
        chain_id: String,
        block: Block,
    },
    Reject {
        chain_id: String,
        reason: String,
    },
    Error {
        chain_id: String,
        message: String,
    },
}

impl NetworkMessage {
    pub fn kind(&self) -> &'static str {
        match self {
            NetworkMessage::NewTransaction { .. } => "NewTransaction",
            NetworkMessage::NewBlock { .. } => "NewBlock",
            NetworkMessage::BlockAnnounce { .. } => "BlockAnnounce",
            NetworkMessage::NewBlockHash { .. } => "NewBlockHash",
            NetworkMessage::InvBlock { .. } => "InvBlock",
            NetworkMessage::GetHeaders { .. } => "GetHeaders",
            NetworkMessage::Headers { .. } => "Headers",
            NetworkMessage::GetTips { .. } => "GetTips",
            NetworkMessage::Tips { .. } => "Tips",
            NetworkMessage::GetBlockHeaders { .. } => "GetBlockHeaders",
            NetworkMessage::BlockHeaders { .. } => "BlockHeaders",
            NetworkMessage::GetBlock { .. } => "GetBlock",
            NetworkMessage::BlockData { .. } => "BlockData",
            NetworkMessage::Block { .. } => "Block",
            NetworkMessage::Reject { .. } => "Reject",
            NetworkMessage::Error { .. } => "Error",
        }
    }

    pub fn chain_id(&self) -> &str {
        match self {
            NetworkMessage::NewTransaction { chain_id, .. }
            | NetworkMessage::NewBlock { chain_id, .. }
            | NetworkMessage::BlockAnnounce { chain_id, .. }
            | NetworkMessage::NewBlockHash { chain_id, .. }
            | NetworkMessage::InvBlock { chain_id, .. }
            | NetworkMessage::GetHeaders { chain_id, .. }
            | NetworkMessage::Headers { chain_id, .. }
            | NetworkMessage::GetTips { chain_id, .. }
            | NetworkMessage::Tips { chain_id, .. }
            | NetworkMessage::GetBlockHeaders { chain_id, .. }
            | NetworkMessage::BlockHeaders { chain_id, .. }
            | NetworkMessage::GetBlock { chain_id, .. }
            | NetworkMessage::BlockData { chain_id, .. }
            | NetworkMessage::Block { chain_id, .. }
            | NetworkMessage::Reject { chain_id, .. }
            | NetworkMessage::Error { chain_id, .. } => chain_id,
        }
    }
}

pub fn topic_names(chain_id: &str) -> Vec<String> {
    vec![
        format!("{}-blocks", chain_id),
        format!("{}-txs", chain_id),
        format!("{}-sync", chain_id),
    ]
}

pub fn message_id_for_tx(tx: &Transaction) -> String {
    format!("tx:{}", tx.txid)
}

pub fn message_id_for_block(block: &Block) -> String {
    format!("block:{}", block.hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::types::{BlockHeader, TxOutput};

    fn sample_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 1,
            nonce: 7,
        }
    }

    fn sample_block(hash: &str) -> Block {
        Block {
            hash: hash.into(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![sample_tx("coinbase-like")],
        }
    }

    fn chain_id(message: &NetworkMessage) -> &str {
        match message {
            NetworkMessage::NewTransaction { chain_id, .. }
            | NetworkMessage::NewBlock { chain_id, .. }
            | NetworkMessage::BlockAnnounce { chain_id, .. }
            | NetworkMessage::NewBlockHash { chain_id, .. }
            | NetworkMessage::InvBlock { chain_id, .. }
            | NetworkMessage::GetHeaders { chain_id, .. }
            | NetworkMessage::Headers { chain_id, .. }
            | NetworkMessage::GetTips { chain_id, .. }
            | NetworkMessage::Tips { chain_id, .. }
            | NetworkMessage::GetBlockHeaders { chain_id, .. }
            | NetworkMessage::BlockHeaders { chain_id, .. }
            | NetworkMessage::GetBlock { chain_id, .. }
            | NetworkMessage::BlockData { chain_id, .. }
            | NetworkMessage::Block { chain_id, .. }
            | NetworkMessage::Reject { chain_id, .. }
            | NetworkMessage::Error { chain_id, .. } => chain_id,
        }
    }

    fn message_kind(message: &NetworkMessage) -> &'static str {
        match message {
            NetworkMessage::NewTransaction { .. } => "NewTransaction",
            NetworkMessage::NewBlock { .. } => "NewBlock",
            NetworkMessage::BlockAnnounce { .. } => "BlockAnnounce",
            NetworkMessage::NewBlockHash { .. } => "NewBlockHash",
            NetworkMessage::InvBlock { .. } => "InvBlock",
            NetworkMessage::GetHeaders { .. } => "GetHeaders",
            NetworkMessage::Headers { .. } => "Headers",
            NetworkMessage::GetTips { .. } => "GetTips",
            NetworkMessage::Tips { .. } => "Tips",
            NetworkMessage::GetBlockHeaders { .. } => "GetBlockHeaders",
            NetworkMessage::BlockHeaders { .. } => "BlockHeaders",
            NetworkMessage::GetBlock { .. } => "GetBlock",
            NetworkMessage::BlockData { .. } => "BlockData",
            NetworkMessage::Block { .. } => "Block",
            NetworkMessage::Reject { .. } => "Reject",
            NetworkMessage::Error { .. } => "Error",
        }
    }

    #[test]
    fn serializes_and_deserializes_every_network_message_variant() {
        let tx = sample_tx("tx-all-variants");
        let block = sample_block("block-all-variants");
        let messages = vec![
            NetworkMessage::NewTransaction {
                chain_id: "testnet".into(),
                transaction: tx.clone(),
            },
            NetworkMessage::NewBlock {
                chain_id: "testnet".into(),
                block: block.clone(),
            },
            NetworkMessage::BlockAnnounce {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
            },
            NetworkMessage::NewBlockHash {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
            },
            NetworkMessage::InvBlock {
                chain_id: "testnet".into(),
                hashes: vec![block.hash.clone()],
            },
            NetworkMessage::GetHeaders {
                chain_id: "testnet".into(),
                locator: vec!["parent".into()],
                stop_hash: Some(block.hash.clone()),
                limit: 64,
            },
            NetworkMessage::Headers {
                chain_id: "testnet".into(),
                headers: vec![HeaderInventory {
                    hash: block.hash.clone(),
                    header: block.header.clone(),
                }],
            },
            NetworkMessage::GetTips {
                chain_id: "testnet".into(),
                inventory: None,
            },
            NetworkMessage::Tips {
                chain_id: "testnet".into(),
                tips: vec![block.hash.clone()],
                inventory: None,
            },
            NetworkMessage::GetBlockHeaders {
                chain_id: "testnet".into(),
                hashes: vec![block.hash.clone()],
            },
            NetworkMessage::BlockHeaders {
                chain_id: "testnet".into(),
                headers: vec![BlockHeaderAnnouncement {
                    hash: block.hash.clone(),
                    header: block.header.clone(),
                }],
            },
            NetworkMessage::GetBlock {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
                request_id: None,
                requesting_peer_id: None,
                requested_peer_id: None,
                request_kind: Some("generic".into()),
            },
            NetworkMessage::BlockData {
                chain_id: "testnet".into(),
                block: Some(block.clone()),
                request_id: None,
                request_hash: Some(block.hash.clone()),
            },
            NetworkMessage::BlockData {
                chain_id: "testnet".into(),
                block: None,
                request_id: None,
                request_hash: None,
            },
            NetworkMessage::Block {
                chain_id: "testnet".into(),
                block,
            },
            NetworkMessage::Reject {
                chain_id: "testnet".into(),
                reason: "not found".into(),
            },
            NetworkMessage::Error {
                chain_id: "testnet".into(),
                message: "malformed".into(),
            },
        ];

        for message in messages {
            let encoded = serde_json::to_vec(&message).expect("message serializes");
            let decoded: NetworkMessage =
                serde_json::from_slice(&encoded).expect("message deserializes");
            assert_eq!(message_kind(&decoded), message_kind(&message));
            assert_eq!(chain_id(&decoded), "testnet");
            assert_eq!(decoded.chain_id(), "testnet");
        }
    }

    #[test]
    fn rejects_malformed_payloads_during_decode() {
        let malformed_json = br#"{"type":"GetBlock","chain_id":"testnet","hash":42}"#;
        assert!(serde_json::from_slice::<NetworkMessage>(malformed_json).is_err());

        let unknown_variant = br#"{"type":"Unknown","chain_id":"testnet"}"#;
        assert!(serde_json::from_slice::<NetworkMessage>(unknown_variant).is_err());
    }

    #[test]
    fn rejects_unsupported_block_header_version_before_hash_shape_checks() {
        let mut block = sample_block("legacy-noncanonical-hash");
        block.header.version = 99;
        let encoded = serde_json::to_vec(&NetworkMessage::NewBlock {
            chain_id: "testnet".into(),
            block,
        })
        .expect("unsupported version still serializes for adversarial test input");

        let err = serde_json::from_slice::<NetworkMessage>(&encoded).unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported block header version 99"));
    }

    #[test]
    fn bounded_wire_vectors_reject_resource_amplifying_shapes() {
        let inventory_at_limit = NetworkMessage::InvBlock {
            chain_id: "testnet".into(),
            hashes: vec!["hash".into(); P2P_WIRE_MAX_INVENTORY_ITEMS_V1],
        };
        let encoded = serde_json::to_vec(&inventory_at_limit).unwrap();
        assert!(serde_json::from_slice::<NetworkMessage>(&encoded).is_ok());

        let locator_at_limit = NetworkMessage::GetHeaders {
            chain_id: "testnet".into(),
            locator: vec!["hash".into(); MAX_SELECTED_CHAIN_LOCATOR_HASHES],
            stop_hash: None,
            limit: 1,
        };
        let encoded = serde_json::to_vec(&locator_at_limit).unwrap();
        assert!(serde_json::from_slice::<NetworkMessage>(&encoded).is_ok());

        let limit_at_max = NetworkMessage::GetHeaders {
            chain_id: "testnet".into(),
            locator: vec!["hash".into()],
            stop_hash: None,
            limit: P2P_WIRE_MAX_INVENTORY_ITEMS_V1,
        };
        let encoded = serde_json::to_vec(&limit_at_max).unwrap();
        assert!(serde_json::from_slice::<NetworkMessage>(&encoded).is_ok());

        let request_at_limit = NetworkMessage::GetBlockHeaders {
            chain_id: "testnet".into(),
            hashes: vec!["hash".into(); P2P_WIRE_MAX_REQUEST_ITEMS_V1],
        };
        let encoded = serde_json::to_vec(&request_at_limit).unwrap();
        assert!(serde_json::from_slice::<NetworkMessage>(&encoded).is_ok());
        let oversized_inventory = NetworkMessage::InvBlock {
            chain_id: "testnet".into(),
            hashes: vec!["hash".into(); P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1],
        };
        let encoded = serde_json::to_vec(&oversized_inventory).unwrap();
        let err = serde_json::from_slice::<NetworkMessage>(&encoded).unwrap_err();
        assert!(err.to_string().contains("maximum item count 512"));

        let oversized_locator = NetworkMessage::GetHeaders {
            chain_id: "testnet".into(),
            locator: vec!["hash".into(); MAX_SELECTED_CHAIN_LOCATOR_HASHES + 1],
            stop_hash: None,
            limit: 1,
        };
        let encoded = serde_json::to_vec(&oversized_locator).unwrap();
        let err = serde_json::from_slice::<NetworkMessage>(&encoded).unwrap_err();
        assert!(err.to_string().contains("GetHeaders.locator"));

        let oversized_limit = NetworkMessage::GetHeaders {
            chain_id: "testnet".into(),
            locator: vec!["hash".into()],
            stop_hash: None,
            limit: P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1,
        };
        let encoded = serde_json::to_vec(&oversized_limit).unwrap();
        let err = serde_json::from_slice::<NetworkMessage>(&encoded).unwrap_err();
        assert!(err
            .to_string()
            .contains("GetHeaders.limit exceeds maximum 512"));

        let oversized_request = serde_json::json!({
            "type": "GetBlockHeaders",
            "chain_id": "testnet",
            "hashes": vec!["hash"; P2P_WIRE_MAX_REQUEST_ITEMS_V1 + 1],
        });
        let encoded = serde_json::to_vec(&oversized_request).unwrap();
        let err = serde_json::from_slice::<NetworkMessage>(&encoded).unwrap_err();
        assert!(err.to_string().contains("GetBlockHeaders.hashes"));
    }

    #[test]
    fn message_ids_for_tx_and_block_are_stable_and_content_addressed() {
        let tx = sample_tx("stable-tx");
        let mut tx_with_different_body = tx.clone();
        tx_with_different_body.fee = 99;
        assert_eq!(message_id_for_tx(&tx), "tx:stable-tx");
        assert_eq!(
            message_id_for_tx(&tx),
            message_id_for_tx(&tx_with_different_body)
        );

        let block = sample_block("stable-block");
        let mut block_with_different_body = block.clone();
        block_with_different_body.header.nonce = 99;
        assert_eq!(message_id_for_block(&block), "block:stable-block");
        assert_eq!(
            message_id_for_block(&block),
            message_id_for_block(&block_with_different_body)
        );
    }
}

#[cfg(test)]
mod selected_tip_inventory_wire_tests {
    use super::{NetworkMessage, TipInventoryStatus};

    #[test]
    fn tips_wire_carries_selected_tip_inventory_status() {
        let inventory = TipInventoryStatus {
            chain_id: "testnet-dev".into(),
            selected_tip: Some("tip-741".into()),
            selected_height: Some(741),
            selected_blue_score: Some(741),
            ordered_dag_tip: Some("ordered-741".into()),
            state_root_digest: Some("state-root".into()),
            observed_at_unix: 1_000,
            inventory_generation: 3,
        };
        let wire = serde_json::to_vec(&NetworkMessage::Tips {
            chain_id: "testnet-dev".into(),
            tips: vec!["tip-741".into()],
            inventory: Some(inventory.clone()),
        })
        .expect("serialize tips");
        let decoded: NetworkMessage = serde_json::from_slice(&wire).expect("decode tips");

        match decoded {
            NetworkMessage::Tips {
                inventory: Some(decoded),
                ..
            } => {
                assert_eq!(decoded, inventory);
            }
            other => panic!("unexpected decoded message: {other:?}"),
        }
    }
}
