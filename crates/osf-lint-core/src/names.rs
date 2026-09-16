//! Known-names loading: a caller's built-in list plus an optional extra file.

use std::collections::HashSet;
use std::path::Path;

/// Names that need no description on first use.
pub struct KnownNames(HashSet<String>);

impl KnownNames {
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}

/// # Errors
/// Returns an error if `path` is given and cannot be read.
pub fn load_known_names(built_in: &[&str], path: Option<&Path>) -> Result<KnownNames, String> {
    let mut set: HashSet<String> = built_in.iter().map(ToString::to_string).collect();
    if let Some(p) = path {
        let text = std::fs::read_to_string(p)
            .map_err(|e| format!("cannot read known names {}: {e}", p.display()))?;
        for line in text.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') {
                set.insert(t.to_string());
            }
        }
    }
    Ok(KnownNames(set))
}
