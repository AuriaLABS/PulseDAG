use std::{sync::Arc, time::SystemTime};

use super::{
    inspect_wall_clock, lock_core, lock_wall, CoreWalletSession, WallClockCheck, WallClockShared,
    WALL_CLOCK_POLL_INTERVAL,
};
use std::sync::Mutex;

pub(super) fn run(inner: Arc<Mutex<CoreWalletSession>>, wall: Arc<WallClockShared>) {
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
                {
                    let mut core = lock_core(&inner);
                    core.lock();
                }
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
