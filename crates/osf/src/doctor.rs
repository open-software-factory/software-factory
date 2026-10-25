//! Read-only setup report: what is configured, what is missing, and what
//! cannot be checked yet. It never writes, installs or fixes anything.

use crate::agents::{self, Agent};
use crate::config;
use crate::githooks::{self, CheckStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How one check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Unknown,
}

impl Status {
    /// The word the text report prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    /// The command or settings page that fixes a failure.
    pub fix: Option<String>,
}

impl Check {
    fn new(name: &str, status: Status, detail: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            status,
            detail: detail.into(),
            fix: fix.map(str::to_string),
        }
    }
}

/// The checks, in report order.
pub const CHECK_NAMES: &[&str] = &[
    "osf",
    "settings",
    "agents",
    "agent-binaries",
    "hooks",
    "container",
];

/// Builds the report for the repository at root.
#[must_use]
pub fn report(root: &Path) -> Vec<Check> {
    vec![
        osf_check(),
        settings_check(),
        agents_check(root),
        binaries_check(),
        hooks_check(root),
        container_check(),
    ]
}

/// Keeps the checks the caller named. An unknown name is an error.
///
/// # Errors
/// Names the first check that is not in `CHECK_NAMES`.
pub fn keep(checks: Vec<Check>, wanted: Option<&str>) -> Result<Vec<Check>, String> {
    let Some(wanted) = wanted else {
        return Ok(checks);
    };
    if !CHECK_NAMES.contains(&wanted) {
        return Err(format!(
            "no check named {wanted}; choose one of {}",
            CHECK_NAMES.join(", ")
        ));
    }
    Ok(checks
        .into_iter()
        .filter(|check| check.name == wanted)
        .collect())
}

fn osf_check() -> Check {
    Check::new(
        "osf",
        Status::Ok,
        format!("osf {}", env!("CARGO_PKG_VERSION")),
        None,
    )
}

fn settings_check() -> Check {
    match config::load(None, &[], &[], false) {
        Ok(loaded) => {
            let where_ = loaded.file.as_deref().map_or_else(
                || "compiled defaults".to_string(),
                |p| p.display().to_string(),
            );
            Check::new(
                "settings",
                Status::Ok,
                format!("config loads from {where_}"),
                None,
            )
        }
        Err(error) => Check::new(
            "settings",
            Status::Fail,
            error.to_string(),
            Some("fix the osf.toml key named above, or unset OSF_CONFIG"),
        ),
    }
}

fn agents_check(root: &Path) -> Check {
    match agents::selection(root) {
        Ok(selection) => {
            let enabled: Vec<&str> = selection.enabled.iter().map(|a| a.name).collect();
            let mut missing = Vec::new();
            for agent in &selection.enabled {
                if let Some(problem) =
                    missing_credential(agent, &|name| std::env::var_os(name).is_some(), &home_dir())
                {
                    missing.push(problem);
                }
            }
            if missing.is_empty() {
                Check::new(
                    "agents",
                    Status::Ok,
                    format!(
                        "{} enabled: {}; builder {}; reviewers {}",
                        enabled.len(),
                        enabled.join(", "),
                        selection.builder.name,
                        selection
                            .reviewers
                            .iter()
                            .map(|a| a.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None,
                )
            } else {
                Check::new(
                    "agents",
                    Status::Fail,
                    missing.join("; "),
                    Some("set the named variable or add the login file"),
                )
            }
        }
        Err(error) => Check::new(
            "agents",
            Status::Fail,
            error,
            Some("fix the [agents] table in osf.toml"),
        ),
    }
}

fn binaries_check() -> Check {
    Check::new(
        "agent-binaries",
        Status::Unknown,
        "agent binaries and pinned versions are not checked yet; the launch templates are #108",
        Some("implement #108, then check each enabled agent's binary and version"),
    )
}

fn hooks_check(root: &Path) -> Check {
    match githooks::check(root) {
        Ok(status) => hooks_from_status(status),
        Err(error) => Check::new(
            "hooks",
            Status::Unknown,
            format!("hooks not checked: {error}"),
            Some("run osf doctor inside the repository"),
        ),
    }
}

fn hooks_from_status(status: CheckStatus) -> Check {
    match status {
        CheckStatus::Installed(dir) => Check::new(
            "hooks",
            Status::Ok,
            format!("core.hooksPath points at {}", dir.display()),
            None,
        ),
        CheckStatus::NotSet => Check::new(
            "hooks",
            Status::Fail,
            "no core.hooksPath is set",
            Some("osf hooks install"),
        ),
        CheckStatus::RetiredTrackedHooks => Check::new(
            "hooks",
            Status::Fail,
            "core.hooksPath points at the retired .osf/hooks path",
            Some("osf hooks install"),
        ),
        CheckStatus::PointsElsewhere(value) => Check::new(
            "hooks",
            Status::Fail,
            format!("core.hooksPath points at {value}"),
            Some("osf hooks install"),
        ),
    }
}

fn container_check() -> Check {
    if Path::new("/.dockerenv").exists() {
        Check::new(
            "container",
            Status::Ok,
            "inside the development container",
            None,
        )
    } else {
        Check::new(
            "container",
            Status::Warn,
            "outside the factory image; container checks do not apply",
            None,
        )
    }
}

/// The first missing credential for an agent, or None when one is present.
///
/// `env_present` is a lookup so a test never has to set a process-wide
/// variable. A login file counts only when it exists under home.
#[must_use]
pub fn missing_credential(
    agent: &Agent,
    env_present: &dyn Fn(&str) -> bool,
    home: &Path,
) -> Option<String> {
    let review = agent.review.as_ref()?;
    let env_ok = review.credential_env.iter().any(|name| env_present(name));
    let login_ok = review
        .login_paths
        .iter()
        .any(|path| home.join(path).is_file());
    if env_ok || login_ok {
        return None;
    }
    let logins = review.login_paths.join(", ");
    let need = if review.credential_env.is_empty() {
        format!("add {logins} under {}", home.display())
    } else {
        format!(
            "set {} or add {logins} under {}",
            review.credential_env.join(" or "),
            home.display()
        )
    };
    Some(format!("{} has no credential: {need}", agent.name))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AGENTS;

    #[test]
    fn an_installed_hook_path_is_ok_and_a_missing_one_fails() {
        let installed = hooks_from_status(CheckStatus::Installed(PathBuf::from("/tmp/githooks")));
        assert_eq!(installed.status, Status::Ok);
        let missing = hooks_from_status(CheckStatus::NotSet);
        assert_eq!(missing.status, Status::Fail);
        assert_eq!(missing.fix.as_deref(), Some("osf hooks install"));
    }

    #[test]
    fn a_reviewer_with_neither_variable_nor_login_file_is_missing() {
        let agent = AGENTS
            .iter()
            .find(|a| {
                a.review
                    .as_ref()
                    .is_some_and(|r| !r.credential_env.is_empty() && !r.login_paths.is_empty())
            })
            .expect("an agent with both a variable and a login path");
        let missing = missing_credential(agent, &|_| false, Path::new("/nonexistent-home"));
        let text = missing.expect("a missing credential is reported");
        assert!(text.contains(agent.name));
    }

    #[test]
    fn a_credential_variable_or_a_login_file_satisfies_the_check() {
        let agent = AGENTS
            .iter()
            .find(|a| {
                a.review
                    .as_ref()
                    .is_some_and(|r| !r.credential_env.is_empty() && !r.login_paths.is_empty())
            })
            .expect("an agent with both a variable and a login path");
        let review = agent.review.as_ref().expect("review");
        let first_env = *review
            .credential_env
            .first()
            .expect("a credential variable");
        assert!(missing_credential(
            agent,
            &|name| name == first_env,
            Path::new("/nonexistent-home")
        )
        .is_none());
        let home = std::env::temp_dir().join(format!("osf-doctor-home-{}", std::process::id())); // osf: temp-dir allowed, a throwaway home for the login-file check
        let path = home.join(review.login_paths.first().expect("a login path"));
        std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        std::fs::write(&path, "present").expect("login file");
        assert!(missing_credential(agent, &|_| false, &home).is_none());
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn an_unknown_check_name_is_an_error() {
        let error = keep(Vec::new(), Some("nope")).expect_err("unknown");
        assert!(error.contains("nope"));
    }
}
