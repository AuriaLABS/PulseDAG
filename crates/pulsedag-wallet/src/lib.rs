#![forbid(unsafe_code)]

mod deterministic;
mod keystore;
mod keystore_crypto;
mod keystore_persistence;
mod keystore_rotation;
mod keystore_seed;
mod secrets;
mod session_clock;
mod session_v1;
mod watch_only;
use session_v1 as session_core;

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
pub use keystore_rotation::{rotate_keystore_password, WalletKeystoreRotationError};
pub use keystore_seed::{decrypt_wallet_seed, encrypt_wallet_seed};
pub use secrets::{
    SecretString, WalletSecretKey, WalletSeed, ED25519_SECRET_KEY_BYTES, REDACTED_SECRET,
    WALLET_SEED_BYTES,
};
pub use session_clock::WalletSession;
pub use session_v1::{
    WalletSessionError, WalletSessionIdentity, WalletSessionLockState, WalletSessionStatus,
    WalletUnlockPolicy, WalletUnlockPolicyError, WALLET_UNLOCK_MAX_FAILURES,
    WALLET_UNLOCK_MAX_LOCKOUT, WALLET_UNLOCK_MAX_TIMEOUT,
};
pub use watch_only::{
    export_watch_only_manifest, verify_watch_only_manifest, WalletWatchOnly, WalletWatchOnlyBranch,
    WalletWatchOnlyEntry, WalletWatchOnlyError, WalletWatchOnlyManifest,
    WalletWatchOnlyOperationError, WalletWatchOnlyScope, WalletWatchOnlySessionExt,
    WALLET_WATCH_ONLY_FORMAT, WALLET_WATCH_ONLY_MAX_ENTRIES, WALLET_WATCH_ONLY_VERSION,
};

use serde::{Deserialize, Serialize};

use pulsedag_core::{
    compute_txid,
    errors::PulseError,
    signing_message,
    types::{Address, OutPoint, Transaction, TxInput, TxOutput, Utxo},
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

/// Legacy low-level result for an unsigned transaction template.
///
/// The returned transaction still has empty input public keys/signatures.
/// Consequently `transaction.txid` is an unsigned-template identifier, not the
/// final broadcast txid, and `signing_message` is not the final message a signer
/// must sign because PulseDAG v1 includes input public keys in that preimage.
/// Professional wallet flows should use `build_transaction_plan` followed by
/// `WalletTransactionPlan::prepare_signing` instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTxResponse {
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
    pub signing_message: String,
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

/// Build the historical low-level unsigned transaction template.
///
/// This function is retained for existing callers. It does not attach the
/// sender public key, so callers must not treat the returned `signing_message`
/// or `transaction.txid` as final signing/broadcast identifiers.
pub fn build_transaction(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
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
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs,
        outputs,
        fee,
        nonce,
    };
    let message = signing_message(&tx);
    tx.txid = compute_txid(&tx);
    Ok(BuildTxResponse {
        transaction: tx,
        selected_utxos: selected
            .iter()
            .map(|u| SelectedUtxo {
                outpoint: u.outpoint.clone(),
                amount: u.amount,
            })
            .collect(),
        total_input,
        change,
        signing_message: hex::encode(message),
    })
}
