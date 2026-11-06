//! Planning matcher for access-set v1.
//!
//! Declared read/write sets for a future programmable tx format.
//! Existing tx v2 has no access-set field; this module MUST NOT infer one.
//! Default evaluation is fail-closed and is not wired into apply/mempool.

use std::collections::BTreeSet;

pub const ACCESS_SET_DOMAIN_V1: &str = "PulseDAG:access-set:v1";
pub const ACCESS_SET_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccessOutpointV1 {
    pub txid: String,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccessKeyIdV1(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSetV1 {
    pub writes: BTreeSet<AccessOutpointV1>,
    pub reads: BTreeSet<AccessOutpointV1>,
    pub write_keys: BTreeSet<AccessKeyIdV1>,
    pub read_keys: BTreeSet<AccessKeyIdV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessSetAdmissionV1 {
    pub format_admitted: bool,
}

impl AccessSetAdmissionV1 {
    pub const INACTIVE: Self = Self {
        format_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.format_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessSetV1Error {
    FormatDisabled,
    EmptyWrites,
    DuplicateDeclaration,
}

impl std::fmt::Display for AccessSetV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FormatDisabled => {
                write!(f, "access-set v1 is not admitted; tx v2 must not infer one")
            }
            Self::EmptyWrites => write!(f, "value-moving access set must declare writes"),
            Self::DuplicateDeclaration => {
                write!(f, "access-set declaration contains internal duplicates")
            }
        }
    }
}

impl std::error::Error for AccessSetV1Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessConflictClassV1 {
    AccessConflict,
    AccessUnderdeclared,
}

pub fn validate_access_set_v1(
    admission: AccessSetAdmissionV1,
    set: &AccessSetV1,
    value_moving: bool,
) -> Result<(), AccessSetV1Error> {
    if !admission.admitted() {
        return Err(AccessSetV1Error::FormatDisabled);
    }
    if value_moving && set.writes.is_empty() {
        return Err(AccessSetV1Error::EmptyWrites);
    }
    if set.writes.intersection(&set.reads).next().is_some() {
        // listed in both is over-declare of the same outpoint as read+write;
        // treat as duplicate declaration of role, not a scheduler conflict.
        return Err(AccessSetV1Error::DuplicateDeclaration);
    }
    if set.write_keys.intersection(&set.read_keys).next().is_some() {
        return Err(AccessSetV1Error::DuplicateDeclaration);
    }
    Ok(())
}

pub fn spends_underdeclared_v1(set: &AccessSetV1, spent: &AccessOutpointV1) -> bool {
    !set.writes.contains(spent)
}

pub fn inspects_underdeclared_v1(set: &AccessSetV1, inspected: &AccessOutpointV1) -> bool {
    !set.writes.contains(inspected) && !set.reads.contains(inspected)
}

/// Write-write, write-read, and key write overlaps. Read-read is not a conflict.
pub fn access_sets_conflict_v1(a: &AccessSetV1, b: &AccessSetV1) -> bool {
    a.writes.intersection(&b.writes).next().is_some()
        || a.writes.intersection(&b.reads).next().is_some()
        || b.writes.intersection(&a.reads).next().is_some()
        || a.write_keys.intersection(&b.write_keys).next().is_some()
        || a.write_keys.intersection(&b.read_keys).next().is_some()
        || b.write_keys.intersection(&a.read_keys).next().is_some()
}

/// Walk `ordered` and keep the first non-conflicting internally valid set.
/// Already-accepted members of the window determine later rejects.
pub fn schedule_access_sets_v1(
    admission: AccessSetAdmissionV1,
    ordered: &[(u32, AccessSetV1)],
    value_moving: bool,
) -> (Vec<u32>, Vec<(u32, AccessConflictClassV1)>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut accepted_sets: Vec<&AccessSetV1> = Vec::new();
    // A duplicated caller identity makes *both* entries ambiguous, even if
    // their resources are disjoint. Never select the first by arrival order.
    // This remains an inactive, non-consensus scheduling hypothesis.
    let mut seen_ids = BTreeSet::new();
    let mut duplicate_ids = BTreeSet::new();
    for (id, _) in ordered {
        if !seen_ids.insert(*id) {
            duplicate_ids.insert(*id);
        }
    }
    for (id, set) in ordered {
        if duplicate_ids.contains(id) {
            rejected.push((*id, AccessConflictClassV1::AccessUnderdeclared));
            continue;
        }
        if !admission.admitted() {
            rejected.push((*id, AccessConflictClassV1::AccessUnderdeclared));
            continue;
        }
        if validate_access_set_v1(admission, set, value_moving).is_err() {
            rejected.push((*id, AccessConflictClassV1::AccessUnderdeclared));
            continue;
        }
        if accepted_sets
            .iter()
            .any(|prev| access_sets_conflict_v1(prev, set))
        {
            rejected.push((*id, AccessConflictClassV1::AccessConflict));
            continue;
        }
        accepted.push(*id);
        accepted_sets.push(set);
    }
    (accepted, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(txid: &str, index: u32) -> AccessOutpointV1 {
        AccessOutpointV1 {
            txid: txid.into(),
            index,
        }
    }

    fn writes(ops: &[AccessOutpointV1]) -> AccessSetV1 {
        AccessSetV1 {
            writes: ops.iter().cloned().collect(),
            reads: BTreeSet::new(),
            write_keys: BTreeSet::new(),
            read_keys: BTreeSet::new(),
        }
    }

    fn admitted() -> AccessSetAdmissionV1 {
        AccessSetAdmissionV1 {
            format_admitted: true,
        }
    }

    #[test]
    fn v2_must_not_infer_an_access_set() {
        let set = writes(&[op("aa", 0)]);
        assert_eq!(
            validate_access_set_v1(AccessSetAdmissionV1::INACTIVE, &set, true),
            Err(AccessSetV1Error::FormatDisabled)
        );
    }

    #[test]
    fn write_write_and_write_read_conflict_read_read_does_not() {
        let a = writes(&[op("aa", 0)]);
        let b = writes(&[op("aa", 0)]);
        assert!(access_sets_conflict_v1(&a, &b));

        let reader = AccessSetV1 {
            writes: [op("bb", 0)].into_iter().collect(),
            reads: [op("aa", 0)].into_iter().collect(),
            write_keys: BTreeSet::new(),
            read_keys: BTreeSet::new(),
        };
        assert!(access_sets_conflict_v1(&a, &reader));

        let r1 = AccessSetV1 {
            writes: [op("c1", 0)].into_iter().collect(),
            reads: [op("zz", 0)].into_iter().collect(),
            write_keys: BTreeSet::new(),
            read_keys: BTreeSet::new(),
        };
        let r2 = AccessSetV1 {
            writes: [op("c2", 0)].into_iter().collect(),
            reads: [op("zz", 0)].into_iter().collect(),
            write_keys: BTreeSet::new(),
            read_keys: BTreeSet::new(),
        };
        assert!(!access_sets_conflict_v1(&r1, &r2));
    }

    #[test]
    fn scheduler_is_deterministic_in_ghostdag_order() {
        let first = writes(&[op("aa", 0)]);
        let second = writes(&[op("aa", 0)]);
        let third = writes(&[op("bb", 1)]);
        let window = [(1, first), (2, second), (3, third)];
        let (accepted, rejected) = schedule_access_sets_v1(admitted(), &window, true);
        assert_eq!(accepted, vec![1, 3]);
        assert_eq!(rejected, vec![(2, AccessConflictClassV1::AccessConflict)]);
        let again = schedule_access_sets_v1(admitted(), &window, true);
        assert_eq!((accepted, rejected), again);
    }

    #[test]
    fn repeated_window_id_rejects_every_alias_even_when_resources_do_not_conflict() {
        let a = writes(&[op("aa", 0)]);
        let b = writes(&[op("bb", 1)]);
        let c = writes(&[op("cc", 2)]);
        let forward = [(17, a.clone()), (29, c.clone()), (17, b.clone())];
        let reverse = [(17, b), (29, c), (17, a)];

        for window in [forward, reverse] {
            let (accepted, rejected) = schedule_access_sets_v1(admitted(), &window, true);
            assert_eq!(accepted, vec![29]);
            assert_eq!(
                rejected,
                vec![
                    (17, AccessConflictClassV1::AccessUnderdeclared),
                    (17, AccessConflictClassV1::AccessUnderdeclared),
                ]
            );
        }
    }

    #[test]
    fn duplicate_window_id_cannot_be_laundered_through_an_invalid_first_entry() {
        let invalid = AccessSetV1 {
            writes: BTreeSet::new(),
            reads: BTreeSet::new(),
            write_keys: BTreeSet::new(),
            read_keys: BTreeSet::new(),
        };
        let window = [(7, invalid), (7, writes(&[op("aa", 0)]))];
        let (accepted, rejected) = schedule_access_sets_v1(admitted(), &window, true);
        assert!(accepted.is_empty());
        assert_eq!(
            rejected,
            vec![
                (7, AccessConflictClassV1::AccessUnderdeclared),
                (7, AccessConflictClassV1::AccessUnderdeclared),
            ]
        );
        let (inactive_accepted, inactive_rejected) =
            schedule_access_sets_v1(AccessSetAdmissionV1::INACTIVE, &window, true);
        assert!(inactive_accepted.is_empty());
        assert_eq!(inactive_rejected, rejected);
    }

    #[test]
    fn underdeclare_spend_is_invalid() {
        let set = writes(&[op("aa", 0)]);
        assert!(spends_underdeclared_v1(&set, &op("aa", 1)));
        assert!(!spends_underdeclared_v1(&set, &op("aa", 0)));
        assert!(inspects_underdeclared_v1(&set, &op("zz", 0)));
    }

    #[test]
    fn inactive_scheduler_rejects_the_whole_window() {
        let window = [(1, writes(&[op("aa", 0)]))];
        let (accepted, rejected) =
            schedule_access_sets_v1(AccessSetAdmissionV1::INACTIVE, &window, true);
        assert!(accepted.is_empty());
        assert_eq!(
            rejected,
            vec![(1, AccessConflictClassV1::AccessUnderdeclared)]
        );
    }
}
