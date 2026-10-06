//! The reviewer roster, and running one reviewer through its coding-agent
//! command-line tool, headless.
//!
//! osf never holds a model provider's API key. A reviewer is one of the
//! agents in [`crate::agents::AGENTS`], already installed and logged in on
//! this machine; osf only starts it with a prompt and, where the tool
//! supports it, a JSON Schema to constrain its answer. A repository's own
//! `osf.toml` picks the reviewers under `[agents]`.

use crate::agents::{self, Agent};
use crate::answer::{self, Answer};
use crate::lenses::Lens;
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt as _;

pub use crate::agents::{ReadOnly, SchemaArg, Switches};

/// One reviewer: an agent, run with its own model family, through a fixed
/// command line.
#[derive(Debug, Clone, PartialEq)]
pub struct Reviewer {
    /// The agent's name.
    pub name: String,
    /// The model family it answers with, from its agent's entry and, for an
    /// agent that runs many families, from its model.
    pub family: String,
    /// Why the family is unknown, when it is. Such a reviewer cannot take
    /// part: its run is could-not-run, with this reason.
    pub family_error: Option<String>,
    /// The argument list to run, in order. `{prompt_file}` is replaced with
    /// the path of a file holding the prompt, for an agent that reads one
    /// from a file rather than from standard input.
    pub command: Vec<String>,
    /// The documented settings that hold this agent to read-only tools.
    /// With `None` the agent has no such mode and is never started.
    pub read_only: Option<ReadOnly>,
    /// The documented switches that make this agent ignore the settings of
    /// the folder it starts in.
    pub clean_copy: Switches,
    /// The flag that introduces the answer schema, when this agent can be
    /// asked to validate its own output against one.
    pub schema_flag: Option<String>,
    /// How `schema_flag`'s value is given. Ignored when `schema_flag` is `None`.
    pub schema_as: SchemaArg,
    /// A JSON pointer to the answer inside the agent's own output
    /// envelope, such as `/structured_output`. Empty when the whole output
    /// is the answer.
    pub answer_pointer: String,
    /// The model this reviewer's agent should use, when pinned. Left
    /// unset, the agent falls back to its own default model.
    pub model: Option<String>,
    /// The flag that introduces `model`'s value on the agent's own
    /// command line, such as `"--model"`. Ignored when `model` is `None`.
    pub model_flag: Option<String>,
    /// The environment variable names that carry this reviewer's own
    /// provider credential. [`run_one`] starts this reviewer with only
    /// these, plus the variables every child needs to run at all; no other
    /// reviewer's credential is ever in its environment.
    pub credential_env: Vec<String>,
    /// The paths, relative to the real home directory, that this agent's
    /// own login needs. [`run_one`] copies only these into the reviewer's
    /// fresh home before it runs; nothing else of the real home, and no other
    /// reviewer's login, goes in. A path that is missing is left out and
    /// named in the run's notes.
    pub login_paths: Vec<String>,
    /// A command, program first, that starts this agent's read-only sandbox
    /// around a harmless command. [`run_one`] runs it before the agent
    /// reviews, and reports could-not-run when it fails. Empty when the
    /// read-only mode has no sandbox to start.
    pub sandbox_check: Vec<String>,
    /// The read-only folder that holds the change's diff and log. It takes
    /// the place of `{review_dir}` in the read-only arguments and environment.
    pub review_dir: Option<PathBuf>,
}

impl Reviewer {
    /// `text` with `{review_dir}` replaced by the review folder's path.
    fn fill(&self, text: &str) -> String {
        match &self.review_dir {
            Some(dir) => text.replace("{review_dir}", &dir.to_string_lossy()),
            None => text.to_string(),
        }
    }

    /// Whether this reviewer sits a change out because its family is one of
    /// `builder_families`: a builder's own family is no independent opinion.
    #[must_use]
    pub fn is_excluded_by(&self, builder_families: &std::collections::BTreeSet<String>) -> bool {
        self.family_error.is_none() && builder_families.contains(&self.family)
    }

    /// `agent` as a reviewer running `model`, with `in_container`'s
    /// container mode when osf runs inside the factory container; `None` when
    /// `agent` never reviews.
    fn from_agent(agent: &Agent, model: Option<&str>, in_container: bool) -> Option<Self> {
        let review = agent.review.as_ref()?;
        let owned = |items: &[&str]| items.iter().map(ToString::to_string).collect();
        let (family, family_error) = match agent.family_for(model) {
            Ok(family) => (family.to_string(), None),
            Err(reason) => (crate::builder::UNKNOWN.to_string(), Some(reason)),
        };
        let container_mode = if in_container {
            review.in_container
        } else {
            None
        };
        let read_only = container_mode.or(review.read_only);
        let sandbox_check: &[&str] = if container_mode.is_some() {
            &[]
        } else {
            review.sandbox_check
        };
        Some(Reviewer {
            name: agent.name.to_string(),
            family,
            family_error,
            command: owned(agent.command),
            read_only,
            clean_copy: review.clean_copy,
            schema_flag: review.schema_flag.map(str::to_string),
            schema_as: review.schema_as,
            answer_pointer: review.answer_pointer.to_string(),
            model: model.map(str::to_string),
            model_flag: review.model_flag.map(str::to_string),
            credential_env: owned(review.credential_env),
            login_paths: owned(review.login_paths),
            sandbox_check: owned(sandbox_check),
            review_dir: None,
        })
    }
}

/// One reviewer's outcome for one lens.
#[derive(Debug)]
pub enum Outcome {
    /// The reviewer answered, and the answer validated.
    Answered(Answer),
    /// The reviewer answered twice, and neither answer validated. Counts as missing.
    Invalid(String),
    /// The reviewer's harness could not be started, timed out, or exited non-zero.
    CouldNotRun(String),
}

/// The reviewers `<root>/osf.toml` selects under `[agents]`, in the order it
/// names them, with the container mode chosen by
/// [`agents::in_factory_container`]. None when it names none.
///
/// # Errors
/// Returns an error when `<root>/osf.toml` is not valid TOML, or its
/// `[agents]` table does not match the shape [`agents::resolve`] accepts.
pub fn roster(root: &Path) -> Result<Vec<Reviewer>, String> {
    roster_in(root, agents::in_factory_container())
}

/// [`roster`] with the container detection given rather than read from the
/// machine.
///
/// # Errors
/// Returns an error when `<root>/osf.toml` is not valid TOML, or its
/// `[agents]` table does not match the shape [`agents::resolve`] accepts.
pub fn roster_in(root: &Path, in_container: bool) -> Result<Vec<Reviewer>, String> {
    let selection = agents::selection(root)?;
    Ok(selection
        .reviewers
        .iter()
        .filter_map(|agent| Reviewer::from_agent(agent, selection.model(agent), in_container))
        .collect())
}

/// Variable names every reviewer's child needs purely to run its own
/// program and find its own files, carried over from `osf`'s own
/// environment when present: never a credential, so the same names are safe
/// for every reviewer regardless of which one is starting.
#[cfg(unix)]
const RUN_ENV_VARS: &[&str] = &["PATH", "LANG", "LC_ALL", "TMPDIR"];
#[cfg(windows)]
const RUN_ENV_VARS: &[&str] = &[
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "COMSPEC",
    "PATHEXT",
    "TEMP",
    "TMP",
    "WINDIR",
];

/// A private, empty home directory for one reviewer run, removed on drop.
struct RunHome(PathBuf);

impl RunHome {
    fn create() -> Result<Self, String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir() // osf: temp-dir allowed, one directory per reviewer run
            .join(format!(
                "osf-review-home-{}-{unique}-{n}",
                std::process::id()
            ));
        std::fs::create_dir(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot protect {}: {e}", path.display()))?;
        }
        let home = Self(path);
        std::fs::create_dir(home.temp())
            .map_err(|e| format!("cannot create {}: {e}", home.temp().display()))?;
        #[cfg(windows)]
        for dir in [home.roaming(), home.local()] {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        Ok(home)
    }

    /// The child's own temporary folder, inside its home. Some agents refuse
    /// to set up their sandbox helper when their home sits under the temporary
    /// folder they see, so the child's temporary folder must not hold its home.
    fn temp(&self) -> PathBuf {
        self.0.join("tmp")
    }

    /// Points the child's temporary folder here, after any variable that
    /// carried the parent's own over.
    fn apply_temp(&self, command: &mut Command) {
        command.env("TMPDIR", self.temp());
        #[cfg(windows)]
        command.env("TEMP", self.temp()).env("TMP", self.temp());
    }

    #[cfg(windows)]
    fn roaming(&self) -> PathBuf {
        self.0.join("AppData").join("Roaming")
    }

    #[cfg(windows)]
    fn local(&self) -> PathBuf {
        self.0.join("AppData").join("Local")
    }

    /// Points the child's home and, on Windows, its profile variables here.
    fn apply(&self, command: &mut Command) {
        command.env("HOME", &self.0);
        #[cfg(windows)]
        command
            .env("USERPROFILE", &self.0)
            .env("APPDATA", self.roaming())
            .env("LOCALAPPDATA", self.local());
    }
}

impl RunHome {
    /// Copies each of `paths` from `real_home` into this home, and returns a
    /// note for each one left out because it is missing or not a path inside
    /// the home. A missing path is no error: the reviewer runs without it.
    ///
    /// # Errors
    /// Returns an error when a path exists but cannot be copied.
    fn seed_login(
        &self,
        real_home: Option<&Path>,
        paths: &[String],
    ) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        for rel in paths {
            let rel_path = Path::new(rel);
            let inside = !rel.is_empty()
                && rel_path
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)));
            if !inside {
                notes.push(format!(
                    "login path \"{rel}\" is not a path inside the home and was left out"
                ));
                continue;
            }
            let copied = match real_home {
                Some(real) => copy_login(&real.join(rel_path), &self.0.join(rel_path))
                    .map_err(|e| format!("cannot copy login path \"{rel}\": {e}"))?,
                None => false,
            };
            if !copied {
                notes.push(format!(
                    "login path \"{rel}\" is missing from the home directory; the reviewer ran without it"
                ));
            }
        }
        Ok(notes)
    }
}

/// The real home directory of the user running `osf`, when it is set.
fn real_home() -> Option<PathBuf> {
    #[cfg(unix)]
    let var = "HOME";
    #[cfg(windows)]
    let var = "USERPROFILE";
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Copies the file or directory at `source` to `dest`, and says whether
/// `source` existed. A symbolic link inside a directory is not followed.
fn copy_login(source: &Path, dest: &Path) -> std::io::Result<bool> {
    let meta = match std::fs::metadata(source) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if meta.is_dir() {
        copy_dir(source, dest)?;
    } else {
        std::fs::copy(source, dest)?;
    }
    Ok(true)
}

fn copy_dir(source: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = dest.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

impl Drop for RunHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `reviewer` against `prompt` for `lens` in `workdir`, and returns its
/// validated answer.
///
/// An answer that does not validate is asked for once more, with the
/// validation reason appended to the prompt; a second failure is reported
/// [`Outcome::Invalid`] rather than retried again. A harness that cannot
/// start, that runs past `timeout`, or that exits non-zero is
/// [`Outcome::CouldNotRun`], naming the reason. A run past `timeout` is
/// killed through [`crate::process::kill_tree`], the same tree-kill helper
/// `moon.rs` uses, so a coding-agent tool that spawns its own child
/// processes cannot outlive the timeout.
#[must_use]
pub fn run_one(
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    workdir: &Path,
    timeout: Duration,
) -> Outcome {
    run_one_noted(reviewer, prompt, lens, workdir, timeout).0
}

/// [`run_one`], and the run's notes: one for each of the reviewer's declared
/// `login_paths` that was missing, so it ran without it.
#[must_use]
pub fn run_one_noted(
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    workdir: &Path,
    timeout: Duration,
) -> (Outcome, Vec<String>) {
    let mut notes = Vec::new();
    let outcome = run_with_retry(
        reviewer,
        prompt,
        lens,
        workdir,
        timeout,
        real_home().as_deref(),
        &mut notes,
    );
    (outcome, notes)
}

fn run_with_retry(
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    workdir: &Path,
    timeout: Duration,
    real_home: Option<&Path>,
    notes: &mut Vec<String>,
) -> Outcome {
    if reviewer.read_only.is_none() {
        return Outcome::CouldNotRun(format!(
            "reviewer '{}' has no read-only mode",
            reviewer.name
        ));
    }
    if let Err(reason) = check_sandbox(reviewer, workdir, timeout, real_home) {
        return Outcome::CouldNotRun(reason);
    }
    let schema = answer::schema_for(lens);
    let raw = match run_child(
        reviewer, prompt, &schema, workdir, timeout, real_home, notes,
    ) {
        Ok(text) => text,
        Err(reason) => return Outcome::CouldNotRun(reason),
    };
    match validate_stage(&raw, reviewer, lens) {
        Ok(answer) => Outcome::Answered(answer),
        Err(reason) => {
            let retry_prompt = format!("{prompt}\n\nThe previous answer was invalid: {reason}");
            match run_child(
                reviewer,
                &retry_prompt,
                &schema,
                workdir,
                timeout,
                real_home,
                notes,
            ) {
                Ok(raw) => match validate_stage(&raw, reviewer, lens) {
                    Ok(answer) => Outcome::Answered(answer),
                    Err(reason) => Outcome::Invalid(reason),
                },
                Err(reason) => Outcome::CouldNotRun(reason),
            }
        }
    }
}

/// The longest the sandbox check may run, whatever the reviewer's own timeout is.
const SANDBOX_CHECK_LIMIT: Duration = Duration::from_secs(30);

/// Runs `reviewer`'s sandbox check, when it has one: the command starts the
/// agent's read-only sandbox around a harmless command, with the same
/// environment and home the reviewer itself gets. A reviewer whose sandbox
/// cannot start where osf runs never reviews.
///
/// # Errors
/// Names the reviewer and the reason when the check cannot start, times
/// out, or exits non-zero. The reason holds the last line the check printed
/// to its error output, which comes from the sandbox tool and never from a model.
fn check_sandbox(
    reviewer: &Reviewer,
    workdir: &Path,
    timeout: Duration,
    real_home: Option<&Path>,
) -> Result<(), String> {
    let Some((program, rest)) = reviewer.sandbox_check.split_first() else {
        return Ok(());
    };
    let name = &reviewer.name;
    let (mut command, _home, _notes) =
        prepare_command(reviewer, program, rest, workdir, real_home)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|e| format!("reviewer '{name}' cannot start its sandbox check: {e}"))?;
    let stderr_reader = child
        .stderr
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let limit = timeout.min(SANDBOX_CHECK_LIMIT);
    let status = match child.wait_timeout(limit) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let _ = crate::process::kill_tree(&child);
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.map(std::thread::JoinHandle::join);
            return Err(format!(
                "reviewer '{name}' sandbox check timed out after {limit:?}, so its read-only sandbox is not known to work here"
            ));
        }
        Err(e) => {
            return Err(format!(
                "cannot wait for reviewer '{name}' sandbox check: {e}"
            ))
        }
    };
    let stderr = stderr_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let code = status
        .code()
        .map_or_else(|| "no exit code".to_string(), |c| c.to_string());
    let line: String = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .chars()
        .take(200)
        .collect();
    let detail = if line.is_empty() {
        String::new()
    } else {
        format!(": {line}")
    };
    Err(format!(
        "reviewer '{name}' cannot start its read-only sandbox here (exit code {code}){detail}"
    ))
}

/// `raw`'s answer text, pulled out of `reviewer.answer_pointer` when it
/// names one, then validated against `lens`.
fn validate_stage(raw: &str, reviewer: &Reviewer, lens: &Lens) -> Result<Answer, String> {
    let text = extract_pointer(raw, &reviewer.answer_pointer)?;
    answer::extract_and_validate(&text, lens)
}

/// `raw`, unchanged, when `pointer` is empty; otherwise the JSON value at
/// `pointer` inside `raw`'s own envelope, re-serialised as text so
/// [`answer::extract_and_validate`] can parse it the same way either path.
///
/// # Errors
/// Names the pointer when `raw` is not valid JSON, or holds nothing at
/// `pointer`.
fn extract_pointer(raw: &str, pointer: &str) -> Result<String, String> {
    if pointer.is_empty() {
        return Ok(raw.to_string());
    }
    let envelope: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
        format!(
            "the answer envelope is not valid JSON at line {} column {}",
            e.line(),
            e.column()
        )
    })?;
    let found = envelope
        .pointer(pointer)
        .ok_or_else(|| format!("the answer envelope has nothing at \"{pointer}\""))?;
    serde_json::to_string(found).map_err(|_| format!("cannot read the value at \"{pointer}\""))
}

/// `program` as given on Unix. On Windows, a bare name is looked up on `PATH`
/// with each `PATHEXT` extension, because `Command` only adds `.exe` and an
/// agent installed with npm is a `.cmd` file.
#[cfg(windows)]
fn resolve_program(program: &str) -> std::ffi::OsString {
    let bare =
        Path::new(program).extension().is_none() && Path::new(program).components().count() == 1;
    let (true, Some(path)) = (bare, std::env::var_os("PATH")) else {
        return program.into();
    };
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    for dir in std::env::split_paths(&path) {
        for ext in exts.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return candidate.into_os_string();
            }
        }
    }
    program.into()
}

#[cfg(not(windows))]
fn resolve_program(program: &str) -> std::ffi::OsString {
    program.into()
}

/// `program` with `rest`, started in `workdir` with the allow-listed
/// environment and a fresh [`RunHome`], which must outlive the child.
fn prepare_command(
    reviewer: &Reviewer,
    program: &str,
    rest: &[String],
    workdir: &Path,
    real_home: Option<&Path>,
) -> Result<(Command, RunHome, Vec<String>), String> {
    let home = RunHome::create()?;
    // A key in the environment is enough to sign in, so no login file is copied where a read tool could reach it.
    let key_in_environment = reviewer
        .credential_env
        .iter()
        .any(|var| std::env::var(var).is_ok_and(|value| !value.is_empty()));
    let notes = if key_in_environment {
        Vec::new()
    } else {
        home.seed_login(real_home, &reviewer.login_paths)?
    };
    let mut command = Command::new(resolve_program(program));
    command.args(rest).current_dir(workdir).env_clear();
    home.apply(&mut command);
    let read_only_env = reviewer.read_only.iter().flat_map(|mode| mode.env);
    for (name, value) in read_only_env.chain(reviewer.clean_copy.env) {
        command.env(name, reviewer.fill(value));
    }
    for var in RUN_ENV_VARS
        .iter()
        .copied()
        .chain(reviewer.credential_env.iter().map(String::as_str))
    {
        if let Ok(value) = std::env::var(var) {
            command.env(var, value);
        }
    }
    home.apply_temp(&mut command);
    Ok((command, home, notes))
}

/// One attempt at running `reviewer`'s harness to completion: its captured
/// standard output on a successful exit, or the reason it does not count as
/// one.
///
/// The child starts with an allow-listed environment: only [`RUN_ENV_VARS`]
/// (the variables any program needs to run at all, such as `PATH`), its own
/// fresh home directory holding only `reviewer.login_paths` copied from
/// `real_home`, and `reviewer.credential_env` (that reviewer's own provider
/// credential, by name). Every other reviewer's credential and login, and
/// `osf`'s own `GH_TOKEN`, stay out, whatever else is set on `osf`'s own
/// process. A missing login path adds a note to `notes`, once.
fn run_child(
    reviewer: &Reviewer,
    prompt: &str,
    schema: &str,
    workdir: &Path,
    timeout: Duration,
    real_home: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<String, String> {
    let prompt_file = write_temp_file("osf-review-prompt", prompt)?;
    let schema_file = write_temp_file("osf-review-schema", schema)?;
    let cleanup = || {
        let _ = std::fs::remove_file(&prompt_file);
        let _ = std::fs::remove_file(&schema_file);
    };

    let args = build_args(reviewer, &prompt_file, &schema_file, schema);
    let Some((program, rest)) = args.split_first() else {
        cleanup();
        return Err(format!("reviewer '{}' has an empty command", reviewer.name));
    };

    let (mut command, _home, home_notes) =
        match prepare_command(reviewer, program, rest, workdir, real_home) {
            Ok(prepared) => prepared,
            Err(e) => {
                cleanup();
                return Err(e);
            }
        };
    for note in home_notes {
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            cleanup();
            return Err(format!("cannot run reviewer '{}': {e}", reviewer.name));
        }
    };

    // Every pipe is drained, and the prompt written, on its own thread,
    // started before `wait_timeout` below: a harness that never reads its
    // stdin, given a prompt bigger than the pipe's own buffer, would
    // otherwise block a synchronous write forever, on the same thread that
    // is supposed to be enforcing `timeout`. `wait_timeout` alone then
    // governs the whole run; the writer unblocks (with an error) once the
    // timeout branch below kills the child and its pipe closes.
    let stdout_reader = child
        .stdout
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let prompt_owned = prompt.to_string();
    let stdin_writer = child
        .stdin
        .take()
        .map(|mut stdin| std::thread::spawn(move || stdin.write_all(prompt_owned.as_bytes())));

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let killed = crate::process::kill_tree(&child);
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.map(std::thread::JoinHandle::join);
            let _ = stderr_reader.map(std::thread::JoinHandle::join);
            let _ = stdin_writer.map(std::thread::JoinHandle::join);
            cleanup();
            return Err(match killed {
                Ok(()) => format!(
                    "reviewer '{}' timed out after {timeout:?} and was killed",
                    reviewer.name
                ),
                Err(e) => format!(
                    "reviewer '{}' timed out and could not be killed: {e}",
                    reviewer.name
                ),
            });
        }
        Err(e) => {
            cleanup();
            return Err(format!("cannot wait for reviewer '{}': {e}", reviewer.name));
        }
    };

    // Joined so the writer thread always finishes cleanly, but never
    // inspected: a harness that exits before reading the whole prompt (or
    // never reads it at all, the way this crate's own fake harness does)
    // closes its end of the pipe under it, and a broken-pipe write error
    // from that is not a sign the harness failed. Its own exit status,
    // checked below, is the only thing that decides that.
    let _stdin_result = stdin_writer.map(|h| {
        h.join()
            .unwrap_or_else(|_| Err(std::io::Error::other("the stdin writer thread panicked")))
    });
    let stdout_text = stdout_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    // Drained and joined so the reader thread always finishes cleanly, but
    // never read: a reviewer's stderr is reviewer-controlled text, and this
    // function's own errors are journalled, so it must never appear in one.
    let _stderr_text = stderr_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    cleanup();

    if !status.success() {
        let code = status
            .code()
            .map_or_else(|| "no exit code".to_string(), |c| c.to_string());
        return Err(format!(
            "reviewer '{}' exited with code {code}",
            reviewer.name
        ));
    }
    Ok(stdout_text)
}

/// `reviewer.command`, with `{prompt_file}` replaced by `prompt_file`'s
/// path, `schema_flag`/its value appended when the reviewer declares one,
/// and `model_flag`/`model` appended when both are set. The schema value is `schema_file`, or `schema` when inlined.
fn build_args(
    reviewer: &Reviewer,
    prompt_file: &Path,
    schema_file: &Path,
    schema: &str,
) -> Vec<String> {
    let prompt_path = prompt_file.to_string_lossy().into_owned();
    let mut args: Vec<String> = reviewer
        .command
        .iter()
        .map(|token| {
            if token == "{prompt_file}" {
                prompt_path.clone()
            } else {
                token.clone()
            }
        })
        .collect();
    if let Some(read_only) = &reviewer.read_only {
        args.extend(read_only.args.iter().map(|arg| reviewer.fill(arg)));
    }
    args.extend(
        reviewer
            .clean_copy
            .args
            .iter()
            .map(|arg| reviewer.fill(arg)),
    );
    if let Some(flag) = &reviewer.schema_flag {
        args.push(flag.clone());
        args.push(match reviewer.schema_as {
            SchemaArg::Path => schema_file.to_string_lossy().into_owned(),
            SchemaArg::Inline => schema.to_string(),
        });
    }
    if let (Some(flag), Some(model)) = (&reviewer.model_flag, &reviewer.model) {
        args.push(flag.clone());
        args.push(model.clone());
    }
    args
}

/// A file unique to this process and this call, under the OS temp
/// directory, holding `content`.
fn write_temp_file(prefix: &str, content: &str) -> Result<PathBuf, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir() // osf: temp-dir allowed, one file per reviewer run
        .join(format!("{prefix}-{}-{unique}", std::process::id()));
    std::fs::write(&path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

/// Reads a pipe to the end, discarding a read error's content but not its
/// occurrence: a partial read is still worth the caller having.
fn read_all(pipe: &mut impl std::io::Read) -> String {
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(pipe, &mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reviewer(name: &str, command: Vec<&str>) -> Reviewer {
        Reviewer {
            name: name.to_string(),
            family: "test-family".to_string(),
            family_error: None,
            command: command.into_iter().map(str::to_string).collect(),
            read_only: Some(ReadOnly {
                args: &[],
                env: &[],
            }),
            clean_copy: Switches {
                args: &[],
                env: &[],
            },
            schema_flag: None,
            schema_as: SchemaArg::default(),
            answer_pointer: String::new(),
            model: None,
            model_flag: None,
            credential_env: Vec::new(),
            login_paths: Vec::new(),
            sandbox_check: Vec::new(),
            review_dir: None,
        }
    }

    /// Prints the child's `HOME` and every file under it, in order.
    #[cfg(unix)]
    fn file_lister(login_paths: &[&str]) -> Reviewer {
        let mut r = reviewer(
            "file-lister",
            vec![
                "sh",
                "-c",
                "printf '%s|' \"$HOME\"; cd \"$HOME\" && find . -type f | sort | tr '\\n' ' '",
            ],
        );
        r.login_paths = login_paths.iter().map(ToString::to_string).collect();
        r
    }

    /// A stand-in real home holding two harnesses' logins and an unrelated file.
    #[cfg(unix)]
    fn stand_in_home() -> crate::test_support::TempDir {
        let home = crate::test_support::TempDir::new("osf-reviewers-real-home");
        for (path, text) in [
            (".a-login/auth.json", "login-a"),
            (".b-login/auth.json", "login-b"),
            (".unrelated", "other"),
        ] {
            let file = home.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dir creates");
            std::fs::write(file, text).expect("file writes");
        }
        home
    }

    #[cfg(unix)]
    fn run_lister(
        reviewer: &Reviewer,
        real_home: &Path,
    ) -> (String, Vec<String>, Result<(), String>) {
        let mut notes = Vec::new();
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the child only lists its HOME
        let out = run_child(
            reviewer,
            "",
            answer::SCHEMA,
            &workdir,
            Duration::from_secs(30),
            Some(real_home),
            &mut notes,
        );
        match out {
            Ok(text) => (text, notes, Ok(())),
            Err(e) => (String::new(), notes, Err(e)),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_reviewers_home_holds_only_its_own_declared_login() {
        let real = stand_in_home();
        let (out, notes, result) = run_lister(&file_lister(&[".a-login/auth.json"]), &real);
        result.expect("the lister runs");
        let (home, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "./.a-login/auth.json");
        assert!(notes.is_empty(), "{notes:?}");
        assert!(
            !Path::new(home).exists(),
            "the home is removed after the run"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_login_directory_is_copied_whole_and_the_real_home_is_left_alone() {
        let real = stand_in_home();
        let (out, _notes, result) = run_lister(&file_lister(&[".b-login"]), &real);
        result.expect("the lister runs");
        let (_, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "./.b-login/auth.json");
        let kept = std::fs::read_to_string(real.join(".b-login/auth.json")).expect("source reads");
        assert_eq!(kept, "login-b");
    }

    #[cfg(unix)]
    #[test]
    fn no_login_file_is_copied_when_the_reviewers_key_is_in_the_environment() {
        let real = stand_in_home();
        let mut lister = file_lister(&[".a-login/auth.json"]);
        lister.credential_env = vec!["OSF_TEST_LOGIN_SKIP_KEY".to_string()];
        std::env::set_var("OSF_TEST_LOGIN_SKIP_KEY", "a-key-held-in-the-environment");
        let (out, notes, result) = run_lister(&lister, &real);
        std::env::remove_var("OSF_TEST_LOGIN_SKIP_KEY");
        result.expect("the lister runs");
        let (_, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "");
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[cfg(unix)]
    #[test]
    fn the_clean_copy_environment_and_arguments_reach_the_child() {
        let mut r = reviewer(
            "switch-reporter",
            vec![
                "sh",
                "-c",
                "printf '%s|%s' \"$IGNORE_PROJECT\" \"$1\"",
                "sh",
            ],
        );
        r.clean_copy = Switches {
            args: &["--ignore-project"],
            env: &[("IGNORE_PROJECT", "yes")],
        };
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the child only prints a variable
        let out = run_child(
            &r,
            "",
            answer::SCHEMA,
            &workdir,
            Duration::from_secs(30),
            None,
            &mut Vec::new(),
        )
        .expect("the reporter runs");
        assert_eq!(out, "yes|--ignore-project");
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_login_path_is_noted_and_is_not_an_error() {
        let real = stand_in_home();
        let lister = file_lister(&[".a-login/auth.json", ".gone/auth.json"]);
        let (out, notes, result) = run_lister(&lister, &real);
        result.expect("a missing path is not an error");
        let (home, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "./.a-login/auth.json");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes.iter().all(|n| n.contains(".gone/auth.json")),
            "{notes:?}"
        );
        assert!(
            !Path::new(home).exists(),
            "the home is removed after the run"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_login_path_outside_the_home_is_noted_and_never_copied() {
        let real = stand_in_home();
        let lister = file_lister(&["../escape", "/etc/hostname", ""]);
        let (out, notes, result) = run_lister(&lister, &real);
        result.expect("the lister runs");
        let (_, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "");
        assert_eq!(notes.len(), 3, "{notes:?}");
    }

    #[cfg(unix)]
    #[test]
    fn an_unknown_real_home_notes_every_login_path_as_missing() {
        let mut notes = Vec::new();
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the child only lists its HOME
        let out = run_child(
            &file_lister(&[".a-login/auth.json"]),
            "",
            answer::SCHEMA,
            &workdir,
            Duration::from_secs(30),
            None,
            &mut notes,
        )
        .expect("the lister runs");
        let (_, files) = out.split_once('|').expect("home and files");
        assert_eq!(files.trim(), "");
        assert_eq!(notes.len(), 1, "{notes:?}");
    }

    /// Prints the child's `HOME` and how many entries that directory holds.
    #[cfg(unix)]
    fn home_reporter() -> Reviewer {
        reviewer(
            "home-reporter",
            vec![
                "sh",
                "-c",
                "printf '%s|%s' \"$HOME\" \"$(ls -A \"$HOME\" | grep -v '^tmp$' | wc -l)\"",
            ],
        )
    }

    #[cfg(unix)]
    #[test]
    fn each_reviewer_run_gets_its_own_fresh_empty_home() {
        let parent_home = std::env::var("HOME").unwrap_or_default();
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the child only prints its HOME
        let mut homes = Vec::new();
        for _ in 0..2 {
            let out = run_child(
                &home_reporter(),
                "",
                answer::SCHEMA,
                &workdir,
                Duration::from_secs(30),
                None,
                &mut Vec::new(),
            )
            .expect("the reporter runs");
            let (home, count) = out.split_once('|').expect("home and count");
            assert_ne!(home, parent_home);
            assert_eq!(
                count.trim(),
                "0",
                "the home starts empty, apart from the child's own temporary folder"
            );
            assert!(
                !Path::new(home).exists(),
                "the home is removed after the run"
            );
            homes.push(home.to_string());
        }
        assert_eq!(homes.len(), 2);
        assert_ne!(homes.first(), homes.get(1));
    }

    #[test]
    fn with_no_agents_table_there_is_no_reviewer() {
        let r = roster(&std::env::temp_dir()).expect("roster"); // osf: temp-dir allowed, no osf.toml is read from it here
        assert!(r.is_empty());
    }

    fn agent(name: &str) -> &'static Agent {
        agents::AGENTS
            .iter()
            .find(|a| a.name == name)
            .expect("agent is in the list")
    }

    #[test]
    fn the_codex_reviewer_inside_the_container_runs_with_its_own_sandbox_off() {
        let r = Reviewer::from_agent(agent("codex"), None, true).expect("codex reviews");
        let args = build_args(&r, Path::new("/tmp/p"), Path::new("/tmp/s"), answer::SCHEMA);
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.contains(&"--sandbox".to_string()));
        assert!(r.sandbox_check.is_empty());
    }

    #[test]
    fn the_codex_reviewer_outside_the_container_runs_in_its_read_only_sandbox() {
        let r = Reviewer::from_agent(agent("codex"), None, false).expect("codex reviews");
        let args = build_args(&r, Path::new("/tmp/p"), Path::new("/tmp/s"), answer::SCHEMA);
        let pair = args
            .windows(2)
            .any(|w| matches!(w, [a, b] if a == "--sandbox" && b == "read-only"));
        assert!(pair, "{args:?}");
        assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert_eq!(r.sandbox_check, vec!["codex", "sandbox", "--", "true"]);
    }

    #[test]
    fn an_agent_without_an_in_container_mode_is_the_same_in_and_out_of_the_container() {
        let inside = Reviewer::from_agent(agent("claude"), None, true).expect("claude reviews");
        let outside = Reviewer::from_agent(agent("claude"), None, false).expect("claude reviews");
        assert_eq!(inside.read_only, outside.read_only);
        assert_eq!(inside.sandbox_check, outside.sandbox_check);
    }

    #[test]
    fn the_roster_is_built_from_the_agent_list_in_the_order_osf_toml_names_it() {
        let root = crate::test_support::TempDir::new("osf-reviewers-roster-select");
        std::fs::write(
            root.join("osf.toml"),
            "[agents]\nreviewers = [\"codex\", \"dsh\"]\n[agents.models]\ncodex = \"o4-mini\"\n",
        )
        .expect("osf.toml writes");
        let r = roster(&root).expect("roster");
        let names: Vec<&str> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["codex", "dsh"]);
        let codex = r.first().expect("codex entry");
        assert_eq!(codex.family, "openai");
        assert_eq!(codex.command, vec!["codex", "exec"]);
        assert_eq!(codex.model.as_deref(), Some("o4-mini"));
        assert_eq!(codex.login_paths, vec![".codex/auth.json"]);
        assert_eq!(r.get(1).expect("dsh entry").model, None);
    }

    fn roster_with_model(agent: &str, model: Option<&str>) -> Reviewer {
        let root = crate::test_support::TempDir::new("osf-reviewers-roster-family");
        let models = model.map_or_else(String::new, |m| {
            format!("[agents.models]\n{agent} = \"{m}\"\n")
        });
        std::fs::write(
            root.join("osf.toml"),
            format!("[agents]\nreviewers = [\"{agent}\"]\n{models}"),
        )
        .expect("osf.toml writes");
        roster(&root).expect("roster").remove(0)
    }

    #[test]
    fn a_many_family_reviewer_takes_its_family_from_its_model() {
        let r = roster_with_model("opencode", Some("openrouter/qwen/qwen3-coder-next"));
        assert_eq!((r.family.as_str(), r.family_error), ("qwen", None));
    }

    #[test]
    fn a_many_family_reviewer_on_a_claude_model_is_anthropic_and_sits_out_an_anthropic_build() {
        let r = roster_with_model("opencode", Some("openrouter/anthropic/claude-sonnet-5"));
        assert_eq!(r.family, "anthropic");
        let builders = std::collections::BTreeSet::from(["anthropic".to_string()]);
        assert!(r.is_excluded_by(&builders));
        let other = std::collections::BTreeSet::from(["openai".to_string()]);
        assert!(!r.is_excluded_by(&other));
    }

    #[test]
    fn a_many_family_reviewer_on_an_unknown_model_has_no_family_and_a_reason() {
        let r = roster_with_model("opencode", Some("acme/frobnicator-1"));
        let reason = r.family_error.expect("a reason");
        assert!(reason.contains("acme/frobnicator-1"), "{reason}");
        let unset = roster_with_model("opencode", None);
        assert!(unset.family_error.is_some());
    }

    #[test]
    fn an_agent_the_list_does_not_hold_is_refused_with_its_name() {
        let root = crate::test_support::TempDir::new("osf-reviewers-roster-unknown");
        std::fs::write(
            root.join("osf.toml"),
            "[agents]\nreviewers = [\"copilot\"]\n",
        )
        .expect("osf.toml writes");
        let e = roster(&root).expect_err("an unknown reviewer is refused");
        assert!(e.contains("unknown agent \"copilot\""), "{e}");
    }
    #[test]
    fn build_args_substitutes_the_prompt_file_and_appends_the_schema_flag() {
        let mut r = reviewer("fake", vec!["fake", "{prompt_file}"]);
        r.schema_flag = Some("--schema".to_string());
        r.schema_as = SchemaArg::Path;
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
            answer::SCHEMA,
        );
        assert_eq!(
            args,
            vec!["fake", "/tmp/prompt.txt", "--schema", "/tmp/schema.json"]
        );
    }

    #[test]
    fn build_args_appends_the_model_flag_and_value_when_both_are_set() {
        let mut r = reviewer("fake", vec!["fake"]);
        r.model_flag = Some("--model".to_string());
        r.model = Some("claude-sonnet-5".to_string());
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
            answer::SCHEMA,
        );
        assert_eq!(args, vec!["fake", "--model", "claude-sonnet-5"]);
    }

    #[test]
    fn build_args_omits_the_model_flag_when_no_model_is_set() {
        let mut r = reviewer("fake", vec!["fake"]);
        r.model_flag = Some("--model".to_string());
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
            answer::SCHEMA,
        );
        assert_eq!(args, vec!["fake"]);
    }

    #[test]
    fn build_args_inlines_the_schema_it_is_given() {
        let mut r = reviewer("fake", vec!["fake"]);
        r.schema_flag = Some("--json-schema".to_string());
        r.schema_as = SchemaArg::Inline;
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
            "{\"given\":true}",
        );
        assert_eq!(args, vec!["fake", "--json-schema", "{\"given\":true}"]);
    }

    #[test]
    fn build_args_puts_the_read_only_arguments_right_after_the_command() {
        let mut r = reviewer("fake", vec!["fake", "exec"]);
        r.read_only = Some(ReadOnly {
            args: &["--sandbox", "read-only"],
            env: &[],
        });
        r.schema_flag = Some("--schema".to_string());
        let args = build_args(&r, Path::new("/tmp/p"), Path::new("/tmp/s"), answer::SCHEMA);
        assert_eq!(
            args,
            vec![
                "fake",
                "exec",
                "--sandbox",
                "read-only",
                "--schema",
                "/tmp/s"
            ]
        );
    }

    #[test]
    fn build_args_puts_the_clean_copy_switches_after_the_read_only_arguments() {
        let mut r = reviewer("fake", vec!["fake", "exec"]);
        r.read_only = Some(ReadOnly {
            args: &["--sandbox", "read-only"],
            env: &[],
        });
        r.clean_copy = Switches {
            args: &["--ignore-user-config"],
            env: &[],
        };
        let args = build_args(&r, Path::new("/tmp/p"), Path::new("/tmp/s"), answer::SCHEMA);
        assert_eq!(
            args,
            vec![
                "fake",
                "exec",
                "--sandbox",
                "read-only",
                "--ignore-user-config"
            ]
        );
    }

    #[test]
    fn the_review_folder_fills_its_placeholder_in_the_read_only_arguments() {
        let mut r = reviewer("fake", vec!["fake"]);
        r.read_only = Some(ReadOnly {
            args: &["--add-dir", "{review_dir}"],
            env: &[],
        });
        r.review_dir = Some(PathBuf::from("/tmp/review-folder"));
        let args = build_args(&r, Path::new("/tmp/p"), Path::new("/tmp/s"), answer::SCHEMA);
        assert_eq!(args, vec!["fake", "--add-dir", "/tmp/review-folder"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_read_only_environment_reaches_the_child() {
        let mut r = reviewer(
            "env-reporter",
            vec!["sh", "-c", "printf '%s' \"$MODE_FLAG\""],
        );
        r.read_only = Some(ReadOnly {
            args: &[],
            env: &[("MODE_FLAG", "locked")],
        });
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the child only prints a variable
        let out = run_child(
            &r,
            "",
            answer::SCHEMA,
            &workdir,
            Duration::from_secs(30),
            None,
            &mut Vec::new(),
        )
        .expect("the reporter runs");
        assert_eq!(out, "locked");
    }

    #[cfg(unix)]
    #[test]
    fn a_reviewer_with_no_read_only_mode_never_starts() {
        let marker = crate::test_support::TempDir::new("osf-reviewers-no-read-only");
        let started = marker.join("started");
        let mut r = reviewer(
            "no-sandbox",
            vec!["sh", "-c", &format!("touch '{}'", started.display())],
        );
        r.read_only = None;
        let lens: Lens = toml::from_str(include_str!("../defaults/review-lenses/correctness.toml"))
            .expect("shipped lens parses");
        let workdir = std::env::temp_dir(); // osf: temp-dir allowed, the harness never starts
        match run_one(&r, "prompt", &lens, &workdir, Duration::from_secs(30)) {
            Outcome::CouldNotRun(reason) => {
                assert!(reason.contains("no read-only mode"), "{reason}");
            }
            other => panic!("expected could-not-run, got {other:?}"),
        }
        assert!(!started.exists(), "the harness must never start");
    }

    #[test]
    fn extract_pointer_returns_the_raw_text_when_no_pointer_is_set() {
        assert_eq!(
            extract_pointer("{\"a\":1}", "").expect("no pointer"),
            "{\"a\":1}"
        );
    }

    #[test]
    fn extract_pointer_pulls_the_named_field_out_of_the_envelope() {
        let envelope = r#"{"structured_output":{"lens":"x"},"other":1}"#;
        let extracted = extract_pointer(envelope, "/structured_output").expect("pointer resolves");
        assert_eq!(extracted, r#"{"lens":"x"}"#);
    }

    #[test]
    fn extract_pointer_names_the_pointer_when_nothing_is_there() {
        let e =
            extract_pointer(r#"{"other":1}"#, "/structured_output").expect_err("missing pointer");
        assert!(e.contains("/structured_output"), "{e}");
    }

    #[test]
    fn extract_pointer_names_the_pointer_when_the_envelope_is_not_json() {
        let e =
            extract_pointer("not json at all", "/structured_output").expect_err("not JSON at all");
        assert!(e.contains("JSON"), "{e}");
    }
}
