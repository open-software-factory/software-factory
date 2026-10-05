//! The coding agents osf can drive, in one place.
//!
//! This is the one list. Each entry carries everything osf needs to know
//! about an agent: its name, model family, command, hook wiring, session
//! state folders, and the login paths a reviewer home needs. Every rule that
//! names an agent reads this list, so adding an agent here is the whole of
//! adding it. A repository selects from it in `osf.toml` under `[agents]`,
//! and naming an agent this list does not hold is a configuration error.
//!
//! Where each agent keeps its conversations was established by listing a
//! home directory on a machine with every agent installed, on 17 September
//! 2026. An agent whose layout changes needs its entry here updated, and
//! the corpus will say so the moment a sample stops matching.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The agent a repository builds with when `[agents]` names none.
pub const DEFAULT_BUILDER: &str = "dsh";

/// The file the container image installs osf to. Its presence is how osf
/// tells that it runs inside the factory container.
pub const FACTORY_CONTAINER_MARKER: &str = "/opt/factory/bin/osf";

/// The file Docker mounts to mark a container.
pub const DOCKER_MARKER: &str = "/.dockerenv";

/// The facts about the marker path that decide whether to trust it.
#[cfg(unix)]
struct MarkerFacts {
    /// Whether the path is a regular file rather than a symlink.
    regular_file: bool,
    /// The user that owns the path.
    owner_uid: u32,
    /// The path's permission bits.
    mode: u32,
}

/// Whether the marker facts and the Docker marker mean the factory container.
#[cfg(unix)]
#[must_use]
fn marker_trusted(facts: Option<MarkerFacts>, dockerenv_exists: bool) -> bool {
    let Some(facts) = facts else {
        return false;
    };
    facts.regular_file && facts.owner_uid == 0 && (facts.mode & 0o022) == 0 && dockerenv_exists
}

/// The marker facts for `path`, or `None` when it cannot be read.
#[cfg(unix)]
#[must_use]
fn facts_of(path: &Path) -> Option<MarkerFacts> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(MarkerFacts {
        regular_file: meta.is_file(),
        owner_uid: meta.uid(),
        mode: meta.mode(),
    })
}

/// Whether osf runs inside the factory container.
#[cfg(unix)]
#[must_use]
pub fn in_factory_container() -> bool {
    marker_trusted(
        facts_of(Path::new(FACTORY_CONTAINER_MARKER)),
        Path::new(DOCKER_MARKER).exists(),
    )
}

/// Whether osf runs inside the factory container.
#[cfg(not(unix))]
#[must_use]
pub fn in_factory_container() -> bool {
    false
}

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

/// How a reviewer's `schema_flag` value is given: most agents take a file
/// path, but at least one takes the schema's own JSON text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaArg {
    #[default]
    Path,
    Inline,
}

/// The documented settings that hold an agent to read-only tools, or its sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOnly {
    /// Arguments added after the agent's command.
    pub args: &'static [&'static str],
    /// Environment variables set on the agent's process.
    pub env: &'static [(&'static str, &'static str)],
}

/// The documented switches that make an agent ignore the settings, plugins and
/// instruction files of the folder it starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    /// Arguments added after the agent's command.
    pub args: &'static [&'static str],
    /// Environment variables set on the agent's process.
    pub env: &'static [(&'static str, &'static str)],
}

/// File and folder names, besides each agent's own, that coding agents read
/// project instructions or settings from. A reviewer's clean copy leaves them out.
pub const OTHER_AGENT_SETTINGS: &[&str] = &[
    ".cursor",
    ".cursorrules",
    ".windsurf",
    ".windsurfrules",
    ".mcp.json",
    "AGENTS.md",
    "AGENTS.override.md",
    "CLAUDE.md",
    "CLAUDE.local.md",
    "GEMINI.md",
];

/// What osf needs to run an agent headless as a reviewer.
#[derive(Debug)]
pub struct Review {
    /// How the agent is told to ignore the settings of the folder it runs in.
    /// A reviewer runs in a clean copy of the change that holds none, and
    /// these switches back that up.
    pub clean_copy: Switches,
    /// How the agent is held to read-only tools. `None` when the agent
    /// documents no such mode: it then never starts as a reviewer.
    pub read_only: Option<ReadOnly>,
    /// The settings used instead of `read_only` when osf runs inside the factory container.
    pub in_container: Option<ReadOnly>,
    /// The flag that introduces the answer schema, when the agent can be
    /// asked to validate its own output against one.
    pub schema_flag: Option<&'static str>,
    /// How `schema_flag`'s value is given.
    pub schema_as: SchemaArg,
    /// A JSON pointer to the answer inside the agent's own output envelope.
    /// Empty when the whole output is the answer.
    pub answer_pointer: &'static str,
    /// The flag that introduces a model name, when the agent takes one.
    pub model_flag: Option<&'static str>,
    /// The environment variables that carry the agent's own provider
    /// credential. A reviewer starts with only these.
    pub credential_env: &'static [&'static str],
    /// Paths, relative to the real home, that the agent's own login lives
    /// in. A reviewer's fresh home holds a copy of only these.
    pub login_paths: &'static [&'static str],
    /// A command, program first, that starts the agent's read-only sandbox
    /// around a harmless command. It exits 0 only when the sandbox works where
    /// osf runs. Used only outside the factory container, where the agent's
    /// own sandbox is on. Empty when the read-only mode has no sandbox to
    /// start. A reviewer whose check fails never starts.
    pub sandbox_check: &'static [&'static str],
}

/// Where and how an agent's stop hook, and its prompt hook when it has one,
/// are configured. Every path is relative to the root a caller names.
#[derive(Debug)]
pub enum Hooks {
    /// A JSON file holding a `hooks` object with `Stop` and
    /// `UserPromptSubmit` entries that run `osf hook stop` and `osf hook prompt`.
    JsonFile { file: &'static str },
    /// A dsh patch file that adds the osf plugin to the profile.
    DshPatch {
        file: &'static str,
        package: &'static str,
    },
    /// An opencode config file whose `plugin` list names the osf plugin.
    PluginList {
        file: &'static str,
        package: &'static str,
    },
    /// An omp extension file that re-exports the osf hook package.
    Extension {
        file: &'static str,
        package: &'static str,
    },
}

impl Hooks {
    /// The settings file, relative to the root.
    #[must_use]
    pub fn file(&self) -> &'static str {
        match self {
            Hooks::JsonFile { file }
            | Hooks::DshPatch { file, .. }
            | Hooks::PluginList { file, .. }
            | Hooks::Extension { file, .. } => file,
        }
    }

    /// The settings file's text.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Hooks::JsonFile { .. } => {
                let entry = |command: &str| {
                    serde_json::json!([{"hooks": [{"type": "command", "command": command}]}])
                };
                let value = serde_json::json!({
                    "hooks": {
                        "Stop": entry("osf hook stop"),
                        "UserPromptSubmit": entry("osf hook prompt"),
                    }
                });
                format!("{value:#}\n")
            }
            Hooks::DshPatch { package, .. } => format!(
                "- insert:\n    - id: osf-writing-check\n      name: '{package}'\n      config:\n        command: osf\n"
            ),
            Hooks::PluginList { package, .. } => {
                format!("{:#}\n", serde_json::json!({ "plugin": [package] }))
            }
            Hooks::Extension { package, .. } => {
                format!("export {{ default }} from \"{package}\";\n")
            }
        }
    }
}

/// Where an agent's model family comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// The agent runs one family only.
    Fixed(&'static str),
    /// The agent runs models from any family, so the model it is set to run
    /// decides the family. See [`MODEL_FAMILIES`].
    ByModel,
}

/// Model-id prefixes and the family each names, for an agent whose family
/// comes from its model. The first prefix that matches wins. The names are
/// the ones `builder::family_of` gives a `Code-Generator:` trailer.
pub const MODEL_FAMILIES: &[(&str, &str)] = &[
    ("anthropic/", "anthropic"),
    ("claude", "anthropic"),
    ("openai/", "openai"),
    ("gpt", "openai"),
    ("o1", "openai"),
    ("o3", "openai"),
    ("o4", "openai"),
    ("deepseek", "deepseek"),
    ("google/", "google"),
    ("gemini", "google"),
    ("qwen", "qwen"),
    ("meta-llama/", "meta"),
    ("llama", "meta"),
    ("mistralai/", "mistral"),
    ("mistral", "mistral"),
    ("codestral", "mistral"),
];

/// The router prefix that may come before the vendor in a model id, such as
/// `openrouter/qwen/qwen3-coder-next`.
const ROUTER_PREFIX: &str = "openrouter/";

/// The family a model id names, or `None` when [`MODEL_FAMILIES`] holds no
/// prefix for it.
#[must_use]
pub fn family_of_model(model: &str) -> Option<&'static str> {
    let id = model.trim().to_lowercase();
    let id = id.strip_prefix(ROUTER_PREFIX).unwrap_or(&id);
    MODEL_FAMILIES
        .iter()
        .find(|(prefix, _)| id.starts_with(prefix))
        .map(|(_, family)| *family)
}

/// One agent osf can drive.
#[derive(Debug)]
pub struct Agent {
    /// The name a person uses for it, and the name `osf.toml` selects it by.
    pub name: &'static str,
    /// Where the model family it answers with comes from. A reviewer whose
    /// family built a change sits that change out.
    pub family: Family,
    /// The command it is started with. A reviewer's schema and model flags
    /// are appended to it.
    pub command: &'static [&'static str],
    /// Where its stop and prompt hooks are configured.
    pub hooks: Hooks,
    /// Directories, under a home or a repository, where it keeps its own
    /// state. Named without a leading slash.
    pub state_dirs: &'static [&'static str],
    /// Paths under each state directory that hold conversations,
    /// transcripts, or logs of them. A committed configuration file lives
    /// under the same directory and is not in this list.
    pub session_paths: &'static [&'static str],
    /// Names of the files and folders this agent loads its project settings,
    /// plugins and instructions from, wherever they sit in a project.
    pub project_settings: &'static [&'static str],
    /// Whether its sessions are reachable by link, and where.
    pub sessions: Sessions,
    /// What running it as a reviewer needs; `None` for an agent that never
    /// reviews.
    pub review: Option<Review>,
}

/// Every agent osf can drive, in the order this project lists them.
pub const AGENTS: &[Agent] = &[
    // dsh keeps every conversation under a `sessions` folder in its home
    // directory. It has no session page on any host. Its headless profile
    // takes the task as an argument, so a shell reads the prompt file.
    Agent {
        name: "dsh",
        family: Family::Fixed("deepseek"),
        command: &[
            "sh",
            "-c",
            "exec dsh --profile headless \"$(cat \"$1\")\"",
            "sh",
            "{prompt_file}",
        ],
        hooks: Hooks::DshPatch {
            file: ".dsh/cordis.patch.yml",
            package: "@open-software-factory/osf-dsh-plugin",
        },
        state_dirs: &[".dsh"],
        session_paths: &["sessions"],
        project_settings: &[".dsh"],
        sessions: Sessions::LocalOnly,
        review: Some(Review {
            // `dsh --help` documents no switch that ignores project settings.
            clean_copy: Switches {
                args: &[],
                env: &[],
            },
            // `dsh --help` and `dsh --profile headless --help` document no read-only or permission mode.
            read_only: None,
            in_container: None,
            schema_flag: None,
            schema_as: SchemaArg::Path,
            answer_pointer: "",
            model_flag: None,
            credential_env: &["DEEPSEEK_API_KEY"],
            login_paths: &[".dsh/.credentials.yaml"],
            sandbox_check: &[],
        }),
    },
    // omp is built on pi, and covers it. It keeps conversations under
    // `agent/sessions`, plus a folder of terminal sessions and one of logs.
    // No session page. It runs many model families, so its model decides
    // its family.
    Agent {
        name: "omp",
        family: Family::ByModel,
        command: &["omp"],
        hooks: Hooks::Extension {
            file: ".omp/agent/hooks/osf-stop/index.js",
            package: "@open-software-factory/osf-omp-hook",
        },
        state_dirs: &[".omp"],
        session_paths: &["agent/sessions", "agent/terminal-sessions", "logs"],
        project_settings: &[".omp"],
        sessions: Sessions::LocalOnly,
        review: Some(Review {
            // `omp --help`: `--no-extensions`, `--no-skills` and `--no-rules` turn off discovery of project extensions, skills and rules.
            clean_copy: Switches {
                args: &["--no-extensions", "--no-skills", "--no-rules"],
                env: &[],
            },
            // `omp --help`: `--tools` is the comma-separated list of tools to enable.
            read_only: Some(ReadOnly {
                args: &["--tools", "read,grep,glob"],
                env: &[],
            }),
            in_container: None,
            schema_flag: None,
            schema_as: SchemaArg::Path,
            answer_pointer: "",
            model_flag: None,
            credential_env: &[],
            login_paths: &[".omp/agent/agent.db"],
            sandbox_check: &[],
        }),
    },
    // opencode keeps configuration in one place and its data, including
    // session storage and tool output, in a data directory. A session can
    // be shared as a page on its host. It runs many model families, so its
    // family comes from its model.
    Agent {
        name: "opencode",
        family: Family::ByModel,
        command: &["opencode", "run", "--format", "json"],
        hooks: Hooks::PluginList {
            file: ".config/opencode/opencode.json",
            package: "@open-software-factory/osf-opencode-plugin",
        },
        state_dirs: &[".opencode", ".config/opencode", ".local/share/opencode"],
        session_paths: &["storage", "snapshot", "tool-output", "log"],
        project_settings: &[".opencode", "opencode.json", "opencode.jsonc"],
        sessions: Sessions::Hosted {
            host: "opencode.ai",
            path: "/s/",
        },
        review: Some(Review {
            // `opencode run --help`: `--pure` runs without external plugins. The opencode configuration docs list the three variables.
            clean_copy: Switches {
                args: &["--pure"],
                env: &[
                    ("OPENCODE_DISABLE_PROJECT_CONFIG", "true"),
                    ("OPENCODE_DISABLE_CLAUDE_CODE", "true"),
                    ("OPENCODE_DISABLE_EXTERNAL_SKILLS", "true"),
                ],
            },
            // The inline permissions config of the opencode CLI docs, `OPENCODE_PERMISSION`; `opencode debug config` shows the rules it sets. Bash is denied entirely; only the folder with the diff is readable outside the checkout.
            read_only: Some(ReadOnly {
                args: &[],
                env: &[(
                    "OPENCODE_PERMISSION",
                    r#"{"edit":"deny","task":"deny","webfetch":"deny","bash":"deny","external_directory":{"{review_dir}/**":"allow"}}"#,
                )],
            }),
            in_container: None,
            schema_flag: None,
            schema_as: SchemaArg::Path,
            answer_pointer: "",
            model_flag: Some("--model"),
            credential_env: &["OPENROUTER_API_KEY"],
            login_paths: &[".local/share/opencode/auth.json"],
            sandbox_check: &[],
        }),
    },
    // codex keeps live and archived sessions, a history file, and logs
    // in its home directory. Its hosted tasks have pages under the
    // vendor's site. With `--output-schema` alone, `codex exec` writes the
    // schema-matching message straight to standard output.
    Agent {
        name: "codex",
        family: Family::Fixed("openai"),
        command: &["codex", "exec"],
        hooks: Hooks::JsonFile {
            file: ".codex/hooks.json",
        },
        state_dirs: &[".codex"],
        session_paths: &["sessions", "archived_sessions", "history.jsonl", "log"],
        project_settings: &[".codex"],
        sessions: Sessions::Hosted {
            host: "chatgpt.com",
            path: "/codex/",
        },
        review: Some(Review {
            // `codex exec --help`: the first three ignore user configuration, rules files and saved sessions; the folder is not a git repository.
            clean_copy: Switches {
                args: &[
                    "--ignore-user-config",
                    "--ignore-rules",
                    "--ephemeral",
                    "--skip-git-repo-check",
                ],
                env: &[],
            },
            // `codex exec --help`: `--sandbox read-only`, which limits file writes and commands.
            read_only: Some(ReadOnly {
                args: &["--sandbox", "read-only"],
                env: &[],
            }),
            // `codex exec --help`: the container is the wall, so codex's own sandbox is off.
            in_container: Some(ReadOnly {
                args: &["--dangerously-bypass-approvals-and-sandbox"],
                env: &[],
            }),
            schema_flag: Some("--output-schema"),
            schema_as: SchemaArg::Path,
            answer_pointer: "",
            model_flag: Some("--model"),
            // `CODEX_API_KEY` is the variable `codex exec` reads; it does not read `OPENAI_API_KEY`.
            credential_env: &["CODEX_API_KEY"],
            login_paths: &[".codex/auth.json"],
            // `codex sandbox --help` runs a command under the same Linux sandbox, and exits non-zero when the sandbox cannot start.
            sandbox_check: &["codex", "sandbox", "--", "true"],
        }),
    },
    // claude keeps transcripts under `projects`, one folder per working
    // directory, plus session and transcript folders and a file history.
    // Its hosted sessions have pages under the vendor's site. Its
    // `--json-schema` takes effect with `--output-format json`, and the
    // answer then sits under the envelope's `structured_output` field.
    Agent {
        name: "claude",
        family: Family::Fixed("anthropic"),
        command: &["claude", "--print", "--output-format", "json"],
        hooks: Hooks::JsonFile {
            file: ".claude/settings.json",
        },
        state_dirs: &[".claude"],
        session_paths: &["projects", "sessions", "transcripts", "file-history"],
        project_settings: &[".claude"],
        sessions: Sessions::Hosted {
            host: "claude.ai",
            path: "/code/session_",
        },
        review: Some(Review {
            // `claude --help`: `--restricted` already ignores user, project and local settings; `--strict-mcp-config` skips every MCP server.
            clean_copy: Switches {
                args: &["--strict-mcp-config"],
                env: &[],
            },
            // `claude --help`: `--restricted` drops the command-running tools and the repository's own settings, and confines file tools to the working directories; `--tools` names the file tools only; `--add-dir` adds the folder with the diff; `--permission-prompts none` denies anything else.
            read_only: Some(ReadOnly {
                args: &[
                    "--restricted",
                    "--tools",
                    "Read,Grep,Glob",
                    "--add-dir",
                    "{review_dir}",
                    "--permission-prompts",
                    "none",
                ],
                env: &[],
            }),
            in_container: None,
            schema_flag: Some("--json-schema"),
            schema_as: SchemaArg::Inline,
            answer_pointer: "/structured_output",
            model_flag: Some("--model"),
            credential_env: &["CLAUDE_CODE_OAUTH_TOKEN"],
            login_paths: &[".claude/.credentials.json"],
            sandbox_check: &[],
        }),
    },
];

impl Agent {
    /// The model family this agent answers with when set to run `model`.
    ///
    /// # Errors
    /// Names the agent and the model when the agent runs many families and
    /// `model` is missing or names none that [`MODEL_FAMILIES`] holds.
    pub fn family_for(&self, model: Option<&str>) -> Result<&'static str, String> {
        match self.family {
            Family::Fixed(family) => Ok(family),
            Family::ByModel => match model {
                None => Err(format!(
                    "{} runs models from any family and has no model set, so its family is unknown",
                    self.name
                )),
                Some(m) => family_of_model(m).ok_or_else(|| {
                    format!(
                        "{} is set to model \"{m}\", which names no known family",
                        self.name
                    )
                }),
            },
        }
    }

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

/// Every file and folder name a coding agent reads project settings,
/// plugins or instructions from: each agent's own, then [`OTHER_AGENT_SETTINGS`].
/// A reviewer's clean copy of a change leaves all of them out.
#[must_use]
pub fn project_settings() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = AGENTS
        .iter()
        .flat_map(|a| a.project_settings.iter().copied())
        .chain(OTHER_AGENT_SETTINGS.iter().copied())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The agents a repository selected in `osf.toml`, checked against [`AGENTS`].
#[derive(Debug)]
pub struct Selection {
    /// The agents in use, in the order `osf.toml` names them.
    pub enabled: Vec<&'static Agent>,
    /// The agent that builds by default.
    pub builder: &'static Agent,
    /// The agents that review, in the order they run.
    pub reviewers: Vec<&'static Agent>,
    models: BTreeMap<&'static str, String>,
}

impl Selection {
    /// The model `osf.toml` names for `agent`; `None` leaves the choice to the agent.
    #[must_use]
    pub fn model(&self, agent: &Agent) -> Option<&str> {
        self.models.get(agent.name).map(String::as_str)
    }
}

fn find(name: &str, field: &str) -> Result<&'static Agent, String> {
    AGENTS.iter().find(|a| a.name == name).ok_or_else(|| {
        let known: Vec<&str> = AGENTS.iter().map(|a| a.name).collect();
        format!(
            "[agents] {field}: unknown agent \"{name}\"; the known agents are {}",
            known.join(", ")
        )
    })
}

fn find_all(names: &[String], field: &str) -> Result<Vec<&'static Agent>, String> {
    let mut found: Vec<&'static Agent> = Vec::with_capacity(names.len());
    for name in names {
        let agent = find(name, field)?;
        if found.iter().any(|a| a.name == agent.name) {
            return Err(format!("[agents] {field}: \"{name}\" is named twice"));
        }
        found.push(agent);
    }
    Ok(found)
}

fn require_enabled(enabled: &[&Agent], agent: &Agent, field: &str) -> Result<(), String> {
    if enabled.iter().any(|a| a.name == agent.name) {
        Ok(())
    } else {
        Err(format!(
            "[agents] {field}: \"{}\" is not in the enabled agents",
            agent.name
        ))
    }
}

/// Checks `cfg` against [`AGENTS`] and fills in every default from it.
///
/// # Errors
/// Names the field and the agent when `cfg` names an agent this list does not
/// hold, names one twice, picks a builder or reviewer that is not enabled,
/// picks a reviewer that cannot review, or sets a model for an agent that
/// takes none.
pub fn resolve(cfg: &crate::config::AgentsConfig) -> Result<Selection, String> {
    let enabled = match &cfg.enabled {
        Some(names) => find_all(names, "enabled")?,
        None => AGENTS.iter().collect(),
    };
    let builder = find(cfg.builder.as_deref().unwrap_or(DEFAULT_BUILDER), "builder")?;
    require_enabled(&enabled, builder, "builder")?;
    let reviewers = find_all(&cfg.reviewers, "reviewers")?;
    for agent in &reviewers {
        require_enabled(&enabled, agent, "reviewers")?;
        if agent.review.is_none() {
            return Err(format!(
                "[agents] reviewers: \"{}\" cannot run as a reviewer",
                agent.name
            ));
        }
    }
    let mut models = BTreeMap::new();
    for (name, model) in &cfg.models {
        let agent = find(name, "models")?;
        require_enabled(&enabled, agent, "models")?;
        if agent.review.as_ref().and_then(|r| r.model_flag).is_none() {
            return Err(format!(
                "[agents] models: \"{name}\" takes no model flag, so it cannot be given a model"
            ));
        }
        models.insert(agent.name, model.clone());
    }
    Ok(Selection {
        enabled,
        builder,
        reviewers,
        models,
    })
}

/// The selection `<root>/osf.toml` makes, or the defaults when it makes none.
///
/// # Errors
/// Returns an error when the `[agents]` table cannot be read or [`resolve`] refuses it.
pub fn selection(root: &Path) -> Result<Selection, String> {
    let cfg = crate::config::agents_config(root).map_err(|e| e.to_string())?;
    resolve(&cfg)
}

/// Writes each of `agents`' hook settings under `root`, replacing any file
/// already there, and returns the paths written.
///
/// # Errors
/// Returns an error when a folder or file cannot be written.
pub fn write_hooks(root: &Path, agents: &[&Agent]) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::with_capacity(agents.len());
    for agent in agents {
        let path = root.join(agent.hooks.file());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, agent.hooks.render())
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentsConfig;

    #[test]
    fn the_list_is_exactly_the_five_agents_osf_drives() {
        let names: Vec<&str> = AGENTS.iter().map(|a| a.name).collect();
        assert_eq!(names, vec!["dsh", "omp", "opencode", "codex", "claude"]);
    }

    #[test]
    fn every_agent_has_a_name_a_family_a_command_a_state_dir_and_a_session_path() {
        for a in AGENTS {
            assert!(!a.name.is_empty());
            if let Family::Fixed(family) = a.family {
                assert!(!family.is_empty(), "{}", a.name);
            }
            assert!(!a.command.is_empty(), "{}", a.name);
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
    fn agents_that_run_one_family_keep_it_whatever_the_model() {
        let family = |name: &str, model: Option<&str>| {
            AGENTS
                .iter()
                .find(|a| a.name == name)
                .expect("agent in the list")
                .family_for(model)
        };
        assert_eq!(family("codex", None), Ok("openai"));
        assert_eq!(family("claude", Some("anything")), Ok("anthropic"));
        assert_eq!(family("dsh", None), Ok("deepseek"));
    }

    #[test]
    fn agents_that_run_many_families_take_the_family_from_their_model() {
        let opencode = AGENTS
            .iter()
            .find(|a| a.name == "opencode")
            .expect("opencode");
        assert_eq!(
            opencode.family_for(Some("openrouter/qwen/qwen3-coder-next")),
            Ok("qwen")
        );
        assert_eq!(
            opencode.family_for(Some("openrouter/anthropic/claude-sonnet-5")),
            Ok("anthropic")
        );
        assert_eq!(
            opencode.family_for(Some("claude-sonnet-5")),
            Ok("anthropic")
        );
        assert_eq!(opencode.family_for(Some("gpt-5")), Ok("openai"));
        assert_eq!(opencode.family_for(Some("openai/gpt-5")), Ok("openai"));
        assert_eq!(opencode.family_for(Some("deepseek-v4")), Ok("deepseek"));
    }

    #[test]
    fn a_many_family_agent_with_an_unknown_or_missing_model_has_no_family() {
        let omp = AGENTS.iter().find(|a| a.name == "omp").expect("omp");
        let unknown = omp
            .family_for(Some("acme/frobnicator-1"))
            .expect_err("unknown");
        assert!(unknown.contains("acme/frobnicator-1"), "{unknown}");
        let missing = omp.family_for(None).expect_err("no model");
        assert!(missing.contains("no model set"), "{missing}");
    }

    /// A hook file that names nothing, or sits outside the agent's own
    /// folders, would write settings the agent never reads.
    #[test]
    fn every_agent_has_hook_wiring_that_runs_the_stop_check() {
        for a in AGENTS {
            let file = a.hooks.file();
            assert!(
                !file.is_empty() && !file.starts_with('/'),
                "{}: hook file {file:?} must be relative and not empty",
                a.name
            );
            assert!(
                a.state_dirs.iter().any(|d| Path::new(file).starts_with(d)),
                "{}: hook file {file} is outside {:?}",
                a.name,
                a.state_dirs
            );
            let text = a.hooks.render();
            assert!(
                text.contains("osf hook stop") || text.contains("osf-"),
                "{}: its hook settings never reach the stop check: {text}",
                a.name
            );
        }
    }

    #[test]
    fn a_json_hook_file_wires_both_the_stop_and_the_prompt_hook() {
        let text = Hooks::JsonFile { file: "x.json" }.render();
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        let command = |event: &str| {
            value
                .pointer(&format!("/hooks/{event}/0/hooks/0/command"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        assert_eq!(command("Stop").as_deref(), Some("osf hook stop"));
        assert_eq!(
            command("UserPromptSubmit").as_deref(),
            Some("osf hook prompt")
        );
    }

    #[test]
    fn a_reviewer_has_login_paths_inside_its_own_state_directories() {
        for a in AGENTS {
            let Some(review) = &a.review else { continue };
            assert!(
                !review.login_paths.is_empty(),
                "{} has no login path",
                a.name
            );
            for path in review.login_paths {
                assert!(
                    a.state_dirs.iter().any(|d| Path::new(path).starts_with(d)),
                    "{}: {path} is outside {:?}",
                    a.name,
                    a.state_dirs
                );
            }
        }
    }

    fn read_only_of(name: &str) -> Option<ReadOnly> {
        AGENTS
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.review.as_ref())
            .and_then(|r| r.read_only)
    }

    fn in_container_of(name: &str) -> Option<ReadOnly> {
        AGENTS
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.review.as_ref())
            .and_then(|r| r.in_container)
    }

    #[test]
    fn codex_runs_in_its_read_only_sandbox_outside_the_container() {
        let mode = read_only_of("codex").expect("codex documents a read-only mode");
        assert_eq!(mode.args, &["--sandbox", "read-only"]);
        assert!(mode.env.is_empty());
    }

    #[test]
    fn codex_runs_with_its_sandbox_off_inside_the_container() {
        let mode = in_container_of("codex").expect("codex documents an in-container mode");
        assert_eq!(mode.args, &["--dangerously-bypass-approvals-and-sandbox"]);
        assert!(mode.env.is_empty());
    }

    #[test]
    fn only_codex_has_an_in_container_mode() {
        let names: Vec<&str> = AGENTS
            .iter()
            .filter(|a| a.review.as_ref().is_some_and(|r| r.in_container.is_some()))
            .map(|a| a.name)
            .collect();
        assert_eq!(names, vec!["codex"]);
    }

    #[test]
    fn the_container_marker_is_the_installed_osf() {
        assert_eq!(FACTORY_CONTAINER_MARKER, "/opt/factory/bin/osf");
    }

    #[cfg(unix)]
    fn root_file(mode: u32) -> MarkerFacts {
        MarkerFacts {
            regular_file: true,
            owner_uid: 0,
            mode,
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_root_owned_regular_file_with_no_group_or_other_write_is_trusted() {
        assert!(marker_trusted(Some(root_file(0o555)), true));
    }

    #[test]
    #[cfg(unix)]
    fn the_docker_marker_must_exist_for_the_marker_to_be_trusted() {
        assert!(!marker_trusted(Some(root_file(0o555)), false));
    }

    #[test]
    #[cfg(unix)]
    fn a_marker_owned_by_a_user_other_than_root_is_not_trusted() {
        assert!(!marker_trusted(
            Some(MarkerFacts {
                regular_file: true,
                owner_uid: 1000,
                mode: 0o755,
            }),
            true
        ));
    }

    #[test]
    #[cfg(unix)]
    fn a_marker_a_group_or_another_user_can_write_is_not_trusted() {
        assert!(!marker_trusted(Some(root_file(0o775)), true));
        assert!(!marker_trusted(Some(root_file(0o757)), true));
    }

    #[test]
    #[cfg(unix)]
    fn a_marker_only_its_owner_can_write_is_trusted() {
        assert!(marker_trusted(Some(root_file(0o755)), true));
    }

    #[test]
    #[cfg(unix)]
    fn a_path_that_is_not_a_regular_file_is_not_trusted() {
        assert!(!marker_trusted(
            Some(MarkerFacts {
                regular_file: false,
                owner_uid: 0,
                mode: 0o555,
            }),
            true
        ));
    }

    #[test]
    #[cfg(unix)]
    fn a_marker_whose_facts_cannot_be_read_is_not_trusted() {
        assert!(!marker_trusted(None, true));
    }

    #[test]
    #[cfg(unix)]
    fn facts_of_reads_a_regular_file_owned_by_the_current_user() {
        use std::os::unix::fs::MetadataExt;
        let dir = crate::test_support::TempDir::new("osf-agents-facts");
        let file = dir.join("osf");
        std::fs::write(&file, "").expect("marker writes");
        let facts = facts_of(&file).expect("facts");
        assert!(facts.regular_file);
        let uid = std::fs::metadata(&*dir).expect("dir metadata").uid();
        assert_eq!(facts.owner_uid, uid);
    }

    #[test]
    #[cfg(unix)]
    fn facts_of_is_none_for_a_path_that_cannot_be_read() {
        let dir = crate::test_support::TempDir::new("osf-agents-facts-missing");
        assert!(facts_of(&dir.join("missing")).is_none());
    }

    #[test]
    #[cfg(unix)]
    fn facts_of_does_not_follow_a_symlink() {
        use std::os::unix::fs::symlink;
        let dir = crate::test_support::TempDir::new("osf-agents-facts-symlink");
        let file = dir.join("osf");
        std::fs::write(&file, "").expect("marker writes");
        let link = dir.join("link");
        symlink(&file, &link).expect("symlink creates");
        let facts = facts_of(&link).expect("facts");
        assert!(!facts.regular_file);
    }

    #[test]
    #[cfg(not(unix))]
    fn in_factory_container_is_false_off_unix() {
        assert!(!in_factory_container());
    }

    #[test]
    fn claude_is_limited_to_file_tools_and_has_no_shell() {
        let mode = read_only_of("claude").expect("claude documents a read-only mode");
        assert_eq!(
            mode.args,
            &[
                "--restricted",
                "--tools",
                "Read,Grep,Glob",
                "--add-dir",
                "{review_dir}",
                "--permission-prompts",
                "none",
            ]
        );
        assert!(!mode
            .args
            .iter()
            .any(|a| a.contains("Bash") || *a == "--allowedTools"));
    }

    #[test]
    fn omp_is_limited_to_read_search_tools() {
        let mode = read_only_of("omp").expect("omp documents a read-only mode");
        assert_eq!(mode.args, &["--tools", "read,grep,glob"]);
    }

    #[test]
    fn opencode_denies_edits_and_every_command() {
        let mode = read_only_of("opencode").expect("opencode documents a read-only mode");
        let [(name, value)] = mode.env else {
            panic!("one variable expected: {:?}", mode.env);
        };
        assert_eq!(*name, "OPENCODE_PERMISSION");
        let json: serde_json::Value = serde_json::from_str(value).expect("valid JSON");
        assert_eq!(json.pointer("/edit"), Some(&serde_json::json!("deny")));
        assert_eq!(json.pointer("/bash"), Some(&serde_json::json!("deny")));
        assert_eq!(
            json.pointer("/external_directory/{review_dir}~1**"),
            Some(&serde_json::json!("allow"))
        );
    }

    #[test]
    fn the_project_settings_list_covers_every_agent_and_the_common_instruction_files() {
        let names = project_settings();
        for expected in [
            ".opencode",
            "opencode.json",
            "opencode.jsonc",
            ".omp",
            ".codex",
            ".claude",
            ".mcp.json",
            ".cursor",
            ".dsh",
            "AGENTS.md",
            "CLAUDE.md",
        ] {
            assert!(
                names.contains(&expected),
                "{expected} is missing: {names:?}"
            );
        }
        for a in AGENTS {
            for dir in a.state_dirs.iter().filter(|d| !d.contains('/')) {
                assert!(
                    names.contains(dir),
                    "{}: its state folder {dir} is not left out of a clean copy",
                    a.name
                );
            }
            for name in a.project_settings {
                assert!(
                    !name.contains('/') && !name.is_empty(),
                    "{}: {name} is a plain name",
                    a.name
                );
            }
        }
    }

    #[test]
    fn codex_reads_the_variable_its_exec_command_reads() {
        let review = AGENTS
            .iter()
            .find(|a| a.name == "codex")
            .and_then(|a| a.review.as_ref())
            .expect("codex reviews");
        assert_eq!(review.credential_env, &["CODEX_API_KEY"]);
    }

    #[test]
    fn codex_has_a_sandbox_check_and_a_check_always_starts_the_agents_own_program() {
        let check_of = |name: &str| {
            AGENTS
                .iter()
                .find(|a| a.name == name)
                .and_then(|a| a.review.as_ref())
                .map(|r| r.sandbox_check)
                .expect("agent reviews")
        };
        assert_eq!(check_of("codex"), &["codex", "sandbox", "--", "true"]);
        for agent in AGENTS {
            let Some(review) = &agent.review else {
                continue;
            };
            if let Some(program) = review.sandbox_check.first() {
                assert_eq!(
                    Some(program),
                    agent.command.first(),
                    "{}: the check starts the agent's own program",
                    agent.name
                );
            }
        }
    }

    #[test]
    fn each_reviewer_that_documents_a_switch_ignores_project_settings_with_it() {
        let switches = |name: &str| {
            AGENTS
                .iter()
                .find(|a| a.name == name)
                .and_then(|a| a.review.as_ref())
                .map(|r| r.clean_copy)
                .expect("agent reviews")
        };
        assert!(switches("omp").args.contains(&"--no-extensions"));
        assert!(switches("opencode")
            .env
            .contains(&("OPENCODE_DISABLE_PROJECT_CONFIG", "true")));
        assert!(switches("codex").args.contains(&"--ignore-user-config"));
        assert!(switches("claude").args.contains(&"--strict-mcp-config"));
    }

    #[test]
    fn dsh_documents_no_read_only_mode() {
        assert!(AGENTS.iter().any(|a| a.name == "dsh" && a.review.is_some()));
        assert_eq!(read_only_of("dsh"), None);
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

    fn cfg(enabled: Option<&[&str]>, reviewers: &[&str], models: &[(&str, &str)]) -> AgentsConfig {
        let owned = |names: &[&str]| names.iter().map(|n| (*n).to_string()).collect();
        AgentsConfig {
            enabled: enabled.map(owned),
            builder: None,
            reviewers: owned(reviewers),
            models: models
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    #[test]
    fn with_nothing_selected_every_agent_is_enabled_dsh_builds_and_no_agent_reviews() {
        let s = resolve(&AgentsConfig::default()).expect("defaults resolve");
        assert_eq!(s.enabled.len(), AGENTS.len());
        assert_eq!(s.builder.name, DEFAULT_BUILDER);
        assert!(s.reviewers.is_empty());
    }

    #[test]
    fn an_agent_the_list_does_not_hold_is_a_clear_error_in_every_field() {
        let unknown = ["pi", "copilot"];
        for name in unknown {
            for config in [
                cfg(Some(&[name]), &[], &[]),
                cfg(None, &[name], &[]),
                cfg(None, &[], &[(name, "m")]),
                AgentsConfig {
                    builder: Some(name.to_string()),
                    ..AgentsConfig::default()
                },
            ] {
                let e = resolve(&config).expect_err("an unknown agent is refused");
                assert!(e.contains(&format!("unknown agent \"{name}\"")), "{e}");
                assert!(e.contains("dsh, omp, opencode, codex, claude"), "{e}");
            }
        }
    }

    #[test]
    fn a_reviewer_or_builder_must_be_enabled_and_a_model_needs_a_model_flag() {
        let e = resolve(&cfg(Some(&["dsh"]), &["codex"], &[])).expect_err("refused");
        assert!(e.contains("\"codex\" is not in the enabled agents"), "{e}");
        let e = resolve(&cfg(Some(&["codex"]), &[], &[]))
            .expect_err("the default builder is not enabled");
        assert!(e.contains("builder"), "{e}");
        let e = resolve(&cfg(None, &[], &[("dsh", "m")])).expect_err("refused");
        assert!(e.contains("takes no model flag"), "{e}");
        let e = resolve(&cfg(None, &["codex", "codex"], &[])).expect_err("refused");
        assert!(e.contains("named twice"), "{e}");
    }

    #[test]
    fn a_selected_model_is_reported_for_its_agent_only() {
        let s = resolve(&cfg(None, &["codex"], &[("codex", "o4-mini")])).expect("resolves");
        let codex = AGENTS.iter().find(|a| a.name == "codex").expect("codex");
        let claude = AGENTS.iter().find(|a| a.name == "claude").expect("claude");
        assert_eq!(s.model(codex), Some("o4-mini"));
        assert_eq!(s.model(claude), None);
    }

    #[test]
    fn writing_hooks_puts_each_agents_file_under_the_root() {
        let root = crate::test_support::TempDir::new("osf-agents-write-hooks");
        let all: Vec<&Agent> = AGENTS.iter().collect();
        let written = write_hooks(&root, &all).expect("hooks write");
        assert_eq!(written.len(), AGENTS.len());
        for (agent, path) in AGENTS.iter().zip(&written) {
            assert_eq!(path, &root.join(agent.hooks.file()));
            assert_eq!(
                std::fs::read_to_string(path).expect("file reads"),
                agent.hooks.render()
            );
        }
    }
}
