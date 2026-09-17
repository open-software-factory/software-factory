//! What the scan knows about the repository it runs in: where it is
//! hosted, who owns it, and whether the public can see it.
//!
//! Resolved once, before any rule runs. The configuration may state each
//! fact; otherwise the git remote gives the host, the owner, and the name,
//! and the host's own command line gives the visibility where one is
//! installed. No rule assumes a public repository. When the visibility
//! cannot be read, the scan treats the repository as public and says so,
//! because a leak into a repository that turns out to be public is the
//! failure this whole tool exists to prevent.

use crate::config::ScanConfig;
use std::path::Path;
use std::process::Command;

/// Whether the public can read the repository.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    /// Could not be read from the configuration or the host. Treated as
    /// public by every rule, and reported as a note.
    #[default]
    Unknown,
}

impl Visibility {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Unknown => "unknown",
        }
    }

    /// `public`, `private`, or the host's `internal`, which reads as
    /// private here: it is not the public.
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "public" => Some(Visibility::Public),
            "private" | "internal" => Some(Visibility::Private),
            _ => None,
        }
    }

    /// True unless the repository is known to be private.
    #[must_use]
    pub fn treated_as_public(self) -> bool {
        !matches!(self, Visibility::Private)
    }
}

/// The repository the scan runs in. Any field may be missing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Repository {
    pub host: Option<String>,
    pub owner: Option<String>,
    pub name: Option<String>,
    pub visibility: Visibility,
}

/// A resolved repository and every fact that could not be established.
#[derive(Debug)]
pub struct Resolved {
    pub repository: Repository,
    pub notes: Vec<String>,
}

/// Resolves the repository at `dir`. Never fails: a fact that cannot be
/// read is left missing and explained in a note.
#[must_use]
pub fn resolve(dir: &Path, cfg: &ScanConfig) -> Resolved {
    let mut repo = Repository::default();
    let mut notes = Vec::new();

    if !cfg.project_owner.is_empty() {
        repo.owner = Some(cfg.project_owner.clone());
    }

    match crate::git::remote(dir) {
        Ok(remote) => {
            repo.host = Some(remote.host);
            repo.name = Some(remote.name);
            if repo.owner.is_none() {
                repo.owner = Some(remote.owner);
            }
        }
        Err(e) => notes.push(format!("the git remote could not be read ({e})")),
    }

    repo.visibility = match visibility_from_config(cfg, &mut notes) {
        Some(v) => v,
        None => visibility_from_host(&repo, &mut notes),
    };

    if repo.visibility == Visibility::Unknown {
        notes.push(
            "repository visibility is unknown, so every finding is treated as if the repository were public; set `[scan] repository_visibility` to state it"
                .to_string(),
        );
    }
    if repo.owner.is_none() {
        notes.push(
            "scan-foreign-reference did not run: no project owner is configured and the git remote gave none"
                .to_string(),
        );
    }

    Resolved {
        repository: repo,
        notes,
    }
}

fn visibility_from_config(cfg: &ScanConfig, notes: &mut Vec<String>) -> Option<Visibility> {
    if cfg.repository_visibility.is_empty() {
        return None;
    }
    let parsed = Visibility::parse(&cfg.repository_visibility);
    if parsed.is_none() {
        notes.push(format!(
            "`[scan] repository_visibility` is '{}', which is neither public nor private, so it was ignored",
            cfg.repository_visibility
        ));
    }
    parsed
}

/// Asks the host's command line for the visibility, where a client for
/// that host is installed and the remote names a repository on it.
fn visibility_from_host(repo: &Repository, notes: &mut Vec<String>) -> Visibility {
    let (Some(host), Some(owner), Some(name)) = (&repo.host, &repo.owner, &repo.name) else {
        return Visibility::Unknown;
    };
    if !host.eq_ignore_ascii_case("github.com") {
        notes.push(format!(
            "no visibility lookup is available for a repository hosted on {host}"
        ));
        return Visibility::Unknown;
    }
    let output = Command::new("gh")
        .args([
            "repo",
            "view",
            &format!("{owner}/{name}"),
            "--json",
            "visibility",
            "--jq",
            ".visibility",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            if let Some(v) = Visibility::parse(&raw) {
                v
            } else {
                notes.push(format!(
                    "the host reported a visibility of '{}', which this tool does not know",
                    raw.trim()
                ));
                Visibility::Unknown
            }
        }
        Ok(out) => {
            notes.push(format!(
                "the host's command line could not report the visibility: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
            Visibility::Unknown
        }
        Err(e) => {
            notes.push(format!(
                "the host's command line is not available to report the visibility ({e})"
            ));
            Visibility::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_words_map_to_the_two_states() {
        assert_eq!(Visibility::parse("PUBLIC"), Some(Visibility::Public));
        assert_eq!(Visibility::parse("private"), Some(Visibility::Private));
        assert_eq!(Visibility::parse("internal"), Some(Visibility::Private));
        assert_eq!(Visibility::parse("secret"), None);
        assert_eq!(Visibility::parse(""), None);
    }

    #[test]
    fn only_private_escapes_public_treatment() {
        assert!(Visibility::Public.treated_as_public());
        assert!(Visibility::Unknown.treated_as_public());
        assert!(!Visibility::Private.treated_as_public());
    }
}
