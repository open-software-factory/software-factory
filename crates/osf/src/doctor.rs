//! Read-only setup report: what is configured, what is missing, and what
//! cannot be checked yet. It never writes, installs or fixes anything.

use crate::agents::{self, Agent};
use crate::config;
use crate::githooks::{self, CheckStatus};
use serde::Serialize;
use std::path::Path;

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
pub fn report(root: &Path, config_file: Option<&Path>) -> Vec<Check> {
    vec![
        osf_check(root),
        settings_check(config_file),
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

fn osf_check(root: &Path) -> Check {
    let pin = std::fs::read_to_string(root.join(".osf/version"))
        .ok()
        .map(|text| text.trim().to_string());
    osf_from_pin(pin.as_deref(), env!("CARGO_PKG_VERSION"))
}

/// Compares the running binary with the repository pin.
#[must_use]
pub fn osf_from_pin(pin: Option<&str>, running: &str) -> Check {
    let Some(pin) = pin else {
        return Check::new(
            "osf",
            Status::Warn,
            format!("osf {running}; no .osf/version pin is present"),
            Some("add .osf/version with a release number, or workspace for this repository"),
        );
    };
    if pin == "workspace" {
        Check::new(
            "osf",
            Status::Ok,
            format!("osf {running}; the repository pins the workspace build"),
            None,
        )
    } else if pin == running {
        Check::new(
            "osf",
            Status::Ok,
            format!("osf {running}; the repository pins {pin}"),
            None,
        )
    } else {
        Check::new(
            "osf",
            Status::Fail,
            format!("osf {running} does not match the pin {pin}"),
            Some("run the pinned osf, or change .osf/version"),
        )
    }
}

fn settings_check(config_file: Option<&Path>) -> Check {
    match config::load(config_file, &[], &[], false) {
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
            let home = crate::reviewers::real_home();
            for agent in &selection.enabled {
                if let Some(problem) = missing_credential(agent, &env_is_set, home.as_deref()) {
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
        None,
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
    if Path::new("/opt/factory").exists() {
        Check::new("container", Status::Ok, "inside the factory image", None)
    } else if Path::new("/.dockerenv").exists() {
        Check::new(
            "container",
            Status::Warn,
            "in a Docker container, but the factory image is not confirmed",
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

fn env_is_set(name: &str) -> bool {
    value_counts(std::env::var(name).ok().as_deref())
}

fn value_counts(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// The first missing credential for an agent, or None when one is present.
///
/// `env_present` is a lookup so a test never has to set a process-wide
/// variable. A login file counts only when it exists under home.
#[must_use]
pub fn missing_credential(
    agent: &Agent,
    env_present: &dyn Fn(&str) -> bool,
    home: Option<&Path>,
) -> Option<String> {
    let review = agent.review.as_ref()?;
    let env_ok = review.credential_env.iter().any(|name| env_present(name));
    let login_ok = home.is_some_and(|home| {
        review
            .login_paths
            .iter()
            .any(|path| home.join(path).is_file())
    });
    if env_ok || login_ok {
        return None;
    }
    let logins = review.login_paths.join(", ");
    let home_text = home.map_or_else(
        || "no home directory is set".to_string(),
        |h| format!("under {}", h.display()),
    );
    let need = if review.credential_env.is_empty() {
        format!("add {logins} {home_text}")
    } else {
        format!(
            "set {} or add {logins} {home_text}",
            review.credential_env.join(" or ")
        )
    };
    Some(format!("{} has no credential: {need}", agent.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AGENTS;
    use std::path::PathBuf;

    #[test]
    fn every_hook_status_maps_to_its_report_line() {
        let installed = hooks_from_status(CheckStatus::Installed(PathBuf::from("/tmp/githooks")));
        assert_eq!(installed.status, Status::Ok);
        for status in [
            CheckStatus::NotSet,
            CheckStatus::RetiredTrackedHooks,
            CheckStatus::PointsElsewhere("elsewhere".to_string()),
        ] {
            let check = hooks_from_status(status);
            assert_eq!(check.status, Status::Fail);
            assert_eq!(check.fix.as_deref(), Some("osf hooks install"));
        }
    }

    #[test]
    fn an_empty_credential_value_does_not_count() {
        assert!(!value_counts(None));
        assert!(!value_counts(Some("")));
        assert!(value_counts(Some("value")));
    }

    #[test]
    fn the_osf_check_compares_the_pin() {
        assert_eq!(osf_from_pin(None, "1.2.3").status, Status::Warn);
        assert_eq!(osf_from_pin(Some("workspace"), "1.2.3").status, Status::Ok);
        assert_eq!(osf_from_pin(Some("1.2.3"), "1.2.3").status, Status::Ok);
        let mismatch = osf_from_pin(Some("1.2.3"), "9.9.9");
        assert_eq!(mismatch.status, Status::Fail);
        assert!(mismatch.detail.contains("9.9.9"));
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
        let missing = missing_credential(agent, &|_| false, Some(Path::new("/nonexistent-home")));
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
            Some(Path::new("/nonexistent-home"))
        )
        .is_none());
        let home = std::env::temp_dir().join(format!("osf-doctor-home-{}", std::process::id())); // osf: temp-dir allowed, a throwaway home for the login-file check
        let path = home.join(review.login_paths.first().expect("a login path"));
        std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        std::fs::write(&path, "present").expect("login file");
        assert!(missing_credential(agent, &|_| false, Some(&home)).is_none());
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn an_unknown_check_name_is_an_error() {
        let error = keep(Vec::new(), Some("nope")).expect_err("unknown");
        assert!(error.contains("nope"));
    }
}
