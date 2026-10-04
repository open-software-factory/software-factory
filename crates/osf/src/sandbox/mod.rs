//! Provider-neutral sandbox interface: the seam between the factory and a sandbox provider.

pub mod fake;

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
}

/// One command to run inside a sandbox.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub timeout_secs: Option<u64>,
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
/// Destroy is not idempotent: destroying a sandbox the provider does not know may be an error.
pub trait Sandbox {
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
    fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError>;

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
            }],
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
    fn sandbox_spec_serializes_every_field() {
        assert_eq!(
            serde_json::to_string(&spec()).expect("serializes"),
            r#"{"name":"build","image":"example/base:1","user":"dev","workdir":"/work","network":"isolated","mounts":[{"host_path":"/host/project","sandbox_path":"/work/project","read_only":true}]}"#
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
