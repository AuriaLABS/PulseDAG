use super::*;
use crate::{
    decrypt_private_key, WalletKeystoreCryptoError, WalletSecretKey, ED25519_SECRET_KEY_BYTES,
    KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_MEMORY_KIB,
};
use ed25519_dalek::SigningKey;
use pulsedag_core::address_from_public_key;

const OLD: &str = "old-test-password";
const NEW: &str = "new-test-password";

fn fixture(label: &str) -> (PathBuf, PathBuf, WalletKeystoreFile) {
    let mut random = [0_u8; 8];
    OsRng.fill_bytes(&mut random);
    let dir = std::env::temp_dir().join(format!(
        "pulsedag-rotation-{label}-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    fs::create_dir(&dir).expect("create test dir");
    let path = dir.join("wallet.json");
    let bytes = [0x5au8; ED25519_SECRET_KEY_BYTES];
    let secret = WalletSecretKey::from_bytes(bytes);
    let signing = SigningKey::from_bytes(&bytes);
    let address = address_from_public_key(&hex::encode(signing.verifying_key().to_bytes()));
    let envelope = encrypt_private_key_with_kdf_costs(
        "public-testnet-v2.4.0-candidate",
        "pulsedag-public-testnet-v2.4.0-candidate",
        &address,
        &secret,
        &SecretString::new(OLD),
        KEYSTORE_KDF_MIN_MEMORY_KIB,
        KEYSTORE_KDF_MIN_ITERATIONS,
        1,
    )
    .expect("encrypt fixture");
    let session = WalletKeystoreFile::try_acquire(&path).expect("lock fixture");
    session.create_new(&envelope).expect("persist fixture");
    (dir, path, session)
}

#[test]
fn rotation_preserves_identity_secret_and_kdf_costs() {
    let (dir, _path, session) = fixture("success");
    let before = session.load().expect("load before");
    rotate_keystore_password(&session, &SecretString::new(OLD), &SecretString::new(NEW))
        .expect("rotate");
    let after = session.load().expect("load after");
    assert_eq!(after.network_profile, before.network_profile);
    assert_eq!(after.chain_id, before.chain_id);
    assert_eq!(after.address, before.address);
    assert_eq!(after.kdf.memory_kib, before.kdf.memory_kib);
    assert_eq!(after.kdf.iterations, before.kdf.iterations);
    assert_eq!(after.kdf.lanes, before.kdf.lanes);
    assert_ne!(after.kdf.salt_hex, before.kdf.salt_hex);
    assert_ne!(after.cipher.nonce_hex, before.cipher.nonce_hex);
    assert_ne!(after.ciphertext_hex, before.ciphertext_hex);
    let recovered = decrypt_private_key(&after, &SecretString::new(NEW)).expect("new decrypts");
    assert_eq!(
        recovered.expose_secret(),
        &[0x5au8; ED25519_SECRET_KEY_BYTES]
    );
    assert!(matches!(
        decrypt_private_key(&after, &SecretString::new(OLD)),
        Err(WalletKeystoreCryptoError::AuthenticationFailed)
    ));
    drop(session);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn wrong_current_password_does_not_mutate_file() {
    let (dir, path, session) = fixture("wrong");
    let before = fs::read(&path).expect("read before");
    assert!(matches!(
        rotate_keystore_password(
            &session,
            &SecretString::new("wrong"),
            &SecretString::new(NEW)
        ),
        Err(WalletKeystoreRotationError::Crypto(
            WalletKeystoreCryptoError::AuthenticationFailed
        ))
    ));
    assert_eq!(fs::read(&path).expect("read after"), before);
    drop(session);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn empty_new_password_does_not_mutate_file() {
    let (dir, path, session) = fixture("empty");
    let before = fs::read(&path).expect("read before");
    assert!(matches!(
        rotate_keystore_password(&session, &SecretString::new(OLD), &SecretString::new("")),
        Err(WalletKeystoreRotationError::Crypto(
            WalletKeystoreCryptoError::EmptyPassword
        ))
    ));
    assert_eq!(fs::read(&path).expect("read after"), before);
    drop(session);
    let _ = fs::remove_dir_all(dir);
}
