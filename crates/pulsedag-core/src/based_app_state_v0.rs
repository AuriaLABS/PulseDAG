//! Deterministic read model and bounded canonical event feed for Based Apps v0.
//!
//! This module does not activate Based Apps, PulseVM, contracts, or proof verification.
//! It folds already-canonical Based Apps events into a deterministic state view and
//! exposes a bounded page helper suitable for public/indexer-facing APIs.

use serde::{Deserialize, Serialize};

use crate::{
    access_set_v1::{AccessKeyIdV1, AccessSetV1},
    based_app_v0::{
        derive_app_id_v0, BasedAppProfileV0, BasedAppV0Error, BASED_CHALLENGE_MAX_PULSES_V0,
    },
};

pub const BASED_APP_EVENT_RETAINED_MAX_V0: usize = 4_096;
pub const BASED_APP_EVENT_PAGE_MAX_V0: usize = 256;
pub const BASED_APP_STATE_SCHEMA_VERSION_V0: u16 = 1;
pub const BASED_APP_EVENT_SCHEMA_VERSION_V0: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BasedAppCanonicalEventKindV0 {
    Opened,
    Challenged,
    Settled,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BasedAppCanonicalEventV0 {
    pub schema_version: u16,
    /// Total canonical order position supplied by the accepted DAG/apply path.
    pub canonical_position: u64,
    pub app_id: [u8; 32],
    pub round: u64,
    pub kind: BasedAppCanonicalEventKindV0,
    pub prev_state_root: [u8; 32],
    pub next_state_root: [u8; 32],
    pub pulse_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BasedAppPendingRoundV0 {
    pub round: u64,
    pub prev_state_root: [u8; 32],
    pub next_state_root: [u8; 32],
    pub opened_pulse_height: u64,
    pub challenged_pulse_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BasedAppStateViewV0 {
    pub schema_version: u16,
    pub app_id: [u8; 32],
    pub settled_round: Option<u64>,
    pub settled_state_root: Option<[u8; 32]>,
    pub pending: Option<BasedAppPendingRoundV0>,
    pub last_canonical_position: Option<u64>,
}

impl BasedAppStateViewV0 {
    pub fn new(profile: &BasedAppProfileV0) -> Result<Self, BasedAppStateV0Error> {
        Ok(Self {
            schema_version: BASED_APP_STATE_SCHEMA_VERSION_V0,
            app_id: derive_app_id_v0(profile)?,
            settled_round: None,
            settled_state_root: None,
            pending: None,
            last_canonical_position: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasedAppStateV0Error {
    Profile(BasedAppV0Error),
    UnsupportedStateSchemaVersion { observed: u16 },
    UnsupportedEventSchemaVersion { observed: u16 },
    MissingAppWriteKey,
    RetainedWindowTooLarge { len: usize, max: usize },
    PageLimitOutOfRange { value: usize, max: usize },
    WrongAppId,
    NonIncreasingCanonicalPosition { previous: u64, observed: u64 },
    PendingRoundExists { round: u64 },
    MissingPendingRound,
    RoundMismatch { expected: u64, observed: u64 },
    RoundOverflow,
    PrevStateRootMismatch,
    EventRootMismatch,
    DuplicateChallenge,
    ChallengeTooLate { current: u64, deadline: u64 },
    SettleTooEarly { current: u64, required: u64 },
    SettledWhileChallenged,
    RejectWithoutChallenge,
    RejectTooEarly { current: u64, required: u64 },
}

impl From<BasedAppV0Error> for BasedAppStateV0Error {
    fn from(value: BasedAppV0Error) -> Self {
        Self::Profile(value)
    }
}

impl std::fmt::Display for BasedAppStateV0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile(source) => write!(f, "invalid based-app profile: {source}"),
            Self::UnsupportedStateSchemaVersion { observed } => {
                write!(f, "unsupported based-app state schema version {observed}")
            }
            Self::UnsupportedEventSchemaVersion { observed } => {
                write!(f, "unsupported based-app event schema version {observed}")
            }
            Self::MissingAppWriteKey => {
                write!(f, "based-app transition must declare app_id as a write key")
            }
            Self::RetainedWindowTooLarge { len, max } => {
                write!(f, "retained based-app event window {len} exceeds max {max}")
            }
            Self::PageLimitOutOfRange { value, max } => {
                write!(f, "based-app event page limit {value} is outside 1..={max}")
            }
            Self::WrongAppId => write!(f, "based-app event app_id does not match the profile"),
            Self::NonIncreasingCanonicalPosition { previous, observed } => write!(
                f,
                "based-app canonical position {observed} is not after {previous}"
            ),
            Self::PendingRoundExists { round } => {
                write!(f, "based-app round {round} is still pending")
            }
            Self::MissingPendingRound => write!(f, "based-app event has no pending round"),
            Self::RoundMismatch { expected, observed } => {
                write!(
                    f,
                    "based-app round {observed} does not match expected {expected}"
                )
            }
            Self::RoundOverflow => write!(f, "based-app settled round cannot advance"),
            Self::PrevStateRootMismatch => {
                write!(
                    f,
                    "based-app open prev_state_root does not match settled state"
                )
            }
            Self::EventRootMismatch => {
                write!(f, "based-app event roots do not match the pending round")
            }
            Self::DuplicateChallenge => write!(f, "based-app pending round is already challenged"),
            Self::ChallengeTooLate { current, deadline } => write!(
                f,
                "based-app challenge at pulse {current} is not before deadline {deadline}"
            ),
            Self::SettleTooEarly { current, required } => write!(
                f,
                "based-app settle at pulse {current} is before required {required}"
            ),
            Self::SettledWhileChallenged => {
                write!(f, "challenged based-app round cannot settle directly")
            }
            Self::RejectWithoutChallenge => {
                write!(f, "based-app round cannot reject without a challenge")
            }
            Self::RejectTooEarly { current, required } => write!(
                f,
                "based-app reject at pulse {current} is before required {required}"
            ),
        }
    }
}

impl std::error::Error for BasedAppStateV0Error {}

pub fn based_app_write_key_v0(
    profile: &BasedAppProfileV0,
) -> Result<AccessKeyIdV1, BasedAppStateV0Error> {
    Ok(AccessKeyIdV1(derive_app_id_v0(profile)?))
}

pub fn validate_based_app_access_set_v0(
    profile: &BasedAppProfileV0,
    access_set: &AccessSetV1,
) -> Result<(), BasedAppStateV0Error> {
    let app_key = based_app_write_key_v0(profile)?;
    if !access_set.write_keys.contains(&app_key) {
        return Err(BasedAppStateV0Error::MissingAppWriteKey);
    }
    Ok(())
}

fn ensure_pending_matches(
    pending: &BasedAppPendingRoundV0,
    event: &BasedAppCanonicalEventV0,
) -> Result<(), BasedAppStateV0Error> {
    if event.round != pending.round {
        return Err(BasedAppStateV0Error::RoundMismatch {
            expected: pending.round,
            observed: event.round,
        });
    }
    if event.prev_state_root != pending.prev_state_root
        || event.next_state_root != pending.next_state_root
    {
        return Err(BasedAppStateV0Error::EventRootMismatch);
    }
    Ok(())
}

/// Apply one already-canonical event to the deterministic Based Apps read model.
///
/// Consensus/admission is intentionally out of scope here. The caller supplies canonical
/// events after normal validation and GHOSTDAG/access-set ordering.
pub fn apply_based_app_canonical_event_v0(
    profile: &BasedAppProfileV0,
    state: &mut BasedAppStateViewV0,
    event: &BasedAppCanonicalEventV0,
) -> Result<(), BasedAppStateV0Error> {
    if state.schema_version != BASED_APP_STATE_SCHEMA_VERSION_V0 {
        return Err(BasedAppStateV0Error::UnsupportedStateSchemaVersion {
            observed: state.schema_version,
        });
    }
    if event.schema_version != BASED_APP_EVENT_SCHEMA_VERSION_V0 {
        return Err(BasedAppStateV0Error::UnsupportedEventSchemaVersion {
            observed: event.schema_version,
        });
    }

    let expected_app_id = derive_app_id_v0(profile)?;
    if state.app_id != expected_app_id || event.app_id != expected_app_id {
        return Err(BasedAppStateV0Error::WrongAppId);
    }

    if let Some(previous) = state.last_canonical_position {
        if event.canonical_position <= previous {
            return Err(BasedAppStateV0Error::NonIncreasingCanonicalPosition {
                previous,
                observed: event.canonical_position,
            });
        }
    }

    match event.kind {
        BasedAppCanonicalEventKindV0::Opened => {
            if let Some(pending) = &state.pending {
                return Err(BasedAppStateV0Error::PendingRoundExists {
                    round: pending.round,
                });
            }
            if let Some(settled_round) = state.settled_round {
                let expected = settled_round
                    .checked_add(1)
                    .ok_or(BasedAppStateV0Error::RoundOverflow)?;
                if event.round != expected {
                    return Err(BasedAppStateV0Error::RoundMismatch {
                        expected,
                        observed: event.round,
                    });
                }
            }
            if let Some(settled_root) = state.settled_state_root {
                if event.prev_state_root != settled_root {
                    return Err(BasedAppStateV0Error::PrevStateRootMismatch);
                }
            }
            state.pending = Some(BasedAppPendingRoundV0 {
                round: event.round,
                prev_state_root: event.prev_state_root,
                next_state_root: event.next_state_root,
                opened_pulse_height: event.pulse_height,
                challenged_pulse_height: None,
            });
        }
        BasedAppCanonicalEventKindV0::Challenged => {
            let pending = state
                .pending
                .as_mut()
                .ok_or(BasedAppStateV0Error::MissingPendingRound)?;
            ensure_pending_matches(pending, event)?;
            if pending.challenged_pulse_height.is_some() {
                return Err(BasedAppStateV0Error::DuplicateChallenge);
            }
            let deadline = pending.opened_pulse_height.saturating_add(u64::from(
                profile.challenge_pulses.min(BASED_CHALLENGE_MAX_PULSES_V0),
            ));
            if event.pulse_height >= deadline {
                return Err(BasedAppStateV0Error::ChallengeTooLate {
                    current: event.pulse_height,
                    deadline,
                });
            }
            pending.challenged_pulse_height = Some(event.pulse_height);
        }
        BasedAppCanonicalEventKindV0::Settled => {
            let pending = state
                .pending
                .as_ref()
                .ok_or(BasedAppStateV0Error::MissingPendingRound)?;
            ensure_pending_matches(pending, event)?;
            if pending.challenged_pulse_height.is_some() {
                return Err(BasedAppStateV0Error::SettledWhileChallenged);
            }
            let required = pending.opened_pulse_height.saturating_add(u64::from(
                profile.challenge_pulses.min(BASED_CHALLENGE_MAX_PULSES_V0),
            ));
            if event.pulse_height < required {
                return Err(BasedAppStateV0Error::SettleTooEarly {
                    current: event.pulse_height,
                    required,
                });
            }
            let round = pending.round;
            let next_state_root = pending.next_state_root;
            state.pending = None;
            state.settled_round = Some(round);
            state.settled_state_root = Some(next_state_root);
        }
        BasedAppCanonicalEventKindV0::Rejected => {
            let pending = state
                .pending
                .as_ref()
                .ok_or(BasedAppStateV0Error::MissingPendingRound)?;
            ensure_pending_matches(pending, event)?;
            let challenged_pulse_height = pending
                .challenged_pulse_height
                .ok_or(BasedAppStateV0Error::RejectWithoutChallenge)?;
            let required = challenged_pulse_height.saturating_add(u64::from(
                profile.challenge_pulses.min(BASED_CHALLENGE_MAX_PULSES_V0),
            ));
            if event.pulse_height < required {
                return Err(BasedAppStateV0Error::RejectTooEarly {
                    current: event.pulse_height,
                    required,
                });
            }
            let prev_state_root = pending.prev_state_root;
            state.pending = None;
            if state.settled_state_root.is_none() {
                state.settled_state_root = Some(prev_state_root);
            }
        }
    }

    state.last_canonical_position = Some(event.canonical_position);
    Ok(())
}

/// Fold an app-specific retained window from a previously persisted bounded state view.
///
/// Input arrival order is irrelevant: events are applied by canonical position. This permits a
/// consensus node to keep only the latest state/checkpoint plus a bounded recent event window,
/// rather than an unbounded full-history application index.
pub fn fold_based_app_events_from_state_v0(
    profile: &BasedAppProfileV0,
    base_state: &BasedAppStateViewV0,
    events: &[BasedAppCanonicalEventV0],
) -> Result<BasedAppStateViewV0, BasedAppStateV0Error> {
    let mut state = base_state.clone();
    if events.len() > BASED_APP_EVENT_RETAINED_MAX_V0 {
        return Err(BasedAppStateV0Error::RetainedWindowTooLarge {
            len: events.len(),
            max: BASED_APP_EVENT_RETAINED_MAX_V0,
        });
    }

    if state.schema_version != BASED_APP_STATE_SCHEMA_VERSION_V0 {
        return Err(BasedAppStateV0Error::UnsupportedStateSchemaVersion {
            observed: state.schema_version,
        });
    }
    let expected_app_id = derive_app_id_v0(profile)?;
    if state.app_id != expected_app_id {
        return Err(BasedAppStateV0Error::WrongAppId);
    }

    let mut ordered = events.to_vec();
    ordered.sort_by_key(|event| event.canonical_position);
    for event in &ordered {
        apply_based_app_canonical_event_v0(profile, &mut state, event)?;
    }
    Ok(state)
}

/// Fold an app-specific retained window from an empty view.
pub fn fold_based_app_events_v0(
    profile: &BasedAppProfileV0,
    events: &[BasedAppCanonicalEventV0],
) -> Result<BasedAppStateViewV0, BasedAppStateV0Error> {
    let base_state = BasedAppStateViewV0::new(profile)?;
    fold_based_app_events_from_state_v0(profile, &base_state, events)
}

/// Build an indexer-friendly bounded page from a bounded retained event window.
///
/// Full-history indexing belongs outside consensus nodes. This helper never accepts more than
/// BASED_APP_EVENT_RETAINED_MAX_V0 input rows and never returns more than
/// BASED_APP_EVENT_PAGE_MAX_V0.
pub fn based_app_event_page_v0(
    events: &[BasedAppCanonicalEventV0],
    app_id: [u8; 32],
    after_canonical_position: Option<u64>,
    limit: usize,
) -> Result<Vec<BasedAppCanonicalEventV0>, BasedAppStateV0Error> {
    if events.len() > BASED_APP_EVENT_RETAINED_MAX_V0 {
        return Err(BasedAppStateV0Error::RetainedWindowTooLarge {
            len: events.len(),
            max: BASED_APP_EVENT_RETAINED_MAX_V0,
        });
    }
    if limit == 0 || limit > BASED_APP_EVENT_PAGE_MAX_V0 {
        return Err(BasedAppStateV0Error::PageLimitOutOfRange {
            value: limit,
            max: BASED_APP_EVENT_PAGE_MAX_V0,
        });
    }

    let mut filtered: Vec<_> = events
        .iter()
        .filter(|event| event.app_id == app_id)
        .filter(|event| {
            after_canonical_position
                .map(|after| event.canonical_position > after)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    if let Some(event) = filtered
        .iter()
        .find(|event| event.schema_version != BASED_APP_EVENT_SCHEMA_VERSION_V0)
    {
        return Err(BasedAppStateV0Error::UnsupportedEventSchemaVersion {
            observed: event.schema_version,
        });
    }
    filtered.sort_by_key(|event| event.canonical_position);
    for pair in filtered.windows(2) {
        if pair[0].canonical_position == pair[1].canonical_position {
            return Err(BasedAppStateV0Error::NonIncreasingCanonicalPosition {
                previous: pair[0].canonical_position,
                observed: pair[1].canonical_position,
            });
        }
    }
    filtered.truncate(limit);
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::based_app_v0::BasedDaModeV0;

    fn profile() -> BasedAppProfileV0 {
        BasedAppProfileV0 {
            chain_id: "state-test".into(),
            operator_set: vec!["op-a".into()],
            challenge_pulses: 64,
            max_blob_bytes: 64,
            da_mode: BasedDaModeV0::Inline,
        }
    }

    fn event(
        profile: &BasedAppProfileV0,
        canonical_position: u64,
        round: u64,
        kind: BasedAppCanonicalEventKindV0,
        prev: u8,
        next: u8,
        pulse_height: u64,
    ) -> BasedAppCanonicalEventV0 {
        BasedAppCanonicalEventV0 {
            schema_version: BASED_APP_EVENT_SCHEMA_VERSION_V0,
            canonical_position,
            app_id: derive_app_id_v0(profile).unwrap(),
            round,
            kind,
            prev_state_root: [prev; 32],
            next_state_root: [next; 32],
            pulse_height,
        }
    }

    #[test]
    fn based_app_write_key_drives_deterministic_conflicts() {
        use crate::access_set_v1::{
            access_sets_conflict_v1, schedule_access_sets_v1, AccessConflictClassV1,
            AccessSetAdmissionV1, AccessSetV1,
        };
        use std::collections::BTreeSet;

        let profile_a = profile();
        let mut profile_b = profile();
        profile_b.chain_id = "state-test-b".into();

        let key_a = based_app_write_key_v0(&profile_a).unwrap();
        let key_b = based_app_write_key_v0(&profile_b).unwrap();
        assert_ne!(key_a, key_b);

        let set_a = AccessSetV1 {
            writes: BTreeSet::new(),
            reads: BTreeSet::new(),
            write_keys: [key_a.clone()].into_iter().collect(),
            read_keys: BTreeSet::new(),
        };
        let same_app = AccessSetV1 {
            writes: BTreeSet::new(),
            reads: BTreeSet::new(),
            write_keys: [key_a].into_iter().collect(),
            read_keys: BTreeSet::new(),
        };
        let other_app = AccessSetV1 {
            writes: BTreeSet::new(),
            reads: BTreeSet::new(),
            write_keys: [key_b].into_iter().collect(),
            read_keys: BTreeSet::new(),
        };

        validate_based_app_access_set_v0(&profile_a, &set_a).unwrap();
        assert!(access_sets_conflict_v1(&set_a, &same_app));
        assert!(!access_sets_conflict_v1(&set_a, &other_app));
        assert_eq!(
            validate_based_app_access_set_v0(&profile_a, &other_app).unwrap_err(),
            BasedAppStateV0Error::MissingAppWriteKey
        );

        let admission = AccessSetAdmissionV1 {
            format_admitted: true,
        };
        let ordered = [(1, set_a), (2, same_app), (3, other_app)];
        let (accepted, rejected) = schedule_access_sets_v1(admission, &ordered, false);
        assert_eq!(accepted, vec![1, 3]);
        assert_eq!(rejected, vec![(2, AccessConflictClassV1::AccessConflict)]);
    }

    #[test]
    fn unknown_state_and_event_schema_versions_fail_closed() {
        let profile = profile();
        let mut state = BasedAppStateViewV0::new(&profile).unwrap();
        state.schema_version = 99;
        let valid_event = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            10,
        );
        assert_eq!(
            apply_based_app_canonical_event_v0(&profile, &mut state, &valid_event).unwrap_err(),
            BasedAppStateV0Error::UnsupportedStateSchemaVersion { observed: 99 }
        );

        let mut state = BasedAppStateViewV0::new(&profile).unwrap();
        let mut invalid_event = valid_event;
        invalid_event.schema_version = 99;
        assert_eq!(
            apply_based_app_canonical_event_v0(&profile, &mut state, &invalid_event).unwrap_err(),
            BasedAppStateV0Error::UnsupportedEventSchemaVersion { observed: 99 }
        );
    }

    #[test]
    fn arrival_permutation_produces_identical_final_state() {
        let profile = profile();
        let opened = event(
            &profile,
            10,
            7,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            100,
        );
        let settled = event(
            &profile,
            11,
            7,
            BasedAppCanonicalEventKindV0::Settled,
            1,
            2,
            164,
        );

        let a = fold_based_app_events_v0(&profile, &[settled.clone(), opened.clone()]).unwrap();
        let b = fold_based_app_events_v0(&profile, &[opened, settled]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.settled_round, Some(7));
        assert_eq!(a.settled_state_root, Some([2; 32]));
        assert!(a.pending.is_none());
    }

    #[test]
    fn bounded_window_can_resume_from_persisted_state() {
        let profile = profile();
        let first_open = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            10,
        );
        let first_settle = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Settled,
            1,
            2,
            74,
        );
        let checkpoint = fold_based_app_events_v0(&profile, &[first_open, first_settle]).unwrap();

        let second_open = event(
            &profile,
            3,
            2,
            BasedAppCanonicalEventKindV0::Opened,
            2,
            3,
            80,
        );
        let second_settle = event(
            &profile,
            4,
            2,
            BasedAppCanonicalEventKindV0::Settled,
            2,
            3,
            144,
        );
        let resumed = fold_based_app_events_from_state_v0(
            &profile,
            &checkpoint,
            &[second_settle, second_open],
        )
        .unwrap();

        assert_eq!(resumed.settled_round, Some(2));
        assert_eq!(resumed.settled_state_root, Some([3; 32]));
        assert_eq!(resumed.last_canonical_position, Some(4));
    }

    #[test]
    fn failed_window_replay_preserves_checkpoint() {
        let profile = profile();
        let opened = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            10,
        );
        let settled = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Settled,
            1,
            2,
            74,
        );
        let checkpoint = fold_based_app_events_v0(&profile, &[opened, settled]).unwrap();
        let checkpoint_before = checkpoint.clone();

        let wrong_root = event(
            &profile,
            3,
            2,
            BasedAppCanonicalEventKindV0::Opened,
            9,
            3,
            80,
        );
        assert_eq!(
            fold_based_app_events_from_state_v0(&profile, &checkpoint, &[wrong_root]).unwrap_err(),
            BasedAppStateV0Error::PrevStateRootMismatch
        );
        assert_eq!(checkpoint, checkpoint_before);
    }

    #[test]
    fn challenged_round_cannot_settle_and_reject_preserves_previous_state() {
        let profile = profile();
        let opened = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            3,
            4,
            50,
        );
        let challenged = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Challenged,
            3,
            4,
            80,
        );
        let settled = event(
            &profile,
            3,
            1,
            BasedAppCanonicalEventKindV0::Settled,
            3,
            4,
            114,
        );

        let err =
            fold_based_app_events_v0(&profile, &[opened.clone(), challenged.clone(), settled])
                .unwrap_err();
        assert_eq!(err, BasedAppStateV0Error::SettledWhileChallenged);

        let rejected = event(
            &profile,
            3,
            1,
            BasedAppCanonicalEventKindV0::Rejected,
            3,
            4,
            144,
        );
        let state = fold_based_app_events_v0(&profile, &[opened, challenged, rejected]).unwrap();
        assert_eq!(state.settled_round, None);
        assert_eq!(state.settled_state_root, Some([3; 32]));
        assert!(state.pending.is_none());
    }

    #[test]
    fn challenged_round_reject_waits_for_challenge_window() {
        let profile = profile();
        let opened = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            3,
            4,
            50,
        );
        let challenged = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Challenged,
            3,
            4,
            80,
        );
        let early_reject = event(
            &profile,
            3,
            1,
            BasedAppCanonicalEventKindV0::Rejected,
            3,
            4,
            143,
        );
        assert_eq!(
            fold_based_app_events_v0(&profile, &[opened, challenged, early_reject]).unwrap_err(),
            BasedAppStateV0Error::RejectTooEarly {
                current: 143,
                required: 144
            }
        );
    }

    #[test]
    fn successor_round_must_continue_settled_root_and_round() {
        let profile = profile();
        let first_open = event(
            &profile,
            1,
            4,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            10,
        );
        let first_settle = event(
            &profile,
            2,
            4,
            BasedAppCanonicalEventKindV0::Settled,
            1,
            2,
            74,
        );
        let wrong_round = event(
            &profile,
            3,
            6,
            BasedAppCanonicalEventKindV0::Opened,
            2,
            3,
            80,
        );
        assert_eq!(
            fold_based_app_events_v0(
                &profile,
                &[first_open.clone(), first_settle.clone(), wrong_round]
            )
            .unwrap_err(),
            BasedAppStateV0Error::RoundMismatch {
                expected: 5,
                observed: 6
            }
        );

        let wrong_root = event(
            &profile,
            3,
            5,
            BasedAppCanonicalEventKindV0::Opened,
            9,
            3,
            80,
        );
        assert_eq!(
            fold_based_app_events_v0(&profile, &[first_open, first_settle, wrong_root])
                .unwrap_err(),
            BasedAppStateV0Error::PrevStateRootMismatch
        );
    }

    #[test]
    fn challenge_and_settle_deadlines_are_pulse_based() {
        let profile = profile();
        let opened = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            100,
        );
        let late_challenge = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Challenged,
            1,
            2,
            164,
        );
        assert_eq!(
            fold_based_app_events_v0(&profile, &[opened.clone(), late_challenge]).unwrap_err(),
            BasedAppStateV0Error::ChallengeTooLate {
                current: 164,
                deadline: 164
            }
        );

        let early_settle = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Settled,
            1,
            2,
            163,
        );
        assert_eq!(
            fold_based_app_events_v0(&profile, &[opened, early_settle]).unwrap_err(),
            BasedAppStateV0Error::SettleTooEarly {
                current: 163,
                required: 164
            }
        );
    }

    #[test]
    fn event_page_rejects_unknown_schema_and_duplicate_position() {
        let profile = profile();
        let app_id = derive_app_id_v0(&profile).unwrap();
        let mut unknown = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            100,
        );
        unknown.schema_version = 99;
        assert_eq!(
            based_app_event_page_v0(&[unknown], app_id, None, 1).unwrap_err(),
            BasedAppStateV0Error::UnsupportedEventSchemaVersion { observed: 99 }
        );

        let first = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            100,
        );
        let duplicate = first.clone();
        assert_eq!(
            based_app_event_page_v0(&[first, duplicate], app_id, None, 2).unwrap_err(),
            BasedAppStateV0Error::NonIncreasingCanonicalPosition {
                previous: 1,
                observed: 1
            }
        );
    }

    #[test]
    fn event_page_is_sorted_filtered_and_bounded() {
        let profile = profile();
        let app_id = derive_app_id_v0(&profile).unwrap();
        let e3 = event(
            &profile,
            3,
            1,
            BasedAppCanonicalEventKindV0::Rejected,
            1,
            2,
            164,
        );
        let e1 = event(
            &profile,
            1,
            1,
            BasedAppCanonicalEventKindV0::Opened,
            1,
            2,
            100,
        );
        let e2 = event(
            &profile,
            2,
            1,
            BasedAppCanonicalEventKindV0::Challenged,
            1,
            2,
            120,
        );

        let page = based_app_event_page_v0(&[e3, e1, e2], app_id, Some(1), 2).unwrap();
        assert_eq!(
            page.iter()
                .map(|event| event.canonical_position)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        assert_eq!(
            based_app_event_page_v0(&[], app_id, None, 0).unwrap_err(),
            BasedAppStateV0Error::PageLimitOutOfRange {
                value: 0,
                max: BASED_APP_EVENT_PAGE_MAX_V0
            }
        );
    }
}
