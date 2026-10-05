//! The Docker [`Sandbox`] adapter: one `docker` invocation per operation,
//! with every argv and every response built and read by a pure function.

use crate::sandbox::{
    Capability, CommandSpec, Destroyed, Limits, Mount, Network, RunOutcome, RunResult, Sandbox,
    SandboxCapabilities, SandboxError, SandboxId, SandboxSpec,
};
use std::collections::VecDeque;
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

/// The largest accepted process count: 2^22.
const MAX_PROCESSES: u32 = 4_194_304;

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
    /// Runs `argv`, writing `stdin` to the program's standard input then closing the pipe; with none, standard input is null, and waits at most `timeout`.
    ///
    /// # Errors
    /// Returns a [`RunnerError`] when the program cannot start or a reader panics.
    fn run_with_stdin(
        &self,
        argv: &[String],
        timeout: Option<Duration>,
        stdin: Option<&[u8]>,
    ) -> Result<ToolOutput, RunnerError>;

    /// Runs `argv` with no standard input, waiting at most `timeout`.
    ///
    /// # Errors
    /// Returns a [`RunnerError`] when the program cannot start or a reader panics.
    fn run(&self, argv: &[String], timeout: Option<Duration>) -> Result<ToolOutput, RunnerError> {
        self.run_with_stdin(argv, timeout, None)
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
    ) -> Result<ToolOutput, RunnerError> {
        let mut command = Command::new(&self.program);
        command
            .args(argv)
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

/// The create output for a failure: trimmed, cut to 200 characters, then debug-formatted.
fn shown_create_output(stdout: &str) -> String {
    let trimmed = stdout.trim();
    let mut shown: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        shown.push_str("...");
    }
    format!("{shown:?}")
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
/// A local user can see each `--env` value in the process list while the command runs.
#[must_use]
pub fn exec_argv(id: &str, command: &CommandSpec) -> Vec<String> {
    let mut argv = vec!["exec".to_string()];
    if let Some(workdir) = &command.workdir {
        argv.push(format!("--workdir={workdir}"));
    }
    for (key, value) in &command.env {
        argv.push(format!("--env={key}={value}"));
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

/// Whether `image` is pinned: a valid reference with a digest or a non-`latest` tag.
#[must_use]
pub fn image_is_pinned(image: &str) -> bool {
    validate_image(image).is_ok()
}

/// One image rejection: the reason, then the image text it names.
fn rejected_image(reason: &str, image: &str) -> SandboxError {
    SandboxError::Rejected(format!("{reason}: {image}"))
}

/// Checks `image` as `name[:tag][@digest]` and requires it to be pinned.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an empty, dashed, malformed or
/// unpinned image.
fn validate_image(image: &str) -> Result<(), SandboxError> {
    if image.is_empty() {
        return Err(rejected_image("the image must not be empty", image));
    }
    if image.starts_with('-') {
        return Err(rejected_image("the image must not start with '-'", image));
    }
    if !is_valid_reference(image) {
        return Err(rejected_image("the image is not a valid reference", image));
    }
    if is_pinned_reference(image) {
        return Ok(());
    }
    Err(rejected_image(
        "the image must be pinned by digest or a non-latest tag",
        image,
    ))
}

/// Whether `image` matches `name[:tag][@digest]`.
fn is_valid_reference(image: &str) -> bool {
    if image.matches('@').count() > 1 {
        return false;
    }
    let (reference, digest) = match image.split_once('@') {
        Some((reference, digest)) => (reference, Some(digest)),
        None => (image, None),
    };
    if digest.is_some_and(|digest| !is_digest(digest)) {
        return false;
    }
    let (name, tag) = split_tag(reference);
    tag.is_none_or(is_tag) && is_name(name)
}

/// Whether a valid `image` carries a digest or a non-`latest` tag.
fn is_pinned_reference(image: &str) -> bool {
    if image.contains('@') {
        return true;
    }
    split_tag(image).1.is_some_and(|tag| tag != "latest")
}

/// Splits `reference` into its name and optional tag: the last `:` after the
/// last `/` (or in the whole text when it holds no `/`); a `:` before the last
/// `/` is a registry port.
fn split_tag(reference: &str) -> (&str, Option<&str>) {
    let start = reference.rfind('/').map_or(0, |slash| slash + 1);
    let Some(colon) = reference.get(start..).and_then(|rest| rest.rfind(':')) else {
        return (reference, None);
    };
    let colon = start + colon;
    let name = reference.get(..colon).unwrap_or_default();
    let tag = reference.get(colon + 1..).unwrap_or_default();
    (name, Some(tag))
}

/// Whether `digest` is `sha256:` then exactly 64 lowercase hex characters.
fn is_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `tag` is a letter, digit or `_`, then up to 127 more letters,
/// digits, `.`, `_` or `-`.
fn is_tag(tag: &str) -> bool {
    let mut bytes = tag.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() && first != b'_' {
        return false;
    }
    tag.len() <= 128
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Whether `name` is `[registry/]path`, with at least one path component.
fn is_name(name: &str) -> bool {
    let components: Vec<&str> = name.split('/').collect();
    let Some(first) = components.first().copied() else {
        return false;
    };
    let path_start = if components.len() > 1
        && (first.contains('.') || first.contains(':') || first == "localhost")
    {
        if !is_registry_host(first) {
            return false;
        }
        1
    } else {
        0
    };
    let Some(path) = components.get(path_start..) else {
        return false;
    };
    !path.is_empty() && path.iter().all(|part| is_path_component(part))
}

/// Whether `component` is `host` or `host:port`: dot-separated labels, then
/// one to five port digits.
fn is_registry_host(component: &str) -> bool {
    let (host, port) = match component.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (component, None),
    };
    if port.is_some_and(|port| !is_port(port)) {
        return false;
    }
    !host.is_empty() && host.split('.').all(is_host_label)
}

/// Whether `port` is one to five ASCII digits.
fn is_port(port: &str) -> bool {
    (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `label` starts and ends with a letter or digit and holds only
/// letters, digits and `-` between them.
fn is_host_label(label: &str) -> bool {
    let (Some(first), Some(last)) = (label.as_bytes().first(), label.as_bytes().last()) else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Whether `part` is lowercase letters and digits joined by `.`, `_`, `__`
/// or one or more `-`.
fn is_path_component(part: &str) -> bool {
    let bytes = part.as_bytes();
    let mut index = 0;
    while bytes.get(index).copied().is_some_and(is_path_byte) {
        index += 1;
    }
    if index == 0 {
        return false;
    }
    while index < bytes.len() {
        match bytes.get(index) {
            Some(b'.') => index += 1,
            Some(b'-') => {
                while bytes.get(index) == Some(&b'-') {
                    index += 1;
                }
            }
            Some(b'_') => {
                index += 1;
                if bytes.get(index) == Some(&b'_') {
                    index += 1;
                }
            }
            _ => return false,
        }
        let token_start = index;
        while bytes.get(index).copied().is_some_and(is_path_byte) {
            index += 1;
        }
        if index == token_start {
            return false;
        }
    }
    true
}

/// Whether `byte` may appear in a path component word: a lowercase letter or digit.
fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

/// Checks `spec` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an invalid or unpinned image, an
/// empty, root, zero or malformed user, a relative workdir, a malformed name,
/// a bad mount, or a bad resource limit.
pub fn validate_spec(spec: &SandboxSpec) -> Result<(), SandboxError> {
    validate_image(&spec.image)?;
    validate_user(&spec.user)?;
    if !spec.workdir.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "the workdir must be absolute: {}",
            spec.workdir
        )));
    }
    validate_name(&spec.name)?;
    for mount in &spec.mounts {
        validate_mount(mount)?;
    }
    validate_limits(&spec.limits)?;
    Ok(())
}

/// Checks the resource limits before any runner call.
fn validate_limits(limits: &Limits) -> Result<(), SandboxError> {
    if limits.max_processes == 0 || limits.max_processes > MAX_PROCESSES {
        return Err(SandboxError::Rejected(format!(
            "the max_processes must be between 1 and {MAX_PROCESSES}: {}",
            limits.max_processes
        )));
    }
    if !is_memory_limit(&limits.memory) {
        return Err(SandboxError::Rejected(format!(
            "the memory must be a positive size with an optional b, k, m or g suffix: {}",
            limits.memory
        )));
    }
    Ok(())
}

/// Whether `memory` is 1-12 digits without a leading zero, then an optional b, k, m or g.
fn is_memory_limit(memory: &str) -> bool {
    let digits = ["b", "k", "m", "g"]
        .iter()
        .find_map(|suffix| memory.strip_suffix(*suffix))
        .unwrap_or(memory);
    let mut bytes = digits.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    matches!(first, b'1'..=b'9') && digits.len() <= 12 && bytes.all(|byte| byte.is_ascii_digit())
}

/// Checks one user string as `name[:group]`, neither part root or zero.
fn validate_user(user: &str) -> Result<(), SandboxError> {
    if user.is_empty() {
        return Err(SandboxError::Rejected(
            "the user must not be empty".to_string(),
        ));
    }
    let mut parts = user.split(':');
    let Some(first) = parts.next() else {
        return Err(SandboxError::Rejected(format!(
            "each user part must not be empty: {user}"
        )));
    };
    validate_user_part(user, first)?;
    if let Some(second) = parts.next() {
        validate_user_part(user, second)?;
    }
    if parts.next().is_some() {
        return Err(SandboxError::Rejected(format!(
            "the user must hold at most one ':': {user}"
        )));
    }
    Ok(())
}

/// Checks one user part: a positive number without a leading zero, or a name.
fn validate_user_part(user: &str, part: &str) -> Result<(), SandboxError> {
    if part.is_empty() {
        return Err(SandboxError::Rejected(format!(
            "each user part must not be empty: {user}"
        )));
    }
    if part.bytes().all(|byte| byte.is_ascii_digit()) {
        return validate_user_number(user, part);
    }
    if !is_user_name(part) {
        return Err(SandboxError::Rejected(format!(
            "a user name must be a letter or '_' then letters, digits, '_' and '-': {user}"
        )));
    }
    if part == "root" {
        return Err(SandboxError::Rejected(format!(
            "the user must not be root: {user}"
        )));
    }
    Ok(())
}

/// Checks a numeric user part: no leading zero, a `u32`, and not zero.
fn validate_user_number(user: &str, part: &str) -> Result<(), SandboxError> {
    if part.len() > 1 && part.starts_with('0') {
        return Err(SandboxError::Rejected(format!(
            "a user number must not have a leading zero: {user}"
        )));
    }
    match part.parse::<u32>() {
        Ok(0) => Err(SandboxError::Rejected(format!(
            "the user must not be root: {user}"
        ))),
        Ok(_) => Ok(()),
        Err(_) => Err(SandboxError::Rejected(format!(
            "a user number must fit in a u32: {user}"
        ))),
    }
}

/// Whether `part` matches `[A-Za-z_][A-Za-z0-9_-]*` over bytes.
fn is_user_name(part: &str) -> bool {
    let mut bytes = part.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Checks one sandbox name: non-empty, then letters, digits, `_`, `.` and `-`.
fn validate_name(name: &str) -> Result<(), SandboxError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(SandboxError::Rejected(
            "the name must not be empty".to_string(),
        ));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(SandboxError::Rejected(format!(
            "the name must start with an ASCII letter or digit: {name}"
        )));
    }
    if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(*c, '_' | '.' | '-')))
    {
        return Err(SandboxError::Rejected(format!(
            "the name may hold only ASCII letters, digits, '_', '.' and '-': {name} ({bad})"
        )));
    }
    Ok(())
}

/// Checks one mount's paths: a non-empty absolute host, an absolute sandbox,
/// no `..` component, and no comma or newline.
fn validate_mount(mount: &Mount) -> Result<(), SandboxError> {
    if mount.host_path.is_empty() {
        return Err(SandboxError::Rejected(
            "a mount host path must not be empty".to_string(),
        ));
    }
    if !mount.host_path.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "a mount host path must be absolute: {}",
            mount.host_path
        )));
    }
    if !mount.sandbox_path.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "a mount sandbox path must be absolute: {}",
            mount.sandbox_path
        )));
    }
    for path in [&mount.host_path, &mount.sandbox_path] {
        if path.split('/').any(|component| component == "..") {
            return Err(SandboxError::Rejected(format!(
                "a mount path must not hold a '..' component: {path}"
            )));
        }
    }
    for path in [&mount.host_path, &mount.sandbox_path] {
        if path.contains(',') || path.contains('\n') || path.contains('\r') {
            return Err(SandboxError::Rejected(format!(
                "a mount path must not hold a comma or a newline: {path}"
            )));
        }
    }
    Ok(())
}

/// The index of the repository mount: the one whose sandbox path is the workdir.
fn repository_mount(spec: &SandboxSpec) -> Option<usize> {
    spec.mounts
        .iter()
        .position(|mount| mount.sandbox_path == spec.workdir)
}

/// Canonicalizes every mount source, refusing a missing source, a symlink
/// unless `follow_symlinks` is set, the host root, and a source that is, or
/// holds, the repository mount's own real source.
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

/// Checks `command` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an empty program, a relative workdir, a zero timeout, a malformed environment key, or an environment value with a NUL byte.
pub fn validate_command(command: &CommandSpec) -> Result<(), SandboxError> {
    if command.program.is_empty() {
        return Err(SandboxError::Rejected(
            "the program must not be empty".to_string(),
        ));
    }
    if let Some(workdir) = &command.workdir {
        if !workdir.starts_with('/') {
            return Err(SandboxError::Rejected(format!(
                "the workdir must be absolute: {workdir}"
            )));
        }
    }
    if command.timeout_secs == Some(0) {
        return Err(SandboxError::Rejected(
            "the timeout must be at least one second".to_string(),
        ));
    }
    for (key, value) in &command.env {
        if !is_env_key(key) {
            return Err(SandboxError::Rejected(format!(
                "an environment variable name is not valid: {key}"
            )));
        }
        if value.contains('\0') {
            return Err(SandboxError::Rejected(format!(
                "the environment variable {key} holds a NUL byte"
            )));
        }
    }
    Ok(())
}

/// Whether `key` matches `[A-Za-z_][A-Za-z0-9_]*`.
fn is_env_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
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
                shown_create_output(&output.stdout)
            )));
        };
        let id = container_id.to_string();
        if let Err(start_error) = self.start_container(&id) {
            let mut message = start_error.to_string();
            if let Err(removal_error) = self.remove_container(&id) {
                message = format!("{message}; {removal_error}");
            }
            return Err(SandboxError::Failed(message));
        }
        Ok(SandboxId(id))
    }

    fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
        validate_command(command)?;
        let limit_secs = command.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let timeout = Some(Duration::from_secs(limit_secs));
        let output = self
            .runner
            .run_with_stdin(
                &exec_argv(&id.0, command),
                timeout,
                command.stdin.as_deref(),
            )
            .map_err(|error| SandboxError::Failed(redact(&error.0, command)))?;

        if output.timed_out {
            let kill = self
                .runner
                .run(&kill_argv(&id.0), None)
                .map_err(|error| SandboxError::Failed(redact(&error.0, command)))?;
            if kill.status != Some(0) || kill.timed_out {
                return Err(SandboxError::Failed(redact(&tool_text(&kill), command)));
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

    /// Docker 29 `rm --force` of a missing container already exits 0, so that
    /// case reads as `Removed`; the `AlreadyGone` path covers runtimes that
    /// report it.
    fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError> {
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
        ) -> Result<ToolOutput, RunnerError> {
            lock(&self.calls).push(Call {
                argv: argv.to_vec(),
                timeout,
                stdin: stdin.map(<[u8]>::to_vec),
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

    /// A real folder source for a create test: the canonical temp folder.
    fn real_dir() -> String {
        let dir = std::env::temp_dir(); // osf: temp-dir allowed, only read as an existing folder
        std::fs::canonicalize(&dir)
            .unwrap_or(dir)
            .to_string_lossy()
            .into_owned()
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
        assert!(sandbox.runner.calls().is_empty());
    }

    fn run_rejected(command: &CommandSpec) {
        let sandbox = sandbox(Vec::new());
        let id = SandboxId("abc".to_string());
        let error = sandbox
            .run(&id, command)
            .expect_err("the command is rejected");
        assert!(matches!(error, SandboxError::Rejected(_)), "{error:?}");
        assert!(sandbox.runner.calls().is_empty());
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
    fn exec_argv_pins_env_entries_in_map_order() {
        let mut command = command();
        command.env.insert("B".to_string(), "2".to_string());
        command.env.insert("A".to_string(), "1".to_string());
        assert_eq!(
            exec_argv("abc", &command),
            strings(&[
                "exec",
                "--env=A=1",
                "--env=B=2",
                "--",
                "abc",
                "run",
                "--fast",
            ])
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
    fn exec_argv_pins_env_stdin_and_workdir_together() {
        let mut command = command();
        command.workdir = Some("/work".to_string());
        command.env.insert("B".to_string(), "2".to_string());
        command.env.insert("A".to_string(), "1".to_string());
        command.stdin = Some(b"input".to_vec());
        assert_eq!(
            exec_argv("abc", &command),
            strings(&[
                "exec",
                "--workdir=/work",
                "--env=A=1",
                "--env=B=2",
                "-i",
                "--",
                "abc",
                "run",
                "--fast",
            ])
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
    fn image_is_pinned_accepts_digests_and_explicit_tags() {
        let hex = "a".repeat(64);
        let long_tag = "a".repeat(128);
        for pinned in [
            "example/base:1".to_string(),
            "example/base:1.2".to_string(),
            "registry:5000/example/base:1.2".to_string(),
            "localhost/a:1".to_string(),
            format!("example/base@sha256:{hex}"),
            format!("example/base:1@sha256:{hex}"),
            "ghcr.io/org/img:v1_2-rc.1".to_string(),
            format!("example/base:{long_tag}"),
        ] {
            assert!(image_is_pinned(&pinned), "{pinned} is pinned");
        }
    }

    #[test]
    fn image_is_pinned_rejects_bad_references_latest_and_untagged() {
        let short = "a".repeat(63);
        let upper = "A".repeat(64);
        let wrong = "z".repeat(64);
        let long_tag = "a".repeat(129);
        let hex = "a".repeat(64);
        for unpinned in [
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
            assert!(!image_is_pinned(&unpinned), "{unpinned:?} is not pinned");
        }
    }

    #[test]
    fn validate_spec_accepts_every_valid_image() {
        let hex = "a".repeat(64);
        let long_tag = "a".repeat(128);
        for image in [
            "example/base:1".to_string(),
            "example/base:1.2".to_string(),
            "registry:5000/example/base:1.2".to_string(),
            "localhost/a:1".to_string(),
            format!("example/base@sha256:{hex}"),
            format!("example/base:1@sha256:{hex}"),
            "ghcr.io/org/img:v1_2-rc.1".to_string(),
            format!("example/base:{long_tag}"),
            "alpine:3.20".to_string(),
            "osf-devcontainer:pr151".to_string(),
            "registry:5000".to_string(),
            "localhost:5000/a:1".to_string(),
            format!("alpine@sha256:{hex}"),
        ] {
            let mut spec = spec();
            spec.image = image.clone();
            validate_spec(&spec).expect(&image);
        }
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
    fn validate_spec_accepts_a_safe_user_without_a_call() {
        for user in ["1", "dev", "dev:dev", "1000:1000", "dev:100", "_svc"] {
            let mut spec = spec();
            spec.user = user.to_string();
            validate_spec(&spec).expect("the user is accepted");
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
    fn validate_spec_accepts_the_largest_process_limit() {
        let mut spec = spec();
        spec.limits.max_processes = 4_194_304;
        validate_spec(&spec).expect("the limit is accepted");
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
    fn validate_spec_accepts_a_good_memory_limit() {
        for memory in ["1", "512m", "4g", "100000k", "7b", "123456789012"] {
            let mut spec = spec();
            spec.limits.memory = memory.to_string();
            validate_spec(&spec).expect("the memory is accepted");
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
    fn validate_command_accepts_good_env_keys() {
        for key in ["A", "_x", "API_KEY1"] {
            let mut command = command();
            command.env.insert(key.to_string(), "value".to_string());
            validate_command(&command).expect("the key is accepted");
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
        let sandbox = sandbox(vec![Ok(ok(&output)), Ok(ok(""))]);
        let id = sandbox.create(&real_spec()).expect("creates");
        assert_eq!(id, SandboxId(created.clone()));
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: create_argv(&resolved_real_spec()),
                    timeout: Some(Duration::from_secs(120)),
                    stdin: None,
                },
                Call {
                    argv: start_argv(&created),
                    timeout: None,
                    stdin: None,
                },
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
                },
                Call {
                    argv: start_argv(&created),
                    timeout: None,
                    stdin: None,
                },
                Call {
                    argv: remove_argv(&created),
                    timeout: None,
                    stdin: None,
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
                },
                Call {
                    argv: kill_argv("abc"),
                    timeout: None,
                    stdin: None,
                },
            ]
        );
    }

    #[test]
    fn a_failed_kill_after_a_timeout_is_failed() {
        let sandbox = sandbox(vec![Ok(timeout_output("")), Ok(fail(1, "boom kill"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.run(&id, &command()).expect_err("kill fails");
        assert_eq!(error, SandboxError::Failed("boom kill".to_string()));
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
            }]
        );
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
    fn destroy_removes_the_container() {
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        assert_eq!(sandbox.destroy(&id).expect("destroys"), Destroyed::Removed);
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: remove_argv("abc"),
                timeout: None,
                stdin: None,
            }]
        );
    }

    #[test]
    fn destroy_of_a_missing_container_reports_already_gone() {
        let sandbox = sandbox(vec![Ok(fail(
            1,
            "Error response from daemon: No such container: x",
        ))]);
        let id = SandboxId("abc".to_string());
        assert_eq!(
            sandbox.destroy(&id).expect("destroys"),
            Destroyed::AlreadyGone
        );
    }

    #[test]
    fn destroy_reports_a_failure() {
        let sandbox = sandbox(vec![Ok(fail(1, "boom remove"))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.destroy(&id).expect_err("destroy fails");
        assert_eq!(error, SandboxError::Failed("boom remove".to_string()));
    }

    #[test]
    fn destroy_reports_a_runner_error_as_failed() {
        let sandbox = sandbox(vec![Err(RunnerError("cannot run docker".to_string()))]);
        let id = SandboxId("abc".to_string());
        let error = sandbox.destroy(&id).expect_err("runner error");
        assert_eq!(error, SandboxError::Failed("cannot run docker".to_string()));
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
                },
                Call {
                    argv: image_inspect_argv("example/base:1"),
                    timeout: None,
                    stdin: None,
                },
                Call {
                    argv: network_probe_argv(),
                    timeout: None,
                    stdin: None,
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
