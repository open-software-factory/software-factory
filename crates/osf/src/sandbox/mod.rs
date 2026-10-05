//! Provider-neutral sandbox interface: the seam between the factory and a sandbox provider.

pub mod fake;
pub mod validate;

use std::collections::BTreeMap;
use std::fmt;

/// The provider's own handle for one sandbox.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SandboxId(pub String);

/// Whether a sandbox can reach the network.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    Isolated,
    Open,
}

/// A host path exposed inside the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Mount {
    pub host_path: String,
    pub sandbox_path: String,
    pub read_only: bool,
    /// A source that is a symbolic link is refused unless this is true.
    pub follow_symlinks: bool,
}

/// The resource limits one sandbox runs under.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Limits {
    pub max_processes: u32,
    pub memory: String,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_processes: 512,
            memory: "4g".to_string(),
        }
    }
}

/// What the factory asks a provider to create.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SandboxSpec {
    pub name: String,
    pub image: String,
    pub user: String,
    pub workdir: String,
    pub network: Network,
    pub mounts: Vec<Mount>,
    pub limits: Limits,
}

/// One command to run inside a sandbox.
#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub timeout_secs: Option<u64>,
    /// The variables for this command only; the engine chooses them and the provider adds none.
    #[serde(serialize_with = "serialize_env_keys")]
    pub env: BTreeMap<String, String>,
    /// The bytes written to the command's standard input, then closed.
    #[serde(serialize_with = "serialize_stdin_len")]
    pub stdin: Option<Vec<u8>>,
}

/// Prints the command without any environment value or standard-input byte.
impl fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env_keys: Vec<&str> = self.env.keys().map(String::as_str).collect();
        let stdin_len = self.stdin.as_ref().map(Vec::len);
        f.debug_struct("CommandSpec")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("workdir", &self.workdir)
            .field("timeout_secs", &self.timeout_secs)
            .field("env", &env_keys)
            .field("stdin", &stdin_len)
            .finish()
    }
}

/// Serializes an environment map as the array of its keys: no value is written.
fn serialize_env_keys<S>(env: &BTreeMap<String, String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.collect_seq(env.keys())
}

/// Serializes standard input as its length, or null when there is none.
// serde's field serializer requires the reference to the option.
#[allow(clippy::ref_option)]
fn serialize_stdin_len<S>(stdin: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serde::Serialize::serialize(&stdin.as_ref().map(Vec::len), serializer)
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunOutcome {
    Exited(i32),
    TimedOut { limit_secs: u64 },
}

/// What a run produced: its outcome and both streams.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RunResult {
    pub outcome: RunOutcome,
    pub stdout: String,
    pub stderr: String,
    /// The per-stream cap in bytes, or none when the provider applied no cap;
    /// a cut stream keeps its first half and its last half with a marker between.
    pub output_cap_bytes: Option<u64>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// What destroying a sandbox did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Destroyed {
    Removed,
    AlreadyGone,
}

/// A sandbox operation failed: the provider refused it, or it could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    Rejected(String),
    Failed(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(text) | Self::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for SandboxError {}

/// Whether one sandbox operation is available.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Supported,
    Unsupported(String),
    Unknown(String),
}

/// What a provider can do, stated rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SandboxCapabilities {
    pub platforms: Vec<String>,
    pub runtime: Capability,
    pub image: Capability,
    pub network_isolation: Capability,
    pub mounts: Capability,
    pub stop_mid_run: Capability,
}

/// The seam between the factory and a sandbox provider.
///
/// Destroying a sandbox the provider does not know is `Ok(Destroyed::AlreadyGone)`,
/// because the goal state is reached.
pub trait Sandbox: Send + Sync {
    /// Creates a sandbox for `spec`.
    ///
    /// # Errors
    /// Returns an error when the provider refuses the request or it cannot run.
    fn create(&self, spec: &SandboxSpec) -> Result<SandboxId, SandboxError>;

    /// Runs `command` inside the sandbox `id`.
    ///
    /// # Errors
    /// Returns an error when the provider refuses the request or it cannot run.
    fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError>;

    /// Destroys the sandbox `id`.
    ///
    /// # Errors
    /// Returns an error when the provider refuses the request or it cannot run.
    fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError>;

    /// Reports what the provider can do for `spec`.
    fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::fake::FakeSandbox;

    fn spec() -> SandboxSpec {
        SandboxSpec {
            name: "build".to_string(),
            image: "example/base:1".to_string(),
            user: "dev".to_string(),
            workdir: "/work".to_string(),
            network: Network::Isolated,
            mounts: vec![Mount {
                host_path: "/host/project".to_string(),
                sandbox_path: "/work/project".to_string(),
                read_only: true,
                follow_symlinks: false,
            }],
            limits: Limits::default(),
        }
    }

    #[test]
    fn sandbox_is_object_safe() {
        let fake = FakeSandbox::new();
        let dynamic: &dyn Sandbox = &fake;
        let boxed: Box<dyn Sandbox> = Box::new(FakeSandbox::new());
        assert_eq!(
            dynamic.capabilities(&spec()).platforms,
            vec!["local".to_string()]
        );
        assert_eq!(
            boxed.capabilities(&spec()).platforms,
            vec!["local".to_string()]
        );
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn the_seam_is_send_and_sync() {
        assert_send_sync::<FakeSandbox>();
        assert_send_sync::<Box<dyn Sandbox>>();
    }

    #[test]
    fn network_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&Network::Isolated).expect("serializes"),
            r#""isolated""#
        );
        assert_eq!(
            serde_json::to_string(&Network::Open).expect("serializes"),
            r#""open""#
        );
    }

    #[test]
    fn run_outcome_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&RunOutcome::Exited(0)).expect("serializes"),
            r#"{"exited":0}"#
        );
        assert_eq!(
            serde_json::to_string(&RunOutcome::TimedOut { limit_secs: 5 }).expect("serializes"),
            r#"{"timed-out":{"limit_secs":5}}"#
        );
    }

    #[test]
    fn destroyed_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&Destroyed::Removed).expect("serializes"),
            r#""removed""#
        );
        assert_eq!(
            serde_json::to_string(&Destroyed::AlreadyGone).expect("serializes"),
            r#""already-gone""#
        );
    }

    #[test]
    fn capability_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&Capability::Supported).expect("serializes"),
            r#""supported""#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unsupported("no".to_string())).expect("serializes"),
            r#"{"unsupported":"no"}"#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unknown("maybe".to_string())).expect("serializes"),
            r#"{"unknown":"maybe"}"#
        );
    }

    #[test]
    fn sandbox_id_serializes_as_its_string() {
        assert_eq!(
            serde_json::to_string(&SandboxId("abc".to_string())).expect("serializes"),
            r#""abc""#
        );
    }

    #[test]
    fn limits_default_to_512_processes_and_4g() {
        assert_eq!(
            Limits::default(),
            Limits {
                max_processes: 512,
                memory: "4g".to_string(),
            }
        );
    }

    #[test]
    fn sandbox_spec_serializes_every_field() {
        assert_eq!(
            serde_json::to_string(&spec()).expect("serializes"),
            r#"{"name":"build","image":"example/base:1","user":"dev","workdir":"/work","network":"isolated","mounts":[{"host_path":"/host/project","sandbox_path":"/work/project","read_only":true,"follow_symlinks":false}],"limits":{"max_processes":512,"memory":"4g"}}"#
        );
    }

    #[test]
    fn command_debug_and_serde_never_show_a_value() {
        let mut command = CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        };
        command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        command.stdin = Some(b"s3cr3t-stdin".to_vec());

        let debug = format!("{command:?}");
        assert!(debug.contains("API_KEY"), "{debug}");
        assert!(!debug.contains("s3cr3t-value"), "{debug}");
        assert!(!debug.contains("s3cr3t-stdin"), "{debug}");
        assert!(debug.contains("Some(12)"), "{debug}");

        let json = serde_json::to_string(&command).expect("serializes");
        assert!(json.contains("API_KEY"), "{json}");
        assert!(!json.contains("s3cr3t-value"), "{json}");
        assert!(!json.contains("s3cr3t-stdin"), "{json}");
        assert!(json.contains(r#""stdin":12"#), "{json}");
    }

    #[test]
    fn command_serde_pins_an_empty_env_and_no_stdin() {
        let command = CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        };
        assert_eq!(
            serde_json::to_string(&command).expect("serializes"),
            r#"{"program":"run","args":["--fast"],"workdir":null,"timeout_secs":30,"env":[],"stdin":null}"#
        );
    }

    #[test]
    fn sandbox_error_display_prints_the_inner_text_only() {
        assert_eq!(
            SandboxError::Rejected("refused".to_string()).to_string(),
            "refused"
        );
        assert_eq!(
            SandboxError::Failed("failed".to_string()).to_string(),
            "failed"
        );
    }
}
