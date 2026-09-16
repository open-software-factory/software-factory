//! Path patterns that `lint writing`, and any check added later, skip, so
//! hygiene checks can be a required gate on this project's own repository,
//! not just on someone else's.

use globset::{Glob, GlobSet, GlobSetBuilder};

/// A compiled set of glob patterns, or none at all: `--no-exclude`, or a
/// configured list that is empty.
#[derive(Debug)]
pub struct Excluder(Option<GlobSet>);

impl Excluder {
    /// # Errors
    /// Returns an error if a pattern is not a valid glob.
    pub fn build(patterns: &[String]) -> Result<Self, String> {
        if patterns.is_empty() {
            return Ok(Excluder(None));
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern)
                .map_err(|e| format!("exclude pattern '{pattern}' is not a valid glob: {e}"))?;
            builder.add(glob);
        }
        let set = builder
            .build()
            .map_err(|e| format!("cannot build the exclude patterns: {e}"))?;
        Ok(Excluder(Some(set)))
    }

    /// No patterns at all.
    #[must_use]
    pub fn none() -> Self {
        Excluder(None)
    }

    /// Matches `path` (a repository-relative path, either separator) against
    /// every configured pattern.
    #[must_use]
    pub fn is_excluded(&self, path: &str) -> bool {
        let normalised = path.replace('\\', "/");
        self.0.as_ref().is_some_and(|set| set.is_match(&normalised))
    }

    /// Splits `paths` into the ones this excluder keeps and how many it dropped.
    #[must_use]
    pub fn partition(&self, paths: Vec<String>) -> (Vec<String>, usize) {
        let mut kept = Vec::with_capacity(paths.len());
        let mut excluded = 0usize;
        for path in paths {
            if self.is_excluded(&path) {
                excluded += 1;
            } else {
                kept.push(path);
            }
        }
        (kept, excluded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_patterns_excludes_nothing() {
        let excluder = Excluder::build(&[]).expect("empty patterns build");
        assert!(!excluder.is_excluded("target/debug/osf.exe"));
    }

    #[test]
    fn a_matching_pattern_excludes_the_path() {
        let excluder = Excluder::build(&["target/**".to_string()]).expect("one pattern builds");
        assert!(excluder.is_excluded("target/debug/osf.exe"));
        assert!(!excluder.is_excluded("crates/osf/src/main.rs"));
    }

    #[test]
    fn a_windows_style_path_is_matched_too() {
        let excluder =
            Excluder::build(&["**/tests/fixtures/**".to_string()]).expect("one pattern builds");
        assert!(excluder.is_excluded(r"crates\osf\tests\fixtures\skills\bad\SKILL.md"));
    }

    #[test]
    fn partition_counts_and_keeps_separately() {
        let matcher = Excluder::build(&["target/**".to_string()]).expect("one pattern builds");
        let (kept, dropped) = matcher.partition(vec![
            "target/debug/osf.exe".to_string(),
            "crates/osf/src/main.rs".to_string(),
        ]);
        assert_eq!(kept, vec!["crates/osf/src/main.rs".to_string()]);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn an_invalid_pattern_is_a_build_error() {
        let err = Excluder::build(&["[".to_string()]).expect_err("an invalid glob is refused");
        assert!(err.contains("exclude pattern"), "{err}");
    }
}
