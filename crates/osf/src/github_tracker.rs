//! The first [`Tracker`] adapter: GitHub Issues and Projects v2, driven
//! through the installed `gh` command-line tool.
//!
//! Every request is one `gh` invocation. The argv and the JSON payload are
//! built by pure functions, and every response is read by a pure parser, so
//! the adapter is a thin loop over [`GhRunner`].

use crate::tracker::{
    BlockedCause, Capability, Dependency, ReadOutcome, StateWrite, Status, Tracker,
    TrackerCapabilities, TrackerError, WorkItem, WorkItemId, WorkState,
};
use serde_json::{json, Value};
use std::io::{Read, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Runs one `gh` invocation: standard output on success, or the trimmed
/// standard error text on a non-zero exit, or `cannot run gh: <error>`.
pub trait GhRunner {
    /// Runs `argv`, feeding `stdin` when it is given.
    ///
    /// A real runner may time the program out; a timeout is an error.
    ///
    /// # Errors
    /// Returns the trimmed standard error text on a non-zero exit, or
    /// `cannot run gh: <error>` when the program cannot be run or it times out.
    fn run(&self, argv: &[String], stdin: Option<&str>) -> Result<String, String>;
}

/// The [`GhRunner`] over the real `gh` program.
///
/// It stops waiting for `gh` after a timeout; the default is 60 seconds.
pub struct RealGhRunner {
    program: PathBuf,
    timeout: Duration,
}

impl RealGhRunner {
    /// The default timeout for one `gh` invocation.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

    /// A runner that spawns `program`.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            timeout: Self::DEFAULT_TIMEOUT,
        }
    }

    /// A runner that waits at most `timeout` for each invocation.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The timeout for one invocation.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Reads one child pipe to its end on its own thread.
    fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            bytes
        })
    }
}

impl Default for RealGhRunner {
    fn default() -> Self {
        Self::new("gh")
    }
}

impl GhRunner for RealGhRunner {
    fn run(&self, argv: &[String], stdin: Option<&str>) -> Result<String, String> {
        let mut child = Command::new(&self.program)
            .args(argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot run gh: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "cannot run gh: no standard output".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "cannot run gh: no standard error".to_string())?;
        let stdout = Self::drain(stdout);
        let stderr = Self::drain(stderr);
        let write = {
            let mut handle = child
                .stdin
                .take()
                .ok_or_else(|| "cannot run gh: no standard input".to_string())?;
            let payload = stdin.map(str::to_string);
            std::thread::spawn(move || match payload {
                Some(text) => handle.write_all(text.as_bytes()),
                None => Ok(()),
            })
        };
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child
                .try_wait()
                .map_err(|error| format!("cannot run gh: {error}"))?
            {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("cannot run gh: timed out after {:?}", self.timeout));
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        let stdout_bytes = stdout.join().unwrap_or_default();
        let stderr_bytes = stderr.join().unwrap_or_default();
        if !status.success() {
            return Err(String::from_utf8_lossy(&stderr_bytes).trim().to_string());
        }
        write
            .join()
            .unwrap_or(Ok(()))
            .map_err(|error| format!("cannot run gh: {error}"))?;
        Ok(String::from_utf8_lossy(&stdout_bytes).into_owned())
    }
}

/// The project status option names the adapter writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateOptions {
    pub ready: String,
    pub in_progress: String,
    pub verifying: Option<String>,
    pub in_review: String,
    pub blocked: String,
    pub failed: String,
}

impl StateOptions {
    /// The default project options.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            ready: "Ready".to_string(),
            in_progress: "In progress".to_string(),
            verifying: Some("Verifying".to_string()),
            in_review: "In review".to_string(),
            blocked: "Blocked".to_string(),
            failed: "Failed".to_string(),
        }
    }

    /// The option name for `state`, or none when it is not configured.
    ///
    /// Every blocked cause shares the one blocked option.
    #[must_use]
    pub fn option_for(&self, state: &WorkState) -> Option<&str> {
        match state {
            WorkState::InProgress => Some(self.in_progress.as_str()),
            WorkState::Verifying => self.verifying.as_deref(),
            WorkState::InReview => Some(self.in_review.as_str()),
            WorkState::Blocked(_) => Some(self.blocked.as_str()),
            WorkState::Failed => Some(self.failed.as_str()),
        }
    }
}

/// What the adapter needs to find the project and its status field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubTrackerConfig {
    pub owner: String,
    pub project_number: u64,
    pub status_field: String,
    /// The status whose items the read leaves out on the server.
    pub skip_status: Option<String>,
    pub options: StateOptions,
}

impl GitHubTrackerConfig {
    /// A config with the `Status` field and the standard options.
    #[must_use]
    pub fn standard(owner: impl Into<String>, project_number: u64) -> Self {
        Self {
            owner: owner.into(),
            project_number,
            status_field: "Status".to_string(),
            skip_status: Some("Done".to_string()),
            options: StateOptions::standard(),
        }
    }
}

/// A [`Tracker`] over GitHub Issues and Projects v2, driven by `R`.
pub struct GitHubTracker<R: GhRunner> {
    runner: R,
    config: GitHubTrackerConfig,
}

impl<R: GhRunner> GitHubTracker<R> {
    /// A tracker that runs every request through `runner`.
    #[must_use]
    pub fn new(runner: R, config: GitHubTrackerConfig) -> Self {
        Self { runner, config }
    }

    /// Runs one GraphQL request and returns its standard output.
    fn run_graphql(&self, argv: &[String], payload: &Value) -> Result<String, TrackerError> {
        self.runner
            .run(argv, Some(&payload.to_string()))
            .map_err(TrackerError::Failed)
    }

    /// Posts one comment on `item` in `repo`.
    fn post_comment(&self, repo: &str, item: &str, text: &str) -> Result<(), TrackerError> {
        let argv = comment_argv(repo, item);
        let payload = comment_payload(text);
        self.runner
            .run(&argv, Some(&payload.to_string()))
            .map(|_| ())
            .map_err(TrackerError::Failed)
    }

    /// Reads the project's id, status field and options.
    fn read_project_status(&self) -> Result<ProjectStatus, TrackerError> {
        let argv = project_status_argv();
        let payload = project_status_payload(
            &self.config.owner,
            self.config.project_number,
            &self.config.status_field,
        );
        let text = self.run_graphql(&argv, &payload)?;
        parse_project_status(&text)
    }

    /// Reads the project item id of one issue in one project.
    fn read_item_id(
        &self,
        owner: &str,
        name: &str,
        number: u64,
        project_id: &str,
    ) -> Result<String, TrackerError> {
        let argv = item_id_argv();
        let payload = item_id_payload(owner, name, number);
        let text = self.run_graphql(&argv, &payload)?;
        parse_item_id(&text, project_id)
    }

    /// The project search string that leaves out the skipped status.
    fn skip_query(&self) -> Option<String> {
        self.config
            .skip_status
            .as_deref()
            .map(|status| skip_status_query(status, &self.config.status_field))
    }
}

impl GitHubTracker<RealGhRunner> {
    /// The adapter over the real command-line tool.
    #[must_use]
    pub fn real(config: GitHubTrackerConfig) -> Self {
        Self::new(RealGhRunner::default(), config)
    }
}

impl<R: GhRunner> Tracker for GitHubTracker<R> {
    fn read_ready(&self) -> ReadOutcome<Vec<WorkItem>> {
        let ready_option = self.config.options.ready.clone();
        let query = self.skip_query();
        let mut cursor: Option<String> = None;
        let mut seen: Vec<String> = Vec::new();
        let mut items: Vec<WorkItem> = Vec::new();
        for _ in 0..50 {
            let argv = read_ready_argv();
            let payload = read_ready_payload(
                &self.config.owner,
                self.config.project_number,
                &self.config.status_field,
                cursor.as_deref(),
                query.as_deref(),
            );
            let text = match self.runner.run(&argv, Some(&payload.to_string())) {
                Ok(text) => text,
                Err(error) => return ReadOutcome::Unknown(error),
            };
            let page = match parse_items_page(&text, &ready_option) {
                Ok(page) => page,
                Err(error) => return ReadOutcome::Unknown(error.to_string()),
            };
            items.extend(page.items);
            match page.next_cursor {
                None => {
                    if items.is_empty() {
                        return ReadOutcome::Empty;
                    }
                    return ReadOutcome::Found(items);
                }
                Some(next) => {
                    if seen.contains(&next) {
                        return ReadOutcome::Unknown(
                            "the tracker repeated a page cursor".to_string(),
                        );
                    }
                    seen.push(next.clone());
                    cursor = Some(next);
                }
            }
        }
        ReadOutcome::Unknown("the tracker returned too many pages".to_string())
    }

    fn write_state(&self, id: &WorkItemId, state: &WorkState) -> Result<(), TrackerError> {
        reject_non_github(id)?;
        let option_name = match self.config.options.option_for(state) {
            Some(name) => name.to_string(),
            None => {
                return Err(TrackerError::Rejected(format!(
                    "this project has no option for {}",
                    state_name(state)
                )))
            }
        };
        let project = self.read_project_status()?;
        let Some((_, option_id)) = project
            .options
            .iter()
            .find(|(name, _)| name == &option_name)
        else {
            return Err(TrackerError::Rejected(format!(
                "the project has no option named {option_name}"
            )));
        };
        let (owner, name) = split_repo(&id.repo)?;
        let number: u64 = id.item.parse().map_err(|_| {
            TrackerError::Rejected(format!("the item is not a number: {}", id.item))
        })?;
        let item_id = self.read_item_id(owner, name, number, &project.project_id)?;
        if let WorkState::Blocked(cause) = state {
            self.post_comment(&id.repo, &id.item, &format!("Blocked: {}.", cause.as_str()))?;
        }
        let argv = mutation_argv();
        let payload = mutation_payload(&project.project_id, &item_id, &project.field_id, option_id);
        let text = self.run_graphql(&argv, &payload)?;
        parse_write_result(&text)?;
        Ok(())
    }

    fn write_recap(&self, id: &WorkItemId, recap: &str) -> Result<(), TrackerError> {
        reject_non_github(id)?;
        self.post_comment(&id.repo, &id.item, recap)
    }

    fn capabilities(&self) -> TrackerCapabilities {
        let argv = project_status_argv();
        let payload = project_status_payload(
            &self.config.owner,
            self.config.project_number,
            &self.config.status_field,
        );
        let read = self
            .runner
            .run(&argv, Some(&payload.to_string()))
            .map_err(TrackerError::Failed)
            .and_then(|text| parse_project_status(&text));
        match read {
            Ok(project) => TrackerCapabilities {
                title: Capability::Supported,
                body: Capability::Supported,
                repository: Capability::Supported,
                dependencies: Capability::Supported,
                ready_mark: option_capability(&self.config.options.ready, &project),
                state_writes: state_writes(&self.config.options, &project),
                recap: Capability::Supported,
            },
            Err(error) => {
                let reason = error.to_string();
                TrackerCapabilities {
                    title: Capability::Unknown(reason.clone()),
                    body: Capability::Unknown(reason.clone()),
                    repository: Capability::Unknown(reason.clone()),
                    dependencies: Capability::Unknown(reason.clone()),
                    ready_mark: Capability::Unknown(reason.clone()),
                    state_writes: state_write_states()
                        .into_iter()
                        .map(|state| StateWrite {
                            state,
                            capability: Capability::Unknown(reason.clone()),
                        })
                        .collect(),
                    recap: Capability::Unknown(reason),
                }
            }
        }
    }
}

/// The [`GhRunner`] argv shared by every GraphQL request.
fn graphql_argv() -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "--input".to_string(),
        "-".to_string(),
    ]
}

/// The argv that reads one page of ready items.
#[must_use]
pub fn read_ready_argv() -> Vec<String> {
    graphql_argv()
}

/// The argv that reads the project's id, status field and options.
#[must_use]
pub fn project_status_argv() -> Vec<String> {
    graphql_argv()
}

/// The argv that reads one issue's project item id.
#[must_use]
pub fn item_id_argv() -> Vec<String> {
    graphql_argv()
}

/// The argv that writes one item's single-select field value.
#[must_use]
pub fn mutation_argv() -> Vec<String> {
    graphql_argv()
}

/// The argv that posts one comment on an issue.
#[must_use]
pub fn comment_argv(repo: &str, item: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!("repos/{repo}/issues/{item}/comments"),
        "--input".to_string(),
        "-".to_string(),
    ]
}

/// The GraphQL query that reads one page of ready items.
const READ_QUERY: &str = "\
query($owner: String!, $number: Int!, $field: String!, $cursor: String, $query: String) {
  repositoryOwner(login: $owner) {
    ... on User { projectV2(number: $number) { ...P } }
    ... on Organization { projectV2(number: $number) { ...P } }
  }
}
fragment P on ProjectV2 {
  items(first: 100, after: $cursor, query: $query) {
    pageInfo { hasNextPage endCursor }
    nodes {
      fieldValueByName(name: $field) {
        ... on ProjectV2ItemFieldSingleSelectValue { name }
      }
      content {
        ... on Issue {
          number title body state repository { nameWithOwner }
          blockedBy(first: 100) {
            pageInfo { hasNextPage }
            nodes { number state repository { nameWithOwner } }
          }
        }
      }
    }
  }
}";

/// The GraphQL query that reads the project's id, status field and options.
const PROJECT_QUERY: &str = "\
query($owner: String!, $number: Int!, $field: String!) {
  repositoryOwner(login: $owner) {
    ... on User { projectV2(number: $number) { id field(name: $field) { ... on ProjectV2SingleSelectField { id options { id name } } } } }
    ... on Organization { projectV2(number: $number) { id field(name: $field) { ... on ProjectV2SingleSelectField { id options { id name } } } } }
  }
}";

/// The GraphQL query that reads one issue's project item id.
const ITEM_QUERY: &str = "\
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    issue(number: $number) {
      projectItems(first: 50) {
        pageInfo { hasNextPage }
        nodes { id project { id } }
      }
    }
  }
}";

/// The GraphQL mutation that writes one item's single-select field value.
const MUTATION: &str = "\
mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) {
  updateProjectV2ItemFieldValue(input: {projectId: $project, itemId: $item, fieldId: $field, value: {singleSelectOptionId: $option}}) {
    projectV2Item { id }
  }
}";

/// The project search string that leaves `status` out of `field`.
#[must_use]
pub fn skip_status_query(status: &str, field: &str) -> String {
    let escaped = status.replace('\\', "\\\\").replace('"', "\\\"");
    format!("-{}:\"{escaped}\"", field.to_lowercase())
}

/// Builds the JSON body of one ready-items page request.
#[must_use]
pub fn read_ready_payload(
    owner: &str,
    project_number: u64,
    field: &str,
    cursor: Option<&str>,
    query: Option<&str>,
) -> Value {
    json!({
        "query": READ_QUERY,
        "variables": {
            "owner": owner,
            "number": project_number,
            "field": field,
            "cursor": cursor,
            "query": query,
        },
    })
}

/// Builds the JSON body of the project status request.
#[must_use]
pub fn project_status_payload(owner: &str, project_number: u64, field: &str) -> Value {
    json!({
        "query": PROJECT_QUERY,
        "variables": {
            "owner": owner,
            "number": project_number,
            "field": field,
        },
    })
}

/// Builds the JSON body of one issue's project-item request.
#[must_use]
pub fn item_id_payload(owner: &str, name: &str, number: u64) -> Value {
    json!({
        "query": ITEM_QUERY,
        "variables": {
            "owner": owner,
            "name": name,
            "number": number,
        },
    })
}

/// Builds the JSON body of the status-write mutation.
#[must_use]
pub fn mutation_payload(project_id: &str, item_id: &str, field_id: &str, option_id: &str) -> Value {
    json!({
        "query": MUTATION,
        "variables": {
            "project": project_id,
            "item": item_id,
            "field": field_id,
            "option": option_id,
        },
    })
}

/// Builds the JSON body of one comment write.
#[must_use]
pub fn comment_payload(text: &str) -> Value {
    json!({ "body": text })
}

/// One page of ready items, and the cursor that reads the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPage {
    pub items: Vec<WorkItem>,
    pub next_cursor: Option<String>,
}

/// The project's id, status field id, and status options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStatus {
    pub project_id: String,
    pub field_id: String,
    pub options: Vec<(String, String)>,
}

/// A top-level `errors` key in a GraphQL response, if it holds one.
fn graphql_error(value: &Value) -> Option<TrackerError> {
    value
        .get("errors")
        .map(|errors| TrackerError::Failed(format!("the tracker returned errors: {errors}")))
}

/// The project object in a GraphQL response, or `project not found`.
fn project_of(value: &Value) -> Result<&Value, TrackerError> {
    value
        .get("data")
        .and_then(|data| data.get("repositoryOwner"))
        .and_then(|owner| owner.get("projectV2"))
        .filter(|project| !project.is_null())
        .ok_or_else(|| TrackerError::Failed("project not found".to_string()))
}

/// Whether `content` holds at least one Issue field.
fn is_issue_content(content: &Value) -> bool {
    content.as_object().is_some_and(|object| {
        [
            "number",
            "title",
            "body",
            "state",
            "repository",
            "blockedBy",
        ]
        .iter()
        .any(|key| object.contains_key(*key))
    })
}

/// Reads one issue's blockers.
fn parse_blockers(content: &Value) -> Result<Vec<Dependency>, TrackerError> {
    let Some(blocked) = content.get("blockedBy").filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let has_next = blocked
        .get("pageInfo")
        .and_then(|page| page.get("hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if has_next {
        return Err(TrackerError::Failed(
            "the issue has more blockers than were read".to_string(),
        ));
    }
    let Some(nodes) = blocked.get("nodes").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut dependencies = Vec::new();
    for node in nodes {
        if node.is_null() {
            return Err(TrackerError::Failed("a blocker is null".to_string()));
        }
        let number = node
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| TrackerError::Failed("a blocker has no number".to_string()))?;
        let repo = node
            .get("repository")
            .and_then(|repository| repository.get("nameWithOwner"))
            .and_then(Value::as_str)
            .ok_or_else(|| TrackerError::Failed("a blocker has no repository".to_string()))?;
        let state = node
            .get("state")
            .and_then(Value::as_str)
            .ok_or_else(|| TrackerError::Failed("a blocker has no state".to_string()))?;
        let closed = match state {
            "CLOSED" => true,
            "OPEN" => false,
            other => {
                return Err(TrackerError::Failed(format!(
                    "the blocker state '{other}' is neither OPEN nor CLOSED"
                )))
            }
        };
        dependencies.push(Dependency {
            id: WorkItemId {
                provider: "github".to_string(),
                repo: repo.to_string(),
                item: number.to_string(),
            },
            closed,
        });
    }
    Ok(dependencies)
}

/// Reads the strict fields of one issue, given the number and repository its
/// caller already read.
fn parse_issue_item(
    content: &Value,
    number: Option<u64>,
    repository: Option<&str>,
    status: &Status,
) -> Result<WorkItem, TrackerError> {
    let number =
        number.ok_or_else(|| TrackerError::Failed("the issue has no number".to_string()))?;
    let title = content
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| TrackerError::Failed("the issue has no title".to_string()))?;
    let repository = repository
        .ok_or_else(|| TrackerError::Failed("the issue has no repository".to_string()))?;
    let state = content
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| TrackerError::Failed("the issue has no state".to_string()))?;
    let closed = match state {
        "OPEN" => false,
        "CLOSED" => true,
        other => {
            return Err(TrackerError::Failed(format!(
                "the issue state '{other}' is neither OPEN nor CLOSED"
            )))
        }
    };
    let body = content
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(WorkItem {
        id: WorkItemId {
            provider: "github".to_string(),
            repo: repository.to_string(),
            item: number.to_string(),
        },
        title: title.to_string(),
        body,
        repository: repository.to_string(),
        status: status.clone(),
        closed,
        blocked_by: parse_blockers(content)?,
    })
}

/// Builds the placeholder for a malformed item that is not in the ready status.
fn unreadable_item(
    reason: &TrackerError,
    status: &Status,
    number: Option<u64>,
    repository: Option<&str>,
) -> WorkItem {
    let name = match status {
        Status::Ready => "Ready",
        Status::Other(name) => name.as_str(),
    };
    let repository = repository.unwrap_or("unknown/unknown");
    let item = match number {
        Some(number) => number.to_string(),
        None => "unknown".to_string(),
    };
    WorkItem {
        id: WorkItemId {
            provider: "github".to_string(),
            repo: repository.to_string(),
            item,
        },
        title: String::new(),
        body: String::new(),
        repository: repository.to_string(),
        status: Status::Other(format!("unreadable: {reason}, status {name}")),
        closed: false,
        blocked_by: Vec::new(),
    }
}

/// Reads one issue item out of one project node, if it is one.
///
/// An item that is not in the ready status and misses a field is kept as a
/// skipped item whose status text begins with `unreadable:`.
///
/// # Errors
/// Returns an error only when an item in the ready status is malformed.
fn parse_item_node(node: &Value, ready_option: &str) -> Result<Option<WorkItem>, TrackerError> {
    let Some(content) = node.get("content").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    if !is_issue_content(content) {
        return Ok(None);
    }
    let (status, ready) = match node
        .get("fieldValueByName")
        .and_then(|field| field.get("name"))
        .and_then(Value::as_str)
    {
        Some(name) if name == ready_option => (Status::Ready, true),
        Some(name) => (Status::Other(name.to_string()), false),
        None => (Status::Other("no status".to_string()), false),
    };
    let number = content.get("number").and_then(Value::as_u64);
    let repository = content
        .get("repository")
        .and_then(|repository| repository.get("nameWithOwner"))
        .and_then(Value::as_str);
    match parse_issue_item(content, number, repository, &status) {
        Ok(item) => Ok(Some(item)),
        Err(error) if ready => Err(error),
        Err(error) => Ok(Some(unreadable_item(&error, &status, number, repository))),
    }
}

/// Reads one page of project items with their status.
///
/// A malformed item that is not in the ready status is kept as a skipped item,
/// so one bad non-ready item does not fail the page.
///
/// # Errors
/// Returns an error when the text is not JSON, when the response carries an
/// `errors` key, when the project or its item list is missing, when a page
/// says it has a next one with no cursor, or when a ready item is malformed.
pub fn parse_items_page(text: &str, ready_option: &str) -> Result<ItemPage, TrackerError> {
    let value: Value = serde_json::from_str(text).map_err(|error| {
        TrackerError::Failed(format!("cannot read the items response: {error}"))
    })?;
    if let Some(error) = graphql_error(&value) {
        return Err(error);
    }
    let project = project_of(&value)?;
    let Some(items) = project.get("items").filter(|value| !value.is_null()) else {
        return Err(TrackerError::Failed(
            "the response has no items list".to_string(),
        ));
    };
    let Some(nodes) = items.get("nodes").and_then(Value::as_array) else {
        return Err(TrackerError::Failed(
            "the response has no items list".to_string(),
        ));
    };
    let has_next = items
        .get("pageInfo")
        .and_then(|page| page.get("hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_cursor = if has_next {
        let cursor = items
            .get("pageInfo")
            .and_then(|page| page.get("endCursor"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                TrackerError::Failed(
                    "the response says there is a next page but has no cursor".to_string(),
                )
            })?;
        Some(cursor.to_string())
    } else {
        None
    };
    let mut parsed = Vec::new();
    for node in nodes {
        if let Some(item) = parse_item_node(node, ready_option)? {
            parsed.push(item);
        }
    }
    Ok(ItemPage {
        items: parsed,
        next_cursor,
    })
}

/// Reads the project's id, status field and options.
///
/// # Errors
/// Returns an error when the text is not JSON, when the response carries an
/// `errors` key, when the project or its status field is missing, or when an
/// id is missing.
pub fn parse_project_status(text: &str) -> Result<ProjectStatus, TrackerError> {
    let value: Value = serde_json::from_str(text).map_err(|error| {
        TrackerError::Failed(format!("cannot read the project response: {error}"))
    })?;
    if let Some(error) = graphql_error(&value) {
        return Err(error);
    }
    let project = project_of(&value)?;
    let project_id = project
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| TrackerError::Failed("the project has no id".to_string()))?
        .to_string();
    let Some(field) = project.get("field").filter(|value| !value.is_null()) else {
        return Err(TrackerError::Failed(
            "the project has no status field".to_string(),
        ));
    };
    let field_id = field
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| TrackerError::Failed("the status field has no id".to_string()))?
        .to_string();
    let options = match field.get("options").and_then(Value::as_array) {
        Some(options) => options
            .iter()
            .filter_map(|option| {
                let name = option.get("name").and_then(Value::as_str)?;
                let id = option.get("id").and_then(Value::as_str)?;
                Some((name.to_string(), id.to_string()))
            })
            .collect(),
        None => Vec::new(),
    };
    Ok(ProjectStatus {
        project_id,
        field_id,
        options,
    })
}

/// Reads the project item id of one issue inside one project.
///
/// # Errors
/// Returns an error when the text is not JSON, when the response carries an
/// `errors` key, when the issue is missing, when it is not in the project,
/// or when there are more project items to read and it was not found.
pub fn parse_item_id(text: &str, project_id: &str) -> Result<String, TrackerError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| TrackerError::Failed(format!("cannot read the item response: {error}")))?;
    if let Some(error) = graphql_error(&value) {
        return Err(error);
    }
    let Some(issue) = value
        .get("data")
        .and_then(|data| data.get("repository"))
        .and_then(|repository| repository.get("issue"))
        .filter(|value| !value.is_null())
    else {
        return Err(TrackerError::Failed("the issue was not found".to_string()));
    };
    let Some(items) = issue.get("projectItems").filter(|value| !value.is_null()) else {
        return Err(TrackerError::Failed(
            "the issue has no project items".to_string(),
        ));
    };
    let has_next = items
        .get("pageInfo")
        .and_then(|page| page.get("hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(nodes) = items.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            let in_project = node
                .get("project")
                .and_then(|project| project.get("id"))
                .and_then(Value::as_str)
                == Some(project_id);
            if in_project {
                let id = node.get("id").and_then(Value::as_str).ok_or_else(|| {
                    TrackerError::Failed("the project item has no id".to_string())
                })?;
                return Ok(id.to_string());
            }
        }
    }
    if has_next {
        Err(TrackerError::Failed(
            "the issue is not in the project and there are more items to read".to_string(),
        ))
    } else {
        Err(TrackerError::Failed(
            "the issue is not in the project".to_string(),
        ))
    }
}

/// Reads one status-write mutation response.
///
/// # Errors
/// Returns an error when the text is not JSON, when the response carries an
/// `errors` key, or when the written project item has no id.
pub fn parse_write_result(text: &str) -> Result<(), TrackerError> {
    let value: Value = serde_json::from_str(text).map_err(|error| {
        TrackerError::Failed(format!("cannot read the write response: {error}"))
    })?;
    if let Some(error) = graphql_error(&value) {
        return Err(error);
    }
    let id = value
        .get("data")
        .and_then(|data| data.get("updateProjectV2ItemFieldValue"))
        .and_then(|mutation| mutation.get("projectV2Item"))
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str);
    match id {
        Some(_) => Ok(()),
        None => Err(TrackerError::Failed(
            "the write response has no project item".to_string(),
        )),
    }
}

/// Whether a project's status field holds an option named `name`.
fn option_capability(name: &str, project: &ProjectStatus) -> Capability {
    if project.options.iter().any(|(option, _)| option == name) {
        Capability::Supported
    } else {
        Capability::Unsupported(format!("the project has no option named {name}"))
    }
}

/// The nine state writes, in the order the capability report lists them.
fn state_write_states() -> Vec<WorkState> {
    vec![
        WorkState::InProgress,
        WorkState::Verifying,
        WorkState::InReview,
        WorkState::Blocked(BlockedCause::Dependency),
        WorkState::Blocked(BlockedCause::Human),
        WorkState::Blocked(BlockedCause::Clarification),
        WorkState::Blocked(BlockedCause::Ambiguous),
        WorkState::Blocked(BlockedCause::Capacity),
        WorkState::Failed,
    ]
}

/// The capability of every state write against one project.
fn state_writes(options: &StateOptions, project: &ProjectStatus) -> Vec<StateWrite> {
    state_write_states()
        .into_iter()
        .map(|state| {
            let capability = match options.option_for(&state) {
                Some(name) => option_capability(name, project),
                None => Capability::Unsupported(format!(
                    "this project has no option for {}",
                    state_name(&state)
                )),
            };
            StateWrite { state, capability }
        })
        .collect()
}

/// The kebab-case name of a work state.
fn state_name(state: &WorkState) -> &'static str {
    match state {
        WorkState::InProgress => "in-progress",
        WorkState::Verifying => "verifying",
        WorkState::InReview => "in-review",
        WorkState::Blocked(_) => "blocked",
        WorkState::Failed => "failed",
    }
}

/// Rejects an id that is not a GitHub item.
fn reject_non_github(id: &WorkItemId) -> Result<(), TrackerError> {
    if id.provider == "github" {
        Ok(())
    } else {
        Err(TrackerError::Rejected(format!("not a github item: {id}")))
    }
}

/// Splits `owner/name` into its two halves.
fn split_repo(repo: &str) -> Result<(&str, &str), TrackerError> {
    repo.split_once('/')
        .filter(|(owner, name)| !owner.is_empty() && !name.is_empty())
        .ok_or_else(|| TrackerError::Rejected(format!("the repository is not owner/name: {repo}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::ready::{pick, Pick};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    const OWNER: &str = "open-software-factory";
    const REPO: &str = "open-software-factory/demo";

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RunCall {
        argv: Vec<String>,
        stdin: Option<String>,
    }

    struct ScriptedRunner {
        answers: RefCell<VecDeque<Result<String, String>>>,
        calls: RefCell<Vec<RunCall>>,
    }

    impl ScriptedRunner {
        fn new(answers: Vec<Result<String, String>>) -> Self {
            Self {
                answers: RefCell::new(answers.into()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<RunCall> {
            self.calls.borrow().clone()
        }
    }

    impl GhRunner for ScriptedRunner {
        fn run(&self, argv: &[String], stdin: Option<&str>) -> Result<String, String> {
            self.calls.borrow_mut().push(RunCall {
                argv: argv.to_vec(),
                stdin: stdin.map(str::to_string),
            });
            self.answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Err("the test scripted too few answers".to_string()))
        }
    }

    fn config() -> GitHubTrackerConfig {
        GitHubTrackerConfig::standard(OWNER, 5)
    }

    fn tracker(answers: Vec<Result<String, String>>) -> GitHubTracker<ScriptedRunner> {
        GitHubTracker::new(ScriptedRunner::new(answers), config())
    }

    fn github_item(item: &str) -> WorkItemId {
        WorkItemId {
            provider: "github".to_string(),
            repo: REPO.to_string(),
            item: item.to_string(),
        }
    }

    fn dependency(item: &str, closed: bool) -> Dependency {
        Dependency {
            id: github_item(item),
            closed,
        }
    }

    fn payload(call: &RunCall) -> Value {
        serde_json::from_str(call.stdin.as_deref().expect("the call carries stdin"))
            .expect("the payload is JSON")
    }

    fn pointer<'a>(value: &'a Value, path: &str) -> &'a Value {
        value.pointer(path).expect("the JSON path exists")
    }

    fn call(calls: &[RunCall], index: usize) -> &RunCall {
        calls.get(index).expect("the call is recorded")
    }

    fn items_page(nodes: impl Into<Value>, has_next: bool, end_cursor: impl Into<Value>) -> String {
        let nodes = nodes.into();
        let end_cursor = end_cursor.into();
        json!({
            "data": {
                "repositoryOwner": {
                    "projectV2": {
                        "items": {
                            "pageInfo": {"hasNextPage": has_next, "endCursor": end_cursor},
                            "nodes": nodes,
                        }
                    }
                }
            }
        })
        .to_string()
    }

    fn issue(
        number: u64,
        title: &str,
        body: impl Into<Value>,
        blocked_by: impl Into<Value>,
    ) -> Value {
        issue_with_state(number, "OPEN", title, body, blocked_by)
    }

    fn issue_with_state(
        number: u64,
        state: &str,
        title: &str,
        body: impl Into<Value>,
        blocked_by: impl Into<Value>,
    ) -> Value {
        let body = body.into();
        let blocked_by = blocked_by.into();
        json!({
            "number": number,
            "title": title,
            "body": body,
            "state": state,
            "repository": {"nameWithOwner": REPO},
            "blockedBy": blocked_by,
        })
    }

    fn blockers(nodes: impl Into<Value>, has_next: bool) -> Value {
        let nodes = nodes.into();
        json!({"pageInfo": {"hasNextPage": has_next}, "nodes": nodes})
    }

    fn ready_node(content: impl Into<Value>) -> Value {
        let content = content.into();
        json!({"fieldValueByName": {"name": "Ready"}, "content": content})
    }

    fn other_node(status: &str, content: impl Into<Value>) -> Value {
        let content = content.into();
        json!({"fieldValueByName": {"name": status}, "content": content})
    }

    struct Malformed {
        label: &'static str,
        reason: &'static str,
        content: Value,
        item: &'static str,
        repo: &'static str,
    }

    fn malformed_shapes() -> Vec<Malformed> {
        let mut shapes = blocker_shapes();
        shapes.extend(issue_shapes());
        shapes
    }

    fn blocker_shapes() -> Vec<Malformed> {
        vec![
            Malformed {
                label: "a null blocker",
                reason: "a blocker is null",
                content: issue(42, "Title", json!("Body."), blockers(json!([null]), false)),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "a blocker with no state",
                reason: "a blocker has no state",
                content: issue(
                    42,
                    "Title",
                    json!("Body."),
                    blockers(
                        json!([{"number": 7, "repository": {"nameWithOwner": REPO}}]),
                        false,
                    ),
                ),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "a blocker with no number",
                reason: "a blocker has no number",
                content: issue(
                    42,
                    "Title",
                    json!("Body."),
                    blockers(
                        json!([{"state": "OPEN", "repository": {"nameWithOwner": REPO}}]),
                        false,
                    ),
                ),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "more blockers than were read",
                reason: "the issue has more blockers than were read",
                content: issue(42, "Title", json!("Body."), blockers(json!([]), true)),
                item: "42",
                repo: REPO,
            },
        ]
    }

    fn issue_shapes() -> Vec<Malformed> {
        vec![
            Malformed {
                label: "a missing issue state",
                reason: "the issue has no state",
                content: json!({
                    "number": 42,
                    "title": "Title",
                    "repository": {"nameWithOwner": REPO},
                    "blockedBy": blockers(json!([]), false),
                }),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "a weird issue state",
                reason: "the issue state 'WEIRD' is neither OPEN nor CLOSED",
                content: issue_with_state(
                    42,
                    "WEIRD",
                    "Title",
                    json!("Body."),
                    blockers(json!([]), false),
                ),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "a missing issue title",
                reason: "the issue has no title",
                content: json!({
                    "number": 42,
                    "repository": {"nameWithOwner": REPO},
                    "blockedBy": blockers(json!([]), false),
                }),
                item: "42",
                repo: REPO,
            },
            Malformed {
                label: "a missing issue number",
                reason: "the issue has no number",
                content: json!({
                    "title": "Title",
                    "repository": {"nameWithOwner": REPO},
                    "blockedBy": blockers(json!([]), false),
                }),
                item: "unknown",
                repo: REPO,
            },
            Malformed {
                label: "a missing issue repository",
                reason: "the issue has no repository",
                content: json!({
                    "number": 42,
                    "title": "Title",
                    "blockedBy": blockers(json!([]), false),
                }),
                item: "42",
                repo: "unknown/unknown",
            },
        ]
    }

    fn full_page() -> String {
        items_page(
            json!([ready_node(issue(
                42,
                "A change",
                json!("Body."),
                blockers(
                    json!([
                        {"number": 7, "state": "CLOSED", "repository": {"nameWithOwner": REPO}},
                        {"number": 8, "state": "OPEN", "repository": {"nameWithOwner": REPO}},
                    ]),
                    false,
                ),
            ))]),
            false,
            Value::Null,
        )
    }

    fn all_options() -> Value {
        json!([
            {"id": "opt-ready", "name": "Ready"},
            {"id": "opt-in-progress", "name": "In progress"},
            {"id": "opt-verifying", "name": "Verifying"},
            {"id": "opt-in-review", "name": "In review"},
            {"id": "opt-blocked", "name": "Blocked"},
            {"id": "opt-failed", "name": "Failed"},
        ])
    }

    fn project_response(options: impl Into<Value>) -> String {
        let options = options.into();
        json!({
            "data": {
                "repositoryOwner": {
                    "projectV2": {
                        "id": "PVT_1",
                        "field": {"id": "PVTSSF_1", "options": options},
                    }
                }
            }
        })
        .to_string()
    }

    fn item_response(issue: impl Into<Value>) -> String {
        let issue = issue.into();
        json!({"data": {"repository": {"issue": issue}}}).to_string()
    }

    fn item_found_response() -> String {
        item_response(json!({
            "projectItems": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {"id": "PVTI_other", "project": {"id": "PVT_OTHER"}},
                    {"id": "PVTI_2", "project": {"id": "PVT_1"}},
                ]
            }
        }))
    }

    fn write_result_response() -> String {
        json!({
            "data": {
                "updateProjectV2ItemFieldValue": {
                    "projectV2Item": {"id": "PVTI_2"}
                }
            }
        })
        .to_string()
    }

    fn graphql_argv_expected() -> Vec<String> {
        vec![
            "api".to_string(),
            "graphql".to_string(),
            "--input".to_string(),
            "-".to_string(),
        ]
    }

    #[test]
    fn every_request_argv_is_exact() {
        let expected = graphql_argv_expected();
        assert_eq!(read_ready_argv(), expected);
        assert_eq!(project_status_argv(), expected);
        assert_eq!(item_id_argv(), expected);
        assert_eq!(mutation_argv(), expected);
        assert_eq!(
            comment_argv(REPO, "42"),
            vec![
                "api".to_string(),
                "--method".to_string(),
                "POST".to_string(),
                "repos/open-software-factory/demo/issues/42/comments".to_string(),
                "--input".to_string(),
                "-".to_string(),
            ]
        );
    }

    #[test]
    fn the_read_payload_pins_its_variables_and_cursor() {
        let first = read_ready_payload(OWNER, 5, "Status", None, None);
        assert_eq!(pointer(&first, "/variables/owner"), &json!(OWNER));
        assert_eq!(pointer(&first, "/variables/number"), &json!(5));
        assert_eq!(pointer(&first, "/variables/field"), &json!("Status"));
        assert_eq!(pointer(&first, "/variables/cursor"), &Value::Null);
        assert_eq!(pointer(&first, "/variables/query"), &Value::Null);
        assert!(pointer(&first, "/query")
            .as_str()
            .expect("a query string")
            .contains("items(first: 100"));
        assert!(pointer(&first, "/query")
            .as_str()
            .expect("a query string")
            .contains("query: $query"));
        let second = read_ready_payload(OWNER, 5, "Status", Some("cursor-2"), None);
        assert_eq!(pointer(&second, "/variables/cursor"), &json!("cursor-2"));
        assert_ne!(first, second);
        let skipped = read_ready_payload(OWNER, 5, "Status", None, Some("-status:\"Done\""));
        assert_eq!(
            pointer(&skipped, "/variables/query"),
            &json!("-status:\"Done\"")
        );
    }

    #[test]
    fn the_project_item_and_mutation_payloads_pin_their_variables() {
        let project = project_status_payload(OWNER, 5, "Status");
        assert_eq!(
            pointer(&project, "/variables"),
            &json!({"owner": OWNER, "number": 5, "field": "Status"})
        );
        assert!(pointer(&project, "/query")
            .as_str()
            .expect("a query string")
            .contains("ProjectV2SingleSelectField"));
        let item = item_id_payload(OWNER, "demo", 42);
        assert_eq!(
            pointer(&item, "/variables"),
            &json!({"owner": OWNER, "name": "demo", "number": 42})
        );
        assert!(pointer(&item, "/query")
            .as_str()
            .expect("a query string")
            .contains("projectItems(first: 50)"));
        let mutation = mutation_payload("PVT_1", "PVTI_2", "PVTSSF_1", "opt-in-progress");
        assert_eq!(
            pointer(&mutation, "/variables"),
            &json!({"project": "PVT_1", "item": "PVTI_2", "field": "PVTSSF_1", "option": "opt-in-progress"})
        );
        assert!(pointer(&mutation, "/query")
            .as_str()
            .expect("a query string")
            .contains("updateProjectV2ItemFieldValue"));
    }

    #[test]
    fn the_comment_payload_pins_the_body() {
        assert_eq!(
            comment_payload("Blocked: human."),
            json!({"body": "Blocked: human."})
        );
    }

    #[test]
    fn parse_items_page_reads_a_full_item() {
        let page = parse_items_page(&full_page(), "Ready").expect("parses");
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.items.len(), 1);
        let item = page.items.first().expect("one item");
        assert_eq!(item.id, github_item("42"));
        assert_eq!(item.title, "A change");
        assert_eq!(item.body, "Body.");
        assert_eq!(item.repository, REPO);
        assert_eq!(item.status, Status::Ready);
        assert!(!item.closed);
        assert_eq!(
            item.blocked_by,
            vec![dependency("7", true), dependency("8", false)]
        );
    }

    #[test]
    fn parse_items_page_reads_a_closed_ready_item() {
        let page = items_page(
            json!([ready_node(issue_with_state(
                42,
                "CLOSED",
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        let item = parsed.items.first().expect("one item");
        assert!(item.closed);
    }

    #[test]
    fn parse_items_page_reads_an_open_ready_item() {
        let page = items_page(
            json!([ready_node(issue_with_state(
                42,
                "OPEN",
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        let item = parsed.items.first().expect("one item");
        assert!(!item.closed);
    }

    #[test]
    fn parse_items_page_rejects_a_missing_issue_state() {
        let content = json!({
            "number": 42,
            "title": "Ready",
            "repository": {"nameWithOwner": REPO},
            "blockedBy": blockers(json!([]), false),
        });
        let page = items_page(json!([ready_node(content)]), false, Value::Null);
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_weird_issue_state() {
        let page = items_page(
            json!([ready_node(issue_with_state(
                42,
                "MERGED",
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_keeps_another_status() {
        let page = items_page(
            json!([
                {
                    "fieldValueByName": {"name": "Backlog"},
                    "content": issue(41, "Not ready", json!("Body."), blockers(json!([]), false)),
                },
                ready_node(issue(42, "Ready", json!("Body."), blockers(json!([]), false))),
            ]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(
            parsed.items.first().expect("one item").status,
            Status::Other("Backlog".to_string())
        );
        assert_eq!(
            parsed.items.get(1).expect("another item").status,
            Status::Ready
        );
    }

    #[test]
    fn parse_items_page_treats_a_null_or_missing_status_as_no_status() {
        let page = items_page(
            json!([
                {
                    "fieldValueByName": {"name": null},
                    "content": issue(41, "No status", json!("Body."), blockers(json!([]), false)),
                },
                {
                    "content": issue(42, "No status", json!("Body."), blockers(json!([]), false)),
                },
            ]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert_eq!(parsed.items.len(), 2);
        assert!(parsed
            .items
            .iter()
            .all(|item| item.status == Status::Other("no status".to_string())));
    }

    #[test]
    fn parse_items_page_skips_a_node_with_no_issue_content() {
        let page = items_page(
            json!([
                {"fieldValueByName": {"name": "Ready"}, "content": null},
                {"fieldValueByName": {"name": "Ready"}, "content": {}},
                {"fieldValueByName": {"name": "Ready"}, "content": {"__typename": "PullRequest"}},
                {
                    "fieldValueByName": {"name": "Ready"},
                    "content": {"__typename": "ProjectV2DraftIssue"},
                },
                ready_node(issue(42, "Ready", json!("Body."), blockers(json!([]), false))),
            ]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(
            parsed.items.first().expect("one item").id,
            github_item("42")
        );
    }

    #[test]
    fn parse_items_page_treats_a_null_body_as_empty() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "Ready",
                Value::Null,
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert_eq!(parsed.items.first().expect("one item").body, String::new());
    }

    #[test]
    fn parse_items_page_reads_the_next_cursor() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            true,
            json!("cursor-2"),
        );
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert_eq!(parsed.next_cursor, Some("cursor-2".to_string()));
    }

    #[test]
    fn parse_items_page_gives_zero_items_for_an_empty_list() {
        let page = items_page(json!([]), false, Value::Null);
        let parsed = parse_items_page(&page, "Ready").expect("parses");
        assert!(parsed.items.is_empty());
        assert_eq!(parsed.next_cursor, None);
    }

    #[test]
    fn parse_items_page_rejects_not_json() {
        assert!(parse_items_page("not json", "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_an_errors_key() {
        let text = json!({"errors": [{"message": "no"}]}).to_string();
        assert!(parse_items_page(&text, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_null_project() {
        let text = json!({"data": {"repositoryOwner": {"projectV2": null}}}).to_string();
        assert!(parse_items_page(&text, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_next_page_without_a_cursor() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            true,
            Value::Null,
        );
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_missing_items_list() {
        let text = json!({"data": {"repositoryOwner": {"projectV2": {"id": "PVT_1"}}}}).to_string();
        assert!(parse_items_page(&text, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_weird_blocker_state() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "Ready",
                json!("Body."),
                blockers(
                    json!([{"number": 7, "state": "WEIRD", "repository": {"nameWithOwner": REPO}}]),
                    false,
                ),
            ))]),
            false,
            Value::Null,
        );
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_more_blockers_than_were_read() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "Ready",
                json!("Body."),
                blockers(
                    json!([{"number": 7, "state": "OPEN", "repository": {"nameWithOwner": REPO}}]),
                    true,
                ),
            ))]),
            false,
            Value::Null,
        );
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_missing_issue_number() {
        let content = json!({
            "title": "Ready",
            "repository": {"nameWithOwner": REPO},
            "blockedBy": blockers(json!([]), false),
        });
        let page = items_page(json!([ready_node(content)]), false, Value::Null);
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_missing_issue_title() {
        let content = json!({
            "number": 42,
            "repository": {"nameWithOwner": REPO},
            "blockedBy": blockers(json!([]), false),
        });
        let page = items_page(json!([ready_node(content)]), false, Value::Null);
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_rejects_a_missing_issue_repository() {
        let content = json!({
            "number": 42,
            "title": "Ready",
            "blockedBy": blockers(json!([]), false),
        });
        let page = items_page(json!([ready_node(content)]), false, Value::Null);
        assert!(parse_items_page(&page, "Ready").is_err());
    }

    #[test]
    fn parse_items_page_keeps_a_malformed_non_ready_item_as_unreadable() {
        for shape in malformed_shapes() {
            let bad = other_node("Backlog", shape.content);
            let good = ready_node(issue(
                43,
                "Ready",
                json!("Body."),
                blockers(json!([]), false),
            ));
            let page = items_page(json!([bad, good]), false, Value::Null);
            let parsed = parse_items_page(&page, "Ready")
                .unwrap_or_else(|error| panic!("{}: {error}", shape.label));
            assert_eq!(parsed.items.len(), 2, "{}", shape.label);
            let skipped = parsed.items.first().expect("the skipped item");
            assert_eq!(skipped.id.provider, "github", "{}", shape.label);
            assert_eq!(skipped.id.item, shape.item, "{}", shape.label);
            assert_eq!(skipped.id.repo, shape.repo, "{}", shape.label);
            assert_eq!(skipped.repository, shape.repo, "{}", shape.label);
            assert!(skipped.title.is_empty(), "{}", shape.label);
            assert!(skipped.body.is_empty(), "{}", shape.label);
            assert!(!skipped.closed, "{}", shape.label);
            assert!(skipped.blocked_by.is_empty(), "{}", shape.label);
            let Status::Other(text) = &skipped.status else {
                panic!("{}: the skipped item is not another status", shape.label);
            };
            assert!(text.contains("unreadable:"), "{}: {text}", shape.label);
            assert!(text.contains(shape.reason), "{}: {text}", shape.label);
            assert!(text.contains("Backlog"), "{}: {text}", shape.label);
            assert_eq!(
                parsed.items.get(1).expect("the ready item").id,
                github_item("43"),
                "{}",
                shape.label
            );
        }
    }

    #[test]
    fn parse_items_page_rejects_a_malformed_ready_item() {
        for shape in malformed_shapes() {
            let page = items_page(json!([ready_node(shape.content)]), false, Value::Null);
            assert!(parse_items_page(&page, "Ready").is_err(), "{}", shape.label);
        }
    }

    #[test]
    fn read_ready_finds_items_over_two_pages() {
        let first = items_page(
            json!([ready_node(issue(
                42,
                "First",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            true,
            json!("cursor-2"),
        );
        let second = items_page(
            json!([ready_node(issue(
                43,
                "Second",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        let tracker = tracker(vec![Ok(first), Ok(second)]);
        let outcome = tracker.read_ready();
        let ReadOutcome::Found(items) = outcome else {
            panic!("two pages must be found");
        };
        assert_eq!(
            items.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
            vec![github_item("42"), github_item("43")]
        );
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(call(&calls, 0).argv, graphql_argv_expected());
        assert_eq!(
            pointer(&payload(call(&calls, 0)), "/variables/cursor"),
            &Value::Null
        );
        assert_eq!(
            pointer(&payload(call(&calls, 1)), "/variables/cursor"),
            &json!("cursor-2")
        );
        for index in 0..2 {
            assert_eq!(
                pointer(&payload(call(&calls, index)), "/variables/query"),
                &json!("-status:\"Done\"")
            );
        }
    }

    #[test]
    fn read_ready_sends_a_null_query_when_no_status_is_skipped() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "First",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            false,
            Value::Null,
        );
        let mut config = config();
        config.skip_status = None;
        let tracker = GitHubTracker::new(ScriptedRunner::new(vec![Ok(page)]), config);
        assert!(matches!(tracker.read_ready(), ReadOutcome::Found(_)));
        let calls = tracker.runner.calls();
        assert_eq!(
            pointer(&payload(call(&calls, 0)), "/variables/query"),
            &Value::Null
        );
    }

    #[test]
    fn skip_status_query_escapes_and_lowercases_the_field() {
        assert_eq!(skip_status_query("Done", "Status"), "-status:\"Done\"");
        assert_eq!(skip_status_query("a\"b", "Status"), "-status:\"a\\\"b\"");
        assert_eq!(skip_status_query("a\\b", "STATUS"), "-status:\"a\\\\b\"");
    }

    #[test]
    fn read_ready_finds_items_in_another_status_with_their_status() {
        let page = items_page(
            json!([{
                "fieldValueByName": {"name": "Backlog"},
                "content": issue(42, "Not ready", json!("Body."), blockers(json!([]), false)),
            }]),
            false,
            Value::Null,
        );
        let tracker = tracker(vec![Ok(page)]);
        let ReadOutcome::Found(items) = tracker.read_ready() else {
            panic!("a backlog item is still found");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(
            items.first().expect("one item").status,
            Status::Other("Backlog".to_string())
        );
        assert_eq!(tracker.runner.calls().len(), 1);
    }

    #[test]
    fn read_ready_keeps_a_done_item_that_still_arrives() {
        let page = items_page(
            json!([{
                "fieldValueByName": {"name": "Done"},
                "content": issue(42, "Already done", json!("Body."), blockers(json!([]), false)),
            }]),
            false,
            Value::Null,
        );
        let tracker = tracker(vec![Ok(page)]);
        let ReadOutcome::Found(items) = tracker.read_ready() else {
            panic!("a done item that still arrives is returned");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(
            items.first().expect("one item").status,
            Status::Other("Done".to_string())
        );
    }

    #[test]
    fn read_ready_keeps_a_malformed_backlog_item_while_picking_the_ready_one() {
        let bad = other_node(
            "Backlog",
            issue_with_state(
                41,
                "WEIRD",
                "Bad",
                json!("Body."),
                blockers(json!([]), false),
            ),
        );
        let good = ready_node(issue(
            42,
            "Ready",
            json!("Body."),
            blockers(json!([]), false),
        ));
        let page = items_page(json!([bad, good]), false, Value::Null);
        let tracker = tracker(vec![Ok(page.clone()), Ok(page)]);
        let ReadOutcome::Found(items) = tracker.read_ready() else {
            panic!("a malformed backlog item must not hide the page");
        };
        assert_eq!(items.len(), 2);
        let Status::Other(text) = &items.first().expect("the malformed item").status else {
            panic!("the malformed item must carry another status");
        };
        assert!(text.contains("unreadable:"));
        let Pick::Item(item) = pick(&tracker) else {
            panic!("the good ready item must be picked");
        };
        assert_eq!(item.id, github_item("42"));
        assert_eq!(item.status, Status::Ready);
    }

    #[test]
    fn read_ready_is_empty_for_a_project_with_no_items() {
        let page = items_page(json!([]), false, Value::Null);
        let tracker = tracker(vec![Ok(page)]);
        assert_eq!(tracker.read_ready(), ReadOutcome::Empty);
        assert_eq!(tracker.runner.calls().len(), 1);
    }

    #[test]
    fn read_ready_is_unknown_when_the_second_page_fails() {
        let first = items_page(
            json!([ready_node(issue(
                42,
                "First",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            true,
            json!("cursor-2"),
        );
        let tracker = tracker(vec![Ok(first), Err("boom".to_string())]);
        let outcome = tracker.read_ready();
        assert_eq!(outcome, ReadOutcome::Unknown("boom".to_string()));
        assert!(!matches!(outcome, ReadOutcome::Found(_)));
    }

    #[test]
    fn read_ready_is_unknown_on_a_runner_error() {
        let tracker = tracker(vec![Err("cannot run gh".to_string())]);
        assert_eq!(
            tracker.read_ready(),
            ReadOutcome::Unknown("cannot run gh".to_string())
        );
    }

    #[test]
    fn read_ready_is_unknown_on_a_repeating_cursor() {
        let page = items_page(
            json!([ready_node(issue(
                42,
                "First",
                json!("Body."),
                blockers(json!([]), false),
            ))]),
            true,
            json!("cursor-2"),
        );
        let tracker = tracker(vec![Ok(page.clone()), Ok(page)]);
        assert!(matches!(tracker.read_ready(), ReadOutcome::Unknown(_)));
        assert_eq!(tracker.runner.calls().len(), 2);
    }

    #[test]
    fn parse_project_status_reads_ids_and_options() {
        let project = parse_project_status(&project_response(all_options())).expect("parses");
        assert_eq!(project.project_id, "PVT_1");
        assert_eq!(project.field_id, "PVTSSF_1");
        assert_eq!(
            project.options,
            vec![
                ("Ready".to_string(), "opt-ready".to_string()),
                ("In progress".to_string(), "opt-in-progress".to_string()),
                ("Verifying".to_string(), "opt-verifying".to_string()),
                ("In review".to_string(), "opt-in-review".to_string()),
                ("Blocked".to_string(), "opt-blocked".to_string()),
                ("Failed".to_string(), "opt-failed".to_string()),
            ]
        );
    }

    #[test]
    fn parse_project_status_rejects_a_missing_project() {
        let text = json!({"data": {"repositoryOwner": {"projectV2": null}}}).to_string();
        assert!(parse_project_status(&text).is_err());
    }

    #[test]
    fn parse_item_id_finds_the_project_item() {
        assert_eq!(
            parse_item_id(&item_found_response(), "PVT_1").expect("found"),
            "PVTI_2"
        );
    }

    #[test]
    fn parse_item_id_rejects_an_issue_not_in_the_project() {
        let text = item_response(json!({
            "projectItems": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [{"id": "PVTI_other", "project": {"id": "PVT_OTHER"}}]
            }
        }));
        assert!(parse_item_id(&text, "PVT_1").is_err());
    }

    #[test]
    fn parse_item_id_rejects_a_missing_item_with_more_pages() {
        let text = item_response(json!({
            "projectItems": {
                "pageInfo": {"hasNextPage": true},
                "nodes": []
            }
        }));
        assert!(parse_item_id(&text, "PVT_1").is_err());
    }

    #[test]
    fn parse_write_result_accepts_a_written_item() {
        assert!(parse_write_result(&write_result_response()).is_ok());
    }

    #[test]
    fn parse_write_result_rejects_an_errors_key() {
        let text = json!({"errors": [{"message": "no"}]}).to_string();
        assert!(parse_write_result(&text).is_err());
    }

    #[test]
    fn parse_write_result_rejects_a_missing_item_id() {
        let text = json!({
            "data": {"updateProjectV2ItemFieldValue": {"projectV2Item": null}}
        })
        .to_string();
        assert!(parse_write_result(&text).is_err());
    }

    #[test]
    fn write_state_runs_the_three_calls_in_order() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Ok(write_result_response()),
        ]);
        tracker
            .write_state(&github_item("42"), &WorkState::InProgress)
            .expect("writes");
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(call(&calls, 0).argv, graphql_argv_expected());
        assert_eq!(
            pointer(&payload(call(&calls, 0)), "/variables/number"),
            &json!(5)
        );
        assert_eq!(
            pointer(&payload(call(&calls, 0)), "/variables/field"),
            &json!("Status")
        );
        assert_eq!(
            pointer(&payload(call(&calls, 1)), "/variables"),
            &json!({"owner": OWNER, "name": "demo", "number": 42})
        );
        assert_eq!(
            pointer(&payload(call(&calls, 2)), "/variables"),
            &json!({"project": "PVT_1", "item": "PVTI_2", "field": "PVTSSF_1", "option": "opt-in-progress"})
        );
    }

    #[test]
    fn write_state_rejects_an_unconfigured_verifying_option_without_calling() {
        let mut config = config();
        config.options.verifying = None;
        let tracker = GitHubTracker::new(ScriptedRunner::new(Vec::new()), config);
        let error = tracker
            .write_state(&github_item("42"), &WorkState::Verifying)
            .expect_err("rejected");
        assert_eq!(
            error,
            TrackerError::Rejected("this project has no option for verifying".to_string())
        );
        assert_eq!(tracker.runner.calls().len(), 0);
    }

    #[test]
    fn write_state_rejects_a_missing_project_option_before_the_mutation() {
        let options = json!([
            {"id": "opt-ready", "name": "Ready"},
            {"id": "opt-verifying", "name": "Verifying"},
            {"id": "opt-in-review", "name": "In review"},
            {"id": "opt-blocked", "name": "Blocked"},
            {"id": "opt-failed", "name": "Failed"},
        ]);
        let tracker = tracker(vec![Ok(project_response(options))]);
        let error = tracker
            .write_state(&github_item("42"), &WorkState::InProgress)
            .expect_err("rejected");
        assert_eq!(
            error,
            TrackerError::Rejected("the project has no option named In progress".to_string())
        );
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 1);
        assert!(!calls.iter().any(|call| {
            pointer(&payload(call), "/query")
                .as_str()
                .is_some_and(|query| query.contains("updateProjectV2ItemFieldValue"))
        }));
    }

    #[test]
    fn write_state_reports_a_runner_error_on_each_step() {
        let first = tracker(vec![Err("project boom".to_string())]);
        assert_eq!(
            first
                .write_state(&github_item("42"), &WorkState::InProgress)
                .expect_err("fails"),
            TrackerError::Failed("project boom".to_string())
        );

        let second = tracker(vec![
            Ok(project_response(all_options())),
            Err("item boom".to_string()),
        ]);
        assert_eq!(
            second
                .write_state(&github_item("42"), &WorkState::InProgress)
                .expect_err("fails"),
            TrackerError::Failed("item boom".to_string())
        );

        let third = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Err("mutation boom".to_string()),
        ]);
        assert_eq!(
            third
                .write_state(&github_item("42"), &WorkState::InProgress)
                .expect_err("fails"),
            TrackerError::Failed("mutation boom".to_string())
        );
    }

    #[test]
    fn write_state_of_a_human_block_posts_the_exact_comment() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Ok(String::new()),
            Ok(write_result_response()),
        ]);
        tracker
            .write_state(&github_item("42"), &WorkState::Blocked(BlockedCause::Human))
            .expect("writes");
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(call(&calls, 2).argv, comment_argv(REPO, "42"));
        assert_eq!(payload(call(&calls, 2)), json!({"body": "Blocked: human."}));
        assert_eq!(
            pointer(&payload(call(&calls, 3)), "/variables/option"),
            &json!("opt-blocked")
        );
    }

    #[test]
    fn write_state_of_a_block_orders_reads_comment_then_mutation() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Ok(String::new()),
            Ok(write_result_response()),
        ]);
        tracker
            .write_state(&github_item("42"), &WorkState::Blocked(BlockedCause::Human))
            .expect("writes");
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(call(&calls, 0).argv, project_status_argv());
        assert_eq!(
            payload(call(&calls, 0)),
            project_status_payload(OWNER, 5, "Status")
        );
        assert_eq!(call(&calls, 1).argv, item_id_argv());
        assert_eq!(payload(call(&calls, 1)), item_id_payload(OWNER, "demo", 42));
        assert_eq!(call(&calls, 2).argv, comment_argv(REPO, "42"));
        assert_eq!(payload(call(&calls, 2)), json!({"body": "Blocked: human."}));
        assert_eq!(call(&calls, 3).argv, mutation_argv());
        assert_eq!(
            payload(call(&calls, 3)),
            mutation_payload("PVT_1", "PVTI_2", "PVTSSF_1", "opt-blocked")
        );
    }

    #[test]
    fn write_state_of_a_block_returns_the_comment_error_before_the_mutation() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Err("comment boom".to_string()),
        ]);
        let error = tracker
            .write_state(&github_item("42"), &WorkState::Blocked(BlockedCause::Human))
            .expect_err("fails");
        assert_eq!(error, TrackerError::Failed("comment boom".to_string()));
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(call(&calls, 2).argv, comment_argv(REPO, "42"));
        assert!(!calls.iter().any(|call| {
            payload(call)
                .pointer("/query")
                .and_then(Value::as_str)
                .is_some_and(|query| query.contains("updateProjectV2ItemFieldValue"))
        }));
    }

    #[test]
    fn write_state_of_a_block_reports_a_failed_mutation_after_the_comment() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Ok(String::new()),
            Err("mutation boom".to_string()),
        ]);
        let error = tracker
            .write_state(&github_item("42"), &WorkState::Blocked(BlockedCause::Human))
            .expect_err("fails");
        assert_eq!(error, TrackerError::Failed("mutation boom".to_string()));
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(call(&calls, 2).argv, comment_argv(REPO, "42"));
        assert_eq!(payload(call(&calls, 2)), json!({"body": "Blocked: human."}));
        assert_eq!(call(&calls, 3).argv, mutation_argv());
    }

    #[test]
    fn write_state_of_a_non_block_makes_no_comment_call() {
        let tracker = tracker(vec![
            Ok(project_response(all_options())),
            Ok(item_found_response()),
            Ok(write_result_response()),
        ]);
        tracker
            .write_state(&github_item("42"), &WorkState::InProgress)
            .expect("writes");
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 3);
        assert!(!calls
            .iter()
            .any(|call| call.argv == comment_argv(REPO, "42")));
    }

    #[test]
    fn write_recap_posts_the_exact_body() {
        let tracker = tracker(vec![Ok(String::new())]);
        tracker
            .write_recap(&github_item("42"), "The recap.")
            .expect("writes");
        let calls = tracker.runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(call(&calls, 0).argv, comment_argv(REPO, "42"));
        assert_eq!(payload(call(&calls, 0)), json!({"body": "The recap."}));
    }

    #[test]
    fn a_non_github_id_is_rejected_without_a_call() {
        let other = WorkItemId {
            provider: "other".to_string(),
            repo: REPO.to_string(),
            item: "42".to_string(),
        };
        let tracker = tracker(Vec::new());
        assert_eq!(
            tracker
                .write_state(&other, &WorkState::InProgress)
                .expect_err("rejected"),
            TrackerError::Rejected(
                "not a github item: other:open-software-factory/demo#42".to_string()
            )
        );
        assert_eq!(
            tracker.write_recap(&other, "done").expect_err("rejected"),
            TrackerError::Rejected(
                "not a github item: other:open-software-factory/demo#42".to_string()
            )
        );
        assert_eq!(tracker.runner.calls().len(), 0);
    }

    #[test]
    fn capabilities_support_all_nine_states_with_a_full_project() {
        let tracker = tracker(vec![Ok(project_response(all_options()))]);
        let capabilities = tracker.capabilities();
        assert_eq!(capabilities.title, Capability::Supported);
        assert_eq!(capabilities.body, Capability::Supported);
        assert_eq!(capabilities.repository, Capability::Supported);
        assert_eq!(capabilities.dependencies, Capability::Supported);
        assert_eq!(capabilities.ready_mark, Capability::Supported);
        assert_eq!(capabilities.recap, Capability::Supported);
        assert_eq!(
            capabilities
                .state_writes
                .iter()
                .map(|write| write.state.clone())
                .collect::<Vec<_>>(),
            state_write_states()
        );
        assert!(capabilities
            .state_writes
            .iter()
            .all(|write| write.capability == Capability::Supported));
    }

    #[test]
    fn capabilities_report_verifying_unsupported_when_the_project_lacks_it() {
        let options = json!([
            {"id": "opt-ready", "name": "Ready"},
            {"id": "opt-in-progress", "name": "In progress"},
            {"id": "opt-in-review", "name": "In review"},
            {"id": "opt-blocked", "name": "Blocked"},
            {"id": "opt-failed", "name": "Failed"},
        ]);
        let tracker = tracker(vec![Ok(project_response(options))]);
        let capabilities = tracker.capabilities();
        assert_eq!(
            capabilities
                .state_writes
                .get(1)
                .expect("verifying is listed")
                .capability,
            Capability::Unsupported("the project has no option named Verifying".to_string())
        );
        assert_eq!(
            capabilities
                .state_writes
                .first()
                .expect("in progress is listed")
                .capability,
            Capability::Supported
        );
    }

    #[test]
    fn capabilities_report_the_ready_mark_unsupported_when_the_option_is_missing() {
        let options = json!([
            {"id": "opt-in-progress", "name": "In progress"},
            {"id": "opt-verifying", "name": "Verifying"},
            {"id": "opt-in-review", "name": "In review"},
            {"id": "opt-blocked", "name": "Blocked"},
            {"id": "opt-failed", "name": "Failed"},
        ]);
        let tracker = tracker(vec![Ok(project_response(options))]);
        let capabilities = tracker.capabilities();
        assert_eq!(
            capabilities.ready_mark,
            Capability::Unsupported("the project has no option named Ready".to_string())
        );
    }

    #[test]
    fn capabilities_are_unknown_on_a_runner_error() {
        let tracker = tracker(vec![Err("boom".to_string())]);
        let capabilities = tracker.capabilities();
        let unknown = Capability::Unknown("boom".to_string());
        assert_eq!(capabilities.title, unknown);
        assert_eq!(capabilities.body, unknown);
        assert_eq!(capabilities.repository, unknown);
        assert_eq!(capabilities.dependencies, unknown);
        assert_eq!(capabilities.ready_mark, unknown);
        assert_eq!(capabilities.recap, unknown);
        assert!(capabilities
            .state_writes
            .iter()
            .all(|write| write.capability == Capability::Unknown("boom".to_string())));
        assert_eq!(capabilities.state_writes.len(), 9);
    }

    #[test]
    fn capabilities_are_unknown_on_malformed_json() {
        let tracker = tracker(vec![Ok("not json".to_string())]);
        let capabilities = tracker.capabilities();
        assert!(matches!(capabilities.title, Capability::Unknown(_)));
        assert!(matches!(capabilities.ready_mark, Capability::Unknown(_)));
        assert!(capabilities
            .state_writes
            .iter()
            .all(|write| matches!(write.capability, Capability::Unknown(_))));
    }

    #[test]
    fn state_options_standard_are_pinned() {
        let options = StateOptions::standard();
        assert_eq!(options.ready, "Ready");
        assert_eq!(options.in_progress, "In progress");
        assert_eq!(options.verifying, Some("Verifying".to_string()));
        assert_eq!(options.in_review, "In review");
        assert_eq!(options.blocked, "Blocked");
        assert_eq!(options.failed, "Failed");
    }

    #[test]
    fn option_for_maps_every_state_to_its_option() {
        let options = StateOptions::standard();
        assert_eq!(
            options.option_for(&WorkState::InProgress),
            Some("In progress")
        );
        assert_eq!(options.option_for(&WorkState::Verifying), Some("Verifying"));
        assert_eq!(options.option_for(&WorkState::InReview), Some("In review"));
        assert_eq!(options.option_for(&WorkState::Failed), Some("Failed"));
        for cause in [
            BlockedCause::Dependency,
            BlockedCause::Human,
            BlockedCause::Clarification,
            BlockedCause::Ambiguous,
            BlockedCause::Capacity,
        ] {
            assert_eq!(
                options.option_for(&WorkState::Blocked(cause)),
                Some("Blocked")
            );
        }
        let no_verifying = StateOptions {
            verifying: None,
            ..StateOptions::standard()
        };
        assert_eq!(no_verifying.option_for(&WorkState::Verifying), None);
    }

    #[test]
    fn pick_over_the_real_adapter_reports_a_backlog_only_page_as_nothing_ready() {
        let mixed = items_page(
            json!([
                {
                    "fieldValueByName": {"name": "Backlog"},
                    "content": issue(41, "Backlog", json!("Body."), blockers(json!([]), false)),
                },
                ready_node(issue(42, "Ready", json!("Body."), blockers(json!([]), false))),
            ]),
            false,
            Value::Null,
        );
        let mixed_tracker = tracker(vec![Ok(mixed)]);
        let Pick::Item(item) = pick(&mixed_tracker) else {
            panic!("the ready item must be picked");
        };
        assert_eq!(item.id, github_item("42"));
        assert_eq!(item.status, Status::Ready);

        let backlog_only = items_page(
            json!([{
                "fieldValueByName": {"name": "Backlog"},
                "content": issue(41, "Backlog", json!("Body."), blockers(json!([]), false)),
            }]),
            false,
            Value::Null,
        );
        let tracker = tracker(vec![Ok(backlog_only)]);
        assert_eq!(pick(&tracker), Pick::NothingReady);
    }
}
