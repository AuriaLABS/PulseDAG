use pulsedag_core::{
    address_from_public_key, signing_message_v2,
    types::{OutPoint, Utxo},
    ProtocolActivationIdentity, TRANSACTION_VERSION_V2,
};
use pulsedag_wallet::protocol_v2::{prepare_wallet_v2_signing_plan, WalletV2PlanRequest};

const PUBLIC_KEY: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

fn identity(chain_id: &str, genesis: &str) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(chain_id, genesis, "ghostdag-order-v1")
}

fn source_address() -> String {
    address_from_public_key(PUBLIC_KEY)
}

fn funding_utxo() -> Utxo {
    Utxo {
        outpoint: OutPoint {
            txid: "11".repeat(32),
            index: 0,
        },
        address: source_address(),
        amount: 10,
        coinbase: false,
        height: 1,
    }
}

fn prepare(
    identity: &ProtocolActivationIdentity,
) -> pulsedag_wallet::protocol_v2::WalletV2SigningPlan {
    let source = source_address();
    let available = [funding_utxo()];
    prepare_wallet_v2_signing_plan(
        identity,
        identity,
        WalletV2PlanRequest {
            public_key: PUBLIC_KEY,
            from: &source,
            to: "pulse1recipient",
            amount: 7,
            fee: 1,
            available_utxos: &available,
            nonce: 9,
        },
    )
    .expect("valid v2 signing plan")
}

#[test]
fn same_intent_is_cryptographically_separated_between_networks() {
    let testnet = identity("pulsedag-testnet-v3", "testnet-genesis-v3");
    let mainnet = identity("pulsedag-mainnet-v3", "mainnet-genesis-v3");

    let testnet_plan = prepare(&testnet);
    let mainnet_plan = prepare(&mainnet);

    assert_eq!(testnet_plan.transaction.version, TRANSACTION_VERSION_V2);
    assert_eq!(mainnet_plan.transaction.version, TRANSACTION_VERSION_V2);
    assert_eq!(testnet_plan.transaction.fee, mainnet_plan.transaction.fee);
    assert_eq!(
        testnet_plan.transaction.nonce,
        mainnet_plan.transaction.nonce
    );
    assert_eq!(
        testnet_plan.transaction.inputs.len(),
        mainnet_plan.transaction.inputs.len()
    );
    assert_eq!(
        testnet_plan.transaction.outputs.len(),
        mainnet_plan.transaction.outputs.len()
    );
    assert_ne!(
        testnet_plan.protocol_fingerprint,
        mainnet_plan.protocol_fingerprint
    );
    assert_ne!(testnet_plan.signing_message, mainnet_plan.signing_message);
}

#[test]
fn observed_identity_from_another_network_is_rejected_before_signing() {
    let expected = identity("pulsedag-testnet-v3", "testnet-genesis-v3");
    let observed = identity("pulsedag-mainnet-v3", "mainnet-genesis-v3");
    let source = source_address();
    let available = [funding_utxo()];

    let error = prepare_wallet_v2_signing_plan(
        &expected,
        &observed,
        WalletV2PlanRequest {
            public_key: PUBLIC_KEY,
            from: &source,
            to: "pulse1recipient",
            amount: 7,
            fee: 1,
            available_utxos: &available,
            nonce: 9,
        },
    )
    .expect_err("cross-network identity must fail closed");

    assert!(matches!(
        error,
        pulsedag_core::errors::PulseError::InvalidTransaction(_)
    ));
}

#[test]
fn prepared_signing_bytes_cannot_be_reinterpreted_under_another_chain_id() {
    let testnet = identity("pulsedag-testnet-v3", "testnet-genesis-v3");
    let plan = prepare(&testnet);

    let wrong_domain = signing_message_v2(&plan.transaction, "pulsedag-mainnet-v3")
        .expect("other non-empty chain domain can be serialized");

    assert_ne!(plan.signing_message, hex::encode(wrong_domain));
}
