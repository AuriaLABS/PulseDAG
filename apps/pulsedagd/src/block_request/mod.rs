mod implementation;

use std::collections::HashSet;

pub use implementation::{
    BlockRequestTracker, DependencyFetchPlan, GetBlockRequestReadiness, HeaderFetchCandidate,
};

/// Runtime wrapper around the dependency-aware scheduler.
///
/// The underlying scheduler intentionally keeps children deferred while their
/// parents are unknown. A rejoin can, however, reach a quiescent state where a
/// parent was scheduled in the current pass, no request remains in flight, and
/// the deferred child has no later event that would call `next_requests` again.
///
/// Drain bounded dependency layers in the same scheduling turn, treating hashes
/// scheduled earlier in this turn as pending. Requests remain parent-first, but
/// the runtime no longer leaves an otherwise actionable child stranded solely
/// because its parent was added to the same plan.
#[derive(Debug, Clone)]
pub struct DependencyAwareFetchScheduler {
    inner: implementation::DependencyAwareFetchScheduler,
}

impl Default for DependencyAwareFetchScheduler {
    fn default() -> Self {
        Self::with_limit(512)
    }
}

impl DependencyAwareFetchScheduler {
    pub fn with_limit(max_queue_depth: usize) -> Self {
        Self {
            inner: implementation::DependencyAwareFetchScheduler::with_limit(max_queue_depth),
        }
    }

    pub fn queue_depth(&self) -> usize {
        self.inner.queue_depth()
    }

    pub fn queue_inventory<I, S>(&mut self, hashes: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inner.queue_inventory(hashes)
    }

    pub fn queue_headers<I>(&mut self, headers: I) -> usize
    where
        I: IntoIterator<Item = HeaderFetchCandidate>,
    {
        self.inner.queue_headers(headers)
    }

    pub fn next_requests(
        &mut self,
        known_blocks: &HashSet<String>,
        pending_blocks: &HashSet<String>,
        max: usize,
    ) -> DependencyFetchPlan {
        let mut combined = DependencyFetchPlan::default();
        if max == 0 {
            return combined;
        }

        let mut scheduled = pending_blocks.clone();
        let mut last_deferred = Vec::new();

        while combined.requests.len() < max {
            let remaining = max.saturating_sub(combined.requests.len());
            let plan = self
                .inner
                .next_requests(known_blocks, &scheduled, remaining);
            combined.parent_first_requests = combined
                .parent_first_requests
                .saturating_add(plan.parent_first_requests);
            last_deferred = plan.deferred;

            let mut added = 0usize;
            for hash in plan.requests {
                if scheduled.insert(hash.clone()) {
                    combined.requests.push(hash);
                    added = added.saturating_add(1);
                }
            }

            if last_deferred.is_empty() || added == 0 {
                break;
            }
        }

        combined.deferred = last_deferred;
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyAwareFetchScheduler, HeaderFetchCandidate};
    use std::collections::HashSet;

    #[test]
    fn drains_child_when_parent_is_scheduled_in_same_turn() {
        let mut scheduler = DependencyAwareFetchScheduler::default();
        scheduler.queue_headers([HeaderFetchCandidate {
            hash: "child".into(),
            parents: vec!["parent".into()],
            height: 2,
        }]);

        let plan = scheduler.next_requests(&HashSet::new(), &HashSet::new(), 4);

        assert_eq!(plan.requests, vec!["parent", "child"]);
        assert!(plan.deferred.is_empty());
        assert_eq!(scheduler.queue_depth(), 0);
    }

    #[test]
    fn keeps_parent_first_order_across_multiple_dependency_layers() {
        let mut scheduler = DependencyAwareFetchScheduler::default();
        scheduler.queue_headers([
            HeaderFetchCandidate {
                hash: "parent".into(),
                parents: vec!["root".into()],
                height: 2,
            },
            HeaderFetchCandidate {
                hash: "child".into(),
                parents: vec!["parent".into()],
                height: 3,
            },
        ]);

        let plan = scheduler.next_requests(&HashSet::new(), &HashSet::new(), 8);

        let root = plan
            .requests
            .iter()
            .position(|hash| hash == "root")
            .unwrap();
        let parent = plan
            .requests
            .iter()
            .position(|hash| hash == "parent")
            .unwrap();
        let child = plan
            .requests
            .iter()
            .position(|hash| hash == "child")
            .unwrap();
        assert!(root < parent);
        assert!(parent < child);
        assert!(plan.deferred.is_empty());
        assert_eq!(scheduler.queue_depth(), 0);
    }
}
