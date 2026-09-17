//! The coding agents this project supports, in one place.
//!
//! Every rule that names an agent reads this list, so no rule knows one
//! agent and not the others, and adding an agent here is the whole of
//! adding it. A test in `tests/scan_rules.rs` fails when a rule's doc text
//! or corpus falls out of step with this list.

/// One supported coding agent.
#[derive(Debug)]
pub struct Agent {
    /// The name a person uses for it.
    pub name: &'static str,
    /// Directories, under a home or a repository, where it keeps its own
    /// state: sessions, transcripts, history. Named without a leading slash.
    pub state_dirs: &'static [&'static str],
    /// The host of a hosted session of it, when it has hosted sessions.
    pub session_host: Option<&'static str>,
    /// The path prefix, on that host, that a session link starts with.
    pub session_path: Option<&'static str>,
}

/// Every supported agent, in the order this project lists them.
///
/// The host and the path of a session link are kept in two fields on
/// purpose. This project scans its own source, and the rule that reads
/// these fields matches the two joined. Kept apart, the source never
/// contains the shape the rule looks for.
pub const AGENTS: &[Agent] = &[
    Agent {
        name: "dsh",
        state_dirs: &[".dsh"],
        session_host: None,
        session_path: None,
    },
    Agent {
        name: "pi",
        state_dirs: &[".pi"],
        session_host: None,
        session_path: None,
    },
    Agent {
        name: "omp",
        state_dirs: &[".omp"],
        session_host: None,
        session_path: None,
    },
    Agent {
        name: "opencode",
        state_dirs: &[".opencode", ".config/opencode"],
        session_host: Some("opencode.ai"),
        session_path: Some("/s/"),
    },
    Agent {
        name: "codex",
        state_dirs: &[".codex"],
        session_host: Some("chatgpt.com"),
        session_path: Some("/codex/"),
    },
    Agent {
        name: "claude code",
        state_dirs: &[".claude"],
        session_host: Some("claude.ai"),
        session_path: Some("/code/session_"),
    },
    Agent {
        name: "copilot",
        state_dirs: &[".copilot"],
        session_host: None,
        session_path: None,
    },
];

impl Agent {
    /// `host/path-prefix` of a hosted session link, when the agent has one.
    #[must_use]
    pub fn session_link_prefix(&self) -> Option<String> {
        match (self.session_host, self.session_path) {
            (Some(host), Some(path)) => Some(format!("{host}{path}")),
            _ => None,
        }
    }
}

/// The agents that have hosted session links, by name.
#[must_use]
pub fn with_hosted_sessions() -> Vec<&'static str> {
    AGENTS
        .iter()
        .filter(|a| a.session_host.is_some())
        .map(|a| a.name)
        .collect()
}

/// The agents whose sessions live only on disk, by name.
#[must_use]
pub fn with_local_sessions_only() -> Vec<&'static str> {
    AGENTS
        .iter()
        .filter(|a| a.session_host.is_none())
        .map(|a| a.name)
        .collect()
}

/// Every state directory of every agent, in list order.
#[must_use]
pub fn state_dirs() -> Vec<&'static str> {
    AGENTS
        .iter()
        .flat_map(|a| a.state_dirs.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_a_name_and_a_state_dir() {
        for a in AGENTS {
            assert!(!a.name.is_empty());
            assert!(!a.state_dirs.is_empty(), "{}", a.name);
            for d in a.state_dirs {
                assert!(
                    d.starts_with('.'),
                    "{}: {d} must be a dot directory",
                    a.name
                );
            }
        }
    }

    #[test]
    fn a_session_host_always_comes_with_a_path() {
        for a in AGENTS {
            assert_eq!(
                a.session_host.is_some(),
                a.session_path.is_some(),
                "{}",
                a.name
            );
        }
    }

    #[test]
    fn names_and_state_dirs_are_unique() {
        let mut names: Vec<&str> = AGENTS.iter().map(|a| a.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), AGENTS.len());
        let mut dirs = state_dirs();
        dirs.sort_unstable();
        dirs.dedup();
        assert_eq!(
            dirs.len(),
            AGENTS.iter().map(|a| a.state_dirs.len()).sum::<usize>()
        );
    }
}
