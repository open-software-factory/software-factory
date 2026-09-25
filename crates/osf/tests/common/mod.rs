//! Shared test plumbing: a throwaway git repository for `scan` and
//! `verify` integration tests, so neither reimplements it.
//! Each integration test binary compiles this module on its own; a helper
//! only one of them calls is not dead code, just unused in this one.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder name unique to this process and this call, so two processes (or
/// two calls in one process) building a directory from the same `name` never
/// share one.
pub fn unique_dir(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let base = std::env::temp_dir(); // osf: temp-dir allowed, the shared helper
    base.join(format!("{prefix}-{}-{unique}", std::process::id()))
}

/// A fresh, empty directory unique to this process and this call, for a
/// throwaway fixture with no git repository of its own. Removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> Self {
        let dir = unique_dir(prefix);
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        TempDir(dir)
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A throwaway git repository under the system temp directory, named
/// uniquely so parallel tests never collide. Removed on drop.
pub struct TempRepo {
    pub dir: PathBuf,
}

impl TempRepo {
    pub fn new(name: &str) -> Self {
        let dir = unique_dir(&format!("osf-verify-test-{name}"));
        std::fs::create_dir_all(&dir).expect("temp repo dir creates");
        let repo = TempRepo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let mut command = Command::new("git");
        command.current_dir(&self.dir).args(args);
        osf::scrub_git_env(&mut command);
        let output = command.output().expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub fn write(&self, path: &str, content: &str) {
        let full = self.dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir creates");
        }
        std::fs::write(&full, content).expect("fixture file writes");
    }

    pub fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Stages a path with `git add`, without committing it.
    pub fn stage(&self, path: &str) {
        self.git(&["add", path]);
    }

    /// Points a remote-tracking `origin/main` at the current `HEAD`, the
    /// way a fresh clone of a real remote would already have one, without
    /// this repository needing an actual remote to fetch from.
    pub fn track_origin_main(&self) {
        self.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    }

    /// Writes `count` numbered lines to `path`, each newline-terminated,
    /// creating any parent directory it needs.
    pub fn write_lines(&self, path: &str, count: usize) {
        use std::fmt::Write as _;
        let mut content = String::new();
        for n in 1..=count {
            writeln!(content, "line {n}").expect("writing to a string never fails");
        }
        self.write(path, &content);
    }

    /// Writes raw bytes to `path`, creating any parent directory it needs.
    pub fn write_bytes(&self, path: &str, content: &[u8]) {
        let full = self.dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir creates");
        }
        std::fs::write(&full, content).expect("fixture file writes");
    }

    /// Writes `.moon/workspace.yml` and a small `.osf/moon.yml` with one
    /// task per checkpoint each of `lint-writing` and `scan` serves:
    /// `lint-writing-hook` (`osf-hook`), `lint-writing-pre-push`
    /// (`osf-pre-push`), `scan-pre-commit` (`osf-pre-commit`), `scan-hook`
    /// (`osf-hook`), and `scan-pre-push` (`osf-pre-push`). Each passes its
    /// own `--checkpoint` value. All read only Markdown
    /// (`inputs: ['/**/*.md']`), so a change to a non-Markdown file affects
    /// none of them. Each task's command is the built `osf` binary under
    /// test. The caller commits these files itself, along with whatever
    /// else the test needs in that first commit.
    pub fn with_moon_workspace(name: &str) -> Self {
        let repo = TempRepo::new(name);
        repo.write(
            ".moon/workspace.yml",
            "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
        );
        // `shell: false` runs the binary directly. Without it, moon's
        // default shell on Windows is PowerShell, which cannot parse a
        // quoted path followed by bare arguments on one line.
        let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
        repo.write(
            ".osf/moon.yml",
            &format!(
                "language: rust\ntasks:\n  \
                lint-writing-hook:\n    command: '\"{bin}\" check lint-writing --checkpoint hook --sarif-out .osf/out/lint-writing-hook.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-hook]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  \
                lint-writing-pre-push:\n    command: '\"{bin}\" check lint-writing --checkpoint pre-push --sarif-out .osf/out/lint-writing-pre-push.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  \
                scan-pre-commit:\n    command: '\"{bin}\" check scan --checkpoint pre-commit --sarif-out .osf/out/scan-pre-commit.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-commit]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  \
                scan-hook:\n    command: '\"{bin}\" check scan --checkpoint hook --sarif-out .osf/out/scan-hook.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-hook]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  \
                scan-pre-push:\n    command: '\"{bin}\" check scan --checkpoint pre-push --sarif-out .osf/out/scan-pre-push.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
            ),
        );
        repo
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A throwaway bare repository: a git repository with no work tree. Removed on drop.
pub struct BareRepo {
    pub dir: PathBuf,
}

impl BareRepo {
    pub fn new(name: &str) -> Self {
        let dir = unique_dir(&format!("osf-verify-test-{name}"));
        let mut command = Command::new("git");
        command.args(["init", "-q", "--bare"]).arg(&dir);
        osf::scrub_git_env(&mut command);
        let status = command.status().expect("git init runs");
        assert!(
            status.success(),
            "bare git init failed for {}",
            dir.display()
        );
        BareRepo { dir }
    }
}

impl Drop for BareRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A fake `moon` for `OSF_MOON`: it copies [`write_fake_moon_report`]'s file into `.moon/cache/runReport.json` for `run`.
#[cfg(windows)]
pub fn write_fake_moon(repo: &TempRepo) -> PathBuf {
    let path = repo.dir.join("fake-moon.cmd");
    let script = "@echo off\r\n\
        if \"%~1\"==\"--version\" (\r\n  echo moon 2.5.5\r\n  exit /b 0\r\n)\r\n\
        if \"%~1\"==\"query\" (\r\n  echo {\"tasks\":{}}\r\n  exit /b 0\r\n)\r\n\
        if \"%~1\"==\"run\" (\r\n  more > nul\r\n  if not exist \".moon\\cache\" mkdir \".moon\\cache\"\r\n  copy /y \"fake-moon-report.json\" \".moon\\cache\\runReport.json\" > nul\r\n  exit /b 0\r\n)\r\n\
        exit /b 1\r\n";
    std::fs::write(&path, script).expect("fake moon script writes");
    path
}

/// The same as the Windows [`write_fake_moon`], as a POSIX shell script.
#[cfg(unix)]
pub fn write_fake_moon(repo: &TempRepo) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = repo.dir.join("fake-moon.sh");
    let script = "#!/bin/sh\ncase \"$1\" in\n  \
        --version) echo 'moon 2.5.5'; exit 0 ;;\n  \
        query) echo '{\"tasks\":{}}'; exit 0 ;;\n  \
        run) cat > /dev/null; mkdir -p .moon/cache; cp fake-moon-report.json .moon/cache/runReport.json; exit 0 ;;\n  \
        *) exit 1 ;;\nesac\n";
    std::fs::write(&path, script).expect("fake moon script writes");
    let mut perms = std::fs::metadata(&path)
        .expect("fake moon metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("fake moon chmod");
    path
}

/// Writes the report `write_fake_moon`'s `run` branch copies, from a raw `targetStates` body.
pub fn write_fake_moon_report(repo: &TempRepo, target_states: &str) {
    repo.write(
        "fake-moon-report.json",
        &format!(r#"{{"actions":[],"context":{{"targetStates":{{{target_states}}}}}}}"#),
    );
}

/// A fresh, empty directory standing in for `HOME`, named uniquely so
/// parallel tests never collide. Removed on drop.
pub struct IsolatedHome(PathBuf);

impl std::ops::Deref for IsolatedHome {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fresh, empty directory to stand in for `HOME`, so a spawned `osf`
/// never picks up this machine's real `~/.osf/config.toml`. Also holds an
/// `AppData\Roaming` folder on Windows: moon's WASM plugin runtime derives
/// its own cache config path from `USERPROFILE`, and fails to start at all
/// when that folder is missing under an overridden profile.
pub fn isolated_home(name: &str) -> IsolatedHome {
    let dir = unique_dir(&format!("osf-verify-test-home-{name}"));
    std::fs::create_dir_all(&dir).expect("isolated home dir creates");
    std::fs::create_dir_all(dir.join("AppData").join("Roaming"))
        .expect("isolated home AppData\\Roaming creates");
    std::fs::create_dir_all(dir.join("AppData").join("Local"))
        .expect("isolated home AppData\\Local creates");
    IsolatedHome(dir)
}

/// Runs the compiled `osf` binary in `dir`, with `home` standing in for
/// `HOME`/`USERPROFILE` so it never reads this machine's real config.
pub fn run_osf(
    dir: &std::path::Path,
    home: &std::path::Path,
    args: &[&str],
) -> std::process::Output {
    run_osf_with_env(dir, home, &[], args)
}

/// The `osf` command, in `dir`, with `home` standing in for
/// `HOME`/`USERPROFILE`, `env` applied on top, and every checkpoint-runner,
/// `MOON_*` and `GIT_*` variable this repository's own checkpoint (or a
/// caller's own git hook) sets on `cargo test` scrubbed first (see
/// [`run_osf_with_env`]'s doc comment).
fn osf_cmd(dir: &std::path::Path, home: &std::path::Path, env: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PROTO_HOME", home.join(".proto"));
    for (key, _) in std::env::vars() {
        if key.starts_with("OSF_") || key.starts_with("MOON_") || key.starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd
}

/// The same as [`run_osf`], with extra environment variables set for this
/// one run, such as `OSF_FILES_FROM` or `OSF_BASE`.
///
/// Removes every inherited `OSF_*` variable from the ambient environment
/// first: this repository's own checkpoint sets some of them (`OSF_CONFIG`,
/// `OSF_DENYLIST`, `OSF_STATE_DIR`, and more) on the `moon` process, which
/// every task it spawns inherits, `cargo test` included, so a test binary
/// that never clears them reads its own checkpoint run's config, denylist,
/// or state directory instead of the caller's `home`/`env`.
///
/// Also removes every inherited `MOON_*` variable: moon sets these on a
/// task's own process (`MOON_WORKSPACE_ROOT`, `MOON_CACHE_DIR`, and the
/// rest), and a test spawning `osf` under this repository's own `cargo
/// test` task inherits them. Left in place, a test's own nested `moon`
/// call reads them back and treats itself as still inside this
/// repository's workspace instead of the throwaway one at `dir`.
pub fn run_osf_with_env(
    dir: &std::path::Path,
    home: &std::path::Path,
    env: &[(&str, &str)],
    args: &[&str],
) -> std::process::Output {
    osf_cmd(dir, home, env)
        .args(args)
        .output()
        .expect("osf runs")
}

/// The same as [`run_osf_with_env`], writing `stdin` to the child's
/// standard input instead of leaving it closed, for a hook command that
/// reads its event from there.
pub fn run_osf_stdin(
    dir: &std::path::Path,
    home: &std::path::Path,
    env: &[(&str, &str)],
    args: &[&str],
    stdin: &str,
) -> std::process::Output {
    use std::io::Write as _;
    let mut child = osf_cmd(dir, home, env)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("osf spawns");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("stdin writes");
    child.wait_with_output().expect("osf runs")
}

/// Spawns the compiled `osf` binary in `dir` without waiting for it, wiring
/// its stdin, stdout and stderr as pipes. For a test that needs two runs to
/// genuinely overlap in wall-clock time, pair this with [`write_stdin`]
/// rather than [`run_osf_stdin`], which blocks until the child exits before
/// the caller can even start a second one.
pub fn spawn_osf_stdin(
    dir: &std::path::Path,
    home: &std::path::Path,
    env: &[(&str, &str)],
    args: &[&str],
) -> std::process::Child {
    osf_cmd(dir, home, env)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("osf spawns")
}

/// Writes `stdin` to `child`'s own standard input and closes it, without
/// waiting for the process to exit, so a caller can feed several
/// already-spawned children before waiting on any of them.
pub fn write_stdin(child: &mut std::process::Child, stdin: &str) {
    use std::io::Write as _;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("stdin writes");
}

/// Builds a session-link-shaped string at run time for one agent, from the
/// host and path the agent list holds apart. A scan rule fixture needs the
/// real shape to prove the rule fires, but this repository's own `osf scan`
/// run has no exclusion for a test file, so the shape must never sit in
/// any source text as one contiguous literal.
pub fn session_link_for(host: &str, path: &str, id: &str) -> String {
    format!("https://{host}{path}{id}")
}

/// A session link for the first agent in the list that has hosted
/// sessions, for tests that need any one link.
pub fn session_link(id: &str) -> String {
    let (host, path) = osf::agents::AGENTS
        .iter()
        .find_map(osf::agents::Agent::hosted)
        .expect("at least one agent has hosted sessions");
    session_link_for(host, path, id)
}

/// A path into one agent's state directory, at one of the places that
/// agent keeps its sessions.
pub fn agent_state_path(state_dir: &str, session_path: &str) -> String {
    let home = "~";
    format!("{home}/{state_dir}/{session_path}/2026-09-17-abc.jsonl")
}

/// A scan configuration that states the repository is public, so a test
/// never depends on a network lookup of the real visibility.
pub fn public_config() -> osf::config::ScanConfig {
    osf::config::ScanConfig {
        repository_visibility: "public".to_string(),
        ..osf::config::ScanConfig::default()
    }
}

/// A throwaway repository with an `origin` remote under `owner`, and the
/// configuration that calls it public.
pub fn public_repo(name: &str, owner: &str) -> (TempRepo, osf::config::ScanConfig) {
    let repo = TempRepo::new(name);
    repo.git(&[
        "remote",
        "add",
        "origin",
        &format!("https://github.com/{owner}/tools.git"),
    ]);
    (repo, public_config())
}

/// A `file:` address for a path, built at run time for the same reason as
/// [`session_link_for`].
pub fn file_url(path: &str) -> String {
    let scheme = "file://";
    if path.as_bytes().get(1) == Some(&b':') {
        format!("{scheme}/{}", path.replace('\\', "/"))
    } else {
        format!("{scheme}{path}")
    }
}

/// Builds a Windows user-path-shaped string at run time, for the same
/// reason as [`session_link_for`].
pub fn windows_user_path(user: &str) -> String {
    let drive = "D:";
    let marker = r"\Users\";
    format!(r"{drive}{marker}{user}\work\notes.md")
}

/// The same Windows path written with forward slashes, as a shell or a
/// hook file often spells it.
pub fn windows_forward_path(user: &str) -> String {
    let drive = "C:";
    let marker = "/Users";
    format!("{drive}{marker}/{user}/work/notes.md")
}

/// A Windows network share path.
pub fn network_share_path(server: &str, share: &str) -> String {
    let sep = "\\";
    format!("{sep}{sep}{server}{sep}{share}{sep}notes.md")
}

/// Builds a Linux home-directory-path-shaped string at run time, for the
/// same reason as [`session_link_for`].
pub fn home_path(user: &str) -> String {
    let prefix = "/home";
    format!("{prefix}/{user}/work/notes.md")
}

/// A macOS home-directory path.
pub fn mac_home_path(user: &str) -> String {
    let prefix = "/Users";
    format!("{prefix}/{user}/work/notes.md")
}

/// The root account's home on Linux.
pub fn root_home_path() -> String {
    format!("/{}/work/notes.md", "root")
}

/// A Windows user folder seen through the Windows Subsystem for Linux.
pub fn wsl_user_path(user: &str) -> String {
    let mount = "/mnt/c";
    let users = "Users";
    format!("{mount}/{users}/{user}/work/notes.md")
}

/// Builds a co-author trailer line at run time, for the same reason as
/// [`session_link`].
pub fn coauthor_trailer(name: &str, email: &str) -> String {
    let label = ["Co", "Authored", "By"].join("-");
    format!("{label}: {name} <{email}>")
}

/// Builds a foreign cross-repository reference at run time, for the same
/// reason as [`session_link`]: once `osf.toml` sets a real project owner,
/// any other owner named next to a `#<digits>` reference reads as foreign.
pub fn foreign_reference(owner: &str, repo: &str, number: u32) -> String {
    format!("{owner}/{repo}#{number}")
}
