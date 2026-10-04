//! The run's finishing writes: write the state, then the recap, stopping at the first failure.

use crate::tracker::{Tracker, TrackerError, WorkItemId, WorkState};
use std::fmt;

/// One write the run makes when it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    WriteState,
    WriteRecap,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WriteState => "write state",
            Self::WriteRecap => "write recap",
        })
    }
}

/// The finishing step that failed, and what the tracker reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishError {
    pub step: Step,
    pub error: TrackerError,
}

impl fmt::Display for FinishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let step = self.step;
        let error = &self.error;
        write!(f, "{step}: {error}")
    }
}

impl std::error::Error for FinishError {}

/// Writes the state then the recap against `tracker`, in that one order.
///
/// # Errors
/// Returns a [`FinishError`] naming the step when the tracker refuses it, and stops there.
pub fn finish_run(
    tracker: &dyn Tracker,
    id: &WorkItemId,
    state: &WorkState,
    recap: &str,
) -> Result<(), FinishError> {
    tracker
        .write_state(id, state)
        .map_err(|error| FinishError {
            step: Step::WriteState,
            error,
        })?;
    tracker.write_recap(id, recap).map_err(|error| FinishError {
        step: Step::WriteRecap,
        error,
    })
}

#[cfg(test)]
mod tests {
    use crate::tracker::fake::{Call, FakeTracker, Operation};
    use crate::tracker::report::{finish_run, FinishError, Step};
    use crate::tracker::{TrackerError, WorkItemId, WorkState};

    fn id() -> WorkItemId {
        WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: "42".to_string(),
        }
    }

    #[test]
    fn finish_run_writes_the_state_then_the_recap() {
        let tracker = FakeTracker::new();
        finish_run(&tracker, &id(), &WorkState::InReview, "done").expect("finishes");
        assert_eq!(
            tracker.calls(),
            vec![
                Call::WriteState {
                    id: id(),
                    state: WorkState::InReview,
                },
                Call::WriteRecap {
                    id: id(),
                    recap: "done".to_string(),
                },
            ]
        );
    }

    #[test]
    fn a_failed_state_write_stops_before_the_recap() {
        let tracker = FakeTracker::new().fail(
            Operation::WriteState,
            TrackerError::Rejected("no".to_string()),
        );
        let error = finish_run(&tracker, &id(), &WorkState::InReview, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::WriteState,
                error: TrackerError::Rejected("no".to_string()),
            }
        );
        assert_eq!(
            tracker.calls(),
            vec![Call::WriteState {
                id: id(),
                state: WorkState::InReview,
            }]
        );
    }

    #[test]
    fn a_failed_recap_write_names_the_recap_step() {
        let tracker = FakeTracker::new().fail(
            Operation::WriteRecap,
            TrackerError::Failed("dropped".to_string()),
        );
        let error = finish_run(&tracker, &id(), &WorkState::Failed, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::WriteRecap,
                error: TrackerError::Failed("dropped".to_string()),
            }
        );
        assert_eq!(
            tracker.calls(),
            vec![
                Call::WriteState {
                    id: id(),
                    state: WorkState::Failed,
                },
                Call::WriteRecap {
                    id: id(),
                    recap: "done".to_string(),
                },
            ]
        );
    }

    #[test]
    fn step_displays_and_serializes_as_kebab_case() {
        assert_eq!(Step::WriteState.to_string(), "write state");
        assert_eq!(Step::WriteRecap.to_string(), "write recap");
        assert_eq!(
            serde_json::to_string(&Step::WriteState).expect("serializes"),
            r#""write-state""#
        );
        assert_eq!(
            serde_json::to_string(&Step::WriteRecap).expect("serializes"),
            r#""write-recap""#
        );
    }

    #[test]
    fn a_finish_error_displays_the_step_then_the_error() {
        let error = FinishError {
            step: Step::WriteRecap,
            error: TrackerError::Failed("dropped".to_string()),
        };
        assert_eq!(error.to_string(), "write recap: dropped");
    }
}
