//! The ready rule: which read items the engine may dispatch, kept out of every adapter.

use crate::tracker::{ReadOutcome, Status, Tracker, WorkItem, WorkItemId};

/// Why the ready rule skipped an item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotDispatchable {
    StatusNotReady(String),
    OpenBlockers(Vec<WorkItemId>),
}

/// A read item the ready rule skipped, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Skipped {
    pub id: WorkItemId,
    pub reason: NotDispatchable,
}

/// The outcome of one ready pick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pick {
    Item(WorkItem),
    NothingReady,
    NoneDispatchable(Vec<Skipped>),
    Unknown(String),
}

/// Whether the ready rule may dispatch this item: its status is ready and every blocker is closed.
#[must_use]
pub fn dispatchable(item: &WorkItem) -> bool {
    matches!(&item.status, Status::Ready)
        && item.blocked_by.iter().all(|dependency| dependency.closed)
}

/// Reads the ready items and picks the first dispatchable one, or explains why none is.
#[must_use]
pub fn pick(tracker: &dyn Tracker) -> Pick {
    match tracker.read_ready() {
        ReadOutcome::Empty => Pick::NothingReady,
        ReadOutcome::Unknown(text) => Pick::Unknown(text),
        ReadOutcome::Found(items) => {
            if items.is_empty() {
                return Pick::NothingReady;
            }
            for item in &items {
                if dispatchable(item) {
                    return Pick::Item(item.clone());
                }
            }
            let skipped = items
                .iter()
                .map(|item| Skipped {
                    id: item.id.clone(),
                    reason: reason(item),
                })
                .collect();
            Pick::NoneDispatchable(skipped)
        }
    }
}

/// The reason one non-dispatchable item was skipped.
fn reason(item: &WorkItem) -> NotDispatchable {
    match &item.status {
        Status::Ready => NotDispatchable::OpenBlockers(
            item.blocked_by
                .iter()
                .filter(|dependency| !dependency.closed)
                .map(|dependency| dependency.id.clone())
                .collect(),
        ),
        Status::Other(text) => NotDispatchable::StatusNotReady(text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use crate::tracker::fake::{Call, FakeTracker};
    use crate::tracker::ready::{dispatchable, pick, NotDispatchable, Pick, Skipped};
    use crate::tracker::{Dependency, ReadOutcome, Status, WorkItem, WorkItemId};

    fn id(item: &str) -> WorkItemId {
        WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: item.to_string(),
        }
    }

    fn dependency(item: &str, closed: bool) -> Dependency {
        Dependency {
            id: id(item),
            closed,
        }
    }

    fn work_item(item: &str, status: Status, blocked_by: Vec<Dependency>) -> WorkItem {
        WorkItem {
            id: id(item),
            title: "A change".to_string(),
            body: "Body.".to_string(),
            repository: "open-software-factory/demo".to_string(),
            status,
            blocked_by,
        }
    }

    #[test]
    fn dispatchable_is_a_truth_table() {
        let ready_closed = work_item("1", Status::Ready, vec![dependency("2", true)]);
        assert!(dispatchable(&ready_closed));
        let ready_clear = work_item("1", Status::Ready, Vec::new());
        assert!(dispatchable(&ready_clear));
        let paused = work_item(
            "1",
            Status::Other("Paused".to_string()),
            vec![dependency("2", true)],
        );
        assert!(!dispatchable(&paused));
        let blocked = work_item(
            "1",
            Status::Ready,
            vec![dependency("2", true), dependency("3", false)],
        );
        assert!(!dispatchable(&blocked));
    }

    #[test]
    fn pick_returns_the_first_dispatchable_item_and_skips_ahead() {
        let mut tracker = FakeTracker::new();
        let skipped = work_item("1", Status::Other("Paused".to_string()), Vec::new());
        let chosen = work_item("2", Status::Ready, Vec::new());
        tracker.ready = ReadOutcome::Found(vec![skipped, chosen.clone()]);
        assert_eq!(pick(&tracker), Pick::Item(chosen));
    }

    #[test]
    fn pick_of_empty_is_nothing_ready() {
        let tracker = FakeTracker::new();
        assert_eq!(pick(&tracker), Pick::NothingReady);
    }

    #[test]
    fn pick_of_unknown_is_unknown_and_not_nothing_ready() {
        let mut tracker = FakeTracker::new();
        tracker.ready = ReadOutcome::Unknown("x".to_string());
        let picked = pick(&tracker);
        assert_eq!(picked, Pick::Unknown("x".to_string()));
        assert_ne!(picked, Pick::NothingReady);
    }

    #[test]
    fn pick_of_an_empty_found_list_is_nothing_ready() {
        let mut tracker = FakeTracker::new();
        tracker.ready = ReadOutcome::Found(Vec::new());
        assert_eq!(pick(&tracker), Pick::NothingReady);
    }

    #[test]
    fn pick_lists_each_skipped_item_with_its_typed_reason() {
        let mut tracker = FakeTracker::new();
        let paused = work_item("1", Status::Other("Paused".to_string()), Vec::new());
        let blocked = work_item(
            "2",
            Status::Ready,
            vec![
                dependency("3", true),
                dependency("4", false),
                dependency("5", false),
            ],
        );
        tracker.ready = ReadOutcome::Found(vec![paused.clone(), blocked.clone()]);
        assert_eq!(
            pick(&tracker),
            Pick::NoneDispatchable(vec![
                Skipped {
                    id: id("1"),
                    reason: NotDispatchable::StatusNotReady("Paused".to_string()),
                },
                Skipped {
                    id: id("2"),
                    reason: NotDispatchable::OpenBlockers(vec![id("4"), id("5")]),
                },
            ])
        );
    }

    #[test]
    fn pick_calls_only_read_ready() {
        let mut tracker = FakeTracker::new();
        tracker.ready = ReadOutcome::Found(vec![work_item("1", Status::Ready, Vec::new())]);
        let picked = pick(&tracker);
        assert_eq!(
            picked,
            Pick::Item(work_item("1", Status::Ready, Vec::new()))
        );
        assert_eq!(tracker.calls(), vec![Call::ReadReady]);
    }
}
