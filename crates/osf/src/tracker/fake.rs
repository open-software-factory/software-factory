//! An in-memory [`Tracker`] test double: it records every call as plain data and does no I/O.

use crate::tracker::{
    BlockedCause, Capability, ReadOutcome, StateWrite, Tracker, TrackerCapabilities, TrackerError,
    WorkItem, WorkItemId, WorkState,
};
use std::cell::RefCell;

/// One call the double recorded, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Call {
    ReadReady,
    WriteState { id: WorkItemId, state: WorkState },
    WriteRecap { id: WorkItemId, recap: String },
    Capabilities,
}

/// The double's writes that can be told to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    WriteState,
    WriteRecap,
}

/// A [`Tracker`] that answers from memory and records every call.
pub struct FakeTracker {
    calls: RefCell<Vec<Call>>,
    failures: Vec<(Operation, TrackerError)>,
    /// The answer [`Tracker::read_ready`] returns.
    pub ready: ReadOutcome<Vec<WorkItem>>,
    /// The answer [`Tracker::capabilities`] returns.
    pub capabilities: TrackerCapabilities,
}

impl FakeTracker {
    /// A double with the documented defaults and no scripted failure.
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            ready: ReadOutcome::Empty,
            capabilities: TrackerCapabilities {
                title: Capability::Supported,
                body: Capability::Supported,
                repository: Capability::Supported,
                dependencies: Capability::Supported,
                ready_mark: Capability::Supported,
                state_writes: vec![
                    StateWrite {
                        state: WorkState::InProgress,
                        capability: Capability::Supported,
                    },
                    StateWrite {
                        state: WorkState::Verifying,
                        capability: Capability::Supported,
                    },
                    StateWrite {
                        state: WorkState::InReview,
                        capability: Capability::Supported,
                    },
                    StateWrite {
                        state: WorkState::Blocked(BlockedCause::Dependency),
                        capability: Capability::Supported,
                    },
                    StateWrite {
                        state: WorkState::Failed,
                        capability: Capability::Supported,
                    },
                ],
                recap: Capability::Supported,
            },
        }
    }

    /// Makes `operation` fail with `error` after still recording its call.
    #[must_use]
    pub fn fail(mut self, operation: Operation, error: TrackerError) -> Self {
        self.failures.push((operation, error));
        self
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// The scripted failure for `operation`, if any.
    fn failure(&self, operation: Operation) -> Option<TrackerError> {
        self.failures
            .iter()
            .find(|(failed, _)| *failed == operation)
            .map(|(_, error)| error.clone())
    }
}

impl Default for FakeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker for FakeTracker {
    fn read_ready(&self) -> ReadOutcome<Vec<WorkItem>> {
        self.calls.borrow_mut().push(Call::ReadReady);
        self.ready.clone()
    }

    fn write_state(&self, id: &WorkItemId, state: &WorkState) -> Result<(), TrackerError> {
        self.calls.borrow_mut().push(Call::WriteState {
            id: id.clone(),
            state: state.clone(),
        });
        if let Some(error) = self.failure(Operation::WriteState) {
            return Err(error);
        }
        Ok(())
    }

    fn write_recap(&self, id: &WorkItemId, recap: &str) -> Result<(), TrackerError> {
        self.calls.borrow_mut().push(Call::WriteRecap {
            id: id.clone(),
            recap: recap.to_string(),
        });
        if let Some(error) = self.failure(Operation::WriteRecap) {
            return Err(error);
        }
        Ok(())
    }

    fn capabilities(&self) -> TrackerCapabilities {
        self.calls.borrow_mut().push(Call::Capabilities);
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::tracker::fake::{Call, FakeTracker, Operation};
    use crate::tracker::{
        BlockedCause, Capability, ReadOutcome, Status, Tracker, TrackerCapabilities, TrackerError,
        WorkItem, WorkItemId, WorkState,
    };

    fn id() -> WorkItemId {
        WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: "42".to_string(),
        }
    }

    fn work_item() -> WorkItem {
        WorkItem {
            id: id(),
            title: "A change".to_string(),
            body: "Body.".to_string(),
            repository: "open-software-factory/demo".to_string(),
            status: Status::Ready,
            closed: false,
            blocked_by: Vec::new(),
        }
    }

    #[test]
    fn the_double_records_every_call_in_order() {
        let tracker = FakeTracker::new();
        let _ = tracker.read_ready();
        tracker
            .write_state(&id(), &WorkState::InProgress)
            .expect("writes");
        tracker.write_recap(&id(), "done").expect("writes");
        let _ = tracker.capabilities();
        assert_eq!(
            tracker.calls(),
            vec![
                Call::ReadReady,
                Call::WriteState {
                    id: id(),
                    state: WorkState::InProgress,
                },
                Call::WriteRecap {
                    id: id(),
                    recap: "done".to_string(),
                },
                Call::Capabilities,
            ]
        );
    }

    #[test]
    fn the_scripted_read_is_returned_unchanged() {
        let mut tracker = FakeTracker::new();
        tracker.ready = ReadOutcome::Found(vec![work_item()]);
        assert_eq!(tracker.read_ready(), ReadOutcome::Found(vec![work_item()]));
    }

    #[test]
    fn the_scripted_capabilities_are_returned_unchanged() {
        let mut tracker = FakeTracker::new();
        let capabilities = TrackerCapabilities {
            title: Capability::Unsupported("no title".to_string()),
            body: Capability::Unknown("n/a".to_string()),
            repository: Capability::Supported,
            dependencies: Capability::Supported,
            ready_mark: Capability::Supported,
            state_writes: Vec::new(),
            recap: Capability::Supported,
        };
        tracker.capabilities = capabilities.clone();
        assert_eq!(tracker.capabilities(), capabilities);
    }

    #[test]
    fn the_default_capabilities_support_every_state() {
        let tracker = FakeTracker::new();
        let capabilities = tracker.capabilities();
        assert_eq!(capabilities.title, Capability::Supported);
        assert_eq!(capabilities.body, Capability::Supported);
        assert_eq!(capabilities.repository, Capability::Supported);
        assert_eq!(capabilities.dependencies, Capability::Supported);
        assert_eq!(capabilities.ready_mark, Capability::Supported);
        assert_eq!(capabilities.recap, Capability::Supported);
        let states: Vec<WorkState> = capabilities
            .state_writes
            .iter()
            .map(|write| write.state.clone())
            .collect();
        assert_eq!(
            states,
            vec![
                WorkState::InProgress,
                WorkState::Verifying,
                WorkState::InReview,
                WorkState::Blocked(BlockedCause::Dependency),
                WorkState::Failed,
            ]
        );
        assert!(capabilities
            .state_writes
            .iter()
            .all(|write| write.capability == Capability::Supported));
    }

    #[test]
    fn a_failed_state_write_is_returned_after_the_call_is_recorded() {
        let tracker = FakeTracker::new().fail(
            Operation::WriteState,
            TrackerError::Rejected("no".to_string()),
        );
        let error = tracker
            .write_state(&id(), &WorkState::Failed)
            .expect_err("fails");
        assert_eq!(error, TrackerError::Rejected("no".to_string()));
        assert_eq!(
            tracker.calls(),
            vec![Call::WriteState {
                id: id(),
                state: WorkState::Failed,
            }]
        );
    }

    #[test]
    fn a_failed_recap_write_is_returned_after_the_call_is_recorded() {
        let tracker = FakeTracker::new().fail(
            Operation::WriteRecap,
            TrackerError::Failed("dropped".to_string()),
        );
        let error = tracker.write_recap(&id(), "done").expect_err("fails");
        assert_eq!(error, TrackerError::Failed("dropped".to_string()));
        assert_eq!(
            tracker.calls(),
            vec![Call::WriteRecap {
                id: id(),
                recap: "done".to_string(),
            }]
        );
    }
}
