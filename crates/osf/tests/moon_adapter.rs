//! `OSF_MOON` integration tests, kept out of `moon.rs`'s unit tests: a
//! test-process-wide env var could race any other test that calls `moon`.
//! Also holds the real-moon end-to-end checks, since they are slow and
//! belong with the env var tests rather than in the fast unit suite. Every
//! test here goes through `serial`, since `run` always reads `OSF_MOON`
//! even when it does not set it.

use osf::moon::{run, Invocation, Outcome, TaskStatus};
use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Runs `f` while holding the process-wide environment lock, setting
/// `vars` first when any are given. Every test in this file goes through
/// here, because `OSF_MOON` is process-global state and two tests must
/// never touch it at the same time.
///
/// Also clears every inherited `MOON_*` variable for the call: this
/// repository's own checkpoint sets them on the `cargo test` process when
/// its `test` task runs, and moon honours an inherited `MOON_WORKSPACE_ROOT`
/// over the workspace a test builds under a throwaway directory, so a
/// nested `run` here would otherwise act on this repository instead of the
/// fixture.
fn serial<T>(vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
    let guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let moon_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("MOON_"))
        .collect();
    for (k, _) in &moon_vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::remove_var(k) };
    }
    for (k, v) in vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::set_var(k, v) };
    }
    let result = f();
    for (k, _) in vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::remove_var(k) };
    }
    for (k, v) in &moon_vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::set_var(k, v) };
    }
    drop(guard);
    result
}

#[test]
fn a_missing_moon_is_could_not_run() {
    serial(&[("OSF_MOON", "does-not-exist-anywhere")], || {
        let root = std::env::temp_dir();
        let out = run(&Invocation {
            root: &root,
            targets: &[":#osf-pre-commit".to_string()],
            files: &[],
            env: &[],
            timeout: None,
        });
        assert!(matches!(out, Outcome::CouldNotRun(_)));
    });
}

/// A throwaway workspace with one project, one task tagged
/// `osf-pre-commit`, whose input glob is every Markdown file.
struct MoonWorkspace {
    root: std::path::PathBuf,
}

impl MoonWorkspace {
    fn new(name: &str) -> Self {
        Self::with_command(name, "echo ok")
    }

    /// The same workspace, with the probe task running `command` as a
    /// `script` (moon's `command` setting rejects shell syntax such as
    /// `;` or `|`, which the timeout test's sleep-then-write needs).
    fn with_command(name: &str, command: &str) -> Self {
        let root = std::env::temp_dir().join(format!("osf-moon-adapter-test-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".moon")).expect("workspace dir creates");
        std::fs::create_dir_all(root.join("app")).expect("project dir creates");
        std::fs::write(
            root.join(".moon").join("workspace.yml"),
            "projects:\n  app: 'app'\nvcs:\n  client: git\n  defaultBranch: main\n",
        )
        .expect("workspace.yml writes");
        std::fs::write(
            root.join("app").join("moon.yml"),
            format!(
                "tasks:\n  probe:\n    script: '{command}'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-commit]\n    options:\n      runFromWorkspaceRoot: true\n"
            ),
        )
        .expect("moon.yml writes");
        std::fs::write(root.join("app").join("notes.md"), "hello\n").expect("fixture md writes");
        let ws = MoonWorkspace { root };
        ws.git(&["init", "-q", "-b", "main"]);
        ws.git(&["config", "user.email", "test@example.com"]);
        ws.git(&["config", "user.name", "Test"]);
        ws.git(&["add", "-A"]);
        ws.git(&["commit", "-q", "-m", "init"]);
        ws
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
}

impl Drop for MoonWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_changed_markdown_file_runs_the_tagged_task() {
    serial(&[], || {
        let ws = MoonWorkspace::new("md");
        let targets = vec![":#osf-pre-commit".to_string()];
        let files = vec!["app/notes.md".to_string()];
        let out = run(&Invocation {
            root: &ws.root,
            targets: &targets,
            files: &files,
            env: &[],
            timeout: Some(std::time::Duration::from_secs(30)),
        });
        match out {
            Outcome::Ran { tasks, .. } => {
                assert_eq!(tasks.len(), 1, "{tasks:?}");
                assert_eq!(tasks.first().expect("one task").status, TaskStatus::Passed);
            }
            Outcome::NothingAffected => panic!("expected Ran, got NothingAffected"),
            Outcome::TimedOut => panic!("expected Ran, got TimedOut"),
            Outcome::CouldNotRun(e) => panic!("expected Ran, got CouldNotRun({e})"),
        }
    });
}

#[test]
fn a_changed_png_file_affects_nothing() {
    serial(&[], || {
        let ws = MoonWorkspace::new("png");
        let targets = vec![":#osf-pre-commit".to_string()];
        let files = vec!["app/notes.png".to_string()];
        let out = run(&Invocation {
            root: &ws.root,
            targets: &targets,
            files: &files,
            env: &[],
            timeout: Some(std::time::Duration::from_secs(30)),
        });
        assert!(matches!(out, Outcome::NothingAffected));
    });
}

#[test]
fn a_timed_out_run_kills_the_whole_process_tree() {
    serial(&[], || {
        // The task sleeps well past the 2s timeout below, then writes a
        // marker file. If only moon.exe/moon were killed and not the task
        // it spawned, the marker would still appear a few seconds later.
        let command = if cfg!(windows) {
            "Start-Sleep -Seconds 6; New-Item -Path marker.txt -ItemType File | Out-Null"
        } else {
            "sleep 6 && touch marker.txt"
        };
        let ws = MoonWorkspace::with_command("tree-kill", command);
        let targets = vec![":#osf-pre-commit".to_string()];
        let files = vec!["app/notes.md".to_string()];
        let out = run(&Invocation {
            root: &ws.root,
            targets: &targets,
            files: &files,
            env: &[],
            timeout: Some(std::time::Duration::from_secs(2)),
        });
        match out {
            Outcome::TimedOut => {}
            Outcome::Ran { .. } => panic!("expected TimedOut, got Ran"),
            Outcome::NothingAffected => panic!("expected TimedOut, got NothingAffected"),
            Outcome::CouldNotRun(e) => panic!("expected TimedOut, got CouldNotRun({e})"),
        }

        // The sleep would finish around the 6s mark from spawn; wait well
        // past that before checking the marker never showed up.
        std::thread::sleep(std::time::Duration::from_secs(6));
        assert!(
            !ws.root.join("marker.txt").exists(),
            "the sleeping task kept running after TimedOut and wrote its marker file"
        );
    });
}

#[test]
fn a_few_thousand_stdin_paths_do_not_deadlock() {
    serial(&[], || {
        let ws = MoonWorkspace::new("many-files");
        let targets = vec![":#osf-pre-commit".to_string()];
        let files: Vec<String> = (0..5000)
            .map(|i| format!("generated/does-not-exist-{i}.txt"))
            .collect();
        let out = run(&Invocation {
            root: &ws.root,
            targets: &targets,
            files: &files,
            env: &[],
            timeout: Some(std::time::Duration::from_secs(30)),
        });
        match out {
            Outcome::Ran { .. } | Outcome::NothingAffected => {}
            Outcome::TimedOut => panic!("run() timed out writing a large stdin payload"),
            Outcome::CouldNotRun(e) => panic!("run() could not run: {e}"),
        }
    });
}
