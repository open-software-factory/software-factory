//! Provider-neutral tracker interface: the seam between the factory and a work tracker.

pub mod fake;
pub mod ready;
pub mod report;

pub use ready::{dispatchable, pick, NotDispatchable, Pick, Skipped};
pub use report::{finish_run, FinishError, Step};

use std::fmt;
use std::str::FromStr;

/// An empty backlog is [`ReadOutcome::Empty`], an unreadable one is [`ReadOutcome::Unknown`], and [`ReadOutcome::Found`] never holds an empty list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadOutcome<T> {
    Found(T),
    Empty,
    Unknown(String),
}

/// Identifies a work item: `provider`, the project path `repo`, and the tracker's own `item`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkItemId {
    pub provider: String,
    pub repo: String,
    pub item: String,
}

impl fmt::Display for WorkItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let provider = &self.provider;
        let repo = &self.repo;
        let item = &self.item;
        write!(f, "{provider}:{repo}#{item}")
    }
}

impl FromStr for WorkItemId {
    type Err = WorkItemIdError;

    /// # Errors
    /// Returns an error when the text is not `provider:repo#item` with a project path.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (provider, rest) = text
            .split_once(':')
            .ok_or_else(|| WorkItemIdError("missing ':'".to_string()))?;
        let (repo, item) = rest
            .rsplit_once('#')
            .ok_or_else(|| WorkItemIdError("missing '#'".to_string()))?;
        if provider.is_empty() {
            return Err(WorkItemIdError("empty provider".to_string()));
        }
        if repo.is_empty() {
            return Err(WorkItemIdError("empty repository".to_string()));
        }
        if item.is_empty() {
            return Err(WorkItemIdError("empty item".to_string()));
        }
        if !repo.contains('/') {
            return Err(WorkItemIdError("repository has no project".to_string()));
        }
        Ok(Self {
            provider: provider.to_string(),
            repo: repo.to_string(),
            item: item.to_string(),
        })
    }
}

/// A [`WorkItemId`] text that did not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemIdError(String);

impl fmt::Display for WorkItemIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WorkItemIdError {}

/// A dependency of a work item, and whether the tracker closed it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Dependency {
    pub id: WorkItemId,
    pub closed: bool,
}

/// The tracker's own status for a work item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ready,
    Other(String),
}

/// `id.repo` is where the tracker keeps the item; `repository` is the code repository the work changes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkItem {
    pub id: WorkItemId,
    pub title: String,
    pub body: String,
    pub repository: String,
    pub status: Status,
    pub blocked_by: Vec<Dependency>,
}

/// Why a work item is blocked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedCause {
    Dependency,
    Human,
    Clarification,
    Ambiguous,
    Capacity,
}

/// The state the tracker records for a work item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkState {
    InProgress,
    Verifying,
    InReview,
    Blocked(BlockedCause),
    Failed,
}

/// A tracker operation failed: the tracker refused it, or it could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerError {
    Rejected(String),
    Failed(String),
}

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(text) | Self::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for TrackerError {}

/// Whether one tracker operation is available.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Supported,
    Unsupported(String),
    Unknown(String),
}

/// Whether one work state can be written, and how.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StateWrite {
    pub state: WorkState,
    pub capability: Capability,
}

/// What a tracker can do, stated rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TrackerCapabilities {
    pub title: Capability,
    pub body: Capability,
    pub repository: Capability,
    pub dependencies: Capability,
    pub ready_mark: Capability,
    pub state_writes: Vec<StateWrite>,
    pub recap: Capability,
}

/// The seam between the factory and a work tracker.
pub trait Tracker {
    /// Reads the items the tracker marks ready.
    fn read_ready(&self) -> ReadOutcome<Vec<WorkItem>>;

    /// Writes the state of one work item.
    ///
    /// # Errors
    /// Returns an error when the tracker refuses the write or it cannot run.
    fn write_state(&self, id: &WorkItemId, state: &WorkState) -> Result<(), TrackerError>;

    /// Writes a recap onto one work item.
    ///
    /// # Errors
    /// Returns an error when the tracker refuses the write or it cannot run.
    fn write_recap(&self, id: &WorkItemId, recap: &str) -> Result<(), TrackerError>;

    /// Reports what the tracker can do.
    fn capabilities(&self) -> TrackerCapabilities;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::fake::FakeTracker;

    #[test]
    fn tracker_is_object_safe() {
        let fake = FakeTracker::new();
        let dynamic: &dyn Tracker = &fake;
        let boxed: Box<dyn Tracker> = Box::new(FakeTracker::new());
        assert!(matches!(dynamic.read_ready(), ReadOutcome::Empty));
        assert!(matches!(boxed.read_ready(), ReadOutcome::Empty));
    }

    #[test]
    fn work_item_id_displays_as_provider_repo_item() {
        let id = WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: "42".to_string(),
        };
        assert_eq!(id.to_string(), "test:open-software-factory/demo#42");
    }

    #[test]
    fn work_item_id_round_trips_and_rejects_a_bad_shape() {
        let id = WorkItemId {
            provider: "test".to_string(),
            repo: "open-software-factory/demo".to_string(),
            item: "42".to_string(),
        };
        assert_eq!(
            "test:open-software-factory/demo#42"
                .parse::<WorkItemId>()
                .expect("parses"),
            id
        );
        for text in [
            "test:open-software-factory/demo",
            "testopen-software-factory/demo#42",
            ":open-software-factory/demo#42",
            "test:#42",
            "test:open-software-factory/demo#",
            "test:demo#42",
        ] {
            assert!(
                text.parse::<WorkItemId>().is_err(),
                "{text} must be rejected"
            );
        }
    }

    #[test]
    fn work_state_and_blocked_cause_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&WorkState::InProgress).expect("serializes"),
            r#""in-progress""#
        );
        assert_eq!(
            serde_json::to_string(&WorkState::Verifying).expect("serializes"),
            r#""verifying""#
        );
        assert_eq!(
            serde_json::to_string(&WorkState::InReview).expect("serializes"),
            r#""in-review""#
        );
        assert_eq!(
            serde_json::to_string(&WorkState::Failed).expect("serializes"),
            r#""failed""#
        );
        assert_eq!(
            serde_json::to_string(&WorkState::Blocked(BlockedCause::Clarification))
                .expect("serializes"),
            r#"{"blocked":"clarification"}"#
        );
        assert_eq!(
            serde_json::to_string(&BlockedCause::Dependency).expect("serializes"),
            r#""dependency""#
        );
        assert_eq!(
            serde_json::to_string(&BlockedCause::Human).expect("serializes"),
            r#""human""#
        );
        assert_eq!(
            serde_json::to_string(&BlockedCause::Clarification).expect("serializes"),
            r#""clarification""#
        );
        assert_eq!(
            serde_json::to_string(&BlockedCause::Ambiguous).expect("serializes"),
            r#""ambiguous""#
        );
        assert_eq!(
            serde_json::to_string(&BlockedCause::Capacity).expect("serializes"),
            r#""capacity""#
        );
    }

    #[test]
    fn read_outcome_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&ReadOutcome::<Vec<WorkItem>>::Unknown("boom".to_string()))
                .expect("serializes"),
            r#"{"unknown":"boom"}"#
        );
        assert_eq!(
            serde_json::to_string(&ReadOutcome::<Vec<WorkItem>>::Empty).expect("serializes"),
            r#""empty""#
        );
    }

    #[test]
    fn tracker_error_display_prints_the_inner_text_only() {
        assert_eq!(
            TrackerError::Rejected("refused".to_string()).to_string(),
            "refused"
        );
        assert_eq!(
            TrackerError::Failed("failed".to_string()).to_string(),
            "failed"
        );
    }

    #[test]
    fn capability_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&Capability::Supported).expect("serializes"),
            r#""supported""#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unsupported("no".to_string())).expect("serializes"),
            r#"{"unsupported":"no"}"#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unknown("maybe".to_string())).expect("serializes"),
            r#"{"unknown":"maybe"}"#
        );
    }
}
