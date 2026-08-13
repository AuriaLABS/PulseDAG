use std::{fs, path::PathBuf, thread, time::Duration};

use ed25519_dalek::SigningKey;
use pulsedag_core::address_from_public_key;
use rand::{rngs::OsRng, RngCore};

use super::*;
use crate::{
    keystore_crypto::{encrypt_private_key_with_kdf_costs, KeystoreKdfCosts},
    WalletSecretKey, ED25519_SECRET_KEY_BYTES, KEYSTORE_KDF_MIN_ITERATIONS,
    KEYSTORE_KDF_MIN_MEMORY_KIB,
};

const PASSWORD: &str = "session-test-password";

fn fixture(label: &str) -> (PathBuf, WalletKeystoreFile, String) {
    let mut random = [0_u8; 8];
    OsRng.fill_bytes(&mut random);
    let dir = std::env::temp_dir().join(format!(
        "pulsedag-session-{label}-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    fs::create_dir(&dir).expect("create fixture dir");
    let path = dir.join("wallet.json");
    let bytes = [0x33; ED25519_SECRET_KEY_BYTES];
    let secret = WalletSecretKey::from_bytes(bytes);
    let address = address_from_public_key(&hex::encode(
        SigningKey::from_bytes(&bytes).verifying_key().to_bytes(),
    ));
    let envelope = encrypt_private_key_with_kdf_costs(
        "public-testnet-v2.4.0-candidate",
        "pulsedag-public-testnet-v2.4.0-candidate",
        &address,
        &secret,
        &SecretString::new(PASSWORD),
        KeystoreKdfCosts::new(KEYSTORE_KDF_MIN_MEMORY_KIB, KEYSTORE_KDF_MIN_ITERATIONS, 1),
    )
    .expect("encrypt fixture");
    let file = WalletKeystoreFile::try_acquire(&path).expect("lock fixture");
    file.create_new(&envelope).expect("persist fixture");
    (dir, file, address)
}

fn cleanup(dir: PathBuf, file: WalletKeystoreFile, session: WalletSession) {
    drop(session);
    drop(file);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn starts_locked_unlocks_and_manual_lock_destroys_active_state() {
    let (dir, file, address) = fixture("manual");
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("session");
    assert_eq!(session.status().lock_state, WalletSessionLockState::Locked);

    let status = session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock");
    assert_eq!(status.lock_state, WalletSessionLockState::Unlocked);
    assert_eq!(status.identity.as_ref().expect("identity").address, address);
    assert_eq!(
        session
            .with_unlocked_secret(|secret| secret.expose_secret()[0])
            .expect("use secret"),
        0x33
    );

    assert!(session.lock());
    assert!(matches!(
        session.with_unlocked_secret(|_| ()),
        Err(WalletSessionError::Locked)
    ));
    cleanup(dir, file, session);
}

#[test]
fn expiry_worker_relocks_without_polling_and_use_does_not_extend_deadline() {
    let (dir, file, _) = fixture("expiry");
    let policy = WalletUnlockPolicy::new(Duration::from_millis(200), 3, Duration::from_millis(100))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("session");
    session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock");

    thread::sleep(Duration::from_millis(80));
    session
        .with_unlocked_secret(|secret| assert_eq!(secret.expose_secret()[0], 0x33))
        .expect("use secret before deadline");
    thread::sleep(Duration::from_millis(220));

    let state = lock_state(&session.shared);
    assert!(state.unlocked.is_none(), "worker must clear expired secret");
    drop(state);
    assert_eq!(session.status().lock_state, WalletSessionLockState::Locked);
    cleanup(dir, file, session);
}

#[test]
fn failed_unlocks_trigger_rate_limit_until_lockout_expires() {
    let (dir, file, _) = fixture("rate-limit");
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 2, Duration::from_millis(150))
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
    assert!(session.status().retry_after.is_some());
    assert!(matches!(
        session.unlock(&file, &SecretString::new(PASSWORD)),
        Err(WalletSessionError::RateLimited { .. })
    ));

    thread::sleep(Duration::from_millis(220));
    let status = session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock after lockout");
    assert_eq!(status.lock_state, WalletSessionLockState::Unlocked);
    assert_eq!(status.consecutive_unlock_failures, 0);
    cleanup(dir, file, session);
}
