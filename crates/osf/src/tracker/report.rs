//! The run's finishing writes: write the state, then the recap, stopping at the first failure.

use crate::tracker::{Capability, Tracker, TrackerError, WorkItemId, WorkState};
use std::fmt;

/// One write the run makes when it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    CheckCapabilities,
    WriteState,
    WriteRecap,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CheckCapabilities => "check capabilities",
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

/// The plain name of `state`, such as `blocked (human)`.
fn state_name(state: &WorkState) -> String {
    match state {
        WorkState::InProgress => "in-progress".to_string(),
        WorkState::Verifying => "verifying".to_string(),
        WorkState::InReview => "in-review".to_string(),
        WorkState::Failed => "failed".to_string(),
        WorkState::Blocked(cause) => format!("blocked ({})", cause.as_str()),
    }
}

/// Checks one capability under `label`, rejecting it unless the tracker reports it supported.
fn require_capability(capability: &Capability, label: &str) -> Result<(), FinishError> {
    match capability {
        Capability::Supported => Ok(()),
        Capability::Unsupported(reason) => Err(FinishError {
            step: Step::CheckCapabilities,
            error: TrackerError::Rejected(format!("{label} is unsupported: {reason}")),
        }),
        Capability::Unknown(reason) => Err(FinishError {
            step: Step::CheckCapabilities,
            error: TrackerError::Failed(format!("{label} is unknown: {reason}")),
        }),
    }
}

/// Checks the states a run needs and the recap against the tracker's reported capabilities.
///
/// # Errors
/// Returns a [`FinishError`] naming `check capabilities` when a needed state or the recap is not supported.
pub fn require_capabilities(
    tracker: &dyn Tracker,
    states: &[WorkState],
    needs_recap: bool,
) -> Result<(), FinishError> {
    let capabilities = tracker.capabilities();
    for state in states {
        match capabilities
            .state_writes
            .iter()
            .find(|write| write.state == *state)
        {
            None => {
                return Err(FinishError {
                    step: Step::CheckCapabilities,
                    error: TrackerError::Rejected(format!("{} is not reported", state_name(state))),
                });
            }
            Some(write) => require_capability(&write.capability, &state_name(state))?,
        }
    }
    if needs_recap {
        require_capability(&capabilities.recap, "recap")?;
    }
    Ok(())
}

/// Writes the state then the recap against `tracker`, after checking both are supported.
///
/// # Errors
/// Returns a [`FinishError`] naming the first step that fails, and stops there.
pub fn finish_run(
    tracker: &dyn Tracker,
    id: &WorkItemId,
    state: &WorkState,
    recap: &str,
) -> Result<(), FinishError> {
    require_capabilities(tracker, std::slice::from_ref(state), true)?;
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
    use crate::tracker::report::{finish_run, require_capabilities, FinishError, Step};
    use crate::tracker::{BlockedCause, Capability, TrackerError, WorkItemId, WorkState};

    fn id() -> WorkItemId {
        WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: "42".to_string(),
        }
    }

    fn set_capability(tracker: &mut FakeTracker, state: &WorkState, capability: Capability) {
        let entry = tracker
            .capabilities
            .state_writes
            .iter_mut()
            .find(|write| write.state == *state)
            .expect("the state is listed");
        entry.capability = capability;
    }

    #[test]
    fn finish_run_writes_the_state_then_the_recap() {
        let tracker = FakeTracker::new();
        finish_run(&tracker, &id(), &WorkState::InReview, "done").expect("finishes");
        assert_eq!(
            tracker.calls(),
            vec![
                Call::Capabilities,
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
            vec![
                Call::Capabilities,
                Call::WriteState {
                    id: id(),
                    state: WorkState::InReview,
                },
            ]
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
                Call::Capabilities,
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
    fn finish_run_stops_before_any_write_when_a_state_is_unsupported() {
        let tracker = FakeTracker::new().unsupported(&WorkState::InReview, "no in-review");
        let error = finish_run(&tracker, &id(), &WorkState::InReview, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::CheckCapabilities,
                error: TrackerError::Rejected("in-review is unsupported: no in-review".to_string()),
            }
        );
        assert_eq!(tracker.calls(), vec![Call::Capabilities]);
    }

    #[test]
    fn finish_run_stops_before_any_write_when_a_state_is_unknown() {
        let mut tracker = FakeTracker::new();
        set_capability(
            &mut tracker,
            &WorkState::InReview,
            Capability::Unknown("maybe".to_string()),
        );
        let error = finish_run(&tracker, &id(), &WorkState::InReview, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::CheckCapabilities,
                error: TrackerError::Failed("in-review is unknown: maybe".to_string()),
            }
        );
        assert_eq!(tracker.calls(), vec![Call::Capabilities]);
    }

    #[test]
    fn finish_run_stops_before_any_write_when_the_recap_is_unsupported() {
        let mut tracker = FakeTracker::new();
        tracker.capabilities.recap = Capability::Unsupported("no recap".to_string());
        let error = finish_run(&tracker, &id(), &WorkState::InReview, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::CheckCapabilities,
                error: TrackerError::Rejected("recap is unsupported: no recap".to_string()),
            }
        );
        assert_eq!(tracker.calls(), vec![Call::Capabilities]);
    }

    #[test]
    fn finish_run_writes_a_supported_blocked_human_state() {
        let tracker = FakeTracker::new();
        finish_run(
            &tracker,
            &id(),
            &WorkState::Blocked(BlockedCause::Human),
            "done",
        )
        .expect("finishes");
        assert_eq!(
            tracker.calls(),
            vec![
                Call::Capabilities,
                Call::WriteState {
                    id: id(),
                    state: WorkState::Blocked(BlockedCause::Human),
                },
                Call::WriteRecap {
                    id: id(),
                    recap: "done".to_string(),
                },
            ]
        );
    }

    #[test]
    fn finish_run_refuses_a_state_missing_from_the_list() {
        let mut tracker = FakeTracker::new();
        tracker
            .capabilities
            .state_writes
            .retain(|write| write.state != WorkState::InReview);
        let error = finish_run(&tracker, &id(), &WorkState::InReview, "done").expect_err("fails");
        assert_eq!(
            error,
            FinishError {
                step: Step::CheckCapabilities,
                error: TrackerError::Rejected("in-review is not reported".to_string()),
            }
        );
        assert_eq!(tracker.calls(), vec![Call::Capabilities]);
    }

    #[test]
    fn require_capabilities_reads_the_tracker_once_for_several_states() {
        let tracker = FakeTracker::new();
        require_capabilities(
            &tracker,
            &[
                WorkState::InProgress,
                WorkState::Verifying,
                WorkState::InReview,
            ],
            true,
        )
        .expect("supported");
        assert_eq!(tracker.calls(), vec![Call::Capabilities]);
    }

    #[test]
    fn step_displays_and_serializes_as_kebab_case() {
        assert_eq!(Step::CheckCapabilities.to_string(), "check capabilities");
        assert_eq!(Step::WriteState.to_string(), "write state");
        assert_eq!(Step::WriteRecap.to_string(), "write recap");
        assert_eq!(
            serde_json::to_string(&Step::CheckCapabilities).expect("serializes"),
            r#""check-capabilities""#
        );
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
