//! A full engine-style run over the in-memory tracker double: no process, file,
//! network or clock is reached anywhere in this test.

use osf::tracker::fake::{Call, FakeTracker, Operation};
use osf::tracker::{
    finish_run, pick, BlockedCause, Dependency, Pick, ReadOutcome, Status, Step, Tracker,
    TrackerError, WorkItem, WorkItemId, WorkState,
};

const REPO: &str = "open-software-factory/demo";
const RECAP: &str = "Recap: one change, reviewed.";

fn id(item: &str) -> WorkItemId {
    WorkItemId {
        provider: "test".to_string(),
        repo: REPO.to_string(),
        item: item.to_string(),
    }
}

fn dependency(item: &str, closed: bool) -> Dependency {
    Dependency {
        id: id(item),
        closed,
    }
}

fn work_item(item: &str, blocked_by: Vec<Dependency>) -> WorkItem {
    WorkItem {
        id: id(item),
        title: "A change".to_string(),
        body: "Body.".to_string(),
        repository: REPO.to_string(),
        status: Status::Ready,
        blocked_by,
    }
}

/// The engine's whole use of the tracker seam: pick one item and report it.
fn run_one(tracker: &dyn Tracker) -> Result<WorkItemId, String> {
    let item = match pick(tracker) {
        Pick::Item(item) => item,
        Pick::NothingReady => return Err("nothing is ready".to_string()),
        Pick::Unknown(text) => return Err(format!("unknown: {text}")),
        Pick::NoneDispatchable(skipped) => {
            let ids: Vec<String> = skipped.iter().map(|entry| entry.id.to_string()).collect();
            return Err(format!("none dispatchable: {}", ids.join(", ")));
        }
    };
    let chosen = item.id;
    tracker
        .write_state(&chosen, &WorkState::InProgress)
        .map_err(|error| error.to_string())?;
    tracker
        .write_state(&chosen, &WorkState::Verifying)
        .map_err(|error| error.to_string())?;
    finish_run(tracker, &chosen, &WorkState::InReview, RECAP).map_err(|error| error.to_string())?;
    Ok(chosen)
}

#[test]
fn a_full_run_reads_a_ready_item_and_writes_state_then_recap() {
    let mut tracker = FakeTracker::new();
    let item = work_item("1", vec![dependency("2", true)]);
    tracker.ready = ReadOutcome::Found(vec![item.clone()]);
    let chosen = run_one(&tracker).expect("runs");
    assert_eq!(chosen, item.id);
    assert_eq!(
        tracker.calls(),
        vec![
            Call::ReadReady,
            Call::WriteState {
                id: chosen.clone(),
                state: WorkState::InProgress,
            },
            Call::WriteState {
                id: chosen.clone(),
                state: WorkState::Verifying,
            },
            Call::WriteState {
                id: chosen.clone(),
                state: WorkState::InReview,
            },
            Call::WriteRecap {
                id: chosen.clone(),
                recap: RECAP.to_string(),
            },
        ]
    );
}

#[test]
fn a_run_on_a_failed_read_writes_nothing() {
    let mut tracker = FakeTracker::new();
    tracker.ready = ReadOutcome::Unknown("boom".to_string());
    let error = run_one(&tracker).expect_err("fails");
    assert!(error.contains("unknown"), "{error}");
    assert_eq!(tracker.calls(), vec![Call::ReadReady]);
}

#[test]
fn a_run_on_an_empty_backlog_writes_nothing_and_says_nothing_is_ready() {
    let tracker = FakeTracker::new();
    let error = run_one(&tracker).expect_err("fails");
    assert!(error.contains("nothing is ready"), "{error}");
    assert!(!error.contains("unknown"), "{error}");
    assert_eq!(tracker.calls(), vec![Call::ReadReady]);
}

#[test]
fn a_run_skips_an_item_with_an_open_blocker() {
    let mut tracker = FakeTracker::new();
    let blocked = work_item("1", vec![dependency("3", false)]);
    let ready = work_item("2", Vec::new());
    tracker.ready = ReadOutcome::Found(vec![blocked.clone(), ready.clone()]);
    let chosen = run_one(&tracker).expect("runs");
    assert_eq!(chosen, ready.id);
    let names_blocked = tracker.calls().iter().any(|call| match call {
        Call::WriteState { id, .. } | Call::WriteRecap { id, .. } => *id == blocked.id,
        Call::ReadReady | Call::Capabilities => false,
    });
    assert!(!names_blocked, "no write may name the skipped item");
}

#[test]
fn a_blocked_item_run_finishes_with_its_cause() {
    let tracker = FakeTracker::new();
    let item = id("1");
    finish_run(
        &tracker,
        &item,
        &WorkState::Blocked(BlockedCause::Human),
        RECAP,
    )
    .expect("finishes");
    assert_eq!(
        tracker.calls(),
        vec![
            Call::WriteState {
                id: item.clone(),
                state: WorkState::Blocked(BlockedCause::Human),
            },
            Call::WriteRecap {
                id: item.clone(),
                recap: RECAP.to_string(),
            },
        ]
    );
}

#[test]
fn a_failed_state_write_stops_the_run_with_its_step_named() {
    let tracker = FakeTracker::new().fail(
        Operation::WriteState,
        TrackerError::Failed("refused".to_string()),
    );
    let item = id("1");
    let error = finish_run(&tracker, &item, &WorkState::InReview, RECAP).expect_err("fails");
    assert_eq!(error.step, Step::WriteState);
    assert!(
        error.to_string().contains("write state: refused"),
        "{error}"
    );
    assert!(
        !tracker
            .calls()
            .iter()
            .any(|call| matches!(call, Call::WriteRecap { .. })),
        "the recap is never written"
    );
}
