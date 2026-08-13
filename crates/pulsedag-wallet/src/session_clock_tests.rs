use std::time::{Duration, SystemTime};

use super::*;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .expect("representable test time")
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
