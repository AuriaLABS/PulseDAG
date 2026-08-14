use std::{fs, path::PathBuf, time::{Duration, SystemTime}};

use rand::{rngs::OsRng, RngCore};

use super::*;
use crate::{
    keystore_seed::{encrypt_wallet_seed_with_kdf_costs, SeedKeystoreKdfCosts},
    wallet_seed_from_mnemonic, SecretString, WalletDerivationBranch, WalletKeystoreFile,
    KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES, KEYSTORE_KDF_MIN_MEMORY_KIB,
};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const PASSWORD: &str = "wall-clock-seed-password";
const NETWORK_PROFILE: &str = "public-testnet-v2.4.0-candidate";
const CHAIN_ID: &str = "pulsedag-public-testnet-v2.4.0-candidate";
const RECEIVE_0: &str = "pulse1db62916ef4d99d98f95003ecfe3cb606c7f710ab";

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .expect("representable test time")
}

fn seed_fixture() -> (PathBuf, WalletKeystoreFile) {
    let mut random = [0_u8; 8];
    OsRng.fill_bytes(&mut random);
    let dir = std::env::temp_dir().join(format!(
        "pulsedag-wall-seed-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    fs::create_dir(&dir).expect("create wall-clock seed directory");
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

#[test]
fn wall_clock_check_keeps_active_session_before_deadline() {
    let mut state = WallClockState {
        deadline: Some(at(130)),
        last_observed: at(100),
        shutdown: false,
    };
    assert_eq!(
        inspect_wall_clock(&mut state, at(110)),
        WallClockCheck::Active(Duration::from_secs(20))
    );
}

#[test]
fn wall_clock_check_expires_at_deadline() {
    let mut state = WallClockState {
        deadline: Some(at(130)),
        last_observed: at(120),
        shutdown: false,
    };
    assert_eq!(
        inspect_wall_clock(&mut state, at(130)),
        WallClockCheck::ExpiredOrDiscontinuous
    );
    assert!(state.deadline.is_none());
}

#[test]
fn wall_clock_check_fails_closed_on_rollback() {
    let mut state = WallClockState {
        deadline: Some(at(150)),
        last_observed: at(120),
        shutdown: false,
    };
    assert_eq!(
        inspect_wall_clock(&mut state, at(119)),
        WallClockCheck::ExpiredOrDiscontinuous
    );
    assert!(state.deadline.is_none());
}

#[test]
fn guarded_session_starts_locked() {
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
        .expect("valid policy");
    let session = WalletSession::new(policy).expect("create guarded session");
    assert_eq!(session.status().lock_state, WalletSessionLockState::Locked);
}

#[test]
fn wall_clock_rollback_locks_seed_session_before_derivation() {
    let (dir, file) = seed_fixture();
    let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
        .expect("valid policy");
    let mut session = WalletSession::new(policy).expect("create guarded session");
    session
        .unlock(&file, &SecretString::new(PASSWORD))
        .expect("unlock seed session");

    {
        let mut wall = lock_wall(&session.wall);
        wall.last_observed = SystemTime::now()
            .checked_add(Duration::from_secs(60))
            .expect("future test wall time");
    }

    assert!(matches!(
        session.with_derived_key(0, WalletDerivationBranch::Receive, 0, |_| ()),
        Err(WalletSessionError::Locked)
    ));
    assert_eq!(session.status().lock_state, WalletSessionLockState::Locked);

    drop(session);
    drop(file);
    let _ = fs::remove_dir_all(dir);
}
