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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub writing: WritingConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    /// Words that, followed by a number, only work inside one conversation.
    pub chat_local_labels: Vec<String>,
    /// Names that need no description on first use, on top of the built-in ones.
    pub known_names: Vec<String>,
    /// Plain words that may open a sentence with a capital, on top of the built-in ones.
    pub sentence_starters: Vec<String>,
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

pub const DEFAULT_CHAT_LOCAL_LABELS: &[&str] = &["phase", "item", "option", "part", "point"];

impl Default for WritingConfig {
    fn default() -> Self {
        WritingConfig {
            max_sentence_words: 25,
            warn_sentence_words: None,
            max_numerals: 2,
            short_text_words: 500,
            filler: strings(DEFAULT_FILLER),
            chat_local_phrases: strings(DEFAULT_CHAT_LOCAL_PHRASES),
            chat_local_labels: strings(DEFAULT_CHAT_LOCAL_LABELS),
            known_names: Vec::new(),
            sentence_starters: Vec::new(),
            levels: BTreeMap::new(),
        }
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
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
        var: "OSF_WRITING_CHAT_LOCAL_LABELS",
        path: &["writing", "chat_local_labels"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_WRITING_KNOWN_NAMES",
        path: &["writing", "known_names"],
        parse: parse_list,
    },
    EnvField {
        var: "OSF_WRITING_SENTENCE_STARTERS",
        path: &["writing", "sentence_starters"],
        parse: parse_list,
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

/// The file to read: `--config`, else `OSF_CONFIG`, else `~/.osf/config.toml`.
#[must_use]
pub fn resolve_path(flag: Option<&Path>) -> Option<PathBuf> {
    osf_lint_core::resolve_path(flag, ENV_VAR, HOME_FILE)
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
/// the command line.
///
/// # Errors
/// Returns an error if an explicit config file cannot be read, if the file
/// or an environment variable is not valid TOML for its field, or if any
/// layer together with the others carries a key this struct does not
/// declare.
pub fn load(
    file_flag: Option<&Path>,
    flags: &[(&[&str], toml::Value)],
) -> Result<Loaded, ConfigError> {
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

    let (config, tree, sources) = layered.finish()?;
    Ok(Loaded {
        config,
        tree,
        sources,
        file,
    })
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

    #[test]
    fn defaults_round_trip() {
        let home = empty_home();
        let loaded = serial(&[("HOME", &home), ("USERPROFILE", &home)], || {
            load(None, &[])
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
        let loaded = serial(&[], || load(Some(&path), &[])).expect("partial file loads");
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
        let err = serial(&[], || load(Some(&path), &[])).expect_err("an unknown key is refused");
        let message = err.to_string();
        assert!(message.contains("max_sentance_words"), "{message}");
        assert!(message.contains("max_sentence_words"), "{message}");
    }

    #[test]
    fn an_environment_variable_overrides_the_file() {
        let dir = std::env::temp_dir().join("osf-config-test-env");
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[writing]\nmax_sentence_words = 30\n").expect("file writes");
        let loaded = serial(&[("OSF_WRITING_MAX_SENTENCE_WORDS", "40")], || {
            load(Some(&path), &[])
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
            || load(None, &flags),
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
        let loaded = serial(&[], || load(Some(&path), &[])).expect("no-flag load succeeds");
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
        let loaded = serial(&[], || load(Some(&path), &[])).expect("every writing key loads");
        let w = &loaded.config.writing;
        assert_eq!(w.max_sentence_words, 30);
        assert_eq!(w.warn_sentence_words, Some(20));
        assert_eq!(w.max_numerals, 2);
        assert_eq!(w.short_text_words, 500);
        assert!(w.known_names.contains(&"Vale".to_string()));
        assert!(w.known_names.contains(&"Postgres".to_string()));
        assert!(w.levels.is_empty());
    }
}
