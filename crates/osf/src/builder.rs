//! Which model families built the change under review, read from each
//! commit's own `Code-Generator:` trailer, so `osf review run` can leave
//! reviewers from those families out of the roster for that run.
//!
//! A builder's own family judging its own change is not an independent
//! second opinion, so decision 0016 asks for reviewers from families other
//! than the builder's. This module only names the families; leaving them
//! out of the roster is `review_run`'s own job.

use crate::config::BuilderFamilyAlias;
use crate::git;
use std::path::Path;

/// A model whose name matches nothing in the family table.
pub const UNKNOWN: &str = "unknown";

/// Model-name prefixes this binary ships, matched case-insensitively
/// against the first word of a model's name. A repository's own
/// `osf.toml` can add more through `[review] builder_family_aliases`,
/// tried first.
const SHIPPED_FAMILIES: &[(&str, &str)] = &[
    ("claude", "anthropic"),
    ("gpt", "openai"),
    ("o1", "openai"),
    ("o3", "openai"),
    ("o4", "openai"),
    ("deepseek", "deepseek"),
    ("gemini", "google"),
    ("qwen", "qwen"),
    ("llama", "meta"),
    ("mistral", "mistral"),
    ("codestral", "mistral"),
];

/// The family a `Code-Generator:` trailer's model name maps to: `aliases`
/// first, then the shipped table, matched case-insensitively against
/// `model_name`'s first word by prefix. [`UNKNOWN`] when nothing matches,
/// including an empty name.
#[must_use]
pub fn family_of(model_name: &str, aliases: &[BuilderFamilyAlias]) -> String {
    let Some(first_word) = model_name.split_whitespace().next() else {
        return UNKNOWN.to_string();
    };
    let first_word = first_word.to_lowercase();
    for alias in aliases {
        if first_word.starts_with(&alias.prefix.to_lowercase()) {
            return alias.family.clone();
        }
    }
    for (prefix, family) in SHIPPED_FAMILIES {
        if first_word.starts_with(prefix) {
            return (*family).to_string();
        }
    }
    UNKNOWN.to_string()
}

/// The model name in one `Code-Generator:` trailer line: the text before
/// any `<...>` address, trimmed. `None` when the line is not this trailer,
/// or names nothing before the address.
fn trailer_model_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("Code-Generator:")?;
    let name = rest.split('<').next().unwrap_or(rest).trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Every `Code-Generator:` trailer's model name across every commit in
/// `range`, however many commits repeat one.
///
/// # Errors
/// Returns an error when git cannot read `range`'s commits.
fn trailer_model_names(root: &Path, range: &str) -> Result<Vec<String>, String> {
    let hashes = git::commit_hashes(root, range).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for hash in hashes {
        let message = git::commit_message(root, &hash).map_err(|e| e.to_string())?;
        for line in message.lines() {
            if let Some(name) = trailer_model_name(line) {
                names.push(name);
            }
        }
    }
    Ok(names)
}

/// The distinct builder families for `range`'s own commits (`base..HEAD`
/// when `range` is built that way): `overrides` verbatim, sorted and
/// deduplicated, when the caller named at least one with
/// `--builder-family`; otherwise every commit's `Code-Generator:` trailer,
/// mapped through `aliases` and the shipped table.
///
/// A trailer whose model maps to nothing contributes [`UNKNOWN`]. No
/// trailer at all in the whole range is reported the same way, as
/// `[UNKNOWN]`, rather than as an empty list: a review with no known
/// builder family still needs to record that plainly, not say nothing.
///
/// # Errors
/// Returns an error when git cannot read `range`'s commits.
pub fn detect(
    root: &Path,
    range: &str,
    aliases: &[BuilderFamilyAlias],
    overrides: &[String],
) -> Result<Vec<String>, String> {
    if !overrides.is_empty() {
        let mut families = overrides.to_vec();
        families.sort();
        families.dedup();
        return Ok(families);
    }
    let names = trailer_model_names(root, range)?;
    if names.is_empty() {
        return Ok(vec![UNKNOWN.to_string()]);
    }
    let mut families: Vec<String> = names.iter().map(|n| family_of(n, aliases)).collect();
    families.sort();
    families.dedup();
    Ok(families)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use std::process::Command;

    /// Runs `git init --quiet` in `dir`, then sets a fixed local identity
    /// so `commit` needs nothing from the ambient environment.
    fn init_repo(dir: &Path) {
        let mut command = Command::new("git");
        command.arg("init").arg("--quiet").arg(dir);
        git::scrub_git_env(&mut command);
        let status = command.status().expect("git init runs");
        assert!(status.success(), "git init failed for {}", dir.display());

        for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
            let mut config = Command::new("git");
            config.args(["config", key, value]).current_dir(dir);
            git::scrub_git_env(&mut config);
            let status = config.status().expect("git config runs");
            assert!(status.success(), "git config {key} failed");
        }
    }

    /// An empty commit on `dir`'s current branch, with `message` as its
    /// whole message (trailers included, as plain trailing lines).
    fn commit(dir: &Path, message: &str) {
        let mut command = Command::new("git");
        command
            .args(["commit", "--allow-empty", "--quiet", "-m", message])
            .current_dir(dir);
        git::scrub_git_env(&mut command);
        let status = command.status().expect("git commit runs");
        assert!(status.success(), "git commit failed for {message:?}");
    }

    fn commit_hash(dir: &Path) -> String {
        git::commit_hashes(dir, "HEAD")
            .expect("HEAD resolves")
            .pop()
            .expect("at least one commit")
    }

    #[test]
    fn family_of_maps_claude_to_anthropic() {
        assert_eq!(family_of("Claude Sonnet 5", &[]), "anthropic");
    }

    #[test]
    fn family_of_is_case_insensitive_and_matches_by_prefix_word() {
        assert_eq!(family_of("gpt-4o", &[]), "openai");
        assert_eq!(family_of("O3-mini", &[]), "openai");
        assert_eq!(family_of("DEEPSEEK-V3", &[]), "deepseek");
        assert_eq!(family_of("Gemini 2.5 Pro", &[]), "google");
        assert_eq!(family_of("Qwen2.5-Coder", &[]), "qwen");
        assert_eq!(family_of("Llama 3.1", &[]), "meta");
        assert_eq!(family_of("Mistral Large", &[]), "mistral");
        assert_eq!(family_of("Codestral", &[]), "mistral");
    }

    #[test]
    fn family_of_reports_unknown_for_a_name_the_table_does_not_have() {
        assert_eq!(family_of("Some New Model", &[]), UNKNOWN);
    }

    #[test]
    fn an_alias_is_tried_before_the_shipped_table() {
        let aliases = [BuilderFamilyAlias {
            prefix: "acme".to_string(),
            family: "acme-labs".to_string(),
        }];
        assert_eq!(family_of("Acme Coder", &aliases), "acme-labs");
    }

    #[test]
    fn a_range_with_claude_trailers_gives_anthropic() {
        let root = TempDir::new("osf-builder-claude");
        init_repo(&root);
        commit(
            &root,
            "first change\n\nCode-Generator: Claude Sonnet 5 <noreply@anthropic.com>\n",
        );
        let base = commit_hash(&root);
        commit(
            &root,
            "second change\n\nCode-Generator: Claude Opus 4 <noreply@anthropic.com>\n",
        );
        let range = format!("{base}..HEAD");
        let families = detect(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(families, vec!["anthropic".to_string()]);
    }

    #[test]
    fn a_mixed_range_gives_both_families() {
        let root = TempDir::new("osf-builder-mixed");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "first change\n\nCode-Generator: Claude Sonnet 5 <noreply@anthropic.com>\n",
        );
        commit(
            &root,
            "second change\n\nCode-Generator: GPT-5 <noreply@openai.com>\n",
        );
        let range = format!("{base}..HEAD");
        let families = detect(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(
            families,
            vec!["anthropic".to_string(), "openai".to_string()]
        );
    }

    #[test]
    fn the_flag_overrides_detection() {
        let root = TempDir::new("osf-builder-override");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "a change\n\nCode-Generator: Claude Sonnet 5 <noreply@anthropic.com>\n",
        );
        let range = format!("{base}..HEAD");
        let overrides = vec!["openai".to_string()];
        let families = detect(&root, &range, &[], &overrides).expect("detect runs");
        assert_eq!(families, vec!["openai".to_string()]);
    }

    #[test]
    fn an_unknown_model_is_recorded_as_unknown() {
        let root = TempDir::new("osf-builder-unknown-model");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "a change\n\nCode-Generator: Some New Model <noreply@example.com>\n",
        );
        let range = format!("{base}..HEAD");
        let families = detect(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(families, vec![UNKNOWN.to_string()]);
    }

    #[test]
    fn no_trailer_at_all_is_recorded_as_unknown() {
        let root = TempDir::new("osf-builder-no-trailer");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(&root, "a change with no trailer at all");
        let range = format!("{base}..HEAD");
        let families = detect(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(families, vec![UNKNOWN.to_string()]);
    }
}
