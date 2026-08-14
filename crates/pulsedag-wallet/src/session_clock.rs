use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use crate::{
    session_core::WalletSession as CoreWalletSession, SecretString, WalletKeystoreFile,
    WalletSecretKey,
};
use crate::{WalletSessionError, WalletSessionLockState, WalletSessionStatus, WalletUnlockPolicy};

const WALL_CLOCK_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct WallClockState {
    deadline: Option<SystemTime>,
    last_observed: SystemTime,
    shutdown: bool,
}

struct WallClockShared {
    state: Mutex<WallClockState>,
    wake: Condvar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WallClockCheck {
    Inactive,
    Active(Duration),
    ExpiredOrDiscontinuous,
}

pub struct WalletSession {
    policy: WalletUnlockPolicy,
    inner: Arc<Mutex<CoreWalletSession>>,
    wall: Arc<WallClockShared>,
    worker: Option<JoinHandle<()>>,
}

impl WalletSession {
    pub fn new(policy: WalletUnlockPolicy) -> Result<Self, WalletSessionError> {
        let inner = Arc::new(Mutex::new(CoreWalletSession::new(policy)?));
        let wall = Arc::new(WallClockShared {
            state: Mutex::new(WallClockState {
                deadline: None,
                last_observed: SystemTime::now(),
                shutdown: false,
            }),
            wake: Condvar::new(),
        });
        let worker_inner = Arc::clone(&inner);
        let worker_wall = Arc::clone(&wall);
        let worker = thread::Builder::new()
            .name("pulsedag-wallet-wall-clock".into())
            .spawn(move || wall_clock_worker(worker_inner, worker_wall))
            .map_err(WalletSessionError::WorkerSpawn)?;
        Ok(Self {
            policy,
            inner,
            wall,
            worker: Some(worker),
        })
    }

    pub fn status(&self) -> WalletSessionStatus {
        let wall_remaining = self.enforce_wall_clock();
        let mut core = lock_core(&self.inner);
        let mut status = core.status();
        if status.lock_state == WalletSessionLockState::Unlocked {
            match wall_remaining {
                Some(remaining) => {
                    status.remaining_unlock = status
                        .remaining_unlock
                        .map(|core_remaining| core_remaining.min(remaining));
                }
                None => {
                    core.lock();
                    status = core.status();
                }
            }
        }
        status
    }

    pub fn unlock(
        &mut self,
        keystore: &WalletKeystoreFile,
        password: &SecretString,
    ) -> Result<WalletSessionStatus, WalletSessionError> {
        self.enforce_wall_clock();
        let wall_now = SystemTime::now();
        let wall_deadline = wall_now
            .checked_add(self.policy.unlock_timeout())
            .ok_or(WalletSessionError::TimeoutOverflow)?;
        {
            let mut core = lock_core(&self.inner);
            core.unlock(keystore, password)?;
        }
        {
            let mut wall = lock_wall(&self.wall);
            wall.deadline = Some(wall_deadline);
            wall.last_observed = wall_now;
        }
        self.wall.wake.notify_all();
        Ok(self.status())
    }

    pub fn lock(&mut self) -> bool {
        self.clear_wall_deadline();
        lock_core(&self.inner).lock()
    }

    pub fn with_unlocked_secret<R>(
        &self,
        action: impl FnOnce(&WalletSecretKey) -> R,
    ) -> Result<R, WalletSessionError> {
        let result = {
            let mut core = lock_core(&self.inner);
            let wall_check = {
                let mut wall = lock_wall(&self.wall);
                inspect_wall_clock(&mut wall, SystemTime::now())
            };
            if !matches!(wall_check, WallClockCheck::Active(_)) {
                core.lock();
                self.wall.wake.notify_all();
                return Err(WalletSessionError::Locked);
            }
            core.with_unlocked_secret(action)
        };
        self.enforce_wall_clock();
        result
    }

    fn clear_wall_deadline(&self) {
        let mut wall = lock_wall(&self.wall);
        wall.deadline = None;
        wall.last_observed = SystemTime::now();
        self.wall.wake.notify_all();
    }

    fn enforce_wall_clock(&self) -> Option<Duration> {
        let check = {
            let mut wall = lock_wall(&self.wall);
            inspect_wall_clock(&mut wall, SystemTime::now())
        };
        match check {
            WallClockCheck::Inactive => None,
            WallClockCheck::Active(remaining) => Some(remaining),
            WallClockCheck::ExpiredOrDiscontinuous => {
                lock_core(&self.inner).lock();
                None
            }
        }
    }
}

impl Drop for WalletSession {
    fn drop(&mut self) {
        {
            let mut wall = lock_wall(&self.wall);
            wall.shutdown = true;
            wall.deadline = None;
            self.wall.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        lock_core(&self.inner).lock();
    }
}

fn inspect_wall_clock(state: &mut WallClockState, now: SystemTime) -> WallClockCheck {
    let Some(deadline) = state.deadline else {
        state.last_observed = now;
        return WallClockCheck::Inactive;
    };
    let rolled_back = now.duration_since(state.last_observed).is_err();
    state.last_observed = now;
    if rolled_back || now.duration_since(deadline).is_ok() {
        state.deadline = None;
        return WallClockCheck::ExpiredOrDiscontinuous;
    }
    WallClockCheck::Active(deadline.duration_since(now).unwrap_or(Duration::ZERO))
}

fn lock_core(inner: &Mutex<CoreWalletSession>) -> MutexGuard<'_, CoreWalletSession> {
    inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_wall(shared: &WallClockShared) -> MutexGuard<'_, WallClockState> {
    shared
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wall_clock_worker(inner: Arc<Mutex<CoreWalletSession>>, wall: Arc<WallClockShared>) {
    let mut state = lock_wall(&wall);
    loop {
        if state.shutdown {
            return;
        }
        match inspect_wall_clock(&mut state, SystemTime::now()) {
            WallClockCheck::Inactive => {
                state = wall
                    .wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            WallClockCheck::ExpiredOrDiscontinuous => {
                drop(state);
                lock_core(&inner).lock();
                state = lock_wall(&wall);
            }
            WallClockCheck::Active(remaining) => {
                let wait_for = remaining.min(WALL_CLOCK_POLL_INTERVAL);
                state = match wall.wake.wait_timeout(state, wait_for) {
                    Ok((state, _)) => state,
                    Err(poisoned) => poisoned.into_inner().0,
                };
            }
        }
    }
}

#[cfg(test)]
#[path = "session_clock_tests.rs"]
mod tests;
