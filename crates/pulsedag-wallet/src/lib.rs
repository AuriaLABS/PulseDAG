#![forbid(unsafe_code)]

mod deterministic;
mod keystore;
mod keystore_crypto;
mod keystore_persistence;
mod keystore_seed;
mod secrets;
pub mod protocol_v2;

pub use deterministic::{
    derive_network_components, derive_wallet_key, derive_wallet_key_from_seed,
    generate_wallet_mnemonic, wallet_seed_from_mnemonic, WalletDerivationBranch, WalletDerivedKey,
    WalletDeterministicError, WalletNetworkContext, WALLET_DERIVATION_DOMAIN,
    WALLET_DERIVATION_MAX_INDEX, WALLET_DERIVATION_VERSION, WALLET_MNEMONIC_WORDS,
    WALLET_NETWORK_COMPONENTS,
};
pub use keystore::{
    WalletCipherMetadata, WalletKdfMetadata, WalletKeystoreEnvelope, WalletKeystoreFormatError,
    KEYSTORE_CIPHER_XCHACHA20_POLY1305, KEYSTORE_DERIVED_KEY_BYTES, KEYSTORE_FORMAT,
    KEYSTORE_KDF_ARGON2ID, KEYSTORE_KDF_DEFAULT_ITERATIONS, KEYSTORE_KDF_DEFAULT_LANES,
    KEYSTORE_KDF_DEFAULT_MEMORY_KIB, KEYSTORE_KDF_MAX_ITERATIONS, KEYSTORE_KDF_MAX_LANES,
    KEYSTORE_KDF_MAX_MEMORY_KIB, KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES,
    KEYSTORE_KDF_MIN_MEMORY_KIB, KEYSTORE_MIN_CIPHERTEXT_BYTES, KEYSTORE_NONCE_BYTES,
    KEYSTORE_SALT_BYTES, KEYSTORE_SEED_VERSION, KEYSTORE_V1_CIPHERTEXT_BYTES,
    KEYSTORE_V1_PLAINTEXT_BYTES, KEYSTORE_V2_CIPHERTEXT_BYTES, KEYSTORE_V2_PLAINTEXT_BYTES,
    KEYSTORE_VERSION,
};
pub use keystore_crypto::{decrypt_private_key, encrypt_private_key, WalletKeystoreCryptoError};
pub use keystore_persistence::{
    WalletKeystoreDirectorySyncStatus, WalletKeystoreFile, WalletKeystorePermissionStatus,
    WalletKeystorePersistenceError, WalletKeystorePersistenceReport, KEYSTORE_FILE_MAX_BYTES,
};
pub use keystore_seed::{decrypt_wallet_seed, encrypt_wallet_seed};
pub use secrets::{
    SecretString, WalletSecretKey, WalletSeed, ED25519_SECRET_KEY_BYTES, REDACTED_SECRET,
    WALLET_SEED_BYTES,
};

use serde::{Deserialize, Serialize};

use pulsedag_core::{
    compute_txid, compute_txid_v2,
    errors::PulseError,
    signing_message, signing_message_v2,
    types::{Address, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    TransactionRejectionClass, TRANSACTION_VERSION_V2,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTxRequest {
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedUtxo {
    pub outpoint: OutPoint,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTxResponse {
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
    pub signing_message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletSubmissionState {
    Accepted,
    Duplicate,
    Conflict,
    Rejected,
    Confirmed,
    /// Reserved for a future protocol that explicitly enables replacement.
    /// v2.4.0 has RBF disabled and no reconciliation path produces this state.
    Replaced,
}

impl WalletSubmissionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Duplicate => "duplicate",
            Self::Conflict => "conflict",
            Self::Rejected => "rejected",
            Self::Confirmed => "confirmed",
            Self::Replaced => "replaced",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletSubmissionObservation {
    Accepted,
    Rejected(TransactionRejectionClass),
    Confirmed,
}

pub fn reconcile_wallet_submission(
    previous: Option<WalletSubmissionState>,
    observation: WalletSubmissionObservation,
) -> WalletSubmissionState {
    if previous == Some(WalletSubmissionState::Confirmed) {
        return WalletSubmissionState::Confirmed;
    }

    match observation {
        WalletSubmissionObservation::Accepted => WalletSubmissionState::Accepted,
        WalletSubmissionObservation::Rejected(TransactionRejectionClass::Duplicate) => {
            WalletSubmissionState::Duplicate
        }
        WalletSubmissionObservation::Rejected(TransactionRejectionClass::Conflict) => {
            WalletSubmissionState::Conflict
        }
        WalletSubmissionObservation::Rejected(_) => WalletSubmissionState::Rejected,
        WalletSubmissionObservation::Confirmed => WalletSubmissionState::Confirmed,
    }
}

pub fn select_utxos(utxos: &[Utxo], target: u64) -> Result<(Vec<Utxo>, u64), PulseError> {
    let mut selected = Vec::new();
    let mut total = 0_u64;
    for utxo in utxos {
        selected.push(utxo.clone());
        total = total.saturating_add(utxo.amount);
        if total >= target {
            return Ok((selected, total));
        }
    }
    Err(PulseError::InsufficientFunds)
}

fn build_transaction_body(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
    version: u32,
) -> Result<(Transaction, Vec<Utxo>, u64, u64), PulseError> {
    let target = amount
        .checked_add(fee)
        .ok_or_else(|| PulseError::InvalidTransaction("amount overflow".into()))?;
    let (selected, total_input) = select_utxos(available_utxos, target)?;
    let change = total_input - target;
    let inputs = selected
        .iter()
        .map(|u| TxInput {
            previous_output: u.outpoint.clone(),
            public_key: String::new(),
            signature: String::new(),
        })
        .collect::<Vec<_>>();
    let mut outputs = vec![TxOutput {
        address: to.to_string(),
        amount,
    }];
    if change > 0 {
        outputs.push(TxOutput {
            address: from.to_string(),
            amount: change,
        });
    }

    Ok((
        Transaction {
            txid: String::new(),
            version,
            inputs,
            outputs,
            fee,
            nonce,
        },
        selected,
        total_input,
        change,
    ))
}

fn build_response(
    transaction: Transaction,
    selected: &[Utxo],
    total_input: u64,
    change: u64,
    signing_message: Vec<u8>,
) -> BuildTxResponse {
    BuildTxResponse {
        transaction,
        selected_utxos: selected
            .iter()
            .map(|u| SelectedUtxo {
                outpoint: u.outpoint.clone(),
                amount: u.amount,
            })
            .collect(),
        total_input,
        change,
        signing_message: hex::encode(signing_message),
    }
}

pub fn build_transaction(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
    let (mut tx, selected, total_input, change) =
        build_transaction_body(from, to, amount, fee, available_utxos, nonce, 1)?;
    let message = signing_message(&tx);
    tx.txid = compute_txid(&tx);
    Ok(build_response(tx, &selected, total_input, change, message))
}

pub fn build_transaction_v2(
    chain_id: &str,
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
    let (mut tx, selected, total_input, change) = build_transaction_body(
        from,
        to,
        amount,
        fee,
        available_utxos,
        nonce,
        TRANSACTION_VERSION_V2,
    )?;
    let message = signing_message_v2(&tx, chain_id)?;
    tx.txid = compute_txid_v2(&tx, chain_id)?;
    Ok(build_response(tx, &selected, total_input, change, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utxo(txid: &str, index: u32, amount: u64) -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: txid.to_string(),
                index,
            },
            address: "pulse1source".to_string(),
            amount,
            coinbase: false,
            height: 1,
        }
    }

    #[test]
    fn wallet_submission_state_codes_are_stable() {
        let cases = [
            (WalletSubmissionState::Accepted, "accepted"),
            (WalletSubmissionState::Duplicate, "duplicate"),
            (WalletSubmissionState::Conflict, "conflict"),
            (WalletSubmissionState::Rejected, "rejected"),
            (WalletSubmissionState::Confirmed, "confirmed"),
            (WalletSubmissionState::Replaced, "replaced"),
        ];

        for (state, expected) in cases {
            assert_eq!(state.as_str(), expected);
        }
    }

    #[test]
    fn duplicate_conflict_and_generic_rejections_remain_distinct() {
        assert_eq!(
            reconcile_wallet_submission(
                Some(WalletSubmissionState::Accepted),
                WalletSubmissionObservation::Rejected(TransactionRejectionClass::Duplicate),
            ),
            WalletSubmissionState::Duplicate
        );
        assert_eq!(
            reconcile_wallet_submission(
                Some(WalletSubmissionState::Accepted),
                WalletSubmissionObservation::Rejected(TransactionRejectionClass::Conflict),
            ),
            WalletSubmissionState::Conflict
        );
        assert_eq!(
            reconcile_wallet_submission(
                None,
                WalletSubmissionObservation::Rejected(TransactionRejectionClass::InvalidSignature),
            ),
            WalletSubmissionState::Rejected
        );
    }

    #[test]
    fn confirmed_is_terminal_for_submission_reconciliation() {
        for observation in [
            WalletSubmissionObservation::Accepted,
            WalletSubmissionObservation::Rejected(TransactionRejectionClass::Duplicate),
            WalletSubmissionObservation::Rejected(TransactionRejectionClass::Conflict),
            WalletSubmissionObservation::Rejected(TransactionRejectionClass::InvalidTxid),
            WalletSubmissionObservation::Confirmed,
        ] {
            assert_eq!(
                reconcile_wallet_submission(Some(WalletSubmissionState::Confirmed), observation),
                WalletSubmissionState::Confirmed
            );
        }
    }

    #[test]
    fn v2_4_reconciliation_never_produces_replaced() {
        let rejection_classes = [
            TransactionRejectionClass::UnsupportedTransactionVersion,
            TransactionRejectionClass::InactiveTransactionVersion,
            TransactionRejectionClass::WrongChainDomain,
            TransactionRejectionClass::InvalidTxid,
            TransactionRejectionClass::InvalidSignature,
            TransactionRejectionClass::Duplicate,
            TransactionRejectionClass::Conflict,
            TransactionRejectionClass::Orphan,
            TransactionRejectionClass::MalformedTransaction,
            TransactionRejectionClass::InsufficientFunds,
            TransactionRejectionClass::MempoolFull,
        ];

        assert_ne!(
            reconcile_wallet_submission(None, WalletSubmissionObservation::Accepted),
            WalletSubmissionState::Replaced
        );
        assert_ne!(
            reconcile_wallet_submission(None, WalletSubmissionObservation::Confirmed),
            WalletSubmissionState::Replaced
        );
        for class in rejection_classes {
            assert_ne!(
                reconcile_wallet_submission(None, WalletSubmissionObservation::Rejected(class)),
                WalletSubmissionState::Replaced
            );
        }
    }

    #[test]
    fn legacy_builder_remains_v1() {
        let built = build_transaction(
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .unwrap();

        assert_eq!(built.transaction.version, 1);
        assert_eq!(built.transaction.txid, compute_txid(&built.transaction));
    }

    #[test]
    fn v2_builder_is_chain_bound() {
        let available = [utxo("funding", 0, 10)];
        let testnet = build_transaction_v2(
            "pulsedag-testnet",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &available,
            9,
        )
        .unwrap();
        let private = build_transaction_v2(
            "pulsedag-private",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &available,
            9,
        )
        .unwrap();

        assert_eq!(testnet.transaction.version, TRANSACTION_VERSION_V2);
        assert_ne!(testnet.transaction.txid, private.transaction.txid);
        assert_ne!(testnet.signing_message, private.signing_message);
        assert_eq!(
            testnet.transaction.txid,
            compute_txid_v2(&testnet.transaction, "pulsedag-testnet").unwrap()
        );
    }

    #[test]
    fn v2_builder_rejects_empty_chain_id() {
        let err = build_transaction_v2(
            "",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .unwrap_err();

        assert!(matches!(err, PulseError::ChainIdMismatch));
    }
}
