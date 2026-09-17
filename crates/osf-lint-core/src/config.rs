//! Generic layered TOML configuration. A caller merges compiled defaults,
//! one file, and named overrides into one tree, then deserializes it once
//! into its own struct with `deny_unknown_fields`. This module never knows
//! a rule's fields; a future linter for design documents brings its own
//! config struct and reuses only this machinery.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Which layer set a field's final value. A later layer in this list wins
/// over an earlier one, field by field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Default,
    File,
    Env,
    Flag,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Layer::Default => "default",
            Layer::File => "file",
            Layer::Env => "environment",
            Layer::Flag => "flag",
        })
    }
}

/// A configuration key `toml` refused, or a value that will not fit the
/// caller's struct. The message names the bad key and lists the valid
/// ones, straight from the strict deserializer underneath.
#[derive(Debug)]
pub struct ConfigError(String);

impl ConfigError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        ConfigError(message.into())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// A rule's level as set in a config file: `error`, `warning`, `info`, or
/// `off`. A later change may let one rule's entry be a level or a table of
/// levels keyed by context, read as an untagged enum wrapped around this
/// type; a file with only plain levels, like every file today, still
/// parses then.
#[derive(Clone, Copy, Debug, serde::Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LevelSetting {
    Error,
    Warning,
    Info,
    Off,
}

/// Applies each rule's level override to a batch of findings, by rule id.
/// `off` drops the finding; `error`, `warning`, or `info` sets its level.
/// A rule with no entry keeps the level its own check chose.
#[must_use]
pub fn apply_level_overrides(
    findings: Vec<crate::Finding>,
    levels: &BTreeMap<String, LevelSetting>,
) -> Vec<crate::Finding> {
    findings
        .into_iter()
        .filter_map(|f| match levels.get(f.rule) {
            None => Some(f),
            Some(LevelSetting::Off) => None,
            Some(LevelSetting::Error) => Some(crate::Finding {
                level: crate::Level::Error,
                ..f
            }),
            Some(LevelSetting::Warning) => Some(crate::Finding {
                level: crate::Level::Warning,
                ..f
            }),
            Some(LevelSetting::Info) => Some(crate::Finding {
                level: crate::Level::Info,
                ..f
            }),
        })
        .collect()
}

/// Serializes any config struct into a `toml::Value` tree, for use as the
/// starting layer.
///
/// # Errors
/// Returns an error if `value` cannot be represented in TOML.
pub fn to_value<T: Serialize>(value: &T) -> Result<toml::Value, ConfigError> {
    let text = toml::to_string(value).map_err(|e| ConfigError::new(e.to_string()))?;
    toml::from_str(&text).map_err(|e| ConfigError::new(e.to_string()))
}

/// The file to read: `flag`, else the value of `env_var`, else
/// `home_relative` under the user's home directory, else no file at all.
#[must_use]
pub fn resolve_path(flag: Option<&Path>, env_var: &str, home_relative: &str) -> Option<PathBuf> {
    flag.map(Path::to_path_buf)
        .or_else(|| std::env::var_os(env_var).map(PathBuf::from))
        .or_else(|| home_dir().map(|h| h.join(home_relative)))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Reads the config file at `path`. With `explicit` set, a missing file is
/// an error; otherwise a missing file is treated as no file at all.
///
/// # Errors
/// Returns an error if the file cannot be read, unless it is not `explicit`
/// and simply does not exist.
pub fn read_config_file(path: &Path, explicit: bool) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if !explicit && e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::new(format!(
            "cannot read config {}: {e}",
            path.display()
        ))),
    }
}

/// Merges configuration layers over one TOML tree, later layers winning
/// field by field, and remembers which layer last set each leaf.
pub struct Layered {
    tree: toml::Value,
    sources: BTreeMap<String, Layer>,
}

impl Layered {
    /// Starts from the compiled defaults, already a TOML tree.
    #[must_use]
    pub fn new(defaults: toml::Value) -> Self {
        let mut sources = BTreeMap::new();
        mark_leaves(&defaults, &mut Vec::new(), Layer::Default, &mut sources);
        Layered {
            tree: defaults,
            sources,
        }
    }

    /// Merges a whole document, such as a parsed config file, over the
    /// tree so far.
    pub fn merge_document(&mut self, overlay: &toml::Value, layer: Layer) {
        let mut path = Vec::new();
        merge(&mut self.tree, overlay, &mut path, layer, &mut self.sources);
    }

    /// Sets one field by its dotted path, such as from an environment
    /// variable or a command-line flag the user actually passed.
    pub fn set(&mut self, path: &[&str], value: toml::Value, layer: Layer) {
        set_path(&mut self.tree, path, value);
        self.sources.insert(path.join("."), layer);
    }

    /// Deserializes the merged tree into `T`, rejecting any key `T` does
    /// not declare, and returns the merged tree and which layer set each
    /// leaf, for a caller that wants to show its values in force.
    ///
    /// # Errors
    /// Returns an error naming the bad key and the valid keys when the
    /// tree carries a key `T` does not declare, or a value of the wrong
    /// shape for its field.
    pub fn finish<T: DeserializeOwned>(
        self,
    ) -> Result<(T, toml::Value, BTreeMap<String, Layer>), ConfigError> {
        let text = toml::to_string(&self.tree).map_err(|e| ConfigError::new(e.to_string()))?;
        let cfg = toml::from_str(&text).map_err(|e| ConfigError::new(e.to_string()))?;
        Ok((cfg, self.tree, self.sources))
    }
}

fn mark_leaves(
    value: &toml::Value,
    path: &mut Vec<String>,
    layer: Layer,
    sources: &mut BTreeMap<String, Layer>,
) {
    match value.as_table() {
        Some(table) => {
            for (k, v) in table {
                path.push(k.clone());
                mark_leaves(v, path, layer, sources);
                path.pop();
            }
        }
        None => {
            sources.insert(path.join("."), layer);
        }
    }
}

fn merge(
    base: &mut toml::Value,
    overlay: &toml::Value,
    path: &mut Vec<String>,
    layer: Layer,
    sources: &mut BTreeMap<String, Layer>,
) {
    let (Some(base_table), Some(overlay_table)) = (base.as_table_mut(), overlay.as_table()) else {
        *base = overlay.clone();
        sources.insert(path.join("."), layer);
        return;
    };
    for (k, v) in overlay_table {
        path.push(k.clone());
        if let Some(existing) = base_table.get_mut(k) {
            merge(existing, v, path, layer, sources);
        } else {
            mark_leaves(v, path, layer, sources);
            base_table.insert(k.clone(), v.clone());
        }
        path.pop();
    }
}

fn set_path(tree: &mut toml::Value, path: &[&str], value: toml::Value) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if tree.as_table().is_none() {
        *tree = toml::Value::Table(toml::value::Table::new());
    }
    let table = tree
        .as_table_mut()
        .expect("just replaced with a table when it was not one");
    if rest.is_empty() {
        table.insert((*head).to_string(), value);
    } else {
        let child = table
            .entry((*head).to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
        set_path(child, rest, value);
    }
}

/// Looks up a value in a merged tree by its dotted path, for display.
#[must_use]
pub fn lookup<'a>(tree: &'a toml::Value, dotted_path: &str) -> Option<&'a toml::Value> {
    dotted_path
        .split('.')
        .try_fold(tree, |v, segment| v.as_table()?.get(segment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, serde::Deserialize, Serialize, PartialEq, Eq)]
    #[serde(default, deny_unknown_fields)]
    struct Inner {
        word: String,
        count: i64,
    }

    #[derive(Debug, Default, serde::Deserialize, Serialize, PartialEq, Eq)]
    #[serde(default, deny_unknown_fields)]
    struct Sample {
        inner: Inner,
        list: Vec<String>,
    }

    fn defaults() -> toml::Value {
        to_value(&Sample {
            inner: Inner {
                word: "default".to_string(),
                count: 1,
            },
            list: vec!["a".to_string()],
        })
        .expect("sample defaults serialise")
    }

    #[test]
    fn a_later_layer_overrides_one_field_and_keeps_the_rest() {
        let mut layered = Layered::new(defaults());
        let overlay: toml::Value = toml::from_str("[inner]\ncount = 9\n").expect("overlay parses");
        layered.merge_document(&overlay, Layer::File);
        let (cfg, _, sources): (Sample, _, _) = layered.finish().expect("merged config parses");
        assert_eq!(cfg.inner.count, 9);
        assert_eq!(cfg.inner.word, "default");
        assert_eq!(sources.get("inner.count"), Some(&Layer::File));
        assert_eq!(sources.get("inner.word"), Some(&Layer::Default));
    }

    #[test]
    fn set_by_path_creates_missing_tables() {
        let mut layered = Layered::new(defaults());
        layered.set(
            &["inner", "word"],
            toml::Value::String("env".to_string()),
            Layer::Env,
        );
        let (cfg, tree, sources): (Sample, _, _) = layered.finish().expect("config parses");
        assert_eq!(cfg.inner.word, "env");
        assert_eq!(sources.get("inner.word"), Some(&Layer::Env));
        assert_eq!(
            lookup(&tree, "inner.word").and_then(toml::Value::as_str),
            Some("env")
        );
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_valid_keys_named() {
        let mut layered = Layered::new(defaults());
        let overlay: toml::Value = toml::from_str("[inner]\nsound = 1\n").expect("overlay parses");
        layered.merge_document(&overlay, Layer::File);
        let err = layered
            .finish::<Sample>()
            .expect_err("an unknown key is refused");
        let message = err.to_string();
        assert!(message.contains("sound"), "{message}");
        assert!(message.contains("word"), "{message}");
        assert!(message.contains("count"), "{message}");
    }

    #[test]
    fn level_off_drops_the_finding_and_the_others_change_level() {
        use crate::{Finding, Level};
        let findings = vec![
            Finding::new("a", Level::Error, 1, "m".to_string(), "x".to_string()),
            Finding::new("b", Level::Warning, 1, "m".to_string(), "x".to_string()),
            Finding::new("c", Level::Error, 1, "m".to_string(), "x".to_string()),
        ];
        let mut levels = BTreeMap::new();
        levels.insert("a".to_string(), LevelSetting::Off);
        levels.insert("b".to_string(), LevelSetting::Error);
        let out = apply_level_overrides(findings, &levels);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|f| f.rule != "a"));
        let b = out.iter().find(|f| f.rule == "b").expect("b kept");
        assert_eq!(b.level, Level::Error);
        let c = out.iter().find(|f| f.rule == "c").expect("c kept");
        assert_eq!(c.level, Level::Error);
    }
}
