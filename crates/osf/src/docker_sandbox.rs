//! The Docker [`Sandbox`] adapter: one `docker` invocation per operation,
//! with every argv and every response built and read by a pure function.

pub use crate::sandbox::validate::{image_is_pinned, validate_command, validate_spec};
use crate::sandbox::{
    Capability, CommandSpec, Destroyed, Mount, Network, RunOutcome, RunResult, Sandbox,
    SandboxCapabilities, SandboxError, SandboxId, SandboxSpec,
};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt as _;

/// The command a created container runs until it is removed.
const KEEPALIVE_PROGRAM: &str = "sleep";
/// The argument of the keep-alive command: the largest 32-bit signed integer.
const KEEPALIVE_SECS: &str = "2147483647";

/// How long `create` waits for the image and the container.
const CREATE_TIMEOUT: Duration = Duration::from_secs(120);

/// The timeout one command gets when it sets none: 30 minutes.
pub const DEFAULT_TIMEOUT_SECS: u64 = 1800;

/// `docker exec` inherits the container's environment; this fixes PATH, HOME and LANG, but Docker cannot unset other variables the image sets with ENV, so the image must hold no secret; the engine passes anything else per command.
const SANDBOX_ENV: [(&str, &str); 3] = [
    (
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ),
    ("HOME", "/tmp"),
    ("LANG", "C.UTF-8"),
];

/// The default per-stream output cap in bytes: 1 MiB.
pub const DEFAULT_OUTPUT_CAP_BYTES: u64 = 1_048_576;

/// What one runner invocation produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// The exit status, or none when a signal or the timeout ended the program.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// True when the runner killed the program at its own timeout.
    pub timed_out: bool,
    /// The per-stream cap in bytes the runner applied.
    pub output_cap_bytes: Option<u64>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// The runner could not start the program at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerError(pub String);

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RunnerError {}

/// Runs one `docker` invocation.
pub trait DockerRunner: Send + Sync {
    /// Runs `argv`, writing `stdin` to the program's standard input then closing the pipe; with none, standard input is null, and waits at most `timeout`. `env` is applied to the child process only, so its values never reach the argv.
    ///
    /// # Errors
    /// Returns a [`RunnerError`] when the program cannot start or a reader panics.
    fn run_with_stdin(
        &self,
        argv: &[String],
        timeout: Option<Duration>,
        stdin: Option<&[u8]>,
        env: &BTreeMap<String, String>,
    ) -> Result<ToolOutput, RunnerError>;

    /// Runs `argv` with no standard input and no extra environment, waiting at most `timeout`.
    ///
    /// # Errors
    /// Returns a [`RunnerError`] when the program cannot start or a reader panics.
    fn run(&self, argv: &[String], timeout: Option<Duration>) -> Result<ToolOutput, RunnerError> {
        self.run_with_stdin(argv, timeout, None, &BTreeMap::new())
    }
}

/// The [`DockerRunner`] over the real `docker` program.
pub struct RealDockerRunner {
    program: PathBuf,
    output_cap_bytes: u64,
}

impl RealDockerRunner {
    /// A runner that spawns `program`.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            output_cap_bytes: DEFAULT_OUTPUT_CAP_BYTES,
        }
    }

    /// Sets the per-stream output cap in bytes.
    #[must_use]
    pub fn with_output_cap(mut self, cap_bytes: u64) -> Self {
        self.output_cap_bytes = cap_bytes;
        self
    }
}

impl Default for RealDockerRunner {
    fn default() -> Self {
        Self::new("docker")
    }
}

impl DockerRunner for RealDockerRunner {
    // One branch per wait outcome, plus the standard-input writer.
    #[allow(clippy::too_many_lines)]
    fn run_with_stdin(
        &self,
        argv: &[String],
        timeout: Option<Duration>,
        stdin: Option<&[u8]>,
        env: &BTreeMap<String, String>,
    ) -> Result<ToolOutput, RunnerError> {
        let mut command = Command::new(&self.program);
        command
            .args(argv)
            .envs(env)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A timeout must stop the whole tree, or a grandchild can keep a pipe open.
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| RunnerError(format!("cannot run docker: {error}")))?;

        // The deadline starts at the spawn and covers the wait and the readers.
        let deadline = timeout.map(|limit| Instant::now() + limit);

        // A large payload is written on its own thread so it never blocks the readers or the wait; a write error is ignored.
        let stdin_writer = stdin.and_then(|bytes| {
            let bytes = bytes.to_vec();
            child.stdin.take().map(|mut pipe| {
                std::thread::spawn(move || {
                    let _ = std::io::Write::write_all(&mut pipe, &bytes);
                })
            })
        });

        // Both pipes are read on their own threads before the wait: a run that
        // fills one pipe buffer would otherwise block. Each reader reports over
        // a channel, so the caller never joins a pipe a grandchild holds open.
        let cap = self.output_cap_bytes;
        let (sender, receiver) = std::sync::mpsc::channel();
        let stdout_reader = child.stdout.take().map(|mut pipe| {
            let sender = sender.clone();
            std::thread::spawn(move || {
                let _ = sender.send((Stream::Stdout, read_capped(&mut pipe, cap)));
            })
        });
        let stderr_reader = child.stderr.take().map(|mut pipe| {
            let sender = sender.clone();
            std::thread::spawn(move || {
                let _ = sender.send((Stream::Stderr, read_capped(&mut pipe, cap)));
            })
        });
        drop(sender);

        let waited = match timeout {
            Some(limit) => child.wait_timeout(limit),
            None => child.wait().map(Some),
        };

        let expected = usize::from(stdout_reader.is_some()) + usize::from(stderr_reader.is_some());
        let mut collected = Collected::default();
        collect_readers(&mut collected, &receiver, expected, deadline);

        match waited {
            Ok(Some(status)) if collected.reported == expected => {
                join_writer(stdin_writer);
                Ok(ToolOutput {
                    status: status.code(),
                    stdout: collected.stdout,
                    stderr: collected.stderr,
                    timed_out: false,
                    output_cap_bytes: Some(self.output_cap_bytes),
                    stdout_truncated: collected.stdout_truncated,
                    stderr_truncated: collected.stderr_truncated,
                })
            }
            Ok(Some(_status)) if timeout.is_none() => {
                // With no deadline the channel disconnects only when a reader
                // ended without reporting: it panicked.
                join_writer(stdin_writer);
                Err(RunnerError("a reader thread panicked".to_string()))
            }
            Ok(Some(_status)) => {
                // The client exited, but a reader never reached its pipe's end:
                // a child that kept the pipe open holds the wall clock. Killing
                // is best effort here, since the client is already reaped.
                let _ = kill_tree(&child);
                let _ = child.kill();
                let _ = child.wait();
                let grace = Some(Instant::now() + KILL_GRACE);
                collect_readers(&mut collected, &receiver, expected, grace);
                Ok(ToolOutput {
                    status: None,
                    stdout: collected.stdout,
                    stderr: collected.stderr,
                    timed_out: true,
                    output_cap_bytes: Some(self.output_cap_bytes),
                    stdout_truncated: collected.stdout_truncated,
                    stderr_truncated: collected.stderr_truncated,
                })
            }
            Ok(None) => {
                let killed = kill_tree(&child);
                let _ = child.kill();
                let _ = child.wait();
                if let Err(error) = killed {
                    return Err(RunnerError(kill_failure(&self.program, &error)));
                }
                let grace = Some(Instant::now() + KILL_GRACE);
                collect_readers(&mut collected, &receiver, expected, grace);
                Ok(ToolOutput {
                    status: None,
                    stdout: collected.stdout,
                    stderr: collected.stderr,
                    timed_out: true,
                    output_cap_bytes: Some(self.output_cap_bytes),
                    stdout_truncated: collected.stdout_truncated,
                    stderr_truncated: collected.stderr_truncated,
                })
            }
            Err(error) => {
                let killed = kill_tree(&child);
                let _ = child.kill();
                let _ = child.wait();
                let mut text = format!("cannot run docker: {error}");
                if let Err(kill_error) = killed {
                    text = format!(
                        "{text}; could not be killed ({}): {kill_error}",
                        self.program.to_string_lossy()
                    );
                }
                Err(RunnerError(text))
            }
        }
    }
}

/// Joins the standard-input writer thread, if any.
fn join_writer(handle: Option<std::thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

/// The text for a tree kill that failed: the program and the kill error.
fn kill_failure(program: &Path, error: &str) -> String {
    format!(
        "docker timed out and could not be killed ({}): {error}",
        program.to_string_lossy()
    )
}

/// Which standard stream one reader thread covers.
#[derive(Debug, Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// How long the readers get to drain their pipes after a kill.
const KILL_GRACE: Duration = Duration::from_secs(2);

/// What both reader threads reported within the deadline.
#[derive(Default)]
struct Collected {
    stdout: String,
    stdout_truncated: bool,
    stderr: String,
    stderr_truncated: bool,
    /// How many readers reported before the deadline or disconnection.
    reported: usize,
}

/// Adds reader results to `collected` until every reader reported or `deadline`
/// passes; a silent reader leaves its stream empty.
fn collect_readers(
    collected: &mut Collected,
    receiver: &std::sync::mpsc::Receiver<(Stream, (String, bool))>,
    expected: usize,
    deadline: Option<Instant>,
) {
    while collected.reported < expected {
        let message = match deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                receiver.recv_timeout(remaining).ok()
            }
            None => receiver.recv().ok(),
        };
        let Some((stream, (text, truncated))) = message else {
            break;
        };
        match stream {
            Stream::Stdout => {
                collected.stdout = text;
                collected.stdout_truncated = truncated;
            }
            Stream::Stderr => {
                collected.stderr = text;
                collected.stderr_truncated = truncated;
            }
        }
        collected.reported += 1;
    }
}

/// Reads a pipe to its end, keeping at most `cap` bytes plus a marker: a
/// stream longer than `cap` keeps its first half and its last half, with a marker between.
fn read_capped(pipe: &mut impl std::io::Read, cap: u64) -> (String, bool) {
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    let head_len = cap / 2;
    let tail_len = cap - head_len;
    let mut head: Vec<u8> = Vec::new();
    let mut tail: VecDeque<u8> = VecDeque::new();
    let mut total = 0usize;
    let mut chunk = [0u8; 8192];
    loop {
        match std::io::Read::read(pipe, &mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                total = total.saturating_add(read);
                let mut rest = chunk.get(..read).unwrap_or_default();
                if head.len() < head_len {
                    let room = head_len - head.len();
                    let take = room.min(rest.len());
                    head.extend_from_slice(rest.get(..take).unwrap_or_default());
                    rest = rest.get(take..).unwrap_or_default();
                }
                tail.extend(rest.iter().copied());
                while tail.len() > tail_len {
                    tail.pop_front();
                }
            }
        }
    }
    if total <= cap {
        let mut whole = head;
        whole.extend(tail.iter().copied());
        return (String::from_utf8_lossy(&whole).into_owned(), false);
    }
    trim_head(&mut head);
    trim_tail(&mut tail);
    let omitted = total.saturating_sub(head.len()).saturating_sub(tail.len());
    let marker = format!("\n[... {omitted} bytes omitted ...]\n");
    let mut text = String::from_utf8_lossy(&head).into_owned();
    text.push_str(&marker);
    text.push_str(&String::from_utf8_lossy(tail.make_contiguous()));
    (text, true)
}

/// Drops an incomplete UTF-8 sequence at the end of `head`.
fn trim_head(head: &mut Vec<u8>) {
    if let Err(error) = std::str::from_utf8(head) {
        if error.error_len().is_none() {
            head.truncate(error.valid_up_to());
        }
    }
}

/// Drops UTF-8 continuation bytes at the start of `tail`.
fn trim_tail(tail: &mut VecDeque<u8>) {
    while tail
        .front()
        .copied()
        .is_some_and(|byte| (byte & 0b1100_0000) == 0b1000_0000)
    {
        tail.pop_front();
    }
}

/// Kills `child` and every process it spawned: `taskkill /T` on Windows,
/// `kill -KILL` on the whole process group on Unix. A failure carries the tool's text.
fn kill_tree(child: &Child) -> Result<(), String> {
    let pid = child.id();
    #[cfg(windows)]
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    #[cfg(unix)]
    let output = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// The `docker create` argv for `spec`, without the program name.
///
/// No `--rm`: a measured kill then `rm --force` fails with "removal of container ... is already in progress".
#[must_use]
pub fn create_argv(spec: &SandboxSpec) -> Vec<String> {
    let mut argv = vec![
        "create".to_string(),
        format!("--name={}", spec.name),
        format!("--label=osf.sandbox={}", spec.name),
    ];
    for (key, value) in SANDBOX_ENV {
        argv.push(format!("--env={key}={value}"));
    }
    argv.extend([
        "--cap-drop".to_string(),
        "ALL".to_string(),
        "--security-opt".to_string(),
        "no-new-privileges".to_string(),
        "--init".to_string(),
        "--pids-limit".to_string(),
        spec.limits.max_processes.to_string(),
        "--memory".to_string(),
        spec.limits.memory.clone(),
        format!("--user={}", spec.user),
        format!("--workdir={}", spec.workdir),
        "--network".to_string(),
        network_name(&spec.network).to_string(),
    ]);
    for mount in &spec.mounts {
        argv.push(format!("--mount={}", mount_spec(mount)));
    }
    argv.push("--".to_string());
    argv.push(spec.image.clone());
    argv.push(KEEPALIVE_PROGRAM.to_string());
    argv.push(KEEPALIVE_SECS.to_string());
    argv
}

/// The id in `stdout`: one trailing newline is removed, then 64 lowercase hex characters.
#[must_use]
pub fn parse_container_id(stdout: &str) -> Option<&str> {
    let candidate = stdout
        .strip_suffix("\r\n")
        .or_else(|| stdout.strip_suffix('\n'))
        .unwrap_or(stdout);
    is_container_id(candidate).then_some(candidate)
}

/// Whether `text` is exactly 64 lowercase hexadecimal characters.
fn is_container_id(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Trims `text`, cuts it to 200 characters, then debug-formats it.
fn shown_text(text: &str) -> String {
    let trimmed = text.trim();
    let mut shown: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        shown.push_str("...");
    }
    format!("{shown:?}")
}

/// The uid from a decimal-digit answer, or none when it is not one.
fn parse_uid(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok()
}

/// The network a sandbox gets: no driver for [`Network::Isolated`], the
/// default bridge for [`Network::Open`].
fn network_name(network: &Network) -> &'static str {
    match network {
        Network::Isolated => "none",
        Network::Open => "bridge",
    }
}

/// One mount's `--mount` value.
fn mount_spec(mount: &Mount) -> String {
    let mut text = format!(
        "type=bind,source={},target={}",
        mount.host_path, mount.sandbox_path
    );
    if mount.read_only {
        text.push_str(",readonly");
    }
    text
}

/// The `docker start` argv for `id`, without the program name.
#[must_use]
pub fn start_argv(id: &str) -> Vec<String> {
    vec!["start".to_string(), "--".to_string(), id.to_string()]
}

/// The `docker exec` argv for `command` in `id`, without the program name.
///
/// Each environment entry contributes its name only; the values travel in the
/// environment of the client process, never in this argv.
#[must_use]
pub fn exec_argv(id: &str, command: &CommandSpec) -> Vec<String> {
    let mut argv = vec!["exec".to_string()];
    if let Some(workdir) = &command.workdir {
        argv.push(format!("--workdir={workdir}"));
    }
    for key in command.env.keys() {
        argv.push("--env".to_string());
        argv.push(key.clone());
    }
    if command.stdin.is_some() {
        argv.push("-i".to_string());
    }
    argv.push("--".to_string());
    argv.push(id.to_string());
    argv.push(command.program.clone());
    argv.extend(command.args.iter().cloned());
    argv
}

/// The `docker kill` argv for `id`, without the program name.
#[must_use]
pub fn kill_argv(id: &str) -> Vec<String> {
    vec!["kill".to_string(), "--".to_string(), id.to_string()]
}

/// The `docker rm --force` argv for `id`, without the program name.
#[must_use]
pub fn remove_argv(id: &str) -> Vec<String> {
    vec![
        "rm".to_string(),
        "--force".to_string(),
        "--".to_string(),
        id.to_string(),
    ]
}

/// The `docker inspect` argv that asks whether the container `id` exists.
#[must_use]
pub fn inspect_argv(id: &str) -> Vec<String> {
    vec![
        "inspect".to_string(),
        "--type".to_string(),
        "container".to_string(),
        "--format".to_string(),
        "{{.Id}}".to_string(),
        "--".to_string(),
        id.to_string(),
    ]
}

/// The `docker version` argv, without the program name.
#[must_use]
pub fn version_argv() -> Vec<String> {
    vec![
        "version".to_string(),
        "--format".to_string(),
        "{{.Server.Version}}".to_string(),
    ]
}

/// The `docker image inspect` argv for `image`, without the program name.
#[must_use]
pub fn image_inspect_argv(image: &str) -> Vec<String> {
    vec![
        "image".to_string(),
        "inspect".to_string(),
        "--format".to_string(),
        "{{.Id}}".to_string(),
        "--".to_string(),
        image.to_string(),
    ]
}

/// The `docker info` argv that reads the network plugins, without the program name.
#[must_use]
pub fn network_probe_argv() -> Vec<String> {
    vec![
        "info".to_string(),
        "--format".to_string(),
        "{{json .Plugins.Network}}".to_string(),
    ]
}

/// The index of the repository mount: the one whose sandbox path is the workdir.
fn repository_mount(spec: &SandboxSpec) -> Option<usize> {
    spec.mounts
        .iter()
        .position(|mount| mount.sandbox_path == spec.workdir)
}

/// The host paths no mount source may be, or lie under.
const FORBIDDEN_MOUNT_PATHS: [&str; 3] = ["/run", "/var/run", "/proc"];

/// Refuses a mount source that is, or lies under, one of
/// [`FORBIDDEN_MOUNT_PATHS`], testing both the path as given and its
/// canonicalized form so a symlink is caught by its real path.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] naming the host path.
fn check_mount_not_system_path(
    given: &Path,
    real: &Path,
    host_path: &str,
) -> Result<(), SandboxError> {
    let forbidden = FORBIDDEN_MOUNT_PATHS
        .iter()
        .copied()
        .any(|prefix| given.starts_with(prefix) || real.starts_with(prefix));
    if forbidden {
        return Err(SandboxError::Rejected(format!(
            "the mount source must not be, or lie under, /run, /var/run or /proc: {host_path}"
        )));
    }
    Ok(())
}

/// Refuses a mount source that is a Unix socket or holds one at its top
/// level; the check reads one level only, so a socket deeper is accepted.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] naming the host path, and fails closed
/// when the source cannot be read.
#[cfg(unix)]
fn check_mount_has_no_socket(real: &Path, host_path: &str) -> Result<(), SandboxError> {
    use std::os::unix::fs::FileTypeExt as _;
    let cannot_read = |error: std::io::Error| {
        SandboxError::Rejected(format!(
            "the mount source cannot be read: {host_path}: {error}"
        ))
    };
    let metadata = std::fs::metadata(real).map_err(cannot_read)?;
    if metadata.file_type().is_socket() {
        return Err(SandboxError::Rejected(format!(
            "the mount source is a socket: {host_path}"
        )));
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(real).map_err(cannot_read)? {
        let entry = entry.map_err(cannot_read)?;
        if entry.file_type().map_err(cannot_read)?.is_socket() {
            return Err(SandboxError::Rejected(format!(
                "the mount source holds a socket at its top level: {host_path}"
            )));
        }
    }
    Ok(())
}

/// The non-Unix stub: sockets do not exist there, so the check passes.
///
/// # Errors
/// Never.
#[cfg(not(unix))]
fn check_mount_has_no_socket(_real: &Path, _host_path: &str) -> Result<(), SandboxError> {
    Ok(())
}

/// Canonicalizes every mount source, refusing a missing source, a symlink
/// unless `follow_symlinks` is set, the host root, a source that is or lies
/// under `/run`, `/var/run` or `/proc`, a source that is or holds a top-level
/// socket, and a source that is, or holds, the repository mount's own real
/// source.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] naming the first mount that fails.
pub fn resolve_mounts(spec: &SandboxSpec) -> Result<SandboxSpec, SandboxError> {
    let mut resolved = spec.clone();
    let mut real_paths: Vec<PathBuf> = Vec::with_capacity(spec.mounts.len());
    for mount in &spec.mounts {
        let host = Path::new(&mount.host_path);
        if std::fs::symlink_metadata(host).is_err() {
            return Err(SandboxError::Rejected(format!(
                "the mount source does not exist: {}",
                mount.host_path
            )));
        }
        let real = std::fs::canonicalize(host).map_err(|error| {
            SandboxError::Rejected(format!(
                "the mount source cannot be resolved: {}: {error}",
                mount.host_path
            ))
        })?;
        if real.as_path() != host && !mount.follow_symlinks {
            return Err(SandboxError::Rejected(format!(
                "the mount source is, or passes through, a symbolic link: {}",
                mount.host_path
            )));
        }
        if real.parent().is_none() {
            return Err(SandboxError::Rejected(
                "the mount source must not be the root of the host".to_string(),
            ));
        }
        check_mount_not_system_path(host, &real, &mount.host_path)?;
        check_mount_has_no_socket(&real, &mount.host_path)?;
        real_paths.push(real);
    }
    if let Some(repository) = repository_mount(spec) {
        if let Some(repository_real) = real_paths.get(repository) {
            for (index, real) in real_paths.iter().enumerate() {
                if index != repository && repository_real.starts_with(real) {
                    return Err(SandboxError::Rejected(
                        "a mount source must not be the repository or a parent of it".to_string(),
                    ));
                }
            }
        }
    }
    for (mount, real) in resolved.mounts.iter_mut().zip(real_paths) {
        mount.host_path = real.to_string_lossy().into_owned();
    }
    Ok(resolved)
}

/// Replaces every environment value of four or more characters with `<redacted>`; shorter values are not treated as secrets.
fn redact(text: &str, command: &CommandSpec) -> String {
    let mut redacted = text.to_string();
    for value in command.env.values() {
        if value.chars().count() >= 4 {
            redacted = redacted.replace(value.as_str(), "<redacted>");
        }
    }
    redacted
}

/// Reads the server version probe.
#[must_use]
pub fn parse_server_version(output: &ToolOutput) -> Capability {
    if output.status == Some(0) && !output.stdout.trim().is_empty() {
        Capability::Supported
    } else {
        Capability::Unknown(tool_text(output))
    }
}

/// Reads the image probe for `image`.
#[must_use]
pub fn parse_image(output: &ToolOutput, image: &str) -> Capability {
    if output.status == Some(0) {
        Capability::Supported
    } else if output.status.is_some_and(|code| code != 0) && output.stderr.contains("No such image")
    {
        Capability::Unsupported(format!("image not present: {image}"))
    } else {
        Capability::Unknown(tool_text(output))
    }
}

/// Reads the network-plugin probe.
#[must_use]
pub fn parse_network(output: &ToolOutput) -> Capability {
    if output.status != Some(0) {
        return Capability::Unknown(tool_text(output));
    }
    match serde_json::from_str::<Vec<String>>(&output.stdout) {
        Ok(names) if names.iter().any(|name| name == "null") => Capability::Supported,
        Ok(_) => Capability::Unsupported("the runtime has no null network driver".to_string()),
        Err(_) => Capability::Unknown(tool_text(output)),
    }
}

/// Whether an exec ended in docker's own failure, not the command's exit.
///
/// Docker 29 exits 1 with `Error response from daemon:` for a missing sandbox and 125 for
/// its own start failures; a command that prints the same text or exits 125 looks the same.
#[must_use]
pub fn is_docker_failure(output: &ToolOutput) -> bool {
    output.status == Some(125)
        || output
            .stderr
            .trim_start()
            .starts_with("Error response from daemon:")
}

/// The text of a failed tool call: stderr, else stdout, else a fixed line.
#[must_use]
pub fn tool_text(output: &ToolOutput) -> String {
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    if output.timed_out {
        return "the tool timed out".to_string();
    }
    match output.status {
        Some(code) => format!("the tool exited with status {code}"),
        None => "the tool ended without an exit status".to_string(),
    }
}

/// The text for a `docker kill` that failed after the command timed out.
fn kill_after_timeout_text(id: &str, detail: &str) -> String {
    format!("the kill of the container {id} after the command timed out failed: {detail}")
}

/// The exact environment names that would change how the client itself behaves.
const RESERVED_CLIENT_ENV: [&str; 18] = [
    "PATH",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "TEMP",
    "TMP",
    "TMPDIR",
    "SYSTEMROOT",
    "SHELL",
    "USER",
    "LOGNAME",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "KUBECONFIG",
];

/// The environment-name prefixes that would change how the client itself behaves.
const RESERVED_CLIENT_ENV_PREFIXES: [&str; 7] = [
    "DOCKER_",
    "LD_",
    "DYLD_",
    "XDG_",
    "BUILDKIT_",
    "COMPOSE_",
    "CONTAINER_",
];

/// Refuses an environment key that would hijack the client process itself.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] naming the key, never its value.
fn check_client_env(command: &CommandSpec) -> Result<(), SandboxError> {
    for key in command.env.keys() {
        let upper = key.to_ascii_uppercase();
        let named = RESERVED_CLIENT_ENV.contains(&upper.as_str());
        let prefixed = RESERVED_CLIENT_ENV_PREFIXES
            .iter()
            .copied()
            .any(|prefix| upper.starts_with(prefix));
        if named || prefixed {
            return Err(SandboxError::Rejected(format!(
                "the environment variable name is reserved for the client process: {key}"
            )));
        }
    }
    Ok(())
}

/// The Docker [`Sandbox`] over `R`.
pub struct DockerSandbox<R: DockerRunner> {
    runner: R,
}

impl<R: DockerRunner> DockerSandbox<R> {
    /// A sandbox that runs every operation through `runner`.
    #[must_use]
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    /// Starts the container `id`, or reports why it did not start.
    fn start_container(&self, id: &str) -> Result<(), SandboxError> {
        let output = self
            .runner
            .run(&start_argv(id), None)
            .map_err(|error| SandboxError::Failed(error.0))?;
        if output.status == Some(0) && !output.timed_out {
            return Ok(());
        }
        Err(SandboxError::Failed(tool_text(&output)))
    }

    /// Removes `id` after a failed create step, folding a removal failure into `error`.
    fn remove_after_failed_step(&self, id: &str, error: &SandboxError) -> SandboxError {
        let mut message = error.to_string();
        if let Err(removal_error) = self.remove_container(id) {
            message = format!("{message}; {removal_error}");
        }
        match error {
            SandboxError::Rejected(_) => SandboxError::Rejected(message),
            SandboxError::Failed(_) => SandboxError::Failed(message),
        }
    }

    /// Runs `id -u` in `id` and refuses uid 0 or any answer it cannot read.
    fn check_user(&self, id: &SandboxId, spec: &SandboxSpec) -> Result<(), SandboxError> {
        let command = CommandSpec {
            program: "id".to_string(),
            args: vec!["-u".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        };
        let result = self.run(id, &command).map_err(|error| {
            SandboxError::Failed(format!("the sandbox user check failed: {error}"))
        })?;
        match result.outcome {
            RunOutcome::Exited(0) => {
                let uid = result.stdout.trim();
                match parse_uid(uid) {
                    Some(0) => Err(SandboxError::Rejected(format!(
                        "the sandbox user resolves to uid 0 (root); the image maps the user {} to the root user",
                        spec.user
                    ))),
                    Some(_) => Ok(()),
                    None => Err(SandboxError::Failed(format!(
                        "the sandbox user check failed: {}",
                        shown_text(uid)
                    ))),
                }
            }
            RunOutcome::Exited(code) => Err(SandboxError::Failed(format!(
                "the sandbox user check failed: exit status {code}; stdout {}; stderr {}",
                shown_text(&result.stdout),
                shown_text(&result.stderr)
            ))),
            RunOutcome::TimedOut { limit_secs } => Err(SandboxError::Failed(format!(
                "the sandbox user check failed: the check timed out after {limit_secs} seconds"
            ))),
        }
    }

    /// Removes the container `id`, or reports why it remains.
    fn remove_container(&self, id: &str) -> Result<Destroyed, SandboxError> {
        let output = self
            .runner
            .run(&remove_argv(id), None)
            .map_err(|error| SandboxError::Failed(error.0))?;
        if output.status == Some(0) && !output.timed_out {
            return Ok(Destroyed::Removed);
        }
        if output.stderr.trim().contains("No such container") {
            return Ok(Destroyed::AlreadyGone);
        }
        Err(SandboxError::Failed(tool_text(&output)))
    }

    /// Whether the container `id` exists: one inspect call that fails closed.
    fn container_exists(&self, id: &str) -> Result<bool, SandboxError> {
        let output = self
            .runner
            .run(&inspect_argv(id), None)
            .map_err(|error| SandboxError::Failed(error.0))?;
        if output.status == Some(0) && !output.timed_out {
            return Ok(true);
        }
        if output.status.is_some_and(|code| code != 0)
            && output.stderr.trim().contains("No such container")
        {
            return Ok(false);
        }
        Err(SandboxError::Failed(tool_text(&output)))
    }

    /// Runs one probe and folds a runner error into an unknown capability.
    fn probe(&self, argv: &[String], parse: impl FnOnce(&ToolOutput) -> Capability) -> Capability {
        match self.runner.run(argv, None) {
            Ok(output) => parse(&output),
            Err(error) => Capability::Unknown(error.0),
        }
    }
}

impl DockerSandbox<RealDockerRunner> {
    /// The sandbox over the real `docker` program.
    #[must_use]
    pub fn real() -> Self {
        Self::new(RealDockerRunner::default())
    }
}

impl<R: DockerRunner> Sandbox for DockerSandbox<R> {
    fn create(&self, spec: &SandboxSpec) -> Result<SandboxId, SandboxError> {
        validate_spec(spec)?;
        let resolved = resolve_mounts(spec)?;
        let output = self
            .runner
            .run(&create_argv(&resolved), Some(CREATE_TIMEOUT))
            .map_err(|error| SandboxError::Failed(error.0))?;
        if output.status != Some(0) || output.timed_out {
            return Err(SandboxError::Failed(tool_text(&output)));
        }
        let Some(container_id) = parse_container_id(&output.stdout) else {
            return Err(SandboxError::Failed(format!(
                "the create call did not return one container id; its output was: {}",
                shown_text(&output.stdout)
            )));
        };
        let id = container_id.to_string();
        if let Err(start_error) = self.start_container(&id) {
            return Err(self.remove_after_failed_step(&id, &start_error));
        }
        let sandbox_id = SandboxId(id);
        if let Err(check_error) = self.check_user(&sandbox_id, spec) {
            return Err(self.remove_after_failed_step(&sandbox_id.0, &check_error));
        }
        Ok(sandbox_id)
    }

    fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
        validate_command(command)?;
        check_client_env(command)?;
        let limit_secs = command.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let timeout = Some(Duration::from_secs(limit_secs));
        let output = self
            .runner
            .run_with_stdin(
                &exec_argv(&id.0, command),
                timeout,
                command.stdin.as_deref(),
                &command.env,
            )
            .map_err(|error| SandboxError::Failed(redact(&error.0, command)))?;

        if output.timed_out {
            let kill = self.runner.run(&kill_argv(&id.0), None).map_err(|error| {
                let text = kill_after_timeout_text(&id.0, &error.0);
                SandboxError::Failed(redact(&text, command))
            })?;
            if kill.status != Some(0) || kill.timed_out {
                let text = kill_after_timeout_text(&id.0, &tool_text(&kill));
                return Err(SandboxError::Failed(redact(&text, command)));
            }
            return Ok(RunResult {
                outcome: RunOutcome::TimedOut { limit_secs },
                stdout: output.stdout,
                stderr: output.stderr,
                output_cap_bytes: output.output_cap_bytes,
                stdout_truncated: output.stdout_truncated,
                stderr_truncated: output.stderr_truncated,
            });
        }

        if is_docker_failure(&output) {
            return Err(SandboxError::Failed(redact(&tool_text(&output), command)));
        }
        if let Some(code) = output.status {
            return Ok(RunResult {
                outcome: RunOutcome::Exited(code),
                stdout: output.stdout,
                stderr: output.stderr,
                output_cap_bytes: output.output_cap_bytes,
                stdout_truncated: output.stdout_truncated,
                stderr_truncated: output.stderr_truncated,
            });
        }
        let mut message = "the tool ended without an exit status".to_string();
        let stderr = output.stderr.trim();
        if !stderr.is_empty() {
            message = format!("{message}: {stderr}");
        }
        Err(SandboxError::Failed(redact(&message, command)))
    }

    /// Checks that the container exists with one inspect call, then removes it;
    /// a missing container is already gone and never reaches the remove call.
    fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError> {
        if !self.container_exists(&id.0)? {
            return Ok(Destroyed::AlreadyGone);
        }
        self.remove_container(&id.0)
    }

    fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities {
        let runtime = self.probe(&version_argv(), parse_server_version);
        let image_argv = image_inspect_argv(&spec.image);
        let image = self.probe(&image_argv, |output| parse_image(output, &spec.image));
        let network_isolation = self.probe(&network_probe_argv(), parse_network);
        SandboxCapabilities {
            platforms: vec!["local".to_string()],
            runtime,
            image,
            network_isolation,
            mounts: Capability::Supported,
            stop_mid_run: Capability::Unsupported(
                "a run stops only by its timeout; no caller-initiated stop".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Limits;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// Locks `mutex`, recovering the value even if a previous holder panicked.
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One recorded runner call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        argv: Vec<String>,
        timeout: Option<Duration>,
        stdin: Option<Vec<u8>>,
        env: BTreeMap<String, String>,
    }

    /// A [`DockerRunner`] that records every call and answers in order.
    struct ScriptedRunner {
        calls: Mutex<Vec<Call>>,
        results: Mutex<VecDeque<Result<ToolOutput, RunnerError>>>,
    }

    impl ScriptedRunner {
        fn new(results: Vec<Result<ToolOutput, RunnerError>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                results: Mutex::new(results.into()),
            }
        }

        fn calls(&self) -> Vec<Call> {
            lock(&self.calls).clone()
        }
    }

    impl DockerRunner for ScriptedRunner {
        fn run_with_stdin(
            &self,
            argv: &[String],
            timeout: Option<Duration>,
            stdin: Option<&[u8]>,
            env: &BTreeMap<String, String>,
        ) -> Result<ToolOutput, RunnerError> {
            lock(&self.calls).push(Call {
                argv: argv.to_vec(),
                timeout,
                stdin: stdin.map(<[u8]>::to_vec),
                env: env.clone(),
            });
            lock(&self.results)
                .pop_front()
                .expect("the scripted runner has an answer for every call")
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn the_docker_adapter_is_send_and_sync() {
        assert_send_sync::<DockerSandbox<RealDockerRunner>>();
    }

    /// A workspace-shaped spec with two mounts, one of them read-only.
    fn spec() -> SandboxSpec {
        SandboxSpec {
            name: "build".to_string(),
            image: "example/base:1".to_string(),
            user: "dev".to_string(),
            workdir: "/work".to_string(),
            network: Network::Isolated,
            mounts: vec![
                Mount {
                    host_path: "/host/project".to_string(),
                    sandbox_path: "/work/project".to_string(),
                    read_only: false,
                    follow_symlinks: false,
                },
                Mount {
                    host_path: "/host/cache".to_string(),
                    sandbox_path: "/cache".to_string(),
                    read_only: true,
                    follow_symlinks: false,
                },
            ],
            limits: Limits::default(),
        }
    }

    thread_local! {
        // A fresh, empty folder per test thread, removed when the thread ends.
        static REAL_DIR: crate::test_support::TempDir =
            crate::test_support::TempDir::new("osf-docker-sandbox-real");
    }

    /// A real folder source for a create test: a fresh, empty folder per test thread.
    fn real_dir() -> String {
        REAL_DIR.with(|dir| {
            std::fs::canonicalize(dir.as_ref())
                .expect("the temp folder canonicalizes")
                .to_string_lossy()
                .into_owned()
        })
    }

    /// The workspace spec with both mount sources in the real temp folder.
    fn real_spec() -> SandboxSpec {
        let dir = real_dir();
        let mut spec = spec();
        spec.mounts = vec![
            Mount {
                host_path: dir.clone(),
                sandbox_path: "/work/project".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: dir,
                sandbox_path: "/cache".to_string(),
                read_only: true,
                follow_symlinks: false,
            },
        ];
        spec
    }

    /// [`real_spec`] after its mount sources have been canonicalized.
    fn resolved_real_spec() -> SandboxSpec {
        resolve_mounts(&real_spec()).expect("the mounts resolve")
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        }
    }

    /// The default command with one environment value set.
    fn command_with_env(value: &str) -> CommandSpec {
        let mut command = command();
        command.env.insert("API_KEY".to_string(), value.to_string());
        command
    }

    /// The `id -u` command `create` runs to check the sandbox user.
    fn user_check_command() -> CommandSpec {
        CommandSpec {
            program: "id".to_string(),
            args: vec!["-u".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        }
    }

    /// The `id -u` call `create` records after a successful start.
    fn user_check_call(id: &str) -> Call {
        Call {
            argv: exec_argv(id, &user_check_command()),
            timeout: Some(Duration::from_secs(30)),
            stdin: None,
            env: BTreeMap::new(),
        }
    }

    fn ok(stdout: &str) -> ToolOutput {
        ToolOutput {
            status: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn fail(status: i32, stderr: &str) -> ToolOutput {
        ToolOutput {
            status: Some(status),
            stdout: String::new(),
            stderr: stderr.to_string(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn timeout_output(stdout: &str) -> ToolOutput {
        ToolOutput {
            status: None,
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: true,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn strings(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_string()).collect()
    }

    /// A 64-character lowercase hexadecimal id built from `seed`.
    fn id64(seed: char) -> String {
        seed.to_string().repeat(64)
    }

    fn sandbox(results: Vec<Result<ToolOutput, RunnerError>>) -> DockerSandbox<ScriptedRunner> {
        DockerSandbox::new(ScriptedRunner::new(results))
    }

    fn create_rejected(spec: &SandboxSpec) {
        let sandbox = sandbox(Vec::new());
        let error = sandbox.create(spec).expect_err("the spec is rejected");
        assert!(matches!(error, SandboxError::Rejected(_)), "{error:?}");
        assert_eq!(sandbox.runner.calls(), Vec::<Call>::new());
    }

    fn run_rejected(command: &CommandSpec) {
        let sandbox = sandbox(Vec::new());
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, command)
            .expect_err("the command is rejected");
        assert!(matches!(error, SandboxError::Rejected(_)), "{error:?}");
        assert_eq!(sandbox.runner.calls(), Vec::<Call>::new());
    }

    #[test]
    fn create_argv_isolated_pins_the_exact_vector() {
        assert_eq!(
            create_argv(&spec()),
            strings(&[
                "create",
                "--name=build",
                "--label=osf.sandbox=build",
                "--env=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "--env=HOME=/tmp",
                "--env=LANG=C.UTF-8",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--init",
                "--pids-limit",
                "512",
                "--memory",
                "4g",
                "--user=dev",
                "--workdir=/work",
                "--network",
                "none",
                "--mount=type=bind,source=/host/project,target=/work/project",
                "--mount=type=bind,source=/host/cache,target=/cache,readonly",
                "--",
                "example/base:1",
                "sleep",
                "2147483647",
            ])
        );
    }

    #[test]
    fn create_argv_puts_the_default_limits_after_no_new_privileges() {
        let argv = create_argv(&spec());
        let security = argv
            .iter()
            .position(|part| part == "no-new-privileges")
            .expect("the security option");
        assert_eq!(
            argv.get(security + 1..security + 6).map(<[String]>::to_vec),
            Some(strings(&[
                "--init",
                "--pids-limit",
                "512",
                "--memory",
                "4g"
            ]))
        );
    }

    #[test]
    fn create_argv_uses_the_spec_limits() {
        let mut spec = spec();
        spec.limits = Limits {
            max_processes: 64,
            memory: "512m".to_string(),
        };
        let argv = create_argv(&spec);
        let pids = argv
            .iter()
            .position(|part| part == "--pids-limit")
            .expect("the process limit flag");
        assert_eq!(argv.get(pids + 1).map(String::as_str), Some("64"));
        let memory = argv
            .iter()
            .position(|part| part == "--memory")
            .expect("the memory flag");
        assert_eq!(argv.get(memory + 1).map(String::as_str), Some("512m"));
    }

    #[test]
    fn create_argv_pins_exactly_the_three_sandbox_env_pairs() {
        let argv = create_argv(&spec());
        let envs: Vec<&str> = argv
            .iter()
            .map(String::as_str)
            .filter(|part| part.starts_with("--env="))
            .collect();
        assert_eq!(
            envs,
            [
                "--env=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "--env=HOME=/tmp",
                "--env=LANG=C.UTF-8",
            ]
        );
    }

    #[test]
    fn create_argv_puts_the_image_after_one_double_dash() {
        let argv = create_argv(&spec());
        let at = argv
            .iter()
            .position(|part| part.as_str() == "--")
            .expect("one --");
        assert_eq!(
            argv.iter().filter(|part| part.as_str() == "--").count(),
            1,
            "{argv:?}"
        );
        assert_eq!(argv.get(at + 1).map(String::as_str), Some("example/base:1"));
    }

    #[test]
    fn create_argv_open_uses_the_bridge_network() {
        let mut spec = spec();
        spec.network = Network::Open;
        let argv = create_argv(&spec);
        let network = argv
            .iter()
            .position(|part| part == "--network")
            .expect("a network flag");
        assert_eq!(argv.get(network + 1).map(String::as_str), Some("bridge"));
    }

    #[test]
    fn start_argv_pins_the_exact_vector() {
        assert_eq!(start_argv("abc"), strings(&["start", "--", "abc"]));
    }

    #[test]
    fn exec_argv_pins_the_exact_vector_without_a_workdir() {
        assert_eq!(
            exec_argv("abc", &command()),
            strings(&["exec", "--", "abc", "run", "--fast"])
        );
    }

    #[test]
    fn exec_argv_pins_the_exact_vector_with_a_workdir() {
        let mut command = command();
        command.workdir = Some("/work".to_string());
        assert_eq!(
            exec_argv("abc", &command),
            strings(&["exec", "--workdir=/work", "--", "abc", "run", "--fast"])
        );
    }

    #[test]
    fn exec_argv_pins_the_env_names_in_map_order_without_a_value() {
        let mut command = command();
        command
            .env
            .insert("OTHER".to_string(), "another-secret".to_string());
        command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        let argv = exec_argv("abc", &command);
        assert_eq!(
            argv,
            strings(&["exec", "--env", "API_KEY", "--env", "OTHER", "--", "abc", "run", "--fast",])
        );
        assert!(
            argv.iter()
                .all(|word| !word.contains("s3cr3t-value") && !word.contains("another-secret")),
            "{argv:?}"
        );
    }

    #[test]
    fn exec_argv_pins_the_interactive_flag_for_stdin() {
        let mut command = command();
        command.stdin = Some(b"input".to_vec());
        assert_eq!(
            exec_argv("abc", &command),
            strings(&["exec", "-i", "--", "abc", "run", "--fast"])
        );
    }

    #[test]
    fn exec_argv_pins_env_stdin_and_workdir_together_without_a_value() {
        let mut command = command();
        command.workdir = Some("/work".to_string());
        command
            .env
            .insert("OTHER".to_string(), "another-secret".to_string());
        command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        command.stdin = Some(b"input".to_vec());
        let argv = exec_argv("abc", &command);
        assert_eq!(
            argv,
            strings(&[
                "exec",
                "--workdir=/work",
                "--env",
                "API_KEY",
                "--env",
                "OTHER",
                "-i",
                "--",
                "abc",
                "run",
                "--fast",
            ])
        );
        assert!(
            argv.iter()
                .all(|word| !word.contains("s3cr3t-value") && !word.contains("another-secret")),
            "{argv:?}"
        );
    }

    #[test]
    fn exec_argv_keeps_dashed_words_after_the_double_dash() {
        let command = CommandSpec {
            program: "--user=0:0".to_string(),
            args: vec![
                "-x".to_string(),
                "--workdir=/".to_string(),
                "--".to_string(),
            ],
            workdir: None,
            timeout_secs: None,
            env: BTreeMap::new(),
            stdin: None,
        };
        let argv = exec_argv("abc", &command);
        assert_eq!(
            argv,
            strings(&["exec", "--", "abc", "--user=0:0", "-x", "--workdir=/", "--",])
        );
        let id_at = argv.iter().position(|part| part == "abc").expect("the id");
        let before = id_at.checked_sub(1).and_then(|at| argv.get(at));
        assert_eq!(before.map(String::as_str), Some("--"));
        let after_id: Vec<&str> = argv
            .get(id_at..)
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(after_id, ["abc", "--user=0:0", "-x", "--workdir=/", "--"]);
        let before_id = argv.get(..id_at).unwrap_or_default();
        assert_eq!(
            before_id
                .iter()
                .filter(|part| part.as_str() == "--")
                .count(),
            1,
            "{argv:?}"
        );
    }

    #[test]
    fn kill_argv_pins_the_exact_vector() {
        assert_eq!(kill_argv("abc"), strings(&["kill", "--", "abc"]));
    }

    #[test]
    fn remove_argv_pins_the_exact_vector() {
        assert_eq!(remove_argv("abc"), strings(&["rm", "--force", "--", "abc"]));
    }

    #[test]
    fn version_argv_pins_the_exact_vector() {
        assert_eq!(
            version_argv(),
            strings(&["version", "--format", "{{.Server.Version}}"])
        );
    }

    #[test]
    fn image_inspect_argv_pins_the_exact_vector() {
        assert_eq!(
            image_inspect_argv("example/base:1"),
            strings(&[
                "image",
                "inspect",
                "--format",
                "{{.Id}}",
                "--",
                "example/base:1",
            ])
        );
    }

    #[test]
    fn network_probe_argv_pins_the_exact_vector() {
        assert_eq!(
            network_probe_argv(),
            strings(&["info", "--format", "{{json .Plugins.Network}}"])
        );
    }

    #[test]
    fn validate_spec_rejects_every_bad_image() {
        let short = "a".repeat(63);
        let upper = "A".repeat(64);
        let wrong = "z".repeat(64);
        let long_tag = "a".repeat(129);
        let hex = "a".repeat(64);
        for image in [
            "--image=--user=0:1".to_string(),
            "--user=0:1".to_string(),
            "-x".to_string(),
            "-".to_string(),
            "sleep".to_string(),
            "example/base".to_string(),
            "example/base:latest".to_string(),
            format!("example/base@sha256:{short}"),
            format!("example/base@sha256:{upper}"),
            format!("example/base@sha256:{wrong}"),
            "Example/base:1".to_string(),
            "example/Base:1".to_string(),
            "example//base:1".to_string(),
            "/example:1".to_string(),
            "example/:1".to_string(),
            "example/base:".to_string(),
            "example/base:1:2".to_string(),
            "example/base:-1".to_string(),
            "example/base:.1".to_string(),
            format!("example/base:{long_tag}"),
            "example/base:1 --user=0:1".to_string(),
            "example/base:1\n".to_string(),
            " example/base:1".to_string(),
            "example base:1".to_string(),
            format!("example/base:1@sha256:{hex}@sha256:{hex}"),
            "alpine".to_string(),
            "alpine:".to_string(),
            "alpine:1:2".to_string(),
            "Alpine:1".to_string(),
            "registry:123456/a:1".to_string(),
            "-registry.io/a:1".to_string(),
            "a/.b:1".to_string(),
            "a/b.:1".to_string(),
            "a/b_:1".to_string(),
            String::new(),
        ] {
            let mut spec = spec();
            spec.image = image.clone();
            create_rejected(&spec);
            assert!(!image_is_pinned(&image), "{image:?} is not pinned");
        }
    }

    #[test]
    fn validate_spec_rejects_a_root_or_empty_user_without_a_call() {
        for user in [
            "",
            "0",
            "00",
            "000",
            "+0",
            "00:1",
            "0:0",
            "root",
            "root:root",
            " 0",
            "0 ",
            "1:0",
            "1:00",
            "1:root",
            "dev:",
            ":dev",
            ":",
            "dev:dev:dev",
            "+1",
            "-1",
            "01",
            "1 ",
            "d ev",
            "dev\n",
            "1.5",
            "4294967296",
            "dev.x",
            "9dev",
        ] {
            let mut spec = spec();
            spec.user = user.to_string();
            create_rejected(&spec);
        }
    }

    #[test]
    fn validate_spec_rejects_a_relative_workdir_without_a_call() {
        let mut spec = spec();
        spec.workdir = "work".to_string();
        create_rejected(&spec);
    }

    #[test]
    fn validate_spec_rejects_a_malformed_name_without_a_call() {
        for name in ["", "-build", "build x", "build/x"] {
            let mut spec = spec();
            spec.name = name.to_string();
            create_rejected(&spec);
        }
    }

    #[test]
    fn validate_spec_rejects_a_malformed_mount_without_a_call() {
        let cases = [
            Mount {
                host_path: String::new(),
                sandbox_path: "/work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "host".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host/../project".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "/work/../project".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host,a".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "/wo,rk".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host\nb".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "/work\nb".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
        ];
        for mount in cases {
            let mut spec = spec();
            spec.mounts = vec![mount];
            create_rejected(&spec);
        }
    }

    #[test]
    fn validate_spec_rejects_a_bad_process_limit_without_a_call() {
        for max_processes in [0, 4_194_305] {
            let mut spec = spec();
            spec.limits.max_processes = max_processes;
            create_rejected(&spec);
        }
    }

    #[test]
    fn validate_spec_rejects_a_bad_memory_limit_without_a_call() {
        for memory in [
            "",
            "0",
            "4G",
            "4gb",
            "-1",
            "4 g",
            "--x",
            "1e3",
            "4g;",
            "0123",
            "1234567890123",
        ] {
            let mut spec = spec();
            spec.limits.memory = memory.to_string();
            create_rejected(&spec);
        }
    }

    #[test]
    fn validate_command_rejects_an_empty_program_without_a_call() {
        let mut command = command();
        command.program = String::new();
        run_rejected(&command);
    }

    #[test]
    fn validate_command_rejects_a_relative_workdir_without_a_call() {
        let mut command = command();
        command.workdir = Some("work".to_string());
        run_rejected(&command);
    }

    #[test]
    fn validate_command_rejects_a_zero_timeout_without_a_call() {
        let mut command = command();
        command.timeout_secs = Some(0);
        run_rejected(&command);
    }

    #[test]
    fn validate_command_rejects_bad_env_keys_without_a_call() {
        for key in ["", "1A", "A B", "A-B", "A=B", "ünï"] {
            let mut command = command();
            command.env.insert(key.to_string(), "value".to_string());
            run_rejected(&command);
        }
    }

    #[test]
    fn validate_command_rejects_a_nul_env_value_without_a_call() {
        let mut command = command();
        command
            .env
            .insert("API_KEY".to_string(), "a\0b".to_string());
        run_rejected(&command);
    }

    #[test]
    fn validate_command_names_a_nul_value_by_its_key_only() {
        let mut command = command();
        command
            .env
            .insert("API_KEY".to_string(), "sec\0ret".to_string());
        let sandbox = sandbox(Vec::new());
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command)
            .expect_err("the value is rejected");
        assert!(error.to_string().contains("API_KEY"), "{error}");
        assert!(!error.to_string().contains("sec"), "{error}");
    }

    #[test]
    fn create_runs_create_then_start_and_returns_the_trimmed_id() {
        let created = id64('a');
        let output = format!("{created}\n");
        let sandbox = sandbox(vec![Ok(ok(&output)), Ok(ok("")), Ok(ok("1000\n"))]);
        let id = sandbox.create(&real_spec()).expect("creates");
        assert_eq!(id, SandboxId(created.clone()));
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: create_argv(&resolved_real_spec()),
                    timeout: Some(Duration::from_secs(120)),
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: start_argv(&created),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
                user_check_call(&created),
            ]
        );
    }

    #[test]
    fn create_rejects_a_uid_zero_check_and_removes_the_container() {
        let created = id64('e');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(ok("")),
            Ok(ok("0\n")),
            Ok(ok("")),
        ]);
        let error = sandbox.create(&real_spec()).expect_err("uid 0 is refused");
        assert_eq!(
            error,
            SandboxError::Rejected(
                "the sandbox user resolves to uid 0 (root); the image maps the user dev to the root user"
                    .to_string()
            )
        );
        let mut expected = vec![
            Call {
                argv: create_argv(&resolved_real_spec()),
                timeout: Some(Duration::from_secs(120)),
                stdin: None,
                env: BTreeMap::new(),
            },
            Call {
                argv: start_argv(&created),
                timeout: None,
                stdin: None,
                env: BTreeMap::new(),
            },
            user_check_call(&created),
        ];
        expected.push(Call {
            argv: remove_argv(&created),
            timeout: None,
            stdin: None,
            env: BTreeMap::new(),
        });
        assert_eq!(sandbox.runner.calls(), expected);
    }

    #[test]
    fn a_uid_zero_check_rejection_appends_a_failed_removal() {
        let created = id64('f');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(ok("")),
            Ok(ok("00")),
            Ok(fail(1, "boom remove")),
        ]);
        let error = sandbox.create(&real_spec()).expect_err("uid 0 is refused");
        assert_eq!(
            error,
            SandboxError::Rejected(
                "the sandbox user resolves to uid 0 (root); the image maps the user dev to the root user; boom remove"
                    .to_string()
            )
        );
    }

    #[test]
    fn create_accepts_a_padded_uid_check() {
        let created = id64('1');
        let sandbox = sandbox(vec![Ok(ok(&created)), Ok(ok("")), Ok(ok(" 1000 \n"))]);
        let id = sandbox.create(&real_spec()).expect("the padded uid passes");
        assert_eq!(id, SandboxId(created));
    }

    #[test]
    fn create_refuses_a_zero_uid_check_spelled_with_zeros() {
        for uid in ["0", "00", "000", "0\n"] {
            let created = id64('2');
            let sandbox = sandbox(vec![Ok(ok(&created)), Ok(ok("")), Ok(ok(uid)), Ok(ok(""))]);
            let error = sandbox
                .create(&real_spec())
                .expect_err("a zero uid is refused");
            assert_eq!(
                error,
                SandboxError::Rejected(
                    "the sandbox user resolves to uid 0 (root); the image maps the user dev to the root user"
                        .to_string()
                ),
                "{uid:?}"
            );
        }
    }

    #[test]
    fn create_refuses_a_uid_check_that_is_not_a_number() {
        for uid in ["+0", "-1", "abc", "", "1000\n1001"] {
            let created = id64('3');
            let sandbox = sandbox(vec![Ok(ok(&created)), Ok(ok("")), Ok(ok(uid)), Ok(ok(""))]);
            let error = sandbox
                .create(&real_spec())
                .expect_err("a non-number is refused");
            assert_eq!(
                error,
                SandboxError::Failed(format!(
                    "the sandbox user check failed: {}",
                    shown_text(uid.trim())
                )),
                "{uid:?}"
            );
        }
    }

    #[test]
    fn create_reports_a_nonzero_user_check_and_removes_the_container() {
        let created = id64('4');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(ok("")),
            Ok(fail(126, "no id")),
            Ok(ok("")),
        ]);
        let error = sandbox
            .create(&real_spec())
            .expect_err("a non-zero check fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the sandbox user check failed: exit status 126; stdout \"\"; stderr \"no id\""
                    .to_string()
            )
        );
        assert_eq!(
            sandbox.runner.calls().last().map(|call| call.argv.clone()),
            Some(remove_argv(&created))
        );
    }

    #[test]
    fn create_reports_a_runner_error_from_the_user_check_and_removes_the_container() {
        let created = id64('5');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(ok("")),
            Err(RunnerError("cannot run docker".to_string())),
            Ok(ok("")),
        ]);
        let error = sandbox
            .create(&real_spec())
            .expect_err("a runner error fails");
        assert_eq!(
            error,
            SandboxError::Failed("the sandbox user check failed: cannot run docker".to_string())
        );
        assert_eq!(
            sandbox.runner.calls().last().map(|call| call.argv.clone()),
            Some(remove_argv(&created))
        );
    }

    #[test]
    fn create_reports_a_timed_out_user_check_and_kills_then_removes() {
        let created = id64('6');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(ok("")),
            Ok(timeout_output("")),
            Ok(ok("")),
            Ok(ok("")),
        ]);
        let error = sandbox.create(&real_spec()).expect_err("a timeout fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the sandbox user check failed: the check timed out after 30 seconds".to_string()
            )
        );
        assert_eq!(
            sandbox
                .runner
                .calls()
                .iter()
                .map(|call| call.argv.first().cloned())
                .collect::<Vec<_>>(),
            vec![
                Some("create".to_string()),
                Some("start".to_string()),
                Some("exec".to_string()),
                Some("kill".to_string()),
                Some("rm".to_string()),
            ]
        );
    }

    #[test]
    fn create_carries_the_tool_text_on_a_nonzero_create() {
        let sandbox = sandbox(vec![Ok(fail(1, "boom create"))]);
        let error = sandbox.create(&real_spec()).expect_err("create fails");
        assert_eq!(error, SandboxError::Failed("boom create".to_string()));
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn create_reports_a_runner_error() {
        let sandbox = sandbox(vec![Err(RunnerError("cannot run docker".to_string()))]);
        let error = sandbox.create(&real_spec()).expect_err("runner error");
        assert_eq!(error, SandboxError::Failed("cannot run docker".to_string()));
    }

    #[test]
    fn create_reports_a_timed_out_create() {
        let sandbox = sandbox(vec![Ok(timeout_output(""))]);
        let error = sandbox.create(&real_spec()).expect_err("timed out");
        assert_eq!(
            error,
            SandboxError::Failed("the tool timed out".to_string())
        );
    }

    #[test]
    fn create_rejects_an_empty_container_id() {
        let sandbox = sandbox(vec![Ok(ok("\n"))]);
        let error = sandbox.create(&real_spec()).expect_err("no id");
        assert!(matches!(error, SandboxError::Failed(_)), "{error:?}");
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn create_rejects_a_short_container_id_and_shows_the_output() {
        let sandbox = sandbox(vec![Ok(ok("abc123\n"))]);
        let error = sandbox.create(&real_spec()).expect_err("no id");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the create call did not return one container id; its output was: \"abc123\""
                    .to_string()
            )
        );
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn create_cuts_a_long_create_output_to_two_hundred_characters() {
        let long = "x".repeat(250);
        let sandbox = sandbox(vec![Ok(ok(&long))]);
        let error = sandbox.create(&real_spec()).expect_err("no id");
        let expected = format!("\"{}...\"", "x".repeat(200));
        assert!(error.to_string().contains(&expected), "{error}");
        assert!(!error.to_string().contains(&"x".repeat(201)), "{error}");
    }

    #[test]
    fn parse_container_id_accepts_one_id_with_one_trailing_newline() {
        let id = id64('d');
        assert_eq!(parse_container_id(&id), Some(id.as_str()));
        assert_eq!(parse_container_id(&format!("{id}\n")), Some(id.as_str()));
        assert_eq!(parse_container_id(&format!("{id}\r\n")), Some(id.as_str()));
    }

    #[test]
    fn parse_container_id_refuses_anything_but_one_id() {
        let id = id64('d');
        for bad in [
            String::new(),
            "abc123".to_string(),
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            format!("{id}\n\n"),
            format!("{id}\n{id}"),
            format!("warning\n{id}"),
            format!("{id} extra"),
            format!("{id} "),
            format!(" {id}"),
        ] {
            assert_eq!(parse_container_id(&bad), None, "{bad:?}");
        }
    }

    #[test]
    fn parse_uid_accepts_only_decimal_digits() {
        assert_eq!(parse_uid("0"), Some(0));
        assert_eq!(parse_uid("00"), Some(0));
        assert_eq!(parse_uid("1000"), Some(1000));
        for bad in ["", "+0", "-1", "abc", "1000\n1001", "1 000", "4294967296"] {
            assert_eq!(parse_uid(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_failed_start_removes_the_container_and_returns_the_start_text() {
        let created = id64('b');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(fail(1, "boom start")),
            Ok(ok("")),
        ]);
        let error = sandbox.create(&real_spec()).expect_err("start fails");
        assert_eq!(error, SandboxError::Failed("boom start".to_string()));
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: create_argv(&resolved_real_spec()),
                    timeout: Some(Duration::from_secs(120)),
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: start_argv(&created),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: remove_argv(&created),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
            ]
        );
    }

    #[test]
    fn a_failed_start_appends_a_failed_removal_text() {
        let created = id64('c');
        let sandbox = sandbox(vec![
            Ok(ok(&created)),
            Ok(fail(1, "boom start")),
            Ok(fail(1, "boom remove")),
        ]);
        let error = sandbox.create(&real_spec()).expect_err("start fails");
        assert_eq!(
            error,
            SandboxError::Failed("boom start; boom remove".to_string())
        );
    }

    #[test]
    fn run_returns_exited_zero_with_both_streams() {
        let sandbox = sandbox(vec![Ok(ToolOutput {
            status: Some(0),
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        })]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command()).expect("runs");
        assert_eq!(
            result,
            RunResult {
                outcome: RunOutcome::Exited(0),
                stdout: "out".to_string(),
                stderr: "err".to_string(),
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            }
        );
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: exec_argv("abc", &command()),
                timeout: Some(Duration::from_secs(30)),
                stdin: None,
                env: BTreeMap::new(),
            }]
        );
    }

    #[test]
    fn run_returns_a_nonzero_exit_as_exited() {
        let sandbox = sandbox(vec![Ok(fail(3, "err"))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command()).expect("runs");
        assert_eq!(result.outcome, RunOutcome::Exited(3));
    }

    #[test]
    fn run_without_a_timeout_gets_the_default_limit() {
        let mut command = command();
        command.timeout_secs = None;
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        sandbox.run(&id, &command).expect("runs");
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: exec_argv("abc", &command),
                timeout: Some(Duration::from_secs(1800)),
                stdin: None,
                env: BTreeMap::new(),
            }]
        );
    }

    #[test]
    fn a_timed_out_run_without_a_timeout_reports_the_default_limit() {
        let mut command = command();
        command.timeout_secs = None;
        let sandbox = sandbox(vec![Ok(timeout_output("")), Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command).expect("runs");
        assert_eq!(result.outcome, RunOutcome::TimedOut { limit_secs: 1800 });
    }

    #[test]
    fn run_reports_dockers_own_exit_125_as_failed() {
        let sandbox = sandbox(vec![Ok(fail(125, "docker: no such container"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("125 fails");
        assert_eq!(
            error,
            SandboxError::Failed("docker: no such container".to_string())
        );
    }

    #[test]
    fn run_reports_a_daemon_error_with_exit_one_as_failed() {
        let sandbox = sandbox(vec![Ok(fail(
            1,
            "Error response from daemon: No such container: abc\n",
        ))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command())
            .expect_err("daemon error fails");
        assert_eq!(
            error,
            SandboxError::Failed("Error response from daemon: No such container: abc".to_string())
        );
    }

    #[test]
    fn run_keeps_a_plain_exit_one_as_exited() {
        let sandbox = sandbox(vec![Ok(fail(1, "tests failed"))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command()).expect("runs");
        assert_eq!(result.outcome, RunOutcome::Exited(1));
    }

    #[test]
    fn run_of_a_timeout_kills_the_container_and_reports_timed_out() {
        let sandbox = sandbox(vec![Ok(timeout_output("partial")), Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command()).expect("runs");
        assert_eq!(
            result,
            RunResult {
                outcome: RunOutcome::TimedOut { limit_secs: 30 },
                stdout: "partial".to_string(),
                stderr: String::new(),
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            }
        );
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: exec_argv("abc", &command()),
                    timeout: Some(Duration::from_secs(30)),
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: kill_argv("abc"),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
            ]
        );
    }

    #[test]
    fn a_failed_kill_after_a_timeout_is_failed() {
        let sandbox = sandbox(vec![Ok(timeout_output("")), Ok(fail(1, "boom kill"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("kill fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the kill of the container abc after the command timed out failed: boom kill"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_failed_kill_with_empty_output_names_the_kill_and_the_status() {
        let sandbox = sandbox(vec![Ok(timeout_output("")), Ok(fail(1, ""))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("kill fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the kill of the container abc after the command timed out failed: the tool exited with status 1"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_kill_that_cannot_start_names_the_kill_and_the_runner_error() {
        let sandbox = sandbox(vec![
            Ok(timeout_output("")),
            Err(RunnerError("no docker".to_string())),
        ]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("kill cannot start");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the kill of the container abc after the command timed out failed: no docker"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_failed_kill_trims_its_stderr_for_the_detail() {
        let sandbox = sandbox(vec![
            Ok(timeout_output("")),
            Ok(fail(1, "  daemon said no\n")),
        ]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("kill fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the kill of the container abc after the command timed out failed: daemon said no"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_signal_end_without_a_timeout_is_failed() {
        let sandbox = sandbox(vec![Ok(ToolOutput {
            status: None,
            stdout: String::new(),
            stderr: "killed".to_string(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        })]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("signal fails");
        assert_eq!(
            error,
            SandboxError::Failed("the tool ended without an exit status: killed".to_string())
        );
    }

    #[test]
    fn a_signal_end_with_empty_stderr_names_the_missing_status() {
        let sandbox = sandbox(vec![Ok(ToolOutput {
            status: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        })]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("signal fails");
        assert_eq!(
            error,
            SandboxError::Failed("the tool ended without an exit status".to_string())
        );
    }

    #[test]
    fn run_reports_a_runner_error_as_failed() {
        let sandbox = sandbox(vec![Err(RunnerError("cannot run docker".to_string()))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("runner error");
        assert_eq!(error, SandboxError::Failed("cannot run docker".to_string()));
    }

    #[test]
    fn run_gives_the_runner_the_command_stdin_bytes() {
        let mut command = command();
        command.stdin = Some(b"the input".to_vec());
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        sandbox.run(&id, &command).expect("runs");
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: exec_argv("abc", &command),
                timeout: Some(Duration::from_secs(30)),
                stdin: Some(b"the input".to_vec()),
                env: BTreeMap::new(),
            }]
        );
    }

    #[test]
    fn run_gives_the_runner_no_stdin_when_the_command_has_none() {
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        sandbox.run(&id, &command()).expect("runs");
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: exec_argv("abc", &command()),
                timeout: Some(Duration::from_secs(30)),
                stdin: None,
                env: BTreeMap::new(),
            }]
        );
    }

    #[test]
    fn run_passes_env_values_to_the_runner_and_keeps_them_out_of_argv() {
        let mut command = command();
        command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        command
            .env
            .insert("OTHER".to_string(), "another-secret".to_string());
        let sandbox = sandbox(vec![Ok(timeout_output("")), Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox.run(&id, &command).expect("runs");
        assert_eq!(result.outcome, RunOutcome::TimedOut { limit_secs: 30 });
        let calls = sandbox.runner.calls();
        assert_eq!(calls.len(), 2, "the exec and the kill");
        let exec = calls.first().expect("the exec call");
        assert_eq!(exec.argv, exec_argv("abc", &command));
        assert_eq!(exec.env, command.env);
        for call in &calls {
            assert!(
                call.argv
                    .iter()
                    .all(|word| !word.contains("s3cr3t-value") && !word.contains("another-secret")),
                "{:?}",
                call.argv
            );
        }
    }

    #[test]
    fn run_rejects_a_reserved_client_env_name_without_a_call() {
        for key in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "TEMP",
            "TMP",
            "TMPDIR",
            "SYSTEMROOT",
            "SHELL",
            "USER",
            "LOGNAME",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "ALL_PROXY",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "KUBECONFIG",
        ] {
            for spelled in [key.to_string(), key.to_ascii_lowercase()] {
                let mut command = command_with_env("s3cr3t-value");
                command
                    .env
                    .insert(spelled.clone(), "s3cr3t-value".to_string());
                let sandbox = sandbox(Vec::new());
                let id = SandboxId("abc".to_string());
                let error = sandbox.run(&id, &command).expect_err("reserved");
                assert!(
                    matches!(error, SandboxError::Rejected(_)),
                    "{spelled}: {error:?}"
                );
                assert!(error.to_string().contains(&spelled), "{spelled}: {error}");
                assert!(
                    !error.to_string().contains("s3cr3t-value"),
                    "{spelled}: {error}"
                );
                assert!(
                    sandbox.runner.calls().is_empty(),
                    "{spelled} reached the runner"
                );
            }
        }
    }

    #[test]
    fn run_rejects_a_reserved_client_env_prefix_without_a_call() {
        for key in [
            "DOCKER_HOST",
            "LD_PRELOAD",
            "DYLD_X",
            "XDG_RUNTIME_DIR",
            "BUILDKIT_X",
            "COMPOSE_X",
            "CONTAINER_X",
        ] {
            for spelled in [key.to_string(), key.to_ascii_lowercase()] {
                let mut command = command_with_env("s3cr3t-value");
                command
                    .env
                    .insert(spelled.clone(), "s3cr3t-value".to_string());
                let sandbox = sandbox(Vec::new());
                let id = SandboxId("abc".to_string());
                let error = sandbox.run(&id, &command).expect_err("reserved");
                assert!(
                    matches!(error, SandboxError::Rejected(_)),
                    "{spelled}: {error:?}"
                );
                assert!(error.to_string().contains(&spelled), "{spelled}: {error}");
                assert!(
                    !error.to_string().contains("s3cr3t-value"),
                    "{spelled}: {error}"
                );
                assert!(
                    sandbox.runner.calls().is_empty(),
                    "{spelled} reached the runner"
                );
            }
        }
    }

    #[test]
    fn run_accepts_ordinary_env_names() {
        for key in [
            "API_KEY",
            "TOKEN",
            "MY_PATH",
            "HOMEPAGE",
            "PATHS_X",
            "GIT_AUTHOR_NAME",
        ] {
            let mut command = command();
            command
                .env
                .insert(key.to_string(), "s3cr3t-value".to_string());
            let sandbox = sandbox(vec![Ok(ok(""))]);
            let id = SandboxId("abc".to_string());
            sandbox.run(&id, &command).expect("the key is accepted");
            assert_eq!(sandbox.runner.calls().len(), 1, "{key}");
        }
    }

    #[test]
    fn run_never_shows_an_env_value_in_debug_json_or_an_error() {
        let mut command = command();
        command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        command
            .env
            .insert("OTHER".to_string(), "another-secret".to_string());
        let id = SandboxId("abc".to_string());

        let mut bad_program = command.clone();
        bad_program.program = String::new();
        let mut reserved = command.clone();
        reserved
            .env
            .insert("PATH".to_string(), "s3cr3t-value".to_string());

        let errors = [
            sandbox(Vec::new())
                .run(&id, &bad_program)
                .expect_err("empty program"),
            sandbox(Vec::new())
                .run(&id, &reserved)
                .expect_err("reserved"),
            sandbox(vec![Err(RunnerError(
                "cannot run docker: s3cr3t-value".to_string(),
            ))])
            .run(&id, &command)
            .expect_err("runner error"),
            sandbox(vec![Ok(fail(125, "boom s3cr3t-value"))])
                .run(&id, &command)
                .expect_err("exec fails"),
            sandbox(vec![
                Ok(timeout_output("")),
                Ok(fail(1, "boom another-secret")),
            ])
            .run(&id, &command)
            .expect_err("kill fails"),
            sandbox(vec![Ok(ToolOutput {
                status: None,
                stdout: String::new(),
                stderr: "died with s3cr3t-value".to_string(),
                timed_out: false,
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            })])
            .run(&id, &command)
            .expect_err("no status"),
        ];

        let mut texts = vec![
            format!("{command:?}"),
            serde_json::to_string(&command).expect("serializes"),
        ];
        for error in &errors {
            texts.push(error.to_string());
            texts.push(format!("{error:?}"));
        }
        for text in &texts {
            for value in ["s3cr3t-value", "another-secret"] {
                assert!(!text.contains(value), "{value} leaked: {text}");
            }
        }
    }

    #[test]
    fn run_redacts_a_runner_error_that_repeats_an_env_value() {
        let sandbox = sandbox(vec![Err(RunnerError(
            "cannot run docker: s3cr3t-value".to_string(),
        ))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect_err("runner error");
        assert!(error.to_string().contains("<redacted>"), "{error}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
        assert!(format!("{error:?}").contains("<redacted>"), "{error:?}");
        assert!(!format!("{error:?}").contains("s3cr3t-value"), "{error:?}");
    }

    #[test]
    fn run_redacts_a_failed_exec_stderr_that_repeats_an_env_value() {
        let sandbox = sandbox(vec![Ok(fail(125, "docker: s3cr3t-value"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect_err("exec fails");
        assert!(error.to_string().contains("<redacted>"), "{error}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
        assert!(format!("{error:?}").contains("<redacted>"), "{error:?}");
        assert!(!format!("{error:?}").contains("s3cr3t-value"), "{error:?}");
    }

    #[test]
    fn run_redacts_a_failed_kill_stderr_that_repeats_an_env_value() {
        let sandbox = sandbox(vec![
            Ok(timeout_output("")),
            Ok(fail(1, "kill failed: s3cr3t-value")),
        ]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect_err("kill fails");
        assert!(error.to_string().contains("<redacted>"), "{error}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
        assert!(format!("{error:?}").contains("<redacted>"), "{error:?}");
        assert!(!format!("{error:?}").contains("s3cr3t-value"), "{error:?}");
    }

    #[test]
    fn run_redacts_the_detail_of_a_failed_kill() {
        let sandbox = sandbox(vec![
            Ok(timeout_output("")),
            Ok(fail(1, "kill said s3cr3t-value")),
        ]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect_err("kill fails");
        assert_eq!(
            error,
            SandboxError::Failed(
                "the kill of the container abc after the command timed out failed: kill said <redacted>"
                    .to_string()
            )
        );
    }

    #[test]
    fn run_redacts_a_missing_status_message_that_repeats_an_env_value() {
        let sandbox = sandbox(vec![Ok(ToolOutput {
            status: None,
            stdout: String::new(),
            stderr: "died with s3cr3t-value".to_string(),
            timed_out: false,
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        })]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect_err("no status");
        assert!(error.to_string().contains("<redacted>"), "{error}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
    }

    #[test]
    fn run_keeps_a_successful_commands_stdout_unchanged() {
        let sandbox = sandbox(vec![Ok(ok("prints s3cr3t-value"))]);
        let id = SandboxId("abc".to_string());
        let result = sandbox
            .run(&id, &command_with_env("s3cr3t-value"))
            .expect("runs");
        assert_eq!(result.stdout, "prints s3cr3t-value");
    }

    #[test]
    fn run_does_not_redact_a_three_character_value() {
        let sandbox = sandbox(vec![Err(RunnerError("boom abc".to_string()))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, &command_with_env("abc"))
            .expect_err("runner error");
        assert!(error.to_string().contains("boom abc"), "{error}");
        assert!(!error.to_string().contains("<redacted>"), "{error}");
    }

    #[test]
    fn inspect_argv_pins_the_exact_vector() {
        assert_eq!(
            inspect_argv("abc"),
            strings(&[
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.Id}}",
                "--",
                "abc",
            ])
        );
    }

    #[test]
    fn destroy_inspects_then_removes_an_existing_container() {
        let sandbox = sandbox(vec![Ok(ok("abc")), Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        assert_eq!(sandbox.destroy(&id).expect("destroys"), Destroyed::Removed);
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: inspect_argv("abc"),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: remove_argv("abc"),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
            ]
        );
    }

    #[test]
    fn destroy_of_a_missing_container_reports_already_gone_with_one_inspect() {
        let sandbox = sandbox(vec![Ok(fail(
            1,
            "Error response from daemon: No such container: abc",
        ))]);
        let id = SandboxId("abc".to_string());
        assert_eq!(
            sandbox.destroy(&id).expect("destroys"),
            Destroyed::AlreadyGone
        );
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: inspect_argv("abc"),
                timeout: None,
                stdin: None,
                env: BTreeMap::new(),
            }]
        );
    }

    #[test]
    fn destroy_reports_an_inspect_failure_with_other_text_as_failed() {
        let sandbox = sandbox(vec![Ok(fail(1, "permission denied"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.destroy(&id).expect_err("destroy fails");
        assert_eq!(error, SandboxError::Failed("permission denied".to_string()));
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn destroy_reports_a_runner_error_on_inspect_as_failed() {
        let sandbox = sandbox(vec![Err(RunnerError("cannot run docker".to_string()))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.destroy(&id).expect_err("runner error");
        assert_eq!(error, SandboxError::Failed("cannot run docker".to_string()));
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn destroy_reports_an_inspect_timeout_as_failed() {
        let sandbox = sandbox(vec![Ok(timeout_output(""))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.destroy(&id).expect_err("destroy times out");
        assert_eq!(
            error,
            SandboxError::Failed("the tool timed out".to_string())
        );
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn destroy_of_a_container_removed_between_the_calls_reports_already_gone() {
        let sandbox = sandbox(vec![
            Ok(ok("abc")),
            Ok(fail(
                1,
                "Error response from daemon: No such container: abc",
            )),
        ]);
        let id = SandboxId("abc".to_string());
        assert_eq!(
            sandbox.destroy(&id).expect("destroys"),
            Destroyed::AlreadyGone
        );
        assert_eq!(sandbox.runner.calls().len(), 2);
    }

    #[test]
    fn capabilities_support_every_probe_and_run_the_probes_in_order() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3\n")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["bridge","null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(capabilities.platforms, vec!["local".to_string()]);
        assert_eq!(capabilities.runtime, Capability::Supported);
        assert_eq!(capabilities.image, Capability::Supported);
        assert_eq!(capabilities.network_isolation, Capability::Supported);
        assert_eq!(capabilities.mounts, Capability::Supported);
        assert_eq!(
            capabilities.stop_mid_run,
            Capability::Unsupported(
                "a run stops only by its timeout; no caller-initiated stop".to_string()
            )
        );
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: version_argv(),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: image_inspect_argv("example/base:1"),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
                Call {
                    argv: network_probe_argv(),
                    timeout: None,
                    stdin: None,
                    env: BTreeMap::new(),
                },
            ]
        );
    }

    #[test]
    fn capabilities_probes_a_dashed_image_after_a_double_dash() {
        let mut spec = spec();
        spec.image = "-x".to_string();
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec);
        assert_eq!(capabilities.image, Capability::Supported);
        let calls = sandbox.runner.calls();
        let inspect = calls.get(1).expect("the image probe");
        assert_eq!(inspect.argv, image_inspect_argv("-x"));
        let image_at = inspect
            .argv
            .iter()
            .position(|part| part == "-x")
            .expect("the image");
        let before = image_at.checked_sub(1).and_then(|at| inspect.argv.get(at));
        assert_eq!(before.map(String::as_str), Some("--"));
    }

    #[test]
    fn capabilities_report_a_missing_image_as_unsupported() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(fail(1, "Error: No such image: example/base:1")),
            Ok(ok(r#"["null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.image,
            Capability::Unsupported("image not present: example/base:1".to_string())
        );
    }

    #[test]
    fn capabilities_report_a_runtime_without_the_null_driver_as_unsupported() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["bridge","host"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.network_isolation,
            Capability::Unsupported("the runtime has no null network driver".to_string())
        );
    }

    #[test]
    fn capabilities_report_an_unknown_runtime_on_a_failed_version_probe() {
        let sandbox = sandbox(vec![
            Ok(fail(1, "cannot connect")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.runtime,
            Capability::Unknown("cannot connect".to_string())
        );
    }

    #[test]
    fn capabilities_report_an_empty_version_probe_as_unknown() {
        let sandbox = sandbox(vec![
            Ok(ok("")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.runtime,
            Capability::Unknown("the tool exited with status 0".to_string())
        );
    }

    #[test]
    fn capabilities_report_an_unknown_image_on_an_unexpected_failure() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(fail(1, "permission denied")),
            Ok(ok(r#"["null"]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.image,
            Capability::Unknown("permission denied".to_string())
        );
    }

    #[test]
    fn capabilities_report_an_unknown_network_on_unparsable_output() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok("not json")),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(
            capabilities.network_isolation,
            Capability::Unknown("not json".to_string())
        );
    }

    #[test]
    fn capabilities_report_an_unknown_network_for_a_non_array_probe() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"{"bridge":true}"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert!(matches!(
            capabilities.network_isolation,
            Capability::Unknown(_)
        ));
    }

    #[test]
    fn capabilities_report_an_unknown_network_for_a_non_string_array() {
        let sandbox = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["null",7]"#)),
        ]);
        let capabilities = sandbox.capabilities(&spec());
        assert!(matches!(
            capabilities.network_isolation,
            Capability::Unknown(_)
        ));
    }

    #[test]
    fn capabilities_report_each_probe_runner_error_as_unknown() {
        let runtime = sandbox(vec![
            Err(RunnerError("cannot run docker".to_string())),
            Ok(ok("sha256:deadbeef")),
            Ok(ok(r#"["null"]"#)),
        ]);
        assert_eq!(
            runtime.capabilities(&spec()).runtime,
            Capability::Unknown("cannot run docker".to_string())
        );
        let image = sandbox(vec![
            Ok(ok("1.2.3")),
            Err(RunnerError("image boom".to_string())),
            Ok(ok(r#"["null"]"#)),
        ]);
        assert_eq!(
            image.capabilities(&spec()).image,
            Capability::Unknown("image boom".to_string())
        );
        let network = sandbox(vec![
            Ok(ok("1.2.3")),
            Ok(ok("sha256:deadbeef")),
            Err(RunnerError("network boom".to_string())),
        ]);
        assert_eq!(
            network.capabilities(&spec()).network_isolation,
            Capability::Unknown("network boom".to_string())
        );
    }

    #[test]
    fn tool_text_prefers_stderr_then_stdout_then_a_fallback() {
        assert_eq!(
            tool_text(&ToolOutput {
                status: Some(1),
                stdout: "out".to_string(),
                stderr: "err".to_string(),
                timed_out: false,
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            }),
            "err"
        );
        assert_eq!(
            tool_text(&ToolOutput {
                status: Some(1),
                stdout: "out".to_string(),
                stderr: String::new(),
                timed_out: false,
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            }),
            "out"
        );
        assert_eq!(tool_text(&fail(4, "")), "the tool exited with status 4");
        assert_eq!(
            tool_text(&ToolOutput {
                status: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
            }),
            "the tool ended without an exit status"
        );
    }

    /// The marker `read_capped` puts between the kept halves.
    fn omission_marker(omitted: usize) -> String {
        format!("\n[... {omitted} bytes omitted ...]\n")
    }

    /// A reader that counts every byte a caller reads from it.
    struct CountingReader {
        data: std::io::Cursor<Vec<u8>>,
        read: usize,
    }

    impl std::io::Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = std::io::Read::read(&mut self.data, buffer)?;
            self.read = self.read.saturating_add(read);
            Ok(read)
        }
    }

    #[test]
    fn read_capped_keeps_everything_under_the_cap() {
        for input in [&b"hello"[..], &b"hello!"[..]] {
            let mut pipe = std::io::Cursor::new(input.to_vec());
            let (text, truncated) = read_capped(&mut pipe, 10);
            assert_eq!(text.as_bytes(), input);
            assert!(!truncated);
            assert_eq!(
                pipe.position(),
                u64::try_from(input.len()).expect("the length fits")
            );
        }
    }

    #[test]
    fn read_capped_keeps_everything_exactly_at_the_cap() {
        let mut pipe = std::io::Cursor::new(b"hello".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 5);
        assert_eq!(text, "hello");
        assert!(!truncated);
        assert!(!text.contains("bytes omitted"), "{text:?}");
    }

    #[test]
    fn read_capped_one_over_the_cap_keeps_both_halves_and_the_omitted_count() {
        let mut pipe = std::io::Cursor::new(b"hello!".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 5);
        assert!(truncated);
        assert_eq!(text, format!("he{}lo!", omission_marker(1)));
        assert_eq!(text.matches("bytes omitted").count(), 1, "{text:?}");
        assert_eq!(pipe.position(), 6, "the reader drains the whole pipe");
    }

    #[test]
    fn read_capped_keeps_the_last_line_of_a_long_chunked_stream() {
        let cap = 16 * 1024;
        let mut input = vec![b'x'; 40 * 8192];
        input.extend_from_slice(b"\nFINAL-EVENT\n");
        let total = input.len();
        let mut pipe = std::io::Cursor::new(input);

        let (text, truncated) = read_capped(&mut pipe, u64::try_from(cap).expect("the cap fits"));

        assert!(truncated);
        assert!(text.starts_with('x'), "{text:?}");
        assert!(text.ends_with("\nFINAL-EVENT\n"), "{text:?}");
        assert_eq!(text.matches("bytes omitted").count(), 1, "{text:?}");
        assert!(
            text.len() <= cap + omission_marker(total - cap).len(),
            "{}",
            text.len()
        );
    }

    #[test]
    fn read_capped_zero_cap_keeps_only_the_marker() {
        let mut pipe = std::io::Cursor::new(b"hello".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 0);
        assert!(truncated);
        assert_eq!(text, omission_marker(5));
        assert_eq!(pipe.position(), 5);
    }

    #[test]
    fn read_capped_one_byte_cap_keeps_the_last_byte_only() {
        let mut pipe = std::io::Cursor::new(b"hello".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 1);
        assert!(truncated);
        assert_eq!(text, format!("{}o", omission_marker(4)));
        assert_eq!(pipe.position(), 5);
    }

    #[test]
    fn read_capped_an_odd_cap_gives_the_extra_byte_to_the_tail() {
        let mut pipe = std::io::Cursor::new(b"abcdefghij".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 7);
        assert!(truncated);
        assert_eq!(text, format!("abc{}ghij", omission_marker(3)));
    }

    #[test]
    fn read_capped_does_not_split_a_multibyte_character_at_the_head() {
        let mut input = b"aaaa".to_vec();
        input.extend_from_slice("é".as_bytes());
        input.extend_from_slice(b"zzzzzz");
        let mut pipe = std::io::Cursor::new(input);
        let (text, truncated) = read_capped(&mut pipe, 10);
        assert!(truncated);
        assert_eq!(text, format!("aaaa{}zzzzz", omission_marker(3)));
        assert!(!text.contains('\u{fffd}'), "{text:?}");
    }

    #[test]
    fn read_capped_does_not_split_a_multibyte_character_at_the_tail() {
        let mut input = vec![b'z'; 6];
        input.extend_from_slice("€".as_bytes());
        input.extend_from_slice(b"aaaa");
        let mut pipe = std::io::Cursor::new(input);
        let (text, truncated) = read_capped(&mut pipe, 10);
        assert!(truncated);
        assert_eq!(text, format!("zzzzz{}aaaa", omission_marker(4)));
        assert!(!text.contains('\u{fffd}'), "{text:?}");
    }

    #[test]
    fn read_capped_keeps_invalid_utf8_lossy() {
        let mut pipe = std::io::Cursor::new(vec![0xff, b'a']);
        let (text, truncated) = read_capped(&mut pipe, 10);
        assert_eq!(text, "\u{fffd}a");
        assert!(!truncated);
    }

    #[test]
    fn read_capped_keeps_a_real_invalid_byte_lossy_over_the_cap() {
        let mut pipe = std::io::Cursor::new(vec![b'a', 0xff, b'b', b'c', b'd']);
        let (text, truncated) = read_capped(&mut pipe, 4);
        assert!(truncated);
        assert_eq!(text, format!("a\u{fffd}{}cd", omission_marker(1)));
    }

    #[test]
    fn read_capped_drains_the_whole_pipe_of_a_cut_stream() {
        let input = vec![b'a'; 40_000];
        let total = input.len();
        let mut pipe = CountingReader {
            data: std::io::Cursor::new(input),
            read: 0,
        };
        let (text, truncated) = read_capped(&mut pipe, 1024);
        assert!(truncated);
        assert_eq!(pipe.read, total, "the reader reached the end");
        assert!(
            text.len() <= 1024 + omission_marker(total - 1024).len(),
            "{}",
            text.len()
        );
    }

    #[test]
    fn run_copies_the_output_cap_and_flags_on_an_exit() {
        let sandbox = sandbox(vec![Ok(ToolOutput {
            status: Some(0),
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            timed_out: false,
            output_cap_bytes: Some(10),
            stdout_truncated: true,
            stderr_truncated: true,
        })]);
        let id = SandboxId("abc".to_string());
        assert_eq!(
            sandbox.run(&id, &command()).expect("runs"),
            RunResult {
                outcome: RunOutcome::Exited(0),
                stdout: "out".to_string(),
                stderr: "err".to_string(),
                output_cap_bytes: Some(10),
                stdout_truncated: true,
                stderr_truncated: true,
            }
        );
    }

    #[test]
    fn run_copies_the_output_cap_and_flags_on_a_timeout() {
        let sandbox = sandbox(vec![
            Ok(ToolOutput {
                status: None,
                stdout: "partial".to_string(),
                stderr: "err".to_string(),
                timed_out: true,
                output_cap_bytes: Some(7),
                stdout_truncated: true,
                stderr_truncated: false,
            }),
            Ok(ok("")),
        ]);
        let id = SandboxId("abc".to_string());
        assert_eq!(
            sandbox.run(&id, &command()).expect("runs"),
            RunResult {
                outcome: RunOutcome::TimedOut { limit_secs: 30 },
                stdout: "partial".to_string(),
                stderr: "err".to_string(),
                output_cap_bytes: Some(7),
                stdout_truncated: true,
                stderr_truncated: false,
            }
        );
    }

    #[test]
    fn default_runner_carries_the_default_cap() {
        let runner = RealDockerRunner::new("docker");
        assert_eq!(runner.output_cap_bytes, DEFAULT_OUTPUT_CAP_BYTES);
    }

    #[test]
    fn with_output_cap_sets_the_cap() {
        let runner = RealDockerRunner::new("docker").with_output_cap(10);
        assert_eq!(runner.output_cap_bytes, 10);
    }

    /// The next unique name a [`RealFolder`] takes.
    static NEXT_FOLDER: AtomicU64 = AtomicU64::new(0);

    /// A unique real folder under the temp directory, removed on drop.
    struct RealFolder {
        path: PathBuf,
    }

    impl RealFolder {
        fn new(name: &str) -> Self {
            let counter = NEXT_FOLDER.fetch_add(1, Ordering::SeqCst);
            let temp = std::env::temp_dir(); // osf: temp-dir allowed, unique per process and counter
            let base = std::fs::canonicalize(&temp).unwrap_or(temp);
            let path = base.join(format!(
                "osf-resolve-{name}-{}-{counter}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("the real folder creates");
            Self { path }
        }

        fn text(&self) -> String {
            self.path.to_string_lossy().into_owned()
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for RealFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// One mount with `follow_symlinks` set by the caller.
    fn mount(host_path: &str, sandbox_path: &str, follow_symlinks: bool) -> Mount {
        Mount {
            host_path: host_path.to_string(),
            sandbox_path: sandbox_path.to_string(),
            read_only: false,
            follow_symlinks,
        }
    }

    /// The workspace spec carrying exactly `mounts`.
    fn mounts_spec(mounts: Vec<Mount>) -> SandboxSpec {
        let mut spec = spec();
        spec.mounts = mounts;
        spec
    }

    #[test]
    fn resolve_mounts_rewrites_a_normal_folder_to_its_canonical_form() {
        let folder = RealFolder::new("normal");
        let project = folder.join("project");
        std::fs::create_dir_all(&project).expect("the project creates");
        let spec = mounts_spec(vec![mount(&project.to_string_lossy(), "/work", false)]);
        let resolved = resolve_mounts(&spec).expect("the folder resolves");
        assert_eq!(
            resolved.mounts.first().map(|mount| mount.host_path.clone()),
            Some(project.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn resolve_mounts_refuses_a_missing_source() {
        let folder = RealFolder::new("missing");
        let missing = folder.join("does-not-exist");
        let spec = mounts_spec(vec![mount(&missing.to_string_lossy(), "/work", false)]);
        let error = resolve_mounts(&spec).expect_err("the source is missing");
        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn resolve_mounts_refuses_the_host_root() {
        let spec = mounts_spec(vec![mount("/", "/work", false)]);
        let error = resolve_mounts(&spec).expect_err("the root is refused");
        assert!(error.to_string().contains("root of the host"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_refuses_a_symlink_to_a_folder_unless_following() {
        let folder = RealFolder::new("symlink");
        let real = folder.join("real");
        std::fs::create_dir_all(&real).expect("the real folder creates");
        let link = folder.join("link");
        std::os::unix::fs::symlink(&real, &link).expect("the symlink creates");

        let refused = resolve_mounts(&mounts_spec(vec![mount(
            &link.to_string_lossy(),
            "/work",
            false,
        )]));
        let error = refused.expect_err("the symlink is refused");
        assert!(error.to_string().contains("symbolic link"), "{error}");

        let followed = resolve_mounts(&mounts_spec(vec![mount(
            &link.to_string_lossy(),
            "/work",
            true,
        )]))
        .expect("following the symlink resolves");
        assert_eq!(
            followed.mounts.first().map(|mount| mount.host_path.clone()),
            Some(real.to_string_lossy().into_owned())
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_refuses_a_symlink_to_the_host_root_even_when_following() {
        let folder = RealFolder::new("root-link");
        let link = folder.join("root-link");
        std::os::unix::fs::symlink("/", &link).expect("the symlink creates");

        let refused = resolve_mounts(&mounts_spec(vec![mount(
            &link.to_string_lossy(),
            "/work",
            false,
        )]));
        assert!(refused.is_err(), "the root symlink is refused");

        let followed = resolve_mounts(&mounts_spec(vec![mount(
            &link.to_string_lossy(),
            "/work",
            true,
        )]));
        let error = followed.expect_err("the root is still refused");
        assert!(error.to_string().contains("root of the host"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_refuses_a_source_under_a_symlinked_parent() {
        let folder = RealFolder::new("parent-link");
        let real_parent = folder.join("real-parent");
        let child = real_parent.join("child");
        std::fs::create_dir_all(&child).expect("the child creates");
        let link_parent = folder.join("link-parent");
        std::os::unix::fs::symlink(&real_parent, &link_parent).expect("the parent link creates");
        let linked_child = link_parent.join("child");

        let error = resolve_mounts(&mounts_spec(vec![mount(
            &linked_child.to_string_lossy(),
            "/work",
            false,
        )]))
        .expect_err("the symlinked parent is refused");
        assert!(error.to_string().contains("symbolic link"), "{error}");
    }

    /// The canonical path text of `path`, for a mount source and its message.
    fn canonical_text(path: &Path) -> String {
        std::fs::canonicalize(path)
            .expect("the path canonicalizes")
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_refuses_a_socket_at_the_top_level_of_a_folder() {
        let folder = crate::test_support::TempDir::new("osf-resolve-socket-folder");
        let text = canonical_text(folder.as_ref());
        let listener = std::os::unix::net::UnixListener::bind(folder.join("socket"))
            .expect("the socket binds");
        let spec = mounts_spec(vec![mount(&text, "/work", false)]);
        let error = resolve_mounts(&spec).expect_err("the top-level socket is refused");
        assert_eq!(
            error.to_string(),
            format!("the mount source holds a socket at its top level: {text}")
        );
        drop(listener);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_accepts_a_socket_below_the_top_level() {
        let folder = crate::test_support::TempDir::new("osf-resolve-socket-subfolder");
        let text = canonical_text(folder.as_ref());
        let sub = folder.join("sub");
        std::fs::create_dir_all(&sub).expect("the subfolder creates");
        let listener =
            std::os::unix::net::UnixListener::bind(sub.join("socket")).expect("the socket binds");
        let spec = mounts_spec(vec![mount(&text, "/work", false)]);
        resolve_mounts(&spec).expect("a socket below the top level passes");
        drop(listener);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_refuses_a_source_that_is_a_socket() {
        let folder = crate::test_support::TempDir::new("osf-resolve-socket-itself");
        let socket = folder.join("socket");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("the socket binds");
        let text = canonical_text(&socket);
        let spec = mounts_spec(vec![mount(&text, "/work", false)]);
        let error = resolve_mounts(&spec).expect_err("the socket is refused");
        assert_eq!(
            error.to_string(),
            format!("the mount source is a socket: {text}")
        );
        drop(listener);
    }

    #[test]
    fn resolve_mounts_accepts_ordinary_files_and_a_plain_subfolder() {
        let folder = RealFolder::new("ordinary");
        std::fs::write(folder.join("file.txt"), "data").expect("the file writes");
        std::fs::create_dir_all(folder.join("sub")).expect("the subfolder creates");
        let spec = mounts_spec(vec![mount(&folder.text(), "/work", false)]);
        resolve_mounts(&spec).expect("ordinary contents pass");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_mounts_refuses_a_linux_system_path() {
        for path in ["/run", "/proc", "/var/run", "/proc/self"] {
            if !Path::new(path).exists() {
                continue;
            }
            let spec = mounts_spec(vec![mount(path, "/work", true)]);
            let error = resolve_mounts(&spec).expect_err(path);
            assert!(
                error.to_string().contains(
                    "the mount source must not be, or lie under, /run, /var/run or /proc"
                ),
                "{path}: {error}"
            );
            assert!(error.to_string().contains(path), "{path}: {error}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_mounts_refuses_a_symlink_that_resolves_to_proc() {
        let folder = RealFolder::new("proc-link");
        let link = folder.join("link");
        std::os::unix::fs::symlink("/proc", &link).expect("the symlink creates");
        let spec = mounts_spec(vec![mount(&link.to_string_lossy(), "/work", true)]);
        let error = resolve_mounts(&spec).expect_err("the real path is refused");
        assert!(
            error
                .to_string()
                .contains("the mount source must not be, or lie under, /run, /var/run or /proc"),
            "{error}"
        );
    }

    #[test]
    fn resolve_mounts_accepts_a_folder_whose_name_starts_like_a_forbidden_path() {
        let folder = RealFolder::new("whole-component");
        let runner = folder.join("runner-xyz");
        std::fs::create_dir_all(&runner).expect("the folder creates");
        let spec = mounts_spec(vec![mount(&runner.to_string_lossy(), "/work", false)]);
        resolve_mounts(&spec).expect("a name starting with the same letters passes");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_mounts_socket_error_names_only_the_host_path() {
        let folder = crate::test_support::TempDir::new("osf-resolve-socket-error");
        let text = canonical_text(folder.as_ref());
        let listener = std::os::unix::net::UnixListener::bind(folder.join("hidden.sock"))
            .expect("the socket binds");
        let spec = mounts_spec(vec![mount(&text, "/work", false)]);
        let error = resolve_mounts(&spec).expect_err("the socket is refused");
        assert_eq!(
            error.to_string(),
            format!("the mount source holds a socket at its top level: {text}")
        );
        assert!(!error.to_string().contains("hidden.sock"), "{error}");
        drop(listener);
    }

    #[test]
    fn resolve_mounts_refuses_a_mount_that_is_a_parent_of_the_repository() {
        let folder = RealFolder::new("parent");
        let repo = folder.join("repo");
        std::fs::create_dir_all(&repo).expect("the repo creates");
        let spec = mounts_spec(vec![
            mount(&folder.text(), "/state", false),
            mount(&repo.to_string_lossy(), "/work", false),
        ]);
        let error = resolve_mounts(&spec).expect_err("the parent is refused");
        assert!(error.to_string().contains("parent"), "{error}");
    }

    #[test]
    fn resolve_mounts_refuses_a_mount_that_is_the_repository() {
        let folder = RealFolder::new("same");
        let repo = folder.join("repo");
        std::fs::create_dir_all(&repo).expect("the repo creates");
        let source = repo.to_string_lossy();
        let spec = mounts_spec(vec![
            mount(&source, "/state", false),
            mount(&source, "/work", false),
        ]);
        let error = resolve_mounts(&spec).expect_err("the repository is refused");
        assert!(error.to_string().contains("repository"), "{error}");
    }

    #[test]
    fn resolve_mounts_accepts_two_sibling_folders() {
        let folder = RealFolder::new("siblings");
        let repo = folder.join("repo");
        let state = folder.join("state");
        std::fs::create_dir_all(&repo).expect("the repo creates");
        std::fs::create_dir_all(&state).expect("the state creates");
        let spec = mounts_spec(vec![
            mount(&repo.to_string_lossy(), "/work", false),
            mount(&state.to_string_lossy(), "/state", false),
        ]);
        resolve_mounts(&spec).expect("the siblings pass");
    }
}
