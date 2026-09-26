//! The `[writing]` configuration: limits and word lists for the writing
//! lint. Resolved in layers, a later one overriding an earlier one field
//! by field: compiled defaults, one file, named environment variables,
//! then the command-line flags the user actually passed.

use osf_lint_core::{ConfigError, Layer, Layered, LevelSetting};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The environment variable naming the config file, when `--config` is not given.
pub const ENV_VAR: &str = "OSF_CONFIG";
const HOME_FILE: &str = ".osf/config.toml";
/// The file name this crate looks for at a git repository's own top level,
/// between `OSF_CONFIG` and the home-directory file. Not configurable: a
/// repository's own config lives at a fixed, predictable name, the same
/// name this project's own `osf.toml` already uses.
const REPO_CONFIG_FILE: &str = "osf.toml";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub writing: WritingConfig,
    /// Path patterns that `lint writing`, and any check added later, all
    /// skip. Only build output by default, so the checks can pass on a
    /// project's own repository. Neither `tests/fixtures` nor a Rust test
    /// file is excluded: `is_fixture_path` and an `osf-expect` declaration
    /// cover a fixture's prose, and a test that needs a literal a scan
    /// rule would match builds that literal at run time instead of
    /// writing it into the file, so there is nothing left to hide there.
    pub exclude: Vec<String>,
    pub skill: SkillConfig,
    pub scan: ScanConfig,
}

/// Only the build output directory: untracked, so excluding it costs
/// nothing. Nothing else is excluded by default. A `tests/fixtures` path
/// is checked against its `osf-expect` declaration instead of skipped,
/// and a Rust test file holds no literal a scan rule would match, so
/// excluding it would only hide real content for no reason.
pub const DEFAULT_EXCLUDE: &[&str] = &["target/**"];

impl Default for Config {
    fn default() -> Self {
        Config {
            writing: WritingConfig::default(),
            exclude: strings(DEFAULT_EXCLUDE),
            skill: SkillConfig::default(),
            scan: ScanConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct WritingConfig {
    /// A sentence with more words than this is an error.
    pub max_sentence_words: usize,
    /// A sentence with more words than this is a warning; none when unset.
    pub warn_sentence_words: Option<usize>,
    /// More numbers than this in one sentence is a warning.
    pub max_numerals: usize,
    /// A message with fewer words than this must not carry a heading.
    pub short_text_words: usize,
    /// Words and phrases that carry no meaning; each is flagged.
    pub filler: Vec<String>,
    /// Phrases that only work inside one conversation.
    pub chat_local_phrases: Vec<String>,
    /// Names that need no description on first use, on top of the built-in ones.
    pub known_names: Vec<String>,
    /// Names this repository has decided are worth explaining on first use,
    /// read even under `--gate`; see [`gate_loaded`] for why that is safe.
    pub must_explain_names: Vec<String>,
    /// Per-rule level overrides, keyed by rule id.
    pub levels: BTreeMap<String, LevelSetting>,
}

pub const DEFAULT_FILLER: &[&str] = &[
    "delve",
    "it's worth noting",
    "it is worth noting",
    "in summary",
    "in conclusion",
    "i hope this helps",
    "let me know if",
    "feel free to",
    "great question",
    "leverage",
    "utilize",
    "utilise",
    "seamless",
    "seamlessly",
    "robust",
    "robustly",
    "navigate the",
    "the landscape of",
    "game-changer",
    "game changer",
    "cutting-edge",
    "unlock",
    "empower",
    "elevate",
    "at the end of the day",
    "going forward",
    "moving forward",
    "please note",
    "as you can see",
    "simply put",
    "needless to say",
    "foster",
    "facilitate",
    "streamline",
    "paradigm",
    "tapestry",
    "multifaceted",
    "meticulous",
    "paramount",
    "transformative",
    "embark",
    "supercharge",
    "ever-evolving",
    "comprehensive",
];

pub const DEFAULT_CHAT_LOCAL_PHRASES: &[&str] = &[
    "as discussed",
    "as mentioned",
    "as noted",
    "as i said",
    "see above",
    "mentioned above",
    "the previous message",
    "earlier today",
    "in my last message",
];

impl Default for WritingConfig {
    fn default() -> Self {
        WritingConfig {
            max_sentence_words: 25,
            warn_sentence_words: None,
            max_numerals: 2,
            short_text_words: 500,
            filler: strings(DEFAULT_FILLER),
            chat_local_phrases: strings(DEFAULT_CHAT_LOCAL_PHRASES),
            known_names: Vec::new(),
            must_explain_names: Vec::new(),
            levels: BTreeMap::new(),
        }
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

/// The `[skill]` configuration: the skill lint's size budget, its trigger
/// phrase list, its manual-shape threshold, and per-rule level overrides.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SkillConfig {
    /// A first section that is not step-shaped may hold up to this many paragraphs.
    pub overview_max_paragraphs: usize,
    /// A first section that is not step-shaped may hold up to this many words.
    pub overview_max_words: usize,
    /// One of these phrases, case-insensitive, must appear in the description.
    pub trigger_phrases: Vec<String>,
    /// A run of numbered steps longer than this needs a stated stopping point.
    pub manual_min_steps: usize,
    /// Per-rule level overrides, keyed by rule id.
    pub levels: BTreeMap<String, LevelSetting>,
}

pub const DEFAULT_TRIGGER_PHRASES: &[&str] = &[
    "use when",
    "use whenever",
    "when the user",
    "when you",
    "if the user",
    "fires on",
    "trigger",
];

impl Default for SkillConfig {
    fn default() -> Self {
        SkillConfig {
            overview_max_paragraphs: 2,
            overview_max_words: 120,
            trigger_phrases: strings(DEFAULT_TRIGGER_PHRASES),
            manual_min_steps: 3,
            levels: BTreeMap::new(),
        }
    }
}

/// The `[scan]` configuration: what `osf scan` must never let reach a
/// public repository, on top of the shapes it always checks.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ScanConfig {
    /// The org or user whose own `owner/repo#N` references are not foreign. Empty means it is read from the git remote.
    pub project_owner: String,
    /// `public` or `private`. Empty means it is read from the host where a client is installed, and is unknown otherwise.
    pub repository_visibility: String,
    /// Patterns that must never appear in a public repository. Never printed in a finding.
    pub denylist: Vec<String>,
    /// Extra hosted-session link prefixes, as `host/path-prefix`, added to the built-in agent list. Adds only; nothing here removes a built-in.
    pub session_links: Vec<String>,
    /// Per-rule level overrides, keyed by rule id.
    pub levels: BTreeMap<String, LevelSetting>,
}

/// One field the environment can set, and how to parse it into a TOML value.
struct EnvField {
    var: &'static str,
    path: &'static [&'static str],
    parse: fn(&str) -> Result<toml::Value, String>,
}

fn parse_uint(raw: &str) -> Result<toml::Value, String> {
    raw.trim()
        .parse::<i64>()
        .map(toml::Value::Integer)
        .map_err(|e| format!("'{raw}' is not a whole number: {e}"))
}

/// Matches `parse_uint`'s signature so both fit one `EnvField::parse` slot.
#[allow(clippy::unnecessary_wraps)]
fn parse_string(raw: &str) -> Result<toml::Value, String> {
    Ok(toml::Value::String(raw.trim().to_string()))
}

/// Matches `parse_uint`'s signature so both fit one `EnvField::parse` slot.
#[allow(clippy::unnecessary_wraps)]
fn parse_list(raw: &str) -> Result<toml::Value, String> {
    Ok(toml::Value::Array(
        raw.split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(|w| toml::Value::String(w.to_string()))
            .collect(),
    ))
}

/// Every field the environment can set, mapped explicitly: an underscore
/// splitter cannot tell a nested section from a field name with an
/// underscore in it, so each variable names its own dotted path.
const ENV_FIELDS: &[EnvField] = &[
    EnvField {
        var: "OSF_WRITING_MAX_SENTENCE_WORDS",
        path: &["writing", "max_sentence_words"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_WRITING_WARN_SENTENCE_WORDS",
        path: &["writing", "warn_sentence_words"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_WRITING_MAX_NUMERALS",
        path: &["writing", "max_numerals"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_WRITING_SHORT_TEXT_WORDS",
        path: &["writing", "short_text_words"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_WRITING_FILLER",
        path: &["writing", "filler"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_WRITING_CHAT_LOCAL_PHRASES",
        path: &["writing", "chat_local_phrases"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_WRITING_KNOWN_NAMES",
        path: &["writing", "known_names"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_WRITING_MUST_EXPLAIN_NAMES",
        path: &["writing", "must_explain_names"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_EXCLUDE",
        path: &["exclude"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_SKILL_OVERVIEW_MAX_PARAGRAPHS",
        path: &["skill", "overview_max_paragraphs"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_SKILL_OVERVIEW_MAX_WORDS",
        path: &["skill", "overview_max_words"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_SKILL_TRIGGER_PHRASES",
        path: &["skill", "trigger_phrases"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_SKILL_MANUAL_MIN_STEPS",
        path: &["skill", "manual_min_steps"],
        parse: parse_uint,
    },
    EnvField {
        var: "OSF_SCAN_PROJECT_OWNER",
        path: &["scan", "project_owner"],
        parse: parse_string,
    },
    EnvField {
        var: "OSF_DENYLIST",
        path: &["scan", "denylist"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_SCAN_SESSION_LINKS",
        path: &["scan", "session_links"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_SCAN_REPOSITORY_VISIBILITY",
        path: &["scan", "repository_visibility"],
        parse: parse_string,
    },
];

fn apply_env(layered: &mut Layered) -> Result<(), ConfigError> {
    for field in ENV_FIELDS {
        let Ok(raw) = std::env::var(field.var) else {
            continue;
        };
        let value = (field.parse)(&raw)
            .map_err(|e| ConfigError::new(format!("{} is invalid: {e}", field.var)))?;
        layered.set(field.path, value, Layer::Env);
    }
    Ok(())
}

/// The file to read: `--config`, else `OSF_CONFIG`, else `osf.toml` at the
/// current git repository's top level, else `~/.osf/config.toml`.
///
/// The repository-root layer is a guess, not something a caller asked
/// for, so it only counts when the file is actually there: an empty
/// repository must still fall through to the home file, the way a caller
/// who names a file at `--config` or `OSF_CONFIG` does not, since asking
/// for a specific file that turns out to be missing is a different
/// situation, handled by [`read_config_file`](osf_lint_core::read_config_file)'s
/// `explicit` flag instead.
#[must_use]
pub fn resolve_path(flag: Option<&Path>) -> Option<PathBuf> {
    flag.map(Path::to_path_buf)
        .or_else(|| std::env::var_os(ENV_VAR).map(PathBuf::from))
        .or_else(|| repo_config_path().filter(|p| p.is_file()))
        .or_else(|| osf_lint_core::resolve_path(None, ENV_VAR, HOME_FILE))
}

/// `osf.toml` at the top level of the git repository the current
/// directory sits in, or `None` when there is no such repository, `git`
/// is not on the path, or the command otherwise fails. None of those is
/// an error: running outside a repository, or without `git` installed,
/// must fall through to the next layer rather than fail.
fn repo_config_path() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let top_level = String::from_utf8(output.stdout).ok()?;
    let top_level = top_level.trim();
    if top_level.is_empty() {
        return None;
    }
    Some(PathBuf::from(top_level).join(REPO_CONFIG_FILE))
}

/// Writes `exclude` into the merged tree, so `osf config show` reports
/// what is really in force after `load` has the last word on the field.
fn set_exclude_tree(tree: &mut toml::Value, exclude: &[String]) {
    if let Some(table) = tree.as_table_mut() {
        table.insert(
            "exclude".to_string(),
            toml::Value::Array(exclude.iter().cloned().map(toml::Value::String).collect()),
        );
    }
}

/// The config resolved from every layer, and where it came from.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub tree: toml::Value,
    pub sources: BTreeMap<String, Layer>,
    pub file: Option<PathBuf>,
}

/// Loads the layered config: compiled defaults, then the file at `--config`
/// or `OSF_CONFIG` or `~/.osf/config.toml`, then the environment, then
/// `flags`, a caller-built list of the fields the user actually passed on
/// the command line. `extra_exclude` is added to whatever `exclude` list
/// the other three layers resolved to, never replacing it: a caller's
/// `--exclude` flag must not be able to wipe the defaults by accident.
///
/// With `gate` set, `file_flag`, `flags`, and `extra_exclude` are all
/// ignored, and the file at `OSF_CONFIG` or `~/.osf/config.toml` is not
/// read either, with one exception: see [`gate_loaded`]. A gate run checks
/// a change that has not yet been approved, so no setting that loosens this check
/// may come from that change's own config file, its environment, or a flag
/// built from either: a rule's level, a word list, a numeric limit, the
/// known-name list, and the exclude list all fall back to the compiled
/// default. `--exclude`, `--no-exclude`, and `--known-names` stay available
/// with `gate` unset, for a person running the tool by hand.
///
/// # Errors
/// Returns an error if an explicit config file cannot be read, if the file
/// or an environment variable is not valid TOML for its field, or if any
/// layer together with the others carries a key this struct does not
/// declare.
pub fn load(
    file_flag: Option<&Path>,
    flags: &[(&[&str], toml::Value)],
    extra_exclude: &[String],
    gate: bool,
) -> Result<Loaded, ConfigError> {
    if gate {
        return gate_loaded();
    }
    let defaults = osf_lint_core::to_value(&Config::default())?;
    let mut layered = Layered::new(defaults);

    let explicit = file_flag.is_some() || std::env::var_os(ENV_VAR).is_some();
    let path = resolve_path(file_flag);
    let mut file = None;
    if let Some(p) = &path {
        if let Some(text) = osf_lint_core::read_config_file(p, explicit)? {
            let value: toml::Value = toml::from_str(&text).map_err(|e| {
                ConfigError::new(format!("config {} is not valid: {e}", p.display()))
            })?;
            layered.merge_document(&value, Layer::File);
            file = Some(p.clone());
        }
    }

    apply_env(&mut layered)?;

    for (path, value) in flags {
        layered.set(path, value.clone(), Layer::Flag);
    }

    let (mut config, mut tree, mut sources): (Config, toml::Value, BTreeMap<String, Layer>) =
        layered.finish().map_err(removed_chat_local_labels_key)?;
    reject_retired_levels(&config.writing.levels)?;
    if !extra_exclude.is_empty() {
        config.exclude.extend(extra_exclude.iter().cloned());
        set_exclude_tree(&mut tree, &config.exclude);
        sources.insert("exclude".to_string(), Layer::Flag);
    }
    Ok(Loaded {
        config,
        tree,
        sources,
        file,
    })
}

/// `writing.chat_local_labels` was removed along with `chat-local-reference`.
/// `deny_unknown_fields` already refuses it; this rewrites that message so
/// it says the key is gone and names its replacements, instead of just
/// listing every field that remains.
fn removed_chat_local_labels_key(err: ConfigError) -> ConfigError {
    let message = err.to_string();
    if message.contains("chat_local_labels") {
        ConfigError::new(format!(
            "the config key 'writing.chat_local_labels' is gone; use \
             'writing.chat_local_phrases' and 'writing.must_explain_names' instead ({message})"
        ))
    } else {
        err
    }
}

/// A `writing.levels` entry keyed by a rule id retired along with the six
/// old reference and name rules is an error naming `unplaceable-reference`,
/// the id that replaced all of them, rather than a silent no-op: nothing
/// reads that key any more, so a level set on it would otherwise vanish
/// without a trace.
///
/// # Errors
/// Returns an error naming the retired key and its replacement.
fn reject_retired_levels(levels: &BTreeMap<String, LevelSetting>) -> Result<(), ConfigError> {
    for (old, new) in crate::lints::RETIRED_RULE_IDS {
        if levels.contains_key(*old) {
            return Err(ConfigError::new(format!(
                "the rule '{old}' is gone; use 'writing.levels.{new}' instead"
            )));
        }
    }
    Ok(())
}

/// The config a gate run always gets: the compiled defaults, with nothing
/// from any file, environment variable, or flag merged in, except
/// `writing.must_explain_names`, read from the file at `OSF_CONFIG`, else
/// `osf.toml` at the current git repository's top level, else
/// `~/.osf/config.toml` (never from `--config`, and never from the
/// environment or a flag).
///
/// This is built from [`Config::default`] rather than by starting from a
/// resolved config and clearing the fields a change could have reached, so
/// a field added to [`Config`] or [`WritingConfig`] later is safe here with
/// no extra code: it was never merged in, so it never needs resetting.
///
/// `must_explain_names` is the one field exempt from that. Every other
/// field can only loosen the gate: turning a rule off, widening the
/// exclude list, or growing the known-name list all shrink what the gate
/// reports. `must_explain_names` cannot: its compiled default is empty, so
/// it starts at the least strict setting already, and every name the
/// repository's own file adds to it can only turn on one more
/// `unplaceable-reference` error, never turn one off. Reading it here from a file
/// this change could itself have edited is therefore safe: a change that
/// deletes an entry only pulls that name back down to the same empty floor
/// every other repository already gates on, and a change that adds one can
/// only make its own gate run stricter than that floor, never looser. Do
/// not read any other field this way; every other field in this struct can
/// remove a finding, which this reasoning does not cover.
///
/// # Errors
/// Returns an error if the compiled defaults themselves fail to serialise
/// into a TOML tree, which does not happen in practice, or if the file this
/// exemption reads is not valid TOML.
fn gate_loaded() -> Result<Loaded, ConfigError> {
    let defaults = osf_lint_core::to_value(&Config::default())?;
    let (mut config, tree, sources): (Config, toml::Value, BTreeMap<String, Layer>) =
        Layered::new(defaults).finish()?;
    config.writing.must_explain_names = gate_must_explain_names()?;
    Ok(Loaded {
        config,
        tree,
        sources,
        file: None,
    })
}

/// Reads `writing.must_explain_names` from `resolve_path`'s file (never
/// from `--config`, since `flag` is always `None` here), or returns an
/// empty list when no such file exists. See [`gate_loaded`] for why this
/// one field is read under `--gate` when nothing else in the file is.
fn gate_must_explain_names() -> Result<Vec<String>, ConfigError> {
    let Some(path) = resolve_path(None) else {
        return Ok(Vec::new());
    };
    let Some(text) = osf_lint_core::read_config_file(&path, false)? else {
        return Ok(Vec::new());
    };
    let value: toml::Value = toml::from_str(&text)
        .map_err(|e| ConfigError::new(format!("config {} is not valid: {e}", path.display())))?;
    let names = value
        .get("writing")
        .and_then(|w| w.get("must_explain_names"))
        .and_then(toml::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Runs `f` while holding the process-wide environment lock, setting
    /// `vars` first when any are given. Every test in this module goes
    /// through here, because `OSF_WRITING_*` is process-global state and
    /// two tests must never touch it at the same time.
    fn serial<T>(vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (k, v) in vars {
            // SAFETY: serialised by `ENV_LOCK`; no other thread touches the environment here.
            unsafe { std::env::set_var(k, v) };
        }
        let result = f();
        for (k, _) in vars {
            // SAFETY: serialised by `ENV_LOCK`; no other thread touches the environment here.
            unsafe { std::env::remove_var(k) };
        }
        drop(guard);
        result
    }

    /// A fresh, empty directory to stand in for the home directory, so a
    /// test for "no config file" is not fooled by a real file this
    /// machine happens to have at `~/.osf/config.toml`.
    fn empty_home() -> String {
        let dir = std::env::temp_dir().join(format!("osf-config-test-home-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("empty home dir creates");
        dir.to_string_lossy().into_owned()
    }

    /// Like `serial`, but also changes the process's current directory to
    /// `dir` for the closure's duration, restoring it afterwards. This is
    /// still race-free: `resolve_path` is the only thing in this crate
    /// that reads the current directory, every test that calls it already
    /// goes through `serial` or this function, and both share one mutex,
    /// so no two of them ever run at the same time.
    fn serial_in_dir<T>(dir: &Path, vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let original = std::env::current_dir().expect("the current directory reads");
        std::env::set_current_dir(dir).expect("chdir into the test directory");
        for (k, v) in vars {
            // SAFETY: serialised by `ENV_LOCK`; no other thread touches the environment here.
            unsafe { std::env::set_var(k, v) };
        }
        let result = f();
        for (k, _) in vars {
            // SAFETY: serialised by `ENV_LOCK`; no other thread touches the environment here.
            unsafe { std::env::remove_var(k) };
        }
        std::env::set_current_dir(original).expect("chdir back to the original directory");
        drop(guard);
        result
    }

    /// A fresh, empty directory that is not inside any git repository, so
    /// a test for "no repository" is not fooled by this worktree's own
    /// `osf.toml`, or by the machine happening to run the test suite
    /// somewhere under a repository of its own.
    fn outside_any_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        dir
    }

    /// Runs `git init --quiet` in `dir`, so `git rev-parse --show-toplevel`
    /// resolves to `dir` from anywhere under it.
    fn init_repo(dir: &Path) {
        let status = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(dir)
            .status()
            .expect("git init runs");
        assert!(status.success(), "git init failed for {}", dir.display());
    }

    #[test]
    fn defaults_round_trip() {
        let dir = outside_any_repo("osf-config-test-defaults");
        let home = empty_home();
        let loaded = serial_in_dir(&dir, &[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &[], false)
        })
        .expect("defaults load with no file");
        assert_eq!(loaded.config.writing.max_sentence_words, 25);
        assert_eq!(loaded.config.writing.filler.len(), DEFAULT_FILLER.len());
        assert!(loaded.file.is_none());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let dir = std::env::temp_dir().join("osf-config-test-partial");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[writing]\nmax_sentence_words = 30\n[writing.levels]\nsemicolon = \"off\"\n",
        )
        .expect("config file writes");
        let loaded =
            serial(&[], || load(Some(&path), &[], &[], false)).expect("partial file loads");
        assert_eq!(loaded.config.writing.max_sentence_words, 30);
        assert_eq!(loaded.config.writing.max_numerals, 2);
        assert_eq!(
            loaded.config.writing.levels.get("semicolon"),
            Some(&LevelSetting::Off)
        );
        assert_eq!(
            loaded.sources.get("writing.max_sentence_words"),
            Some(&Layer::File)
        );
        assert_eq!(
            loaded.sources.get("writing.max_numerals"),
            Some(&Layer::Default)
        );
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_valid_keys_named() {
        let dir = std::env::temp_dir().join("osf-config-test-unknown");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmax_sentance_words = 30\n").expect("file writes");
        let err = serial(&[], || load(Some(&path), &[], &[], false))
            .expect_err("an unknown key is refused");
        let message = err.to_string();
        assert!(message.contains("max_sentance_words"), "{message}");
        assert!(message.contains("max_sentence_words"), "{message}");
    }

    /// `writing.chat_local_labels` was removed along with `chat-local-reference`;
    /// a config file that still sets it must say the key is gone, not just
    /// list every field that remains.
    #[test]
    fn a_config_file_setting_the_removed_chat_local_labels_key_says_it_is_gone() {
        let dir = std::env::temp_dir().join("osf-config-test-removed-key");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nchat_local_labels = [\"phase\"]\n").expect("file writes");
        let err = serial(&[], || load(Some(&path), &[], &[], false))
            .expect_err("the removed key is refused");
        let message = err.to_string();
        assert!(message.contains("chat_local_labels"), "{message}");
        assert!(message.contains("is gone"), "{message}");
        assert!(message.contains("chat_local_phrases"), "{message}");
        assert!(message.contains("must_explain_names"), "{message}");
    }

    /// A `writing.levels` entry naming one of the six retired reference and
    /// name rules is an error naming `unplaceable-reference`, the rule that
    /// replaced them, rather than a silent no-op.
    #[test]
    fn a_level_override_naming_a_retired_rule_names_its_replacement() {
        let dir = std::env::temp_dir().join("osf-config-test-retired-level");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[writing.levels]\nundefined-name-at-start = \"off\"\n",
        )
        .expect("file writes");
        let err = serial(&[], || load(Some(&path), &[], &[], false))
            .expect_err("a retired rule id in writing.levels is refused");
        let message = err.to_string();
        assert!(message.contains("undefined-name-at-start"), "{message}");
        assert!(message.contains("unplaceable-reference"), "{message}");
    }

    #[test]
    fn an_environment_variable_overrides_the_file() {
        let dir = std::env::temp_dir().join("osf-config-test-env");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmax_sentence_words = 30\n").expect("file writes");
        let loaded = serial(&[("OSF_WRITING_MAX_SENTENCE_WORDS", "40")], || {
            load(Some(&path), &[], &[], false)
        })
        .expect("env override loads");
        assert_eq!(loaded.config.writing.max_sentence_words, 40);
        assert_eq!(
            loaded.sources.get("writing.max_sentence_words"),
            Some(&Layer::Env)
        );
    }

    #[test]
    fn a_flag_overrides_the_environment_variable() {
        let home = empty_home();
        let flags: Vec<(&[&str], toml::Value)> =
            vec![(&["writing", "max_sentence_words"], toml::Value::Integer(50))];
        let loaded = serial(
            &[
                ("HOME", &home),
                ("USERPROFILE", &home),
                ("OSF_WRITING_MAX_SENTENCE_WORDS", "40"),
            ],
            || load(None, &flags, &[], false),
        )
        .expect("flag override loads");
        assert_eq!(loaded.config.writing.max_sentence_words, 50);
        assert_eq!(
            loaded.sources.get("writing.max_sentence_words"),
            Some(&Layer::Flag)
        );
    }

    #[test]
    fn a_flag_not_passed_leaves_the_file_value_alone() {
        let dir = std::env::temp_dir().join("osf-config-test-no-flag");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmax_sentence_words = 30\n").expect("file writes");
        let loaded =
            serial(&[], || load(Some(&path), &[], &[], false)).expect("no-flag load succeeds");
        assert_eq!(loaded.config.writing.max_sentence_words, 30);
        assert_eq!(
            loaded.sources.get("writing.max_sentence_words"),
            Some(&Layer::File)
        );
    }

    #[test]
    fn a_file_setting_every_writing_key_resolves() {
        let text = concat!(
            "[writing]\n",
            "max_sentence_words = 30\n",
            "warn_sentence_words = 20\n",
            "max_numerals = 2\n",
            "short_text_words = 500\n",
            "known_names = [\n",
            "  \"Clippy\", \"Vale\", \"Tauri\",\n",
            "  \"Postgres\", \"Redis\", \"GraphQL\",\n",
            "]\n",
            "\n",
            "[writing.levels]\n",
        );
        let dir = std::env::temp_dir().join("osf-config-test-every-key");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, text).expect("the file writes");
        let loaded =
            serial(&[], || load(Some(&path), &[], &[], false)).expect("every writing key loads");
        let w = &loaded.config.writing;
        assert_eq!(w.max_sentence_words, 30);
        assert_eq!(w.warn_sentence_words, Some(20));
        assert_eq!(w.max_numerals, 2);
        assert_eq!(w.short_text_words, 500);
        assert!(w.known_names.contains(&"Vale".to_string()));
        assert!(w.known_names.contains(&"Postgres".to_string()));
        assert!(w.levels.is_empty());
    }

    #[test]
    fn the_compiled_default_exclude_list_is_in_force_with_no_file() {
        let home = empty_home();
        let loaded = serial(&[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &[], false)
        })
        .expect("defaults load with no file");
        assert_eq!(loaded.config.exclude, DEFAULT_EXCLUDE);
    }

    #[test]
    fn a_file_exclude_list_replaces_the_compiled_default() {
        let dir = std::env::temp_dir().join("osf-config-test-exclude-file");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "exclude = [\"vendor/**\"]\n").expect("file writes");
        let loaded = serial(&[], || load(Some(&path), &[], &[], false)).expect("file loads");
        assert_eq!(loaded.config.exclude, vec!["vendor/**".to_string()]);
    }

    #[test]
    fn extra_exclude_adds_to_the_compiled_default_instead_of_replacing_it() {
        let home = empty_home();
        let extra = vec!["local-only/**".to_string()];
        let loaded = serial(&[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &extra, false)
        })
        .expect("defaults load with an extra exclude");
        for pattern in DEFAULT_EXCLUDE {
            assert!(
                loaded.config.exclude.contains(&(*pattern).to_string()),
                "{:?}",
                loaded.config.exclude
            );
        }
        assert!(loaded.config.exclude.contains(&"local-only/**".to_string()));
        assert_eq!(loaded.sources.get("exclude"), Some(&Layer::Flag));
    }

    #[test]
    fn gate_ignores_a_file_exclude_list() {
        let dir = std::env::temp_dir().join("osf-config-test-gate-file");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "exclude = [\"vendor/**\"]\n").expect("file writes");
        let loaded = serial(&[], || load(Some(&path), &[], &[], true)).expect("gate load succeeds");
        assert_eq!(loaded.config.exclude, DEFAULT_EXCLUDE);
        assert_eq!(loaded.sources.get("exclude"), Some(&Layer::Default));
    }

    #[test]
    fn gate_ignores_the_environment_variable() {
        let home = empty_home();
        let loaded = serial(
            &[
                ("HOME", &home),
                ("USERPROFILE", &home),
                ("OSF_EXCLUDE", "vendor/**"),
            ],
            || load(None, &[], &[], true),
        )
        .expect("gate load succeeds");
        assert_eq!(loaded.config.exclude, DEFAULT_EXCLUDE);
    }

    #[test]
    fn gate_ignores_an_extra_exclude_a_caller_still_passes() {
        let home = empty_home();
        let extra = vec!["local-only/**".to_string()];
        let loaded = serial(&[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &extra, true)
        })
        .expect("gate load succeeds");
        assert_eq!(loaded.config.exclude, DEFAULT_EXCLUDE);
    }

    /// Unlike every other field, `must_explain_names` is read from the
    /// file at `OSF_CONFIG` even under `--gate`: see `gate_loaded` for why
    /// growing this one list can only add errors, never remove one.
    #[test]
    fn gate_reads_must_explain_names_from_the_configured_file() {
        let dir = std::env::temp_dir().join("osf-config-test-gate-must-explain");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmust_explain_names = [\"Widgetly\"]\n")
            .expect("file writes");
        let path_str = path.to_string_lossy().into_owned();
        let loaded = serial(&[("OSF_CONFIG", &path_str)], || load(None, &[], &[], true))
            .expect("gate load succeeds");
        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["Widgetly".to_string()]
        );
    }

    /// `--config` itself stays ignored under gate, same as every other
    /// field: only `OSF_CONFIG`, the repository's own `osf.toml`, or the
    /// home file reaches this exemption. Run from outside any repository,
    /// so this worktree's own `osf.toml` cannot answer in the flag's place.
    #[test]
    fn gate_ignores_the_config_flag_for_must_explain_names() {
        let outside = outside_any_repo("osf-config-test-gate-must-explain-flag-outside");
        let dir = std::env::temp_dir().join("osf-config-test-gate-must-explain-flag");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmust_explain_names = [\"Widgetly\"]\n")
            .expect("file writes");
        let home = empty_home();
        let loaded = serial_in_dir(&outside, &[("HOME", &home), ("USERPROFILE", &home)], || {
            load(Some(&path), &[], &[], true)
        })
        .expect("gate load succeeds");
        assert!(loaded.config.writing.must_explain_names.is_empty());
    }

    /// No `OSF_CONFIG`, no repository, and no home file: the exemption
    /// reads nothing rather than erroring.
    #[test]
    fn gate_must_explain_names_is_empty_with_no_file() {
        let outside = outside_any_repo("osf-config-test-gate-must-explain-empty-outside");
        let home = empty_home();
        let loaded = serial_in_dir(&outside, &[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &[], true)
        })
        .expect("gate load succeeds");
        assert!(loaded.config.writing.must_explain_names.is_empty());
    }

    /// `resolve_path` finds a git repository's own `osf.toml` from any
    /// subdirectory, with no `OSF_CONFIG` set, the gap the earlier fix
    /// left: a repository's config only ever loaded when something
    /// pointed `OSF_CONFIG` at it by hand.
    #[test]
    fn the_repository_root_file_is_found_from_a_subdirectory() {
        let dir = outside_any_repo("osf-config-test-repo-root-subdir");
        init_repo(&dir);
        std::fs::write(
            dir.join("osf.toml"),
            "[writing]\nmust_explain_names = [\"Repotool\"]\n",
        )
        .expect("repo config writes");
        let sub = dir.join("nested").join("deeper");
        std::fs::create_dir_all(&sub).expect("nested dir creates");
        let home = empty_home();

        let loaded = serial_in_dir(&sub, &[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &[], false)
        })
        .expect("load succeeds from a subdirectory");
        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["Repotool".to_string()]
        );
    }

    /// Running outside any git repository must not be an error: the
    /// repository-root layer is simply skipped, falling through to the
    /// home file, here also absent.
    #[test]
    fn outside_a_repository_the_layer_is_skipped_without_error() {
        let dir = outside_any_repo("osf-config-test-repo-root-outside");
        let home = empty_home();

        let loaded = serial_in_dir(&dir, &[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[], &[], false)
        })
        .expect("load still succeeds outside a repository");
        assert!(loaded.config.writing.must_explain_names.is_empty());
        assert!(loaded.file.is_none());
    }

    /// `OSF_CONFIG` still wins over a repository's own `osf.toml`.
    #[test]
    fn the_environment_variable_still_wins_over_the_repository_root_file() {
        let dir = outside_any_repo("osf-config-test-repo-root-env-wins");
        init_repo(&dir);
        std::fs::write(
            dir.join("osf.toml"),
            "[writing]\nmust_explain_names = [\"FromRepo\"]\n",
        )
        .expect("repo config writes");
        let explicit = dir.join("explicit.toml");
        std::fs::write(&explicit, "[writing]\nmust_explain_names = [\"FromEnv\"]\n")
            .expect("explicit config writes");
        let explicit_str = explicit.to_string_lossy().into_owned();

        let loaded = serial_in_dir(&dir, &[("OSF_CONFIG", &explicit_str)], || {
            load(None, &[], &[], false)
        })
        .expect("load succeeds");
        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["FromEnv".to_string()]
        );
    }

    /// `--config` still wins over a repository's own `osf.toml`, outside a
    /// gate run.
    #[test]
    fn the_flag_still_wins_over_the_repository_root_file() {
        let dir = outside_any_repo("osf-config-test-repo-root-flag-wins");
        init_repo(&dir);
        std::fs::write(
            dir.join("osf.toml"),
            "[writing]\nmust_explain_names = [\"FromRepo\"]\n",
        )
        .expect("repo config writes");
        let explicit = dir.join("explicit.toml");
        std::fs::write(
            &explicit,
            "[writing]\nmust_explain_names = [\"FromFlag\"]\n",
        )
        .expect("explicit config writes");

        let loaded = serial_in_dir(&dir, &[], || load(Some(&explicit), &[], &[], false))
            .expect("load succeeds");
        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["FromFlag".to_string()]
        );
    }

    /// The gate still gets the repository's own `osf.toml`, same as it
    /// gets one named by `OSF_CONFIG`, and every other field still falls
    /// back to the compiled default.
    #[test]
    fn the_gate_still_gets_the_repository_root_file() {
        let dir = outside_any_repo("osf-config-test-repo-root-gate");
        init_repo(&dir);
        std::fs::write(
            dir.join("osf.toml"),
            "[writing]\nmust_explain_names = [\"Repotool\"]\nfiller = []\n",
        )
        .expect("repo config writes");

        let loaded =
            serial_in_dir(&dir, &[], || load(None, &[], &[], true)).expect("gate load succeeds");
        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["Repotool".to_string()]
        );
        assert_eq!(loaded.config.writing.filler.len(), DEFAULT_FILLER.len());
    }

    /// The exact case the adversarial review proved: a config file that
    /// turns `unplaceable-reference` off must not reach a gate run.
    #[test]
    fn gate_ignores_a_file_level_override() {
        let dir = std::env::temp_dir().join("osf-config-test-gate-levels");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing.levels]\nunplaceable-reference = \"off\"\n")
            .expect("file writes");
        let loaded = serial(&[], || load(Some(&path), &[], &[], true)).expect("gate load succeeds");
        assert!(
            loaded.config.writing.levels.is_empty(),
            "{:?}",
            loaded.config.writing.levels
        );
    }

    /// The known-name list and the word lists must fall back to the
    /// compiled defaults too, the same as `exclude` already did.
    #[test]
    fn gate_ignores_known_names_and_word_lists_from_a_file() {
        let dir = std::env::temp_dir().join("osf-config-test-gate-lists");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nknown_names = [\"Vale\"]\nfiller = []\n")
            .expect("file writes");
        let loaded = serial(&[], || load(Some(&path), &[], &[], true)).expect("gate load succeeds");
        assert!(loaded.config.writing.known_names.is_empty());
        assert_eq!(loaded.config.writing.filler.len(), DEFAULT_FILLER.len());
    }

    /// Recursively changes every leaf value in a TOML tree. A config file
    /// built from this poisons every field the compiled defaults hold a
    /// value for, including one added to `Config` or `WritingConfig` after
    /// this test was written, with nothing here naming that field by hand.
    fn poison(value: &toml::Value) -> toml::Value {
        match value {
            toml::Value::String(s) => toml::Value::String(format!("{s}-poisoned")),
            toml::Value::Integer(n) => toml::Value::Integer(n + 999),
            toml::Value::Float(f) => toml::Value::Float(f + 999.0),
            toml::Value::Boolean(b) => toml::Value::Boolean(!b),
            toml::Value::Datetime(d) => toml::Value::Datetime(*d),
            toml::Value::Array(items) => {
                let mut out: Vec<toml::Value> = items.iter().map(poison).collect();
                out.push(toml::Value::String("poison-extra".to_string()));
                toml::Value::Array(out)
            }
            toml::Value::Table(table) => {
                let mut out = toml::value::Table::new();
                for (k, v) in table {
                    out.insert(k.clone(), poison(v));
                }
                toml::Value::Table(out)
            }
        }
    }

    /// The generic form of the two tests above: a gate run must return
    /// exactly the compiled defaults no matter what a file, the
    /// environment, or a flag sets, field by field, with no list of fields
    /// in this test to fall out of date. A future field the gate forgets
    /// to reset would fail this test the day it is given a compiled
    /// default other than its own zero value, without anyone updating
    /// this test to know about it.
    ///
    /// `must_explain_names` is carved out of the final comparison on
    /// purpose: it is the one field [`gate_loaded`] deliberately still
    /// reads from the file, so this test also proves that carve-out reads
    /// exactly the file's value and nothing the environment or a flag set.
    #[test]
    fn gate_config_matches_compiled_defaults_even_from_a_maximally_poisoned_source() {
        let defaults_tree =
            osf_lint_core::to_value(&Config::default()).expect("defaults serialise");
        let mut poisoned = poison(&defaults_tree);
        // The compiled defaults hold these empty, so poisoning the tree
        // above touches nothing for them; set them by hand here so this
        // one file also proves a per-rule level and the known-name list
        // cannot reach a gate run either.
        if let Some(writing) = poisoned
            .as_table_mut()
            .and_then(|root| root.get_mut("writing"))
            .and_then(toml::Value::as_table_mut)
        {
            writing.insert(
                "known_names".to_string(),
                toml::Value::Array(vec![toml::Value::String("Poisoned".to_string())]),
            );
            writing.insert(
                "must_explain_names".to_string(),
                toml::Value::Array(vec![toml::Value::String("FileNamed".to_string())]),
            );
            let mut levels = toml::value::Table::new();
            levels.insert(
                "unplaceable-reference".to_string(),
                toml::Value::String("off".to_string()),
            );
            writing.insert("levels".to_string(), toml::Value::Table(levels));
        }
        let text = toml::to_string(&poisoned).expect("poisoned tree renders as TOML");

        let dir = std::env::temp_dir().join("osf-config-test-gate-poison");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, &text).expect("poisoned file writes");
        let path_str = path.to_string_lossy().into_owned();

        let flags: Vec<(&[&str], toml::Value)> = vec![
            (&["writing", "max_sentence_words"], toml::Value::Integer(1)),
            (&["writing", "max_numerals"], toml::Value::Integer(1)),
            (&["writing", "short_text_words"], toml::Value::Integer(1)),
        ];
        let extra_exclude = vec!["also-poisoned/**".to_string()];

        let loaded = serial(
            &[
                ("OSF_CONFIG", &path_str),
                ("OSF_WRITING_MAX_SENTENCE_WORDS", "1"),
                ("OSF_WRITING_WARN_SENTENCE_WORDS", "1"),
                ("OSF_WRITING_MAX_NUMERALS", "1"),
                ("OSF_WRITING_SHORT_TEXT_WORDS", "1"),
                ("OSF_WRITING_FILLER", "poisoned"),
                ("OSF_WRITING_CHAT_LOCAL_PHRASES", "poisoned"),
                ("OSF_WRITING_KNOWN_NAMES", "Poisoned"),
                // Proves the exemption reads the file, not the environment:
                // if this leaked through, the assertion below would see it.
                ("OSF_WRITING_MUST_EXPLAIN_NAMES", "EnvNamed"),
                ("OSF_EXCLUDE", "also-poisoned/**"),
            ],
            || load(Some(&path), &flags, &extra_exclude, true),
        )
        .expect("a gate load succeeds even over a poisoned file");

        assert_eq!(
            loaded.config.writing.must_explain_names,
            vec!["FileNamed".to_string()],
            "must_explain_names should read only the file's value"
        );
        let mut without_the_exemption = loaded.config;
        without_the_exemption.writing.must_explain_names = Vec::new();
        assert_eq!(without_the_exemption, Config::default());
    }

    #[test]
    fn the_skill_section_layers_the_same_way_as_writing() {
        let dir = std::env::temp_dir().join("osf-config-test-skill-section");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[skill]\noverview_max_paragraphs = 1\n").expect("file writes");
        let loaded = serial(&[("OSF_SKILL_OVERVIEW_MAX_WORDS", "40")], || {
            load(Some(&path), &[], &[], false)
        })
        .expect("skill section loads");
        assert_eq!(loaded.config.skill.overview_max_paragraphs, 1);
        assert_eq!(loaded.config.skill.overview_max_words, 40);
        assert_eq!(loaded.config.skill.manual_min_steps, 3);
        assert_eq!(
            loaded.sources.get("skill.overview_max_paragraphs"),
            Some(&Layer::File)
        );
        assert_eq!(
            loaded.sources.get("skill.overview_max_words"),
            Some(&Layer::Env)
        );
    }

    #[test]
    fn an_unknown_skill_key_is_refused() {
        let dir = std::env::temp_dir().join("osf-config-test-skill-unknown");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[skill]\noverview_max_paragraph = 1\n").expect("file writes");
        let err = serial(&[], || load(Some(&path), &[], &[], false))
            .expect_err("an unknown key is refused");
        assert!(err.to_string().contains("overview_max_paragraph"));
    }
}
