//! The coding agents this project supports, in one place.
//!
//! Every rule that names an agent reads this list, so no rule knows one
//! agent and not the others, and adding an agent here is the whole of
//! adding it. A test in `tests/scan_rules.rs` fails when a rule's doc text
//! or corpus falls out of step with this list.
//!
//! Where each agent keeps its conversations was established by listing a
//! home directory on a machine with every agent installed, on 17 September
//! 2026. An agent whose layout changes needs its entry here updated, and
//! the corpus will say so the moment a sample stops matching.

/// Where an agent's sessions can be reached from outside the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sessions {
    /// The agent publishes a page per session on a host. `host` is the
    /// host, `path` the prefix every session page's path starts with. The
    /// two are kept apart on purpose: this project scans its own source,
    /// and the joined form is exactly what the scan looks for.
    Hosted {
        host: &'static str,
        path: &'static str,
    },
    /// Sessions live only on the machine that ran them. A leak from such
    /// an agent is a path into its state directory, never a link.
    LocalOnly,
}

/// One supported coding agent.
#[derive(Debug)]
pub struct Agent {
    /// The name a person uses for it.
    pub name: &'static str,
    /// Directories, under a home or a repository, where it keeps its own
    /// state. Named without a leading slash.
    pub state_dirs: &'static [&'static str],
    /// Paths under each state directory that hold conversations,
    /// transcripts, or logs of them. A committed configuration file lives
    /// under the same directory and is not in this list.
    pub session_paths: &'static [&'static str],
    /// Whether its sessions are reachable by link, and where.
    pub sessions: Sessions,
}

/// Every supported agent, in the order this project lists them.
pub const AGENTS: &[Agent] = &[
    // dsh keeps every conversation under a `sessions` folder in its home
    // directory. It has no session page on any host.
    Agent {
        name: "dsh",
        state_dirs: &[".dsh"],
        session_paths: &["sessions"],
        sessions: Sessions::LocalOnly,
    },
    // pi keeps conversations under `agent/sessions`, one file per
    // session. No session page on any host.
    Agent {
        name: "pi",
        state_dirs: &[".pi"],
        session_paths: &["agent/sessions"],
        sessions: Sessions::LocalOnly,
    },
    // omp is built on pi and uses the same `agent/sessions` layout, plus
    // a folder of terminal sessions and one of logs. No session page.
    Agent {
        name: "omp",
        state_dirs: &[".omp"],
        session_paths: &["agent/sessions", "agent/terminal-sessions", "logs"],
        sessions: Sessions::LocalOnly,
    },
    // opencode keeps configuration in one place and its data, including
    // session storage and tool output, in a data directory. A session can
    // be shared as a page on its host.
    Agent {
        name: "opencode",
        state_dirs: &[".opencode", ".config/opencode", ".local/share/opencode"],
        session_paths: &["storage", "snapshot", "tool-output", "log"],
        sessions: Sessions::Hosted {
            host: "opencode.ai",
            path: "/s/",
        },
    },
    // codex keeps live and archived sessions, a history file, and logs
    // in its home directory. Its hosted tasks have pages under the
    // vendor's site.
    Agent {
        name: "codex",
        state_dirs: &[".codex"],
        session_paths: &["sessions", "archived_sessions", "history.jsonl", "log"],
        sessions: Sessions::Hosted {
            host: "chatgpt.com",
            path: "/codex/",
        },
    },
    // claude code keeps transcripts under `projects`, one folder per
    // working directory, plus session and transcript folders and a file
    // history. Its hosted sessions have pages under the vendor's site.
    Agent {
        name: "claude code",
        state_dirs: &[".claude"],
        session_paths: &["projects", "sessions", "transcripts", "file-history"],
        sessions: Sessions::Hosted {
            host: "claude.ai",
            path: "/code/session_",
        },
    },
    // copilot keeps each session's events and database under a
    // `session-state` folder, plus logs. No session page on any host.
    Agent {
        name: "copilot",
        state_dirs: &[".copilot"],
        session_paths: &["session-state", "logs"],
        sessions: Sessions::LocalOnly,
    },
];

impl Agent {
    /// The host and path prefix of a hosted session link, when the agent
    /// has one.
    #[must_use]
    pub fn hosted(&self) -> Option<(&'static str, &'static str)> {
        match self.sessions {
            Sessions::Hosted { host, path } => Some((host, path)),
            Sessions::LocalOnly => None,
        }
    }
}

/// The agents that have hosted session links, by name.
#[must_use]
pub fn with_hosted_sessions() -> Vec<&'static str> {
    AGENTS
        .iter()
        .filter(|a| a.hosted().is_some())
        .map(|a| a.name)
        .collect()
}

/// The agents whose sessions live only on disk, by name.
#[must_use]
pub fn with_local_sessions_only() -> Vec<&'static str> {
    AGENTS
        .iter()
        .filter(|a| a.hosted().is_none())
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
    fn every_agent_has_a_name_a_state_dir_and_a_session_path() {
        for a in AGENTS {
            assert!(!a.name.is_empty());
            assert!(!a.state_dirs.is_empty(), "{}", a.name);
            assert!(!a.session_paths.is_empty(), "{}", a.name);
            for d in a.state_dirs {
                assert!(
                    d.starts_with('.'),
                    "{}: {d} must be a dot directory",
                    a.name
                );
            }
            for p in a.session_paths {
                assert!(
                    !p.starts_with('/') && !p.ends_with('/'),
                    "{}: {p} is relative, no leading or trailing slash",
                    a.name
                );
            }
        }
    }

    #[test]
    fn a_hosted_session_names_a_host_and_a_path() {
        for a in AGENTS {
            if let Some((host, path)) = a.hosted() {
                assert!(host.contains('.'), "{}: host {host}", a.name);
                assert!(path.starts_with('/'), "{}: path {path}", a.name);
            }
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
