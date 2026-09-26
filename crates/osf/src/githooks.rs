//! `osf hooks install`: writes the git hook scripts osf owns to a folder
//! outside the repository, and points the repository's own git config at
//! them. This is the routine choice for a plain shell; a coding agent's own
//! hook checkpoint runs a different way, through `osf hook post-tool`.

use crate::git;
use std::path::{Path, PathBuf};

/// Where osf writes the hook scripts it owns: a sibling of the journal's own
/// state directory. `OSF_HOOKS_DIR` overrides it directly; otherwise a test
/// that already isolates `OSF_STATE_DIR` isolates this folder too, with no
/// second variable to set.
///
/// # Errors
/// Returns an error under the same condition [`crate::journal::state_dir`] does.
pub fn hooks_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("OSF_HOOKS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let state_dir = crate::journal::state_dir()?;
    let base = state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(state_dir);
    Ok(base.join("githooks"))
}

struct HookScript {
    name: &'static str,
    body: &'static str,
}

// pre-commit takes no arguments; commit-msg's "$1" is its one true argument; pre-push forwards "$@", git's remote name and URL.
const SCRIPTS: &[HookScript] = &[
    HookScript {
        name: "pre-commit",
        body: "#!/bin/sh\nexec osf verify --checkpoint pre-commit\n",
    },
    HookScript {
        name: "commit-msg",
        body: "#!/bin/sh\nexec osf lint writing --context commit --format human \"$1\"\n",
    },
    HookScript {
        name: "pre-push",
        body: "#!/bin/sh\nexec osf verify --checkpoint pre-push \"$@\"\n",
    },
];

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .map_err(|e| format!("cannot make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Writes every hook script into `dir`, creating it first. Safe to call more
/// than once: each script's content is fixed, so a repeat run writes the
/// same bytes over the same paths.
fn write_scripts(dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut written = Vec::with_capacity(SCRIPTS.len());
    for script in SCRIPTS {
        let path = dir.join(script.name);
        std::fs::write(&path, script.body)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        make_executable(&path)?;
        written.push(path);
    }
    Ok(written)
}

/// The full path to `osf` on `PATH`, when it resolves to one there.
#[must_use]
pub fn find_osf_on_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(windows) { "osf.exe" } else { "osf" };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(exe_name))
        .find(|p| p.is_file())
}

/// What `osf hooks install` did.
pub struct InstallReport {
    pub repo_root: PathBuf,
    pub hooks_dir: PathBuf,
    pub scripts: Vec<PathBuf>,
    pub osf_on_path: Option<PathBuf>,
}

/// Writes the hook scripts and points `dir`'s repository at them. Safe to
/// call more than once: it writes the same scripts to the same folder and
/// sets the same config value each time.
///
/// # Errors
/// Returns an error when `dir` is not inside a git repository, the hooks
/// folder cannot be written, or the git config write fails.
pub fn install(dir: &Path) -> Result<InstallReport, String> {
    let repo_root = git::repo_root(dir).map_err(|e| e.to_string())?;
    let dir_for_hooks = hooks_dir()?;
    let scripts = write_scripts(&dir_for_hooks)?;
    git::config_set_local(
        &repo_root,
        "core.hooksPath",
        &dir_for_hooks.to_string_lossy(),
    )
    .map_err(|e| e.to_string())?;
    Ok(InstallReport {
        repo_root,
        hooks_dir: dir_for_hooks,
        scripts,
        osf_on_path: find_osf_on_path(),
    })
}

/// What `osf hooks install --check` found.
pub enum CheckStatus {
    /// `core.hooksPath` already points at osf's own folder.
    Installed(PathBuf),
    /// `core.hooksPath` is not set at all.
    NotSet,
    /// `core.hooksPath` still names the tracked folder this project retired.
    RetiredTrackedHooks,
    /// `core.hooksPath` names something else entirely.
    PointsElsewhere(String),
}

impl CheckStatus {
    #[must_use]
    pub const fn is_installed(&self) -> bool {
        matches!(self, CheckStatus::Installed(_))
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            CheckStatus::Installed(dir) => {
                format!("core.hooksPath is {}, osf's own folder", dir.display())
            }
            CheckStatus::NotSet => "core.hooksPath is not set; run `osf hooks install`".to_string(),
            CheckStatus::RetiredTrackedHooks => {
                "core.hooksPath is '.osf/hooks', a tracked folder this project retired; \
                 run `osf hooks install`"
                    .to_string()
            }
            CheckStatus::PointsElsewhere(value) => {
                format!(
                    "core.hooksPath is '{value}', not osf's own folder; run `osf hooks install`"
                )
            }
        }
    }
}

/// Whether `value`, a `core.hooksPath` setting, names the tracked
/// `.osf/hooks` folder this project retired, under either path separator
/// and with or without a leading `./`.
fn names_retired_tracked_hooks(value: &str) -> bool {
    let normalised = value.trim().replace('\\', "/");
    let normalised = normalised
        .strip_prefix("./")
        .unwrap_or(&normalised)
        .trim_end_matches('/');
    normalised == ".osf/hooks"
}

/// Whether `value`, resolved against `repo_root` when relative, names the
/// same folder as `want`.
fn same_folder(repo_root: &Path, value: &str, want: &Path) -> bool {
    let candidate = Path::new(value);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        repo_root.join(candidate)
    };
    match (
        std::fs::canonicalize(&candidate),
        std::fs::canonicalize(want),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => candidate == want,
    }
}

/// Whether `dir`'s repository already points its hooks at osf's own folder.
///
/// # Errors
/// Returns an error when `dir` is not inside a git repository, or git
/// itself cannot run.
pub fn check(dir: &Path) -> Result<CheckStatus, String> {
    let repo_root = git::repo_root(dir).map_err(|e| e.to_string())?;
    let want = hooks_dir()?;
    let current = git::config_get_local(&repo_root, "core.hooksPath").map_err(|e| e.to_string())?;
    Ok(match current {
        None => CheckStatus::NotSet,
        Some(value) if same_folder(&repo_root, &value, &want) => CheckStatus::Installed(want),
        Some(value) if names_retired_tracked_hooks(&value) => CheckStatus::RetiredTrackedHooks,
        Some(value) => CheckStatus::PointsElsewhere(value),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retired_tracked_hooks_path_is_recognised_under_either_separator() {
        assert!(names_retired_tracked_hooks(".osf/hooks"));
        assert!(names_retired_tracked_hooks("./.osf/hooks"));
        assert!(names_retired_tracked_hooks(".osf/hooks/"));
        assert!(names_retired_tracked_hooks(".osf\\hooks"));
        assert!(!names_retired_tracked_hooks("/somewhere/.osf/githooks"));
        assert!(!names_retired_tracked_hooks(".osf/hooksomething"));
    }

    #[test]
    fn each_check_status_names_the_retired_hooks_path_or_the_fix() {
        assert!(CheckStatus::NotSet.message().contains("osf hooks install"));
        assert!(CheckStatus::RetiredTrackedHooks
            .message()
            .contains(".osf/hooks"));
        assert!(CheckStatus::PointsElsewhere("elsewhere".to_string())
            .message()
            .contains("elsewhere"));
        assert!(
            CheckStatus::Installed(PathBuf::from("/somewhere/.osf/githooks"))
                .message()
                .contains("githooks")
        );
    }

    #[test]
    fn only_the_installed_status_reports_itself_installed() {
        assert!(CheckStatus::Installed(PathBuf::from("/x")).is_installed());
        assert!(!CheckStatus::NotSet.is_installed());
        assert!(!CheckStatus::RetiredTrackedHooks.is_installed());
        assert!(!CheckStatus::PointsElsewhere(String::new()).is_installed());
    }
}
