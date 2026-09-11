use pulsedag_core::{
    mempool_resource_v1::{
        transaction_message_size_for_resource_v1, MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1,
    },
    types::{Transaction, TxOutput},
    TRANSACTION_VERSION_V1,
};
use pulsedag_p2p::messages::NetworkMessage;

#[test]
fn core_transaction_carrier_measure_matches_network_message_json() {
    let chain_id = "carrier-parity-\"escaped\"";
    let tx = Transaction {
        txid: "carrier-parity".to_string(),
        version: TRANSACTION_VERSION_V1,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: "\"\\\n".repeat(128),
            amount: 7,
        }],
        fee: 10,
        nonce: 9,
    };

    let actual = serde_json::to_vec(&NetworkMessage::NewTransaction {
        chain_id: chain_id.to_string(),
        transaction: tx.clone(),
    })
    .unwrap();
    let measured = transaction_message_size_for_resource_v1(&tx, chain_id).unwrap();

    assert_eq!(measured, actual.len() as u64);
    assert_eq!(
        MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1,
        64 * 1_024
    );
}
