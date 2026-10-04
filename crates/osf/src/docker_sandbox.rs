//! The Docker [`Sandbox`] adapter: one `docker` invocation per operation,
//! with every argv and every response built and read by a pure function.

use crate::sandbox::{
    Capability, CommandSpec, Destroyed, Mount, Network, RunOutcome, RunResult, Sandbox,
    SandboxCapabilities, SandboxError, SandboxId, SandboxSpec,
};
use std::fmt;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt as _;

/// The command a created container runs until it is removed.
const KEEPALIVE_PROGRAM: &str = "sleep";
/// The argument of the keep-alive command: the largest 32-bit signed integer.
const KEEPALIVE_SECS: &str = "2147483647";

/// How long `create` waits for the image and the container.
const CREATE_TIMEOUT: Duration = Duration::from_secs(120);

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
pub trait DockerRunner {
    /// Runs `argv`, waiting at most `timeout`.
    ///
    /// # Errors
    /// Returns a [`RunnerError`] when the program cannot start or a reader panics.
    fn run(&self, argv: &[String], timeout: Option<Duration>) -> Result<ToolOutput, RunnerError>;
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
    fn run(&self, argv: &[String], timeout: Option<Duration>) -> Result<ToolOutput, RunnerError> {
        let mut command = Command::new(&self.program);
        command
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A timeout must stop the whole tree, or a grandchild can keep a pipe
        // open and the reader joins would outlive the timeout by far.
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| RunnerError(format!("cannot run docker: {error}")))?;

        // Both pipes are read on their own threads before the wait: a run
        // that fills one pipe buffer would otherwise block before it exits.
        let cap = self.output_cap_bytes;
        let stdout_reader = child
            .stdout
            .take()
            .map(|mut pipe| std::thread::spawn(move || read_capped(&mut pipe, cap)));
        let stderr_reader = child
            .stderr
            .take()
            .map(|mut pipe| std::thread::spawn(move || read_capped(&mut pipe, cap)));

        let waited = match timeout {
            Some(limit) => child.wait_timeout(limit),
            None => child.wait().map(Some),
        };

        match waited {
            Ok(Some(status)) => {
                let (stdout, stdout_truncated) = join_reader(stdout_reader)?;
                let (stderr, stderr_truncated) = join_reader(stderr_reader)?;
                Ok(ToolOutput {
                    status: status.code(),
                    stdout,
                    stderr,
                    timed_out: false,
                    output_cap_bytes: Some(self.output_cap_bytes),
                    stdout_truncated,
                    stderr_truncated,
                })
            }
            Ok(None) => {
                let killed = kill_tree(&child);
                let _ = child.kill();
                let _ = child.wait();
                if let Err(error) = killed {
                    return Err(RunnerError(format!(
                        "docker timed out and could not be killed: {error}"
                    )));
                }
                let (stdout, stdout_truncated) = join_reader(stdout_reader)?;
                let (stderr, stderr_truncated) = join_reader(stderr_reader)?;
                Ok(ToolOutput {
                    status: None,
                    stdout,
                    stderr,
                    timed_out: true,
                    output_cap_bytes: Some(self.output_cap_bytes),
                    stdout_truncated,
                    stderr_truncated,
                })
            }
            Err(error) => {
                let killed = kill_tree(&child);
                let _ = child.kill();
                let _ = child.wait();
                let mut text = format!("cannot run docker: {error}");
                match killed {
                    Ok(()) => {
                        let _ = join_reader(stdout_reader);
                        let _ = join_reader(stderr_reader);
                    }
                    Err(kill_error) => text = format!("{text}; could not be killed: {kill_error}"),
                }
                Err(RunnerError(text))
            }
        }
    }
}

/// Joins one reader thread, turning a panic into a [`RunnerError`].
fn join_reader(
    handle: Option<std::thread::JoinHandle<(String, bool)>>,
) -> Result<(String, bool), RunnerError> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| RunnerError("a reader thread panicked".to_string())),
        None => Ok((String::new(), false)),
    }
}

/// Reads a pipe to its end, keeping at most `cap` bytes and draining the rest.
fn read_capped(pipe: &mut impl std::io::Read, cap: u64) -> (String, bool) {
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    let mut kept: Vec<u8> = Vec::new();
    let mut dropped = false;
    let mut chunk = [0u8; 8192];
    loop {
        match std::io::Read::read(pipe, &mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if kept.len() < cap {
                    let room = cap - kept.len();
                    let take = room.min(read);
                    kept.extend_from_slice(chunk.get(..take).unwrap_or_default());
                    if take < read {
                        dropped = true;
                    }
                } else {
                    dropped = true;
                }
            }
        }
    }
    (String::from_utf8_lossy(&kept).into_owned(), dropped)
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
#[must_use]
pub fn create_argv(spec: &SandboxSpec) -> Vec<String> {
    let mut argv = vec![
        "create".to_string(),
        "--name".to_string(),
        spec.name.clone(),
        "--cap-drop".to_string(),
        "ALL".to_string(),
        "--security-opt".to_string(),
        "no-new-privileges".to_string(),
        "--user".to_string(),
        spec.user.clone(),
        "--workdir".to_string(),
        spec.workdir.clone(),
        "--network".to_string(),
        network_name(&spec.network).to_string(),
    ];
    for mount in &spec.mounts {
        argv.push("--mount".to_string());
        argv.push(mount_spec(mount));
    }
    argv.push(spec.image.clone());
    argv.push(KEEPALIVE_PROGRAM.to_string());
    argv.push(KEEPALIVE_SECS.to_string());
    argv
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
    vec!["start".to_string(), id.to_string()]
}

/// The `docker exec` argv for `command` in `id`, without the program name.
#[must_use]
pub fn exec_argv(id: &str, command: &CommandSpec) -> Vec<String> {
    let mut argv = vec!["exec".to_string()];
    if let Some(workdir) = &command.workdir {
        argv.push("--workdir".to_string());
        argv.push(workdir.clone());
    }
    argv.push(id.to_string());
    argv.push(command.program.clone());
    argv.extend(command.args.iter().cloned());
    argv
}

/// The `docker kill` argv for `id`, without the program name.
#[must_use]
pub fn kill_argv(id: &str) -> Vec<String> {
    vec!["kill".to_string(), id.to_string()]
}

/// The `docker rm --force` argv for `id`, without the program name.
#[must_use]
pub fn remove_argv(id: &str) -> Vec<String> {
    vec!["rm".to_string(), "--force".to_string(), id.to_string()]
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

/// Whether `image` is pinned: a `name@sha256:` digest or a non-`latest` tag.
#[must_use]
pub fn image_is_pinned(image: &str) -> bool {
    if let Some((_, digest)) = image.rsplit_once('@') {
        return digest.strip_prefix("sha256:").is_some_and(is_digest_hex);
    }
    // The tag is what follows the last `:` of the last `/`-separated part, so
    // a registry port such as `registry:5000/image` names no tag at all.
    let last = image.rsplit('/').next().unwrap_or(image);
    last.rsplit_once(':')
        .is_some_and(|(_, tag)| !tag.is_empty() && tag != "latest")
}

/// Whether `hex` is exactly 64 hexadecimal digits.
fn is_digest_hex(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Checks `spec` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an unpinned image, a root or empty
/// user, a relative workdir, a malformed name, or a bad mount.
pub fn validate_spec(spec: &SandboxSpec) -> Result<(), SandboxError> {
    if !image_is_pinned(&spec.image) {
        return Err(SandboxError::Rejected(format!(
            "the image must be pinned by digest or a non-latest tag: {}",
            spec.image
        )));
    }
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
    Ok(())
}

/// Checks one user string: non-empty and not root.
fn validate_user(user: &str) -> Result<(), SandboxError> {
    if user.is_empty() {
        return Err(SandboxError::Rejected(
            "the user must not be empty".to_string(),
        ));
    }
    if user == "root" || user == "0" || user.starts_with("root:") || user.starts_with("0:") {
        return Err(SandboxError::Rejected(format!(
            "the user must not be root: {user}"
        )));
    }
    Ok(())
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

/// Checks one mount's paths: non-empty host, absolute sandbox, no comma or newline.
fn validate_mount(mount: &Mount) -> Result<(), SandboxError> {
    if mount.host_path.is_empty() {
        return Err(SandboxError::Rejected(
            "a mount host path must not be empty".to_string(),
        ));
    }
    if !mount.sandbox_path.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "a mount sandbox path must be absolute: {}",
            mount.sandbox_path
        )));
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

/// Checks `command` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an empty program or a relative workdir.
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
    Ok(())
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
        let output = self
            .runner
            .run(&create_argv(spec), Some(CREATE_TIMEOUT))
            .map_err(|error| SandboxError::Failed(error.0))?;
        if output.status != Some(0) || output.timed_out {
            return Err(SandboxError::Failed(tool_text(&output)));
        }
        let id = output.stdout.trim().to_string();
        if id.is_empty() {
            return Err(SandboxError::Failed(
                "the create call returned no container id".to_string(),
            ));
        }
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
        let timeout = command.timeout_secs.map(Duration::from_secs);
        let output = self
            .runner
            .run(&exec_argv(&id.0, command), timeout)
            .map_err(|error| SandboxError::Failed(error.0))?;

        if output.timed_out {
            let kill = self
                .runner
                .run(&kill_argv(&id.0), None)
                .map_err(|error| SandboxError::Failed(error.0))?;
            if kill.status != Some(0) || kill.timed_out {
                return Err(SandboxError::Failed(tool_text(&kill)));
            }
            return Ok(RunResult {
                outcome: RunOutcome::TimedOut {
                    limit_secs: command.timeout_secs.unwrap_or(0),
                },
                stdout: output.stdout,
                stderr: output.stderr,
                output_cap_bytes: output.output_cap_bytes,
                stdout_truncated: output.stdout_truncated,
                stderr_truncated: output.stderr_truncated,
            });
        }

        if is_docker_failure(&output) {
            return Err(SandboxError::Failed(tool_text(&output)));
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
        Err(SandboxError::Failed(message))
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
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// One recorded runner call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        argv: Vec<String>,
        timeout: Option<Duration>,
    }

    /// A [`DockerRunner`] that records every call and answers in order.
    struct ScriptedRunner {
        calls: RefCell<Vec<Call>>,
        results: RefCell<VecDeque<Result<ToolOutput, RunnerError>>>,
    }

    impl ScriptedRunner {
        fn new(results: Vec<Result<ToolOutput, RunnerError>>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                results: RefCell::new(results.into()),
            }
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.borrow().clone()
        }
    }

    impl DockerRunner for ScriptedRunner {
        fn run(
            &self,
            argv: &[String],
            timeout: Option<Duration>,
        ) -> Result<ToolOutput, RunnerError> {
            self.calls.borrow_mut().push(Call {
                argv: argv.to_vec(),
                timeout,
            });
            self.results
                .borrow_mut()
                .pop_front()
                .expect("the scripted runner has an answer for every call")
        }
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
                },
                Mount {
                    host_path: "/host/cache".to_string(),
                    sandbox_path: "/cache".to_string(),
                    read_only: true,
                },
            ],
        }
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
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
                "--name",
                "build",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--user",
                "dev",
                "--workdir",
                "/work",
                "--network",
                "none",
                "--mount",
                "type=bind,source=/host/project,target=/work/project",
                "--mount",
                "type=bind,source=/host/cache,target=/cache,readonly",
                "example/base:1",
                "sleep",
                "2147483647",
            ])
        );
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
        assert_eq!(start_argv("abc"), strings(&["start", "abc"]));
    }

    #[test]
    fn exec_argv_pins_the_exact_vector_without_a_workdir() {
        assert_eq!(
            exec_argv("abc", &command()),
            strings(&["exec", "abc", "run", "--fast"])
        );
    }

    #[test]
    fn exec_argv_pins_the_exact_vector_with_a_workdir() {
        let mut command = command();
        command.workdir = Some("/work".to_string());
        assert_eq!(
            exec_argv("abc", &command),
            strings(&["exec", "--workdir", "/work", "abc", "run", "--fast"])
        );
    }

    #[test]
    fn kill_argv_pins_the_exact_vector() {
        assert_eq!(kill_argv("abc"), strings(&["kill", "abc"]));
    }

    #[test]
    fn remove_argv_pins_the_exact_vector() {
        assert_eq!(remove_argv("abc"), strings(&["rm", "--force", "abc"]));
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
            strings(&["image", "inspect", "--format", "{{.Id}}", "example/base:1",])
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
        for pinned in [
            format!("example/base@sha256:{hex}"),
            "example/base:1.2".to_string(),
            "registry:5000/example/base:1.2".to_string(),
            "example/base:22.04".to_string(),
        ] {
            assert!(image_is_pinned(&pinned), "{pinned} is pinned");
        }
    }

    #[test]
    fn image_is_pinned_rejects_latest_untagged_and_bad_digests() {
        let wrong = "z".repeat(64);
        for unpinned in [
            "example/base".to_string(),
            "example/base:latest".to_string(),
            "example/base:".to_string(),
            "registry:5000/example/base".to_string(),
            "registry:5000/example/base:latest".to_string(),
            "example/base@sha256:short".to_string(),
            format!("example/base@sha256:{wrong}"),
            String::new(),
        ] {
            assert!(!image_is_pinned(&unpinned), "{unpinned} is not pinned");
        }
    }

    #[test]
    fn validate_spec_rejects_a_non_pinned_image_without_a_call() {
        let mut spec = spec();
        spec.image = "example/base:latest".to_string();
        create_rejected(&spec);
    }

    #[test]
    fn validate_spec_rejects_a_root_or_empty_user_without_a_call() {
        for user in ["", "root", "0", "root:root", "0:0"] {
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
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "work".to_string(),
                read_only: false,
            },
            Mount {
                host_path: "/host,a".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "/wo,rk".to_string(),
                read_only: false,
            },
            Mount {
                host_path: "/host\nb".to_string(),
                sandbox_path: "/work".to_string(),
                read_only: false,
            },
            Mount {
                host_path: "/host".to_string(),
                sandbox_path: "/work\nb".to_string(),
                read_only: false,
            },
        ];
        for mount in cases {
            let mut spec = spec();
            spec.mounts = vec![mount];
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
    fn create_runs_create_then_start_and_returns_the_trimmed_id() {
        let sandbox = sandbox(vec![Ok(ok("abc123\n")), Ok(ok(""))]);
        let id = sandbox.create(&spec()).expect("creates");
        assert_eq!(id, SandboxId("abc123".to_string()));
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: create_argv(&spec()),
                    timeout: Some(Duration::from_secs(120)),
                },
                Call {
                    argv: start_argv("abc123"),
                    timeout: None,
                },
            ]
        );
    }

    #[test]
    fn create_carries_the_tool_text_on_a_nonzero_create() {
        let sandbox = sandbox(vec![Ok(fail(1, "boom create"))]);
        let error = sandbox.create(&spec()).expect_err("create fails");
        assert_eq!(error, SandboxError::Failed("boom create".to_string()));
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn create_reports_a_runner_error() {
        let sandbox = sandbox(vec![Err(RunnerError("cannot run docker".to_string()))]);
        let error = sandbox.create(&spec()).expect_err("runner error");
        assert_eq!(error, SandboxError::Failed("cannot run docker".to_string()));
    }

    #[test]
    fn create_reports_a_timed_out_create() {
        let sandbox = sandbox(vec![Ok(timeout_output(""))]);
        let error = sandbox.create(&spec()).expect_err("timed out");
        assert_eq!(
            error,
            SandboxError::Failed("the tool timed out".to_string())
        );
    }

    #[test]
    fn create_rejects_an_empty_container_id() {
        let sandbox = sandbox(vec![Ok(ok("\n"))]);
        let error = sandbox.create(&spec()).expect_err("no id");
        assert!(matches!(error, SandboxError::Failed(_)), "{error:?}");
        assert_eq!(sandbox.runner.calls().len(), 1);
    }

    #[test]
    fn a_failed_start_removes_the_container_and_returns_the_start_text() {
        let sandbox = sandbox(vec![Ok(ok("abc")), Ok(fail(1, "boom start")), Ok(ok(""))]);
        let error = sandbox.create(&spec()).expect_err("start fails");
        assert_eq!(error, SandboxError::Failed("boom start".to_string()));
        assert_eq!(
            sandbox.runner.calls(),
            vec![
                Call {
                    argv: create_argv(&spec()),
                    timeout: Some(Duration::from_secs(120)),
                },
                Call {
                    argv: start_argv("abc"),
                    timeout: None,
                },
                Call {
                    argv: remove_argv("abc"),
                    timeout: None,
                },
            ]
        );
    }

    #[test]
    fn a_failed_start_appends_a_failed_removal_text() {
        let sandbox = sandbox(vec![
            Ok(ok("abc")),
            Ok(fail(1, "boom start")),
            Ok(fail(1, "boom remove")),
        ]);
        let error = sandbox.create(&spec()).expect_err("start fails");
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
    fn run_without_a_timeout_asks_the_runner_to_wait_forever() {
        let mut command = command();
        command.timeout_secs = None;
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        sandbox.run(&id, &command).expect("runs");
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: exec_argv("abc", &command),
                timeout: None,
            }]
        );
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
                },
                Call {
                    argv: kill_argv("abc"),
                    timeout: None,
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
    fn destroy_removes_the_container() {
        let sandbox = sandbox(vec![Ok(ok(""))]);
        let id = SandboxId("abc".to_string());
        assert_eq!(sandbox.destroy(&id).expect("destroys"), Destroyed::Removed);
        assert_eq!(
            sandbox.runner.calls(),
            vec![Call {
                argv: remove_argv("abc"),
                timeout: None,
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
                },
                Call {
                    argv: image_inspect_argv("example/base:1"),
                    timeout: None,
                },
                Call {
                    argv: network_probe_argv(),
                    timeout: None,
                },
            ]
        );
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

    #[test]
    fn read_capped_keeps_everything_under_the_cap() {
        let mut pipe = std::io::Cursor::new(b"hello".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 10);
        assert_eq!(text, "hello");
        assert!(!truncated);
        assert_eq!(pipe.position(), 5);
    }

    #[test]
    fn read_capped_keeps_everything_exactly_at_the_cap() {
        let mut pipe = std::io::Cursor::new(b"hello".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 5);
        assert_eq!(text, "hello");
        assert!(!truncated);
        assert_eq!(pipe.position(), 5);
    }

    #[test]
    fn read_capped_drops_the_remainder_but_drains_the_pipe() {
        let mut pipe = std::io::Cursor::new(b"hello world".to_vec());
        let (text, truncated) = read_capped(&mut pipe, 5);
        assert_eq!(text, "hello");
        assert!(truncated);
        assert_eq!(pipe.position(), 11, "the reader drains the whole pipe");
    }

    #[test]
    fn read_capped_keeps_invalid_utf8_lossy() {
        let mut pipe = std::io::Cursor::new(vec![0xff, b'a']);
        let (text, truncated) = read_capped(&mut pipe, 10);
        assert_eq!(text, "\u{fffd}a");
        assert!(!truncated);
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
}
