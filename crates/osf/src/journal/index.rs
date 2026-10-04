//! The per-work-item run index: one JSON file under `<state_dir>/index` whose
//! name is the SHA-256 hex of the work item id and whose body lists that work
//! item's run ids in the order they were recorded.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{sha256_hex, validate_run_id};

/// How long a writer waits for another writer to release the lock file.
const LOCK_WAIT: Duration = Duration::from_secs(10);

/// How long a writer sleeps between attempts to take the lock file.
const LOCK_RETRY: Duration = Duration::from_millis(5);

/// The index file's body: the work item id and its run ids in order.
#[derive(Debug, Deserialize, Serialize)]
struct WorkItemIndex {
    work_item: String,
    runs: Vec<String>,
}

/// The index path for `work_item`, named by the SHA-256 hex of its id because
/// a provider-qualified id holds characters unsafe in a file name.
pub(super) fn index_path(state_dir: &Path, work_item: &str) -> PathBuf {
    state_dir.join("index").join(index_file_name(work_item))
}

/// The file name for `work_item`'s index: its SHA-256 hex plus `.json`.
fn index_file_name(work_item: &str) -> String {
    format!("{}.json", sha256_hex(work_item.as_bytes()))
}

/// Reads the index at `path`; a missing file reads as `None`.
fn read_index(path: &Path) -> Result<Option<WorkItemIndex>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot read the index file {}: {e}",
                path.display()
            ));
        }
    };
    let index: WorkItemIndex = serde_json::from_str(&text)
        .map_err(|e| format!("the index file {} is malformed: {e}", path.display()))?;
    Ok(Some(index))
}

/// Refuses an index whose stored work item is not the requested one.
fn check_owner(path: &Path, index: &WorkItemIndex, work_item: &str) -> Result<(), String> {
    if index.work_item != work_item {
        return Err(format!(
            "the index file {} belongs to work item '{}', not '{work_item}'",
            path.display(),
            index.work_item
        ));
    }
    Ok(())
}

/// A temporary sibling of `path`, unique to this process and call.
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    path.with_file_name(format!(".{name}.{}.{nanos}.tmp", std::process::id()))
}

/// Appends a failed temporary-file cleanup to `primary`, hiding no error.
fn with_cleanup_result(primary: String, temp: &Path) -> String {
    match std::fs::remove_file(temp) {
        Ok(()) => primary,
        Err(e) => format!(
            "{primary}; the temporary file {} could not be removed: {e}",
            temp.display()
        ),
    }
}

/// Writes `index` to `path` through a temporary file renamed over it, so a
/// reader never sees a half-written index.
fn write_index(path: &Path, index: &WorkItemIndex) -> Result<(), String> {
    let text = serde_json::to_string(index)
        .map_err(|e| format!("cannot serialise the index for {}: {e}", path.display()))?;
    let temp = temporary_path(path);
    if let Err(e) = std::fs::write(&temp, format!("{text}\n")) {
        let primary = format!("cannot write the index file {}: {e}", path.display());
        return Err(with_cleanup_result(primary, &temp));
    }
    if let Err(e) = std::fs::rename(&temp, path) {
        let primary = format!("cannot replace the index file {}: {e}", path.display());
        return Err(with_cleanup_result(primary, &temp));
    }
    Ok(())
}

/// The lock file beside the index file `path`: its file name plus `.lock`.
fn lock_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    path.with_file_name(format!("{name}.lock"))
}

/// Takes the lock file for `path`, waiting and refusing a stale lock at [`LOCK_WAIT`].
fn take_lock(path: &Path) -> Result<PathBuf, String> {
    let lock = lock_path(path);
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => return Ok(lock),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "cannot take the lock file {}: another writer holds it, or a stale lock file is present and can be removed by hand",
                        lock.display()
                    ));
                }
                std::thread::sleep(LOCK_RETRY);
            }
            Err(e) => {
                return Err(format!(
                    "cannot create the lock file {}: {e}",
                    lock.display()
                ));
            }
        }
    }
}

/// Releases the lock file `lock` taken by [`take_lock`].
fn release_lock(lock: &Path) -> Result<(), String> {
    std::fs::remove_file(lock)
        .map_err(|e| format!("cannot remove the lock file {}: {e}", lock.display()))
}

/// The read, append and write for `record_run`, run while the lock is held.
fn record_run_locked(path: &Path, work_item: &str, run: &str) -> Result<(), String> {
    let mut index = match read_index(path)? {
        Some(existing) => {
            check_owner(path, &existing, work_item)?;
            existing
        }
        None => WorkItemIndex {
            work_item: work_item.to_string(),
            runs: Vec::new(),
        },
    };
    if index.runs.iter().any(|listed| listed == run) {
        return Ok(());
    }
    index.runs.push(run.to_string());
    write_index(path, &index)
}

/// Appends `run` to the index for `work_item`, creating the file when it does
/// not exist yet. Recording a run already listed is a no-op. An index that
/// cannot be read, is malformed, or names another work item is refused, never
/// silently replaced.
///
/// # Errors
/// Returns an error when `work_item` is empty, when `run` is not a safe
/// single path component, or when the index cannot be read or written.
pub fn record_run(state_dir: &Path, work_item: &str, run: &str) -> Result<(), String> {
    if work_item.is_empty() {
        return Err("work item id must not be empty".to_string());
    }
    validate_run_id(run)?;
    let dir = state_dir.join("index");
    let path = dir.join(index_file_name(work_item));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create the index directory {}: {e}", dir.display()))?;
    let lock = take_lock(&path)?;
    let work = record_run_locked(&path, work_item, run);
    match (work, release_lock(&lock)) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(release)) => Err(release),
        (Err(work), Ok(())) => Err(work),
        (Err(work), Err(release)) => Err(format!("{work}; {release}")),
    }
}

/// The run ids recorded for `work_item`, in recorded order. A missing index
/// reads as an empty list; a malformed one is an error.
///
/// # Errors
/// Returns an error when the index file exists but cannot be read, is
/// malformed, or names another work item.
pub fn runs_for_work_item(state_dir: &Path, work_item: &str) -> Result<Vec<String>, String> {
    let path = index_path(state_dir, work_item);
    match read_index(&path)? {
        Some(index) => {
            check_owner(&path, &index, work_item)?;
            Ok(index.runs)
        }
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    const ITEM: &str = "github:open-software-factory/example#1";

    /// Writes `contents` at `work_item`'s index path, creating the directory.
    fn write_index_text(dir: &Path, work_item: &str, contents: &str) -> PathBuf {
        let path = index_path(dir, work_item);
        std::fs::create_dir_all(path.parent().expect("index dir")).expect("index dir");
        std::fs::write(&path, contents).expect("write index");
        path
    }

    #[test]
    fn runs_come_back_in_recorded_order() {
        let dir = TempDir::new("osf-index-order");
        record_run(&dir, ITEM, "run-1").expect("record");
        record_run(&dir, ITEM, "run-2").expect("record");
        record_run(&dir, ITEM, "run-3").expect("record");
        let runs = runs_for_work_item(&dir, ITEM).expect("runs");
        assert_eq!(runs, ["run-1", "run-2", "run-3"].map(str::to_string));
    }

    #[test]
    fn recording_the_same_run_twice_lists_it_once() {
        let dir = TempDir::new("osf-index-twice");
        record_run(&dir, ITEM, "run-1").expect("record");
        record_run(&dir, ITEM, "run-1").expect("record again");
        let runs = runs_for_work_item(&dir, ITEM).expect("runs");
        assert_eq!(runs, ["run-1"].map(str::to_string));
    }

    #[test]
    fn two_work_items_have_separate_index_files_and_lists() {
        let dir = TempDir::new("osf-index-separate");
        let other = "github:open-software-factory/example#2";
        record_run(&dir, ITEM, "run-1").expect("record");
        record_run(&dir, other, "run-2").expect("record");
        assert_ne!(index_path(&dir, ITEM), index_path(&dir, other));
        assert_eq!(
            runs_for_work_item(&dir, ITEM).expect("runs"),
            ["run-1"].map(str::to_string)
        );
        assert_eq!(
            runs_for_work_item(&dir, other).expect("runs"),
            ["run-2"].map(str::to_string)
        );
    }

    #[test]
    fn an_id_with_separator_colon_and_hash_works() {
        let dir = TempDir::new("osf-index-unsafe-chars");
        record_run(&dir, ITEM, "run-1").expect("record");
        assert!(index_path(&dir, ITEM).is_file());
        assert_eq!(
            runs_for_work_item(&dir, ITEM).expect("runs"),
            ["run-1"].map(str::to_string)
        );
    }

    #[test]
    fn a_missing_index_is_an_empty_list() {
        let dir = TempDir::new("osf-index-missing");
        assert!(runs_for_work_item(&dir, ITEM).expect("runs").is_empty());
    }

    #[test]
    fn a_malformed_index_file_is_an_error_naming_the_file() {
        let dir = TempDir::new("osf-index-malformed");
        let path = write_index_text(&dir, ITEM, "not JSON");
        let err = runs_for_work_item(&dir, ITEM).expect_err("malformed");
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn an_index_holding_another_work_item_is_an_error() {
        let dir = TempDir::new("osf-index-wrong-owner");
        let other = "github:open-software-factory/example#2";
        let path = write_index_text(
            &dir,
            ITEM,
            &format!(r#"{{"work_item":"{other}","runs":["run-1"]}}"#),
        );
        let read_err = runs_for_work_item(&dir, ITEM).expect_err("wrong owner");
        assert!(read_err.contains(&path.display().to_string()), "{read_err}");
        let write_err = record_run(&dir, ITEM, "run-2").expect_err("wrong owner");
        assert!(
            write_err.contains(&path.display().to_string()),
            "{write_err}"
        );
    }

    #[test]
    fn an_empty_work_item_and_a_traversal_run_are_refused() {
        let dir = TempDir::new("osf-index-refused");
        let empty = record_run(&dir, "", "run-1").expect_err("empty work item");
        assert!(empty.contains("empty"), "{empty}");
        let traversal = record_run(&dir, ITEM, "../escape").expect_err("traversal run");
        assert!(traversal.contains(".."), "{traversal}");
        assert!(!dir.join("index").exists());
    }

    #[test]
    fn the_index_file_is_json_of_the_documented_shape() {
        let dir = TempDir::new("osf-index-shape");
        record_run(&dir, ITEM, "run-1").expect("record");
        record_run(&dir, ITEM, "run-2").expect("record");
        let text = std::fs::read_to_string(index_path(&dir, ITEM)).expect("read");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(
            value.get("work_item").and_then(serde_json::Value::as_str),
            Some(ITEM)
        );
        let runs = value
            .get("runs")
            .and_then(serde_json::Value::as_array)
            .expect("runs array");
        let ids: Vec<&str> = runs.iter().filter_map(serde_json::Value::as_str).collect();
        assert_eq!(ids, ["run-1", "run-2"]);
    }

    #[test]
    fn no_temporary_file_is_left_after_a_record() {
        let dir = TempDir::new("osf-index-temp");
        record_run(&dir, ITEM, "run-1").expect("record");
        record_run(&dir, ITEM, "run-2").expect("record");
        let entries: Vec<PathBuf> = std::fs::read_dir(dir.join("index"))
            .expect("index dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert_eq!(entries, [index_path(&dir, ITEM)]);
    }

    #[test]
    fn a_failed_cleanup_is_appended_to_the_error() {
        let dir = TempDir::new("osf-index-cleanup-failed");
        let temp = dir.join("absent.tmp");
        let primary = "cannot write the index file: boom".to_string();
        let message = with_cleanup_result(primary.clone(), &temp);
        assert!(message.starts_with(&primary), "{message}");
        assert!(message.contains("could not be removed"), "{message}");
        assert!(message.contains(&temp.display().to_string()), "{message}");
    }

    #[test]
    fn a_successful_cleanup_leaves_the_error_unchanged() {
        let dir = TempDir::new("osf-index-cleanup-succeeded");
        let temp = dir.join("present.tmp");
        std::fs::write(&temp, "temporary").expect("temp file");
        let primary = "cannot replace the index file: boom".to_string();
        let message = with_cleanup_result(primary.clone(), &temp);
        assert_eq!(message, primary);
        assert!(!temp.exists());
    }

    #[test]
    fn a_failed_rename_returns_the_error_and_leaves_no_temporary_file() {
        let dir = TempDir::new("osf-index-rename-failed");
        let path = index_path(&dir, ITEM);
        std::fs::create_dir_all(&path).expect("index path as a directory");
        std::fs::write(path.join("blocker"), "blocker").expect("blocker file");
        let index = WorkItemIndex {
            work_item: ITEM.to_string(),
            runs: vec!["run-1".to_string()],
        };
        let err = write_index(&path, &index).expect_err("rename fails");
        assert!(err.contains("cannot replace the index file"), "{err}");
        let leftover_tmp = std::fs::read_dir(path.parent().expect("index dir"))
            .expect("index dir")
            .map(|entry| entry.expect("entry").path())
            .any(|entry| entry.extension().is_some_and(|ext| ext == "tmp"));
        assert!(!leftover_tmp, "a temporary file was left behind");
    }

    #[test]
    fn concurrent_writers_for_one_work_item_lose_no_run() {
        let dir = TempDir::new("osf-index-concurrent");
        std::thread::scope(|scope| {
            for n in 0..32 {
                let dir = &dir;
                scope.spawn(move || {
                    record_run(dir, ITEM, &format!("run-{n}")).expect("record");
                });
            }
        });
        let mut runs = runs_for_work_item(&dir, ITEM).expect("runs");
        assert_eq!(runs.len(), 32);
        runs.sort();
        let mut expected: Vec<String> = (0..32).map(|n| format!("run-{n}")).collect();
        expected.sort();
        assert_eq!(runs, expected);
    }

    #[test]
    fn a_stale_lock_file_is_an_error_naming_the_lock_file() {
        let dir = TempDir::new("osf-index-stale-lock");
        let index = index_path(&dir, ITEM);
        std::fs::create_dir_all(index.parent().expect("index dir")).expect("index dir");
        let name = index
            .file_name()
            .expect("index file name")
            .to_string_lossy();
        let lock = index.with_file_name(format!("{name}.lock"));
        std::fs::write(&lock, "").expect("stale lock");
        let started = Instant::now();
        let err = record_run(&dir, ITEM, "run-1").expect_err("stale lock");
        assert!(err.contains(&lock.display().to_string()), "{err}");
        assert!(err.contains("stale"), "{err}");
        assert!(
            started.elapsed() < LOCK_WAIT + Duration::from_secs(5),
            "{err}"
        );
    }
}
