//! `OSF_MOON` integration tests, kept out of `moon.rs`'s unit tests: a
//! test-process-wide env var could race any other test that calls `moon`.
//! Also holds the real-moon end-to-end checks, since they are slow and
//! belong with the env var tests rather than in the fast unit suite.

use osf::moon::{run, Invocation, Outcome, TaskStatus};
use std::process::Command;

#[test]
fn a_missing_moon_is_could_not_run() {
    unsafe {
        std::env::set_var("OSF_MOON", "does-not-exist-anywhere");
    }
    let root = std::env::temp_dir();
    let out = run(&Invocation {
        root: &root,
        targets: &[":#osf-pre-commit".to_string()],
        files: &[],
        env: &[],
        timeout: None,
    });
    unsafe {
        std::env::remove_var("OSF_MOON");
    }
    assert!(matches!(out, Outcome::CouldNotRun(_)));
}

/// A throwaway workspace with one project, one task tagged
/// `osf-pre-commit`, whose input glob is every Markdown file.
struct MoonWorkspace {
    root: std::path::PathBuf,
}

impl MoonWorkspace {
    fn new(name: &str) -> Self {
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
            "tasks:\n  probe:\n    command: 'echo ok'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-commit]\n    options:\n      runFromWorkspaceRoot: true\n",
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
}

#[test]
fn a_changed_png_file_affects_nothing() {
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
}
