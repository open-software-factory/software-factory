//! The work-tracker command group's logic: build the GitHub adapter from
//! `[work_tracker]`, read the Ready item, and write a state and recap.

use crate::config;
use crate::github_work_tracker::{GitHubWorkTracker, GitHubWorkTrackerConfig, RealGhRunner};
use crate::work_tracker::ready::{self, Pick};
use crate::work_tracker::report::finish_run;
use crate::work_tracker::{BlockedCause, WorkItemId, WorkState};
use std::path::Path;
use std::str::FromStr;

/// The adapter config from `<root>/osf.toml`.
fn adapter_config(root: &Path) -> Result<GitHubWorkTrackerConfig, String> {
    let Some(config) = config::work_tracker_config(root).map_err(|error| error.to_string())? else {
        return Err(
            "no [work_tracker] table in osf.toml; set owner and project_number".to_string(),
        );
    };
    let mut adapter = GitHubWorkTrackerConfig::standard(config.owner, config.project_number);
    adapter.status_field = config.status_field;
    adapter.skip_status = if config.skip_status.is_empty() {
        None
    } else {
        Some(config.skip_status)
    };
    Ok(adapter)
}

/// The real GitHub tracker for `root`.
///
/// # Errors
/// Returns an error when `[work_tracker]` is missing or invalid.
pub fn tracker(root: &Path) -> Result<GitHubWorkTracker<RealGhRunner>, String> {
    Ok(GitHubWorkTracker::real(adapter_config(root)?))
}

/// Reads the one Ready item.
///
/// # Errors
/// Returns an error when the tracker cannot be built.
pub fn ready(root: &Path) -> Result<Pick, String> {
    let tracker = tracker(root)?;
    Ok(ready::pick(&tracker))
}

/// Writes the state and then the recap.
///
/// # Errors
/// Returns an error when the tracker cannot be built, or a write fails.
pub fn report(root: &Path, id: &WorkItemId, state: &WorkState, recap: &str) -> Result<(), String> {
    let tracker = tracker(root)?;
    finish_run(&tracker, id, state, recap).map_err(|error| error.to_string())
}

/// Parses `--state`, and the `--cause` a blocked state needs.
///
/// # Errors
/// Names the state or cause it does not know, or a blocked state without a cause.
pub fn parse_state(state: &str, cause: Option<&str>) -> Result<WorkState, String> {
    if state != "blocked" && cause.is_some() {
        return Err("--cause is only for the blocked state".to_string());
    }
    match state {
        "in-progress" => Ok(WorkState::InProgress),
        "in-review" => Ok(WorkState::InReview),
        "failed" => Ok(WorkState::Failed),
        "blocked" => {
            let cause = cause.ok_or_else(|| {
                "the blocked state needs --cause: dependency, human, clarification, ambiguous or capacity"
                    .to_string()
            })?;
            let cause = match cause {
                "dependency" => BlockedCause::Dependency,
                "human" => BlockedCause::Human,
                "clarification" => BlockedCause::Clarification,
                "ambiguous" => BlockedCause::Ambiguous,
                "capacity" => BlockedCause::Capacity,
                other => {
                    return Err(format!(
                        "unknown cause {other}: use dependency, human, clarification, ambiguous or capacity"
                    ));
                }
            };
            Ok(WorkState::Blocked(cause))
        }
        other => Err(format!(
            "unknown state {other}: use in-progress, in-review, failed or blocked"
        )),
    }
}

/// Parses a work item id such as `github:open-software-factory/software-factory#42`.
///
/// # Errors
/// Returns the parser's reason when the id is not a provider-qualified item.
pub fn work_item(text: &str) -> Result<WorkItemId, String> {
    WorkItemId::from_str(text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_and_causes_parse() {
        assert_eq!(
            parse_state("in-progress", None).expect("parses"),
            WorkState::InProgress
        );
        assert_eq!(
            parse_state("in-review", None).expect("parses"),
            WorkState::InReview
        );
        assert_eq!(
            parse_state("failed", None).expect("parses"),
            WorkState::Failed
        );
        for (cause, expected) in [
            ("dependency", BlockedCause::Dependency),
            ("human", BlockedCause::Human),
            ("clarification", BlockedCause::Clarification),
            ("ambiguous", BlockedCause::Ambiguous),
            ("capacity", BlockedCause::Capacity),
        ] {
            assert_eq!(
                parse_state("blocked", Some(cause)).expect("parses"),
                WorkState::Blocked(expected)
            );
        }
        assert!(parse_state("blocked", None).is_err());
        assert!(parse_state("blocked", Some("other")).is_err());
        assert!(parse_state("other", None).is_err());
        assert!(parse_state("in-review", Some("human")).is_err());
    }

    #[test]
    fn a_work_item_parses_only_with_a_provider_and_a_number() {
        let id = work_item("github:open-software-factory/software-factory#42").expect("parses");
        assert_eq!(id.provider, "github");
        assert_eq!(id.item, "42");
        assert!(work_item("open-software-factory/software-factory#42").is_err());
    }

    #[test]
    fn a_missing_table_is_named() {
        let root = crate::test_support::TempDir::new("osf-work-tracker-config");
        let error = ready(&root).expect_err("no table");
        assert!(error.contains("[work_tracker]"), "{error}");
    }
}
