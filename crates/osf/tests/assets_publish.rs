//! End-to-end cases for `osf assets publish`, against a local bare
//! repository standing in for GitHub. Nothing here reaches the network.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A local bare repository, removed when it goes out of scope.
struct BareRemote {
    dir: PathBuf,
}

impl BareRemote {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osf-assets-publish-remote-{name}"));
        let _ = fs::remove_dir_all(&dir);
        let output = Command::new("git")
            .args(["init", "--quiet", "--bare"])
            .arg(&dir)
            .output()
            .expect("git init --bare runs");
        assert!(
            output.status.success(),
            "git init --bare failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        BareRemote { dir }
    }

    fn path_str(&self) -> &str {
        self.dir.to_str().expect("a utf-8 path")
    }

    /// Every commit hash on `branch`, oldest first, or empty when the
    /// branch does not exist yet.
    fn commits_on(&self, branch: &str) -> Vec<String> {
        let output = Command::new("git")
            .args(["log", "--format=%H", "--reverse", branch])
            .current_dir(&self.dir)
            .output()
            .expect("git log runs");
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The content of `path` as committed on `branch`.
    fn file_at(&self, branch: &str, path: &str) -> String {
        let output = Command::new("git")
            .args(["show", &format!("{branch}:{path}")])
            .current_dir(&self.dir)
            .output()
            .expect("git show runs");
        assert!(
            output.status.success(),
            "git show {branch}:{path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 content")
    }
}

impl Drop for BareRemote {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A local folder of files to publish, removed when it goes out of scope.
struct SourceDir {
    dir: PathBuf,
}

impl SourceDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osf-assets-publish-source-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("source dir creates");
        SourceDir { dir }
    }

    fn write(&self, name: &str, content: &str) {
        fs::write(self.dir.join(name), content).expect("writes a source file");
    }
}

impl Drop for SourceDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn run_osf(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_osf"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("osf runs")
}

fn assert_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn publishes_to_a_new_orphan_branch_and_prints_the_raw_address() {
    let remote = BareRemote::new("new-branch");
    let source = SourceDir::new("new-branch");
    source.write("diagram.svg", "<svg>one</svg>");

    let output = run_osf(
        &source.dir,
        &[
            "assets",
            "publish",
            "--branch",
            "data",
            "--path",
            "pr/1/abc",
            "--dir",
            source.dir.to_str().expect("utf-8 path"),
            "--repo",
            "test-owner/test-repo",
            "--remote",
            remote.path_str(),
        ],
    );
    assert_ok(&output);
    assert_eq!(
        stdout_of(&output),
        "https://raw.githubusercontent.com/test-owner/test-repo/data/pr/1/abc"
    );

    let commits = remote.commits_on("data");
    assert_eq!(commits.len(), 1, "{commits:?}");
    assert_eq!(
        remote.file_at("data", "pr/1/abc/diagram.svg"),
        "<svg>one</svg>"
    );
}

#[test]
fn publishing_identical_content_again_makes_no_new_commit() {
    let remote = BareRemote::new("no-op");
    let source = SourceDir::new("no-op");
    source.write("diagram.svg", "<svg>same</svg>");
    let args = |remote: &BareRemote| {
        vec![
            "assets".to_string(),
            "publish".to_string(),
            "--branch".to_string(),
            "data".to_string(),
            "--path".to_string(),
            "pr/2/def".to_string(),
            "--dir".to_string(),
            source.dir.to_str().expect("utf-8 path").to_string(),
            "--repo".to_string(),
            "test-owner/test-repo".to_string(),
            "--remote".to_string(),
            remote.path_str().to_string(),
        ]
    };

    let first_args = args(&remote);
    let first_refs: Vec<&str> = first_args.iter().map(String::as_str).collect();
    assert_ok(&run_osf(&source.dir, &first_refs));
    let after_first = remote.commits_on("data");
    assert_eq!(after_first.len(), 1, "{after_first:?}");

    let second_args = args(&remote);
    let second_refs: Vec<&str> = second_args.iter().map(String::as_str).collect();
    let second_output = run_osf(&source.dir, &second_refs);
    assert_ok(&second_output);
    assert_eq!(
        stdout_of(&second_output),
        "https://raw.githubusercontent.com/test-owner/test-repo/data/pr/2/def"
    );

    let after_second = remote.commits_on("data");
    assert_eq!(
        after_second, after_first,
        "an unchanged publish must not add a commit"
    );
}

#[test]
fn publishing_changed_content_adds_a_second_commit() {
    let remote = BareRemote::new("changed");
    let source = SourceDir::new("changed");
    source.write("diagram.svg", "<svg>v1</svg>");

    let publish = |source: &SourceDir, remote: &BareRemote| {
        let args = [
            "assets",
            "publish",
            "--branch",
            "data",
            "--path",
            "pr/3/ghi",
            "--dir",
            source.dir.to_str().expect("utf-8 path"),
            "--repo",
            "test-owner/test-repo",
            "--remote",
            remote.path_str(),
        ];
        run_osf(&source.dir, &args)
    };

    assert_ok(&publish(&source, &remote));
    assert_eq!(remote.commits_on("data").len(), 1);

    source.write("diagram.svg", "<svg>v2</svg>");
    assert_ok(&publish(&source, &remote));

    let commits = remote.commits_on("data");
    assert_eq!(commits.len(), 2, "{commits:?}");
    assert_eq!(
        remote.file_at("data", "pr/3/ghi/diagram.svg"),
        "<svg>v2</svg>"
    );
}
