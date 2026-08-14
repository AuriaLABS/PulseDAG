use std::{fs, path::PathBuf, thread, time::Duration};

use ed25519_dalek::SigningKey;
use pulsedag_core::address_from_public_key;
use rand::{rngs::OsRng, RngCore};

use super::*;
use crate::{
    keystore_crypto::{encrypt_private_key_with_kdf_costs, KeystoreKdfCosts},
    keystore_seed::{encrypt_wallet_seed_with_kdf_costs, SeedKeystoreKdfCosts},
    wallet_seed_from_mnemonic, WalletDerivationBranch, WalletSecretKey,
    ED25519_SECRET_KEY_BYTES, KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES,
    KEYSTORE_KDF_MIN_MEMORY_KIB,
};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const PASSWORD: &str = "seed-session-password";
const NETWORK_PROFILE: &str = "public-testnet-v2.4.0-candidate";
const CHAIN_ID: &str = "pulsedag-public-testnet-v2.4.0-candidate";
const RECEIVE_0: &str = "pulse1db62916ef4d99d98f95003ecfe3cb606c7f710ab";
const CHANGE_2: &str = "pulse116db0da992b6a80cb5aa9541fa63eb404755f183";

fn test_dir(label: &str) -> PathBuf {
    let mut random = [0_u8; 8];
    OsRng.fill_bytes(&mut random);
    let dir = std::env::temp_dir().join(format!(
        "pulsedag-seed-session-{label}-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    fs::create_dir(&dir).expect("create session test directory");
    dir
}

fn seed_fixture(label: &str) -> (PathBuf, WalletKeystoreFile) {
    let dir = test_dir(label);
    let path = dir.join("wallet.json");
    let mnemonic = SecretString::new(MNEMONIC);
    let seed = wallet_seed_from_mnemonic(&mnemonic, None).expect("seed");
    let envelope = encrypt_wallet_seed_with_kdf_costs(
        NETWORK_PROFILE,
        CHAIN_ID,
        RECEIVE_0,
        &seed,
        &SecretString::new(PASSWORD),
        SeedKeystoreKdfCosts::new(
            KEYSTORE_KDF_MIN_MEMORY_KIB,
            KEYSTORE_KDF_MIN_ITERATIONS,
            KEYSTORE_KDF_MIN_LANES,
        ),
    )
    .expect("encrypt seed fixture");
    let file = WalletKeystoreFile::try_acquire(&path).expect("acquire seed fixture");
    file.create_new(&envelope).expect("persist seed fixture");
    (dir, file)
}

fn v1_fixture(label: &str) -> (PathBuf, WalletKeystoreFile) {
    let dir = test_dir(label);
    let path = dir.join("wallet.json");
    let bytes = [0x55; ED25519_SECRET_KEY_BYTES];
    let secret = WalletSecretKey::from_bytes(bytes);
    let address = address_from_public_key(&hex::encode(
        SigningKey::from_bytes(&bytes).verifying_key().to_bytes(),
    ));
    let envelope = encrypt_private_key_with_kdf_costs(
        NETWORK_PROFILE,
        CHAIN_ID,
        &address,
        &secret,
        &SecretString::new(PASSWORD),
        KeystoreKdfCosts::new(KEYSTORE_KDF_MIN_MEMORY_KIB, KEYSTORE_KDF_MIN_ITERATIONS, 1),
    )
    .expect("encrypt v1 fixture");
    let file = WalletKeystoreFile::try_acquire(&path).expect("acquire v1 fixture");
    file.create_new(&envelope).expect("persist v1 fixture");
    (dir, file)
}

fn cleanup(dir: PathBuf, file: WalletKeystoreFile, session: WalletSession) {
    drop(session);
    drop(file);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn seed_session_derives_golden_children_without_exposing_raw_seed() {
    let (dir, file) = seed_fixture("derive");
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("session");
    let status = session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock seed session");
    assert_eq!(status.lock_state, WalletSessionLockState::Unlocked);
    assert_eq!(status.identity.as_ref().expect("identity").address, RECEIVE_0);

    assert!(matches!(
        session.with_unlocked_secret(|_| ()),
        Err(WalletSessionError::WrongSecretKind)
    ));

    let receive = session
        .with_derived_key(0, WalletDerivationBranch::Receive, 0, |derived| {
            derived.address().to_string()
        })
        .expect("derive receive child");
    assert_eq!(receive, RECEIVE_0);

    let change = session
        .with_derived_key(0, WalletDerivationBranch::Change, 2, |derived| {
            derived.address().to_string()
        })
        .expect("derive change child");
    assert_eq!(change, CHANGE_2);

    assert!(session.lock());
    assert!(matches!(
        session.with_derived_key(0, WalletDerivationBranch::Receive, 0, |_| ()),
        Err(WalletSessionError::Locked)
    ));
    cleanup(dir, file, session);
}

#[test]
fn v1_session_rejects_deterministic_child_operation_distinctly() {
    let (dir, file) = v1_fixture("wrong-kind");
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("session");
    session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock v1 session");
    assert!(matches!(
        session.with_derived_key(0, WalletDerivationBranch::Receive, 0, |_| ()),
        Err(WalletSessionError::WrongSecretKind)
    ));
    cleanup(dir, file, session);
}

#[test]
fn seed_password_failures_rate_limit_and_expiry_relocks() {
    let (dir, file) = seed_fixture("rate-limit-expiry");
    let policy = WalletUnlockPolicy::new(Duration::from_millis(180), 2, Duration::from_millis(120))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("session");

    for _ in 0..2 {
        assert!(matches!(
            session.unlock(&file, &SecretString::new("wrong-password")),
            Err(WalletSessionError::Crypto(
                WalletKeystoreCryptoError::AuthenticationFailed
            ))
        ));
    }
    assert_eq!(session.status().consecutive_unlock_failures, 2);
    assert!(matches!(
        session.unlock(&file, &SecretString::new(PASSWORD)),
        Err(WalletSessionError::RateLimited { .. })
    ));

    thread::sleep(Duration::from_millis(180));
    session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock after rate limit");
    session
        .with_derived_key(0, WalletDerivationBranch::Receive, 0, |derived| {
            assert_eq!(derived.address(), RECEIVE_0)
        })
        .expect("derive before expiry");

    thread::sleep(Duration::from_millis(220));
    assert_eq!(session.status().lock_state, WalletSessionLockState::Locked);
    assert!(matches!(
        session.with_derived_key(0, WalletDerivationBranch::Receive, 0, |_| ()),
        Err(WalletSessionError::Locked)
    ));
    cleanup(dir, file, session);
}
