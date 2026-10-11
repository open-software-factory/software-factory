//! Which model families built the change under review, read from each
//! commit's own `Code-Generator:` trailer, so `osf review run` can leave
//! reviewers from those families out of the roster for that run.
//!
//! A builder's own family judging its own change is not an independent
//! second opinion, so decision 0016 asks for reviewers from families other
//! than the builder's. This module only names the families; leaving them
//! out of the roster is `review_run`'s own job.

use crate::agents;
use crate::config::BuilderFamilyAlias;
use crate::git;
use std::collections::BTreeSet;
use std::path::Path;

/// A model whose name matches nothing in the family table.
pub const UNKNOWN: &str = "unknown";

/// The family a `Code-Generator:` trailer's model name maps to.
///
/// Every whitespace-separated word of `model_name`, and every `/`-separated
/// part of one, is matched case-insensitively by prefix. `aliases` are tried
/// first; when none matches, the shipped [`agents::MODEL_FAMILIES`] table is
/// used. A vendor name (`Anthropic`, `OpenAI`, `Google`) or a model id
/// (`claude-...`, `gpt-...`) anywhere in the name therefore decides the
/// family, so `Anthropic Claude Code claude-sonnet-5-5` and
/// `Claude Sonnet 5.5` both give `anthropic`.
///
/// [`UNKNOWN`] when nothing matches, when the name is empty, and when words
/// of one name point to different families: a name that cannot be told
/// apart is not guessed.
#[must_use]
pub fn family_of(model_name: &str, aliases: &[BuilderFamilyAlias]) -> String {
    let words: Vec<String> = model_name
        .split_whitespace()
        .flat_map(|word| {
            let word = word.to_lowercase();
            let parts: Vec<String> = word.split('/').map(str::to_string).collect();
            std::iter::once(word).chain(parts)
        })
        .collect();
    let mut families: BTreeSet<String> = BTreeSet::new();
    for alias in aliases {
        let prefix = alias.prefix.to_lowercase();
        if words.iter().any(|w| w.starts_with(&prefix)) {
            families.insert(alias.family.clone());
        }
    }
    if families.is_empty() {
        for (prefix, family) in agents::MODEL_FAMILIES {
            if words.iter().any(|w| w.starts_with(prefix)) {
                families.insert((*family).to_string());
            }
        }
    }
    let mut found = families.into_iter();
    match (found.next(), found.next()) {
        (Some(family), None) => family,
        _ => UNKNOWN.to_string(),
    }
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

/// Each commit in `range` with the model names of its own `Code-Generator:`
/// trailers, oldest first, except a clean merge with no trailer (see
/// [`git::is_clean_merge`], which compares the merge's tree with a fresh merge of its parents), which carries no work of its own and is left
/// out. A merge with a trailer counts like any commit. Any other commit with
/// none, or whose trailer names nothing, has an empty list.
///
/// # Errors
/// Returns an error when git cannot read `range`'s commits.
fn trailer_model_names_per_commit(
    root: &Path,
    range: &str,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let hashes = git::commit_hashes(root, range).map_err(|e| e.to_string())?;
    let mut commits = Vec::new();
    for hash in hashes {
        let message = git::commit_message(root, &hash).map_err(|e| e.to_string())?;
        let names: Vec<String> = message.lines().filter_map(trailer_model_name).collect();
        if names.is_empty() && git::is_clean_merge(root, &hash).map_err(|e| e.to_string())? {
            continue;
        }
        commits.push((hash, names));
    }
    Ok(commits)
}

/// What [`detect_detailed`] found: the distinct families, and why any of
/// them is [`UNKNOWN`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// Sorted and deduplicated. [`UNKNOWN`] stands for a trailer, or the
    /// lack of one, that names no family.
    pub families: Vec<String>,
    /// One entry per reason a family is unknown: the trailer as written, or
    /// the commit that carries none. Empty when every family is
    /// known, and always empty for an override.
    pub unknown: Vec<String>,
}

/// The distinct builder families for `range`'s own commits (`base..HEAD`
/// when `range` is built that way): `overrides` verbatim, sorted and
/// deduplicated, when the caller named at least one with
/// `--builder-family`; otherwise every commit's `Code-Generator:` trailer,
/// mapped through `aliases` and the shipped table.
///
/// A trailer whose model maps to nothing contributes [`UNKNOWN`] and is
/// named in [`Detected::unknown`]. So does each commit with no
/// trailer, or an empty one, even when other commits carry good ones, except
/// a clean merge with none; a
/// range with no commit at all is reported the same way.
///
/// # Errors
/// Returns an error when git cannot read `range`'s commits.
pub fn detect_detailed(
    root: &Path,
    range: &str,
    aliases: &[BuilderFamilyAlias],
    overrides: &[String],
) -> Result<Detected, String> {
    if !overrides.is_empty() {
        let mut families = overrides.to_vec();
        families.sort();
        families.dedup();
        return Ok(Detected {
            families,
            unknown: Vec::new(),
        });
    }
    let commits = trailer_model_names_per_commit(root, range)?;
    if commits.is_empty() {
        return Ok(Detected {
            families: vec![UNKNOWN.to_string()],
            unknown: vec!["no commits with a Code-Generator trailer to read".to_string()],
        });
    }
    let mut families = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for (hash, names) in &commits {
        if names.is_empty() {
            // One commit without attribution leaves the builder unknown, whatever
            // the other commits say.
            let described = format!(
                "commit {} has no Code-Generator trailer",
                hash.chars().take(12).collect::<String>()
            );
            unknown.push(described);
            families.push(UNKNOWN.to_string());
        }
        for name in names {
            let family = family_of(name, aliases);
            let described = format!("Code-Generator: {name}");
            if family == UNKNOWN && !unknown.contains(&described) {
                unknown.push(described);
            }
            families.push(family);
        }
    }
    families.sort();
    families.dedup();
    Ok(Detected { families, unknown })
}

/// [`detect_detailed`]'s families alone.
///
/// # Errors
/// Returns an error when git cannot read `range`'s commits.
pub fn detect(
    root: &Path,
    range: &str,
    aliases: &[BuilderFamilyAlias],
    overrides: &[String],
) -> Result<Vec<String>, String> {
    detect_detailed(root, range, aliases, overrides).map(|d| d.families)
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
    fn family_of_finds_the_family_anywhere_in_the_name() {
        for (name, family) in [
            ("Anthropic Claude Code claude-sonnet-5-5", "anthropic"),
            ("Claude Sonnet 5.5", "anthropic"),
            ("DeepSeek dsh deepseek-v4", "deepseek"),
            ("DeepSeek dsh", "deepseek"),
            ("OpenAI Codex gpt-5.1", "openai"),
            ("Codex", "openai"),
            ("Google Gemini 3 Pro", "google"),
            ("Google Antigravity", "google"),
            ("Alibaba Qwen Code qwen3-coder", "qwen"),
            ("openrouter/qwen/qwen3-coder-next", "qwen"),
            ("Meta Llama 4", "meta"),
            ("Mistral Vibe devstral", "mistral"),
        ] {
            assert_eq!(family_of(name, &[]), family, "{name}");
        }
    }

    #[test]
    fn family_of_does_not_guess_between_families() {
        assert_eq!(family_of("Claude via gpt-5 router", &[]), UNKNOWN);
    }

    #[test]
    fn an_alias_matches_a_later_word() {
        let aliases = [BuilderFamilyAlias {
            prefix: "acme".to_string(),
            family: "acme-labs".to_string(),
        }];
        assert_eq!(family_of("Vendor Acme Coder", &aliases), "acme-labs");
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

    #[test]
    fn an_unrecognised_trailer_is_named_in_the_detail() {
        let root = TempDir::new("osf-builder-detail");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "a change\n\nCode-Generator: Some New Model <noreply@example.com>\n",
        );
        let range = format!("{base}..HEAD");
        let found = detect_detailed(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(
            found.unknown,
            vec!["Code-Generator: Some New Model".to_string()]
        );
    }

    #[test]
    fn a_vendor_first_trailer_in_a_commit_gives_anthropic() {
        let root = TempDir::new("osf-builder-vendor-first");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "a change\n\nCode-Generator: Anthropic Claude Code claude-sonnet-5-5 <noreply@anthropic.com>\n",
        );
        let range = format!("{base}..HEAD");
        let found = detect_detailed(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(found.families, vec!["anthropic".to_string()]);
        assert_eq!(found.unknown, Vec::<String>::new());
    }

    #[test]
    fn one_commit_without_a_trailer_makes_a_mixed_history_unknown() {
        let root = TempDir::new("osf-builder-mixed-missing");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "first change\n\nCode-Generator: Claude Sonnet 5 <noreply@anthropic.com>\n",
        );
        commit(&root, "second change with no attribution");
        let range = format!("{base}..HEAD");
        let found = detect_detailed(&root, &range, &[], &[]).expect("detect runs");
        assert_eq!(
            found.families,
            vec!["anthropic".to_string(), UNKNOWN.to_string()]
        );
        assert_eq!(found.unknown.len(), 1);
        assert!(
            found
                .unknown
                .first()
                .is_some_and(|u| u.contains("has no Code-Generator trailer")),
            "{:?}",
            found.unknown
        );
    }

    #[test]
    fn a_commit_with_an_empty_trailer_makes_a_mixed_history_unknown() {
        let root = TempDir::new("osf-builder-mixed-empty");
        init_repo(&root);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        commit(
            &root,
            "first change\n\nCode-Generator: GPT-5 <noreply@example.com>\n",
        );
        commit(
            &root,
            "second change\n\nCode-Generator: <noreply@example.com>\n",
        );
        let range = format!("{base}..HEAD");
        let found = detect_detailed(&root, &range, &[], &[]).expect("detect runs");
        assert!(found.families.contains(&UNKNOWN.to_string()));
        assert!(found.families.contains(&"openai".to_string()));
        assert_eq!(found.unknown.len(), 1);
    }

    /// Runs `git <args>` in `dir`, which must succeed.
    fn git_ok(dir: &Path, args: &[&str]) {
        let mut c = Command::new("git");
        c.args(args).current_dir(dir);
        git::scrub_git_env(&mut c);
        assert!(c.status().expect("git runs").success(), "{args:?}");
    }

    fn write_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).expect("file writes");
    }

    /// A repository with a base commit holding `shared.txt`, then a `side`
    /// branch and the original branch each with one commit built by
    /// `Claude Sonnet 5`; returns the base hash. The checked-out branch is
    /// the original one. `side` and `main` set the new text of `shared.txt`
    /// on each branch, or `None` to add a separate file instead.
    fn forked(name: &str, side: Option<&str>, main: Option<&str>) -> (TempDir, String) {
        let root = TempDir::new(name);
        init_repo(&root);
        write_file(&root, "shared.txt", "one\ntwo\nthree\n");
        git_ok(&root, &["add", "-A"]);
        commit(&root, "root commit");
        let base = commit_hash(&root);
        let message = "change\n\nCode-Generator: Claude Sonnet 5 <noreply@anthropic.com>\n";
        git_ok(&root, &["checkout", "--quiet", "-b", "side"]);
        match side {
            Some(text) => write_file(&root, "shared.txt", text),
            None => write_file(&root, "side.txt", "side\n"),
        }
        git_ok(&root, &["add", "-A"]);
        git_ok(&root, &["commit", "--quiet", "-m", message]);
        git_ok(&root, &["checkout", "--quiet", "-"]);
        match main {
            Some(text) => write_file(&root, "shared.txt", text),
            None => write_file(&root, "main.txt", "main\n"),
        }
        git_ok(&root, &["add", "-A"]);
        git_ok(&root, &["commit", "--quiet", "-m", message]);
        (root, base)
    }

    #[test]
    fn a_clean_merge_without_a_trailer_is_exempt() {
        let (root, base) = forked("osf-builder-merge-clean", None, None);
        git_ok(
            &root,
            &["merge", "--quiet", "--no-ff", "-m", "Merge side", "side"],
        );
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert_eq!(found.families, vec!["anthropic".to_string()]);
        assert_eq!(found.unknown, Vec::<String>::new());
    }

    #[test]
    fn a_merge_with_a_conflict_resolution_and_no_trailer_is_unknown() {
        let (root, base) = forked(
            "osf-builder-merge-conflict",
            Some("one\nSIDE\nthree\n"),
            Some("one\nMAIN\nthree\n"),
        );
        let mut merge = Command::new("git");
        merge
            .args(["merge", "--no-ff", "--no-commit", "side"])
            .current_dir(&root);
        git::scrub_git_env(&mut merge);
        assert!(
            !merge.status().expect("git runs").success(),
            "must conflict"
        );
        write_file(&root, "shared.txt", "one\nRESOLVED\nthree\n");
        git_ok(&root, &["add", "-A"]);
        git_ok(&root, &["commit", "--quiet", "-m", "Merge side"]);
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert!(found.families.contains(&UNKNOWN.to_string()), "{found:?}");
        assert_eq!(found.unknown.len(), 1);
    }

    /// Conflicts `side` and `main` on `shared.txt`, then resolves by
    /// checking out `pick` (`--ours` or `--theirs`) and commits the merge.
    fn conflict_resolved_by(name: &str, pick: &str) -> (TempDir, String) {
        let (root, base) = forked(name, Some("one\nSIDE\nthree\n"), Some("one\nMAIN\nthree\n"));
        let mut merge = Command::new("git");
        merge
            .args(["merge", "--no-ff", "--no-commit", "side"])
            .current_dir(&root);
        git::scrub_git_env(&mut merge);
        assert!(
            !merge.status().expect("git runs").success(),
            "must conflict"
        );
        git_ok(&root, &["checkout", pick, "shared.txt"]);
        git_ok(&root, &["add", "-A"]);
        git_ok(&root, &["commit", "--quiet", "-m", "Merge side"]);
        (root, base)
    }

    #[test]
    fn a_conflict_resolved_by_taking_the_first_parents_side_is_unknown() {
        let (root, base) = conflict_resolved_by("osf-builder-merge-ours", "--ours");
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert!(found.families.contains(&UNKNOWN.to_string()), "{found:?}");
    }

    #[test]
    fn a_conflict_resolved_by_taking_the_second_parents_side_is_unknown() {
        let (root, base) = conflict_resolved_by("osf-builder-merge-theirs", "--theirs");
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert!(found.families.contains(&UNKNOWN.to_string()), "{found:?}");
    }

    #[test]
    fn a_merge_that_drops_an_incoming_change_is_unknown() {
        let (root, base) = forked("osf-builder-merge-drop", None, None);
        git_ok(
            &root,
            &["merge", "--quiet", "--no-ff", "--no-commit", "side"],
        );
        git_ok(&root, &["rm", "--quiet", "-f", "side.txt"]);
        git_ok(&root, &["commit", "--quiet", "-m", "Merge side"]);
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert!(found.families.contains(&UNKNOWN.to_string()), "{found:?}");
    }

    /// The platform's separator for a list of paths in a git variable.
    const LIST_SEPARATOR: char = if cfg!(windows) { ';' } else { ':' };

    #[cfg(unix)]
    #[test]
    fn a_clean_merge_is_exempt_in_a_repository_path_holding_the_list_separator() {
        let prefix = format!("osf-builder-sep{LIST_SEPARATOR}dir");
        let (root, base) = forked(&prefix, None, None);
        git_ok(
            &root,
            &["merge", "--quiet", "--no-ff", "-m", "Merge side", "side"],
        );
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert_eq!(found.families, vec!["anthropic".to_string()], "{found:?}");
    }

    /// Makes every file under `path` read-only, or writable again.
    #[cfg(unix)]
    fn set_tree_writable(path: &Path, writable: bool) {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode();
        let mode = if writable {
            mode | 0o200
        } else {
            mode & !0o222
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("permissions set");
        if path.is_dir() {
            for entry in std::fs::read_dir(path).expect("dir reads") {
                set_tree_writable(&entry.expect("entry").path(), writable);
            }
        }
    }

    /// A review job mounts the checkout read-only, and `merge-tree` writes
    /// objects, so the check must not write into the repository.
    #[cfg(unix)]
    #[test]
    fn a_clean_merge_is_exempt_in_a_read_only_checkout() {
        let (root, base) = forked("osf-builder-merge-readonly", None, None);
        git_ok(
            &root,
            &["merge", "--quiet", "--no-ff", "-m", "Merge side", "side"],
        );
        let git_dir = root.join(".git");
        set_tree_writable(&git_dir, false);
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]);
        set_tree_writable(&git_dir, true);
        let found = found.expect("runs");
        assert_eq!(found.families, vec!["anthropic".to_string()], "{found:?}");
        assert_eq!(found.unknown, Vec::<String>::new());
    }

    #[test]
    fn a_merge_with_an_extra_edit_and_no_trailer_is_unknown() {
        let (root, base) = forked("osf-builder-merge-extra", None, None);
        git_ok(
            &root,
            &["merge", "--quiet", "--no-ff", "--no-commit", "side"],
        );
        write_file(&root, "extra.txt", "slipped in\n");
        git_ok(&root, &["add", "-A"]);
        git_ok(&root, &["commit", "--quiet", "-m", "Merge side"]);
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert!(found.families.contains(&UNKNOWN.to_string()), "{found:?}");
        assert_eq!(found.unknown.len(), 1);
    }

    #[test]
    fn a_merge_carrying_another_familys_trailer_is_counted() {
        let (root, base) = forked("osf-builder-merge-trailer", None, None);
        git_ok(
            &root,
            &[
                "merge",
                "--quiet",
                "--no-ff",
                "-m",
                "Merge side\n\nCode-Generator: GPT-5 <noreply@example.com>",
                "side",
            ],
        );
        let found = detect_detailed(&root, &format!("{base}..HEAD"), &[], &[]).expect("runs");
        assert_eq!(
            found.families,
            vec!["anthropic".to_string(), "openai".to_string()]
        );
        assert_eq!(found.unknown, Vec::<String>::new());
    }
}
