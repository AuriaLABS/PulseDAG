use std::{
    error::Error,
    fmt, io,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    decrypt_private_key, SecretString, WalletKeystoreCryptoError, WalletKeystoreFile,
    WalletKeystorePersistenceError, WalletSecretKey,
};

pub const WALLET_UNLOCK_MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
pub const WALLET_UNLOCK_MAX_FAILURES: u32 = 10;
pub const WALLET_UNLOCK_MAX_LOCKOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalletUnlockPolicy {
    unlock_timeout: Duration,
    max_failures: u32,
    lockout_duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletUnlockPolicyError {
    ZeroTimeout,
    TimeoutTooLong,
    ZeroMaxFailures,
    TooManyFailures,
    ZeroLockout,
    LockoutTooLong,
}

impl fmt::Display for WalletUnlockPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroTimeout => "wallet unlock timeout must be greater than zero",
            Self::TimeoutTooLong => "wallet unlock timeout exceeds the supported maximum",
            Self::ZeroMaxFailures => "wallet unlock failure limit must be greater than zero",
            Self::TooManyFailures => "wallet unlock failure limit exceeds the supported maximum",
            Self::ZeroLockout => "wallet unlock lockout duration must be greater than zero",
            Self::LockoutTooLong => "wallet unlock lockout duration exceeds the supported maximum",
        };
        f.write_str(message)
    }
}

impl Error for WalletUnlockPolicyError {}

impl WalletUnlockPolicy {
    pub fn new(
        unlock_timeout: Duration,
        max_failures: u32,
        lockout_duration: Duration,
    ) -> Result<Self, WalletUnlockPolicyError> {
        if unlock_timeout.is_zero() {
            return Err(WalletUnlockPolicyError::ZeroTimeout);
        }
        if unlock_timeout > WALLET_UNLOCK_MAX_TIMEOUT {
            return Err(WalletUnlockPolicyError::TimeoutTooLong);
        }
        if max_failures == 0 {
            return Err(WalletUnlockPolicyError::ZeroMaxFailures);
        }
        if max_failures > WALLET_UNLOCK_MAX_FAILURES {
            return Err(WalletUnlockPolicyError::TooManyFailures);
        }
        if lockout_duration.is_zero() {
            return Err(WalletUnlockPolicyError::ZeroLockout);
        }
        if lockout_duration > WALLET_UNLOCK_MAX_LOCKOUT {
            return Err(WalletUnlockPolicyError::LockoutTooLong);
        }
        Ok(Self {
            unlock_timeout,
            max_failures,
            lockout_duration,
        })
    }

    pub fn unlock_timeout(self) -> Duration {
        self.unlock_timeout
    }

    pub fn max_failures(self) -> u32 {
        self.max_failures
    }

    pub fn lockout_duration(self) -> Duration {
        self.lockout_duration
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletSessionLockState {
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSessionIdentity {
    pub network_profile: String,
    pub chain_id: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSessionStatus {
    pub lock_state: WalletSessionLockState,
    pub identity: Option<WalletSessionIdentity>,
    pub remaining_unlock: Option<Duration>,
    pub consecutive_unlock_failures: u32,
    pub retry_after: Option<Duration>,
}

#[derive(Debug)]
pub enum WalletSessionError {
    WorkerSpawn(io::Error),
    AlreadyUnlocked,
    Locked,
    RateLimited { retry_after: Duration },
    TimeoutOverflow,
    Persistence(WalletKeystorePersistenceError),
    Crypto(WalletKeystoreCryptoError),
}

impl fmt::Display for WalletSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerSpawn(_) => f.write_str("wallet unlock expiry worker could not start"),
            Self::AlreadyUnlocked => f.write_str("wallet session is already unlocked"),
            Self::Locked => f.write_str("wallet session is locked"),
            Self::RateLimited { retry_after } => write!(
                f,
                "wallet unlock attempts are rate-limited for another {} ms",
                retry_after.as_millis()
            ),
            Self::TimeoutOverflow => f.write_str("wallet unlock deadline could not be represented"),
            Self::Persistence(error) => write!(f, "wallet keystore access failed: {error}"),
            Self::Crypto(error) => write!(f, "wallet unlock failed: {error}"),
        }
    }
}

impl Error for WalletSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerSpawn(error) => Some(error),
            Self::Persistence(error) => Some(error),
            Self::Crypto(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletKeystorePersistenceError> for WalletSessionError {
    fn from(value: WalletKeystorePersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl From<WalletKeystoreCryptoError> for WalletSessionError {
    fn from(value: WalletKeystoreCryptoError) -> Self {
        Self::Crypto(value)
    }
}

struct UnlockedState {
    secret_key: WalletSecretKey,
    identity: WalletSessionIdentity,
    expires_at: Instant,
}

struct SessionState {
    unlocked: Option<UnlockedState>,
    consecutive_failures: u32,
    blocked_until: Option<Instant>,
    shutdown: bool,
}

struct SessionShared {
    state: Mutex<SessionState>,
    wake: Condvar,
}

pub struct WalletSession {
    policy: WalletUnlockPolicy,
    shared: Arc<SessionShared>,
    worker: Option<JoinHandle<()>>,
}

impl WalletSession {
    pub fn new(policy: WalletUnlockPolicy) -> Result<Self, WalletSessionError> {
        let shared = Arc::new(SessionShared {
            state: Mutex::new(SessionState {
                unlocked: None,
                consecutive_failures: 0,
                blocked_until: None,
                shutdown: false,
            }),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("pulsedag-wallet-expiry".into())
            .spawn(move || expiry_worker(worker_shared))
            .map_err(WalletSessionError::WorkerSpawn)?;
        Ok(Self {
            policy,
            shared,
            worker: Some(worker),
        })
    }

    pub fn status(&self) -> WalletSessionStatus {
        let now = Instant::now();
        let mut state = lock_state(&self.shared);
        expire_if_needed(&mut state, now);
        refresh_rate_limit(&mut state, now);
        snapshot(&state, now)
    }

    pub fn unlock(
        &mut self,
        keystore: &WalletKeystoreFile,
        password: &SecretString,
    ) -> Result<WalletSessionStatus, WalletSessionError> {
        let now = Instant::now();
        {
            let mut state = lock_state(&self.shared);
            expire_if_needed(&mut state, now);
            refresh_rate_limit(&mut state, now);
            if state.unlocked.is_some() {
                return Err(WalletSessionError::AlreadyUnlocked);
            }
            if let Some(blocked_until) = state.blocked_until {
                return Err(WalletSessionError::RateLimited {
                    retry_after: blocked_until.saturating_duration_since(now),
                });
            }
        }

        let envelope = keystore.load()?;
        let secret_key = match decrypt_private_key(&envelope, password) {
            Ok(secret_key) => secret_key,
            Err(error) => {
                if matches!(error, WalletKeystoreCryptoError::AuthenticationFailed) {
                    self.record_authentication_failure(Instant::now())?;
                }
                return Err(error.into());
            }
        };

        let unlocked_at = Instant::now();
        let expires_at = unlocked_at
            .checked_add(self.policy.unlock_timeout)
            .ok_or(WalletSessionError::TimeoutOverflow)?;
        let identity = WalletSessionIdentity {
            network_profile: envelope.network_profile,
            chain_id: envelope.chain_id,
            address: envelope.address,
        };
        let mut state = lock_state(&self.shared);
        state.consecutive_failures = 0;
        state.blocked_until = None;
        state.unlocked = Some(UnlockedState {
            secret_key,
            identity,
            expires_at,
        });
        self.shared.wake.notify_all();
        Ok(snapshot(&state, unlocked_at))
    }

    pub fn lock(&mut self) -> bool {
        let mut state = lock_state(&self.shared);
        let was_unlocked = state.unlocked.take().is_some();
        self.shared.wake.notify_all();
        was_unlocked
    }

    pub fn with_unlocked_secret<R>(
        &self,
        action: impl FnOnce(&WalletSecretKey) -> R,
    ) -> Result<R, WalletSessionError> {
        let now = Instant::now();
        let mut state = lock_state(&self.shared);
        expire_if_needed(&mut state, now);
        let result = {
            let unlocked = state.unlocked.as_ref().ok_or(WalletSessionError::Locked)?;
            action(&unlocked.secret_key)
        };
        expire_if_needed(&mut state, Instant::now());
        Ok(result)
    }

    fn record_authentication_failure(&self, now: Instant) -> Result<(), WalletSessionError> {
        let mut state = lock_state(&self.shared);
        refresh_rate_limit(&mut state, now);
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= self.policy.max_failures {
            state.blocked_until = Some(
                now.checked_add(self.policy.lockout_duration)
                    .ok_or(WalletSessionError::TimeoutOverflow)?,
            );
        }
        Ok(())
    }
}

impl Drop for WalletSession {
    fn drop(&mut self) {
        {
            let mut state = lock_state(&self.shared);
            state.shutdown = true;
            state.unlocked.take();
            self.shared.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn lock_state(shared: &SessionShared) -> MutexGuard<'_, SessionState> {
    shared
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn refresh_rate_limit(state: &mut SessionState, now: Instant) {
    if state.blocked_until.is_some_and(|deadline| now >= deadline) {
        state.blocked_until = None;
        state.consecutive_failures = 0;
    }
}

fn expire_if_needed(state: &mut SessionState, now: Instant) {
    if state
        .unlocked
        .as_ref()
        .is_some_and(|unlocked| now >= unlocked.expires_at)
    {
        state.unlocked.take();
    }
}

fn snapshot(state: &SessionState, now: Instant) -> WalletSessionStatus {
    let (lock_state, identity, remaining_unlock) = match state.unlocked.as_ref() {
        Some(unlocked) => (
            WalletSessionLockState::Unlocked,
            Some(unlocked.identity.clone()),
            Some(unlocked.expires_at.saturating_duration_since(now)),
        ),
        None => (WalletSessionLockState::Locked, None, None),
    };
    WalletSessionStatus {
        lock_state,
        identity,
        remaining_unlock,
        consecutive_unlock_failures: state.consecutive_failures,
        retry_after: state
            .blocked_until
            .map(|deadline| deadline.saturating_duration_since(now)),
    }
}

fn expiry_worker(shared: Arc<SessionShared>) {
    let mut state = lock_state(&shared);
    loop {
        if state.shutdown {
            state.unlocked.take();
            return;
        }
        let Some(expires_at) = state.unlocked.as_ref().map(|unlocked| unlocked.expires_at) else {
            state = shared
                .wake
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            continue;
        };
        let now = Instant::now();
        if now >= expires_at {
            state.unlocked.take();
            continue;
        }
        let wait_for = expires_at.saturating_duration_since(now);
        state = match shared.wake.wait_timeout(state, wait_for) {
            Ok((state, _)) => state,
            Err(poisoned) => poisoned.into_inner().0,
        };
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
