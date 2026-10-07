//! Integration tests for `osf scan`: a throwaway git repository stands in
//! for a real one, so the git-tracked-files path and the commit-range path
//! are both exercised against real git plumbing, not a mock.

mod common;

use common::{
    coauthor_trailer, fake_secret_assignment, isolated_home, run_osf, session_link,
    suppress_marker, windows_user_path, TempRepo,
};
use osf::config::ScanConfig;
use osf::exclude::Excluder;
use osf::scan::{scan_commits, scan_paths, Rules};

/// Rules for a throwaway repository with no remote and no configuration:
/// the visibility is unknown, so every finding is an error, as before.
fn rules(repo: &TempRepo) -> Rules {
    Rules::build(&repo.dir, &ScanConfig::default()).expect("empty config builds")
}

fn no_exclude() -> Excluder {
    Excluder::none()
}

#[test]
fn scanning_with_no_paths_covers_every_tracked_file() {
    let repo = TempRepo::new("tracked-files");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.write("clean.md", "Nothing to see here.\n");
    repo.commit("add fixtures");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let with_findings: Vec<&(String, Vec<osf_lint_core::Finding>)> =
        found.files.iter().filter(|(_, f)| !f.is_empty()).collect();
    assert_eq!(with_findings.len(), 1, "{:?}", found.files);
    let (name, findings) = with_findings.first().expect("one flagged file");
    assert_eq!(name.as_str(), "notes.md");
    assert_eq!(findings.len(), 1);
}

#[test]
fn a_binary_tracked_file_is_skipped_not_scanned() {
    let repo = TempRepo::new("binary-file");
    std::fs::write(repo.dir.join("blob.bin"), [0u8, 1, 2, 3, b'C', b'o']).expect("binary writes");
    repo.commit("add a binary file");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    assert!(
        found.files.iter().all(|(_, f)| f.is_empty()),
        "a binary file must never be scanned as text: {:?}",
        found.files
    );
}

/// A scan reads only this repository's own files, each at its own path. A
/// link's target is either one of those, scanned at its real path, or lies
/// outside the repository, so the link itself is skipped and counted.
#[test]
fn a_tracked_symlink_to_a_directory_is_skipped_not_read_as_a_file() {
    let repo = TempRepo::new("symlink-to-dir");
    repo.write("real/file.md", "Nothing to see here.\n");
    repo.symlink("link", "real");
    repo.commit("add a symlinked directory");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    assert!(
        found.files.iter().all(|(_, f)| f.is_empty()),
        "a symlink must never be read as if it were the file or directory it points at: {:?}",
        found.files
    );
    assert_eq!(
        found.excluded, 1,
        "a skipped symlink must be counted: {found:?}"
    );
}

/// A repository whose only tracked entry is a symlink must still show the
/// summary skipped something, so it never reads as a clean scan of nothing.
#[test]
fn a_repo_of_only_a_symlink_reports_it_excluded_not_clean() {
    let repo = TempRepo::new("only-a-symlink");
    repo.symlink("link", "nowhere");
    repo.commit("track only a symlink");
    let home = isolated_home("only-a-symlink");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "human"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 excluded"), "{stdout}");
}

/// An explicit scan path that is itself a symlinked directory is skipped and
/// counted as excluded. A path named on the command line follows the same
/// rule as a tracked file, so the result depends only on the files in the tree.
#[test]
fn an_explicit_path_that_is_a_symlinked_directory_is_not_followed() {
    let repo = TempRepo::new("explicit-path-symlinked-dir");
    repo.write(
        "real/file.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.symlink("link", "real");

    let target = repo.dir.join("link");
    let found = scan_paths(
        &repo.dir,
        std::slice::from_ref(&target),
        &rules(&repo),
        &no_exclude(),
        false,
    )
    .expect("scan runs");
    assert!(found.files.is_empty(), "{found:?}");
    assert_eq!(found.excluded, 1, "{found:?}");
}

/// A symlinked directory found while walking a scanned folder is skipped
/// and counted. Its target belongs to another folder, or to no folder in
/// this repository, so the walk stays inside the folder it was given.
#[test]
fn a_symlinked_directory_inside_a_scanned_folder_is_not_followed() {
    let repo = TempRepo::new("nested-symlinked-dir");
    repo.write(
        "outside/secret.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.write("scanned/plain.md", "Nothing to see here.\n");
    repo.symlink("scanned/link", "../outside");

    let target = repo.dir.join("scanned");
    let found = scan_paths(
        &repo.dir,
        std::slice::from_ref(&target),
        &rules(&repo),
        &no_exclude(),
        false,
    )
    .expect("scan runs");
    assert!(
        found.files.iter().all(|(_, f)| f.is_empty()),
        "the file behind the symlink must never be read: {found:?}"
    );
    assert_eq!(found.excluded, 1, "{found:?}");
}

/// A symlink that cycles back on an ancestor directory is skipped the
/// first time it is seen, instead of recursing forever.
#[test]
fn a_symlink_cycle_is_skipped_once_not_followed_forever() {
    let repo = TempRepo::new("symlink-cycle");
    repo.write("real/sub/file.md", "Nothing to see here.\n");
    repo.symlink("real/sub/loop", "..");

    let target = repo.dir.join("real");
    let found = scan_paths(
        &repo.dir,
        std::slice::from_ref(&target),
        &rules(&repo),
        &no_exclude(),
        false,
    )
    .expect("scan runs");
    assert!(found.files.iter().all(|(_, f)| f.is_empty()), "{found:?}");
    assert_eq!(found.excluded, 1, "{found:?}");
}

#[test]
fn an_explicit_path_is_scanned_even_when_not_tracked() {
    let repo = TempRepo::new("explicit-path");
    repo.write(
        "untracked.md",
        &format!("Fixed near {} today.\n", windows_user_path("pat")),
    );

    let target = repo.dir.join("untracked.md");
    let found = scan_paths(
        &repo.dir,
        std::slice::from_ref(&target),
        &rules(&repo),
        &no_exclude(),
        false,
    )
    .expect("scan runs");
    assert_eq!(found.files.len(), 1);
    let (_, findings) = found.files.first().expect("one file scanned");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings.first().map(|f| f.rule), Some("scan-local-path"));
}

#[test]
fn a_commit_range_scan_reports_the_hash_and_the_line() {
    let repo = TempRepo::new("commit-range");
    repo.write("a.txt", "one\n");
    let base = repo.commit("base commit");
    repo.write("b.txt", "two\n");
    let bad = repo.commit(&format!(
        "Fix the bug.\n\n{}\n",
        coauthor_trailer("Someone", "someone@example.com")
    ));

    let range = format!("{base}..{bad}");
    let found = scan_commits(&repo.dir, &range, &rules(&repo)).expect("commit scan runs");
    let flagged: Vec<&(String, Vec<osf_lint_core::Finding>)> =
        found.iter().filter(|(_, f)| !f.is_empty()).collect();
    assert_eq!(flagged.len(), 1, "{found:?}");
    let (hash, findings) = flagged.first().expect("one flagged commit");
    assert_eq!(hash.as_str(), bad);
    let finding = findings.first().expect("one finding");
    assert_eq!(finding.rule, "scan-coauthor-trailer");
    assert_eq!(finding.line, 3);
}

/// Exit code 0: git runs fine, and nothing it finds is a problem.
#[test]
fn a_clean_scan_exits_zero() {
    let repo = TempRepo::new("exit-clean");
    repo.write("clean.md", "Nothing to see here.\n");
    repo.commit("add a clean file");
    let home = isolated_home("exit-clean");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// Exit code 1: git runs fine, and it finds something that must never
/// reach a public repository.
#[test]
fn a_scan_that_finds_something_exits_one() {
    let repo = TempRepo::new("exit-dirty");
    repo.write(
        "notes.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.commit("add a flagged file");
    let home = isolated_home("exit-dirty");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// Exit code 2: the tool itself could not run, distinct from running and
/// finding nothing. A named path that does not exist is never "clean".
#[test]
fn scanning_a_path_that_does_not_exist_exits_two() {
    let repo = TempRepo::new("exit-missing-path");
    repo.write("a.txt", "one\n");
    repo.commit("add a file");
    let home = isolated_home("exit-missing-path");

    let output = run_osf(
        &repo.dir,
        &home,
        &["scan", "--format", "json", "does-not-exist.md"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

/// Exit code 2 again, this time because `--commits` names a range git
/// cannot resolve: still "could not run", not "found nothing".
#[test]
fn scanning_an_unresolvable_commit_range_exits_two() {
    let repo = TempRepo::new("exit-bad-range");
    repo.write("a.txt", "one\n");
    repo.commit("add a file");
    let home = isolated_home("exit-bad-range");

    let output = run_osf(
        &repo.dir,
        &home,
        &["scan", "--commits", "not-a-real-ref..HEAD"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

/// The compiled default exclude list is `target/**` only: a file under a
/// vendor path is not excluded by default, so it still shows up.
#[test]
fn the_default_exclude_list_does_not_hide_a_non_target_path() {
    let repo = TempRepo::new("default-exclude-is-target-only");
    repo.write(
        "vendor/generated.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.commit("add a vendor file with a bad pattern");
    let home = isolated_home("default-exclude-is-target-only");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "human"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// `--exclude` adds a pattern on top of the compiled defaults; the summary
/// counts the dropped candidate.
#[test]
fn exclude_flag_hides_a_matching_path_and_the_summary_counts_it() {
    let repo = TempRepo::new("exclude-flag-hides");
    repo.write(
        "vendor/generated.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.commit("add a vendor file");
    let home = isolated_home("exclude-flag-hides");

    let output = run_osf(
        &repo.dir,
        &home,
        &["scan", "--format", "human", "--exclude", "vendor/**"],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 excluded"), "{stdout}");
}

/// `--no-exclude` ignores the list entirely, so a path an exclude pattern
/// would otherwise hide shows up again.
#[test]
fn no_exclude_flag_shows_everything_again() {
    let repo = TempRepo::new("no-exclude-shows-everything");
    repo.write(
        "vendor/generated.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.commit("add a vendor file");
    let home = isolated_home("no-exclude-shows-everything");

    let excluded = run_osf(
        &repo.dir,
        &home,
        &["scan", "--format", "json", "--exclude", "vendor/**"],
    );
    assert_eq!(excluded.status.code(), Some(0), "{excluded:?}");

    let shown = run_osf(
        &repo.dir,
        &home,
        &[
            "scan",
            "--format",
            "json",
            "--exclude",
            "vendor/**",
            "--no-exclude",
        ],
    );
    assert_eq!(shown.status.code(), Some(1), "{shown:?}");
    let stdout = String::from_utf8_lossy(&shown.stdout);
    assert!(stdout.contains("scan-coauthor-trailer"), "{stdout}");
}

/// `--gate` forces the exclude list back to the compiled defaults, ignoring
/// a config file's own exclude setting, so a change under review cannot
/// loosen the check it is being checked against.
#[test]
fn gate_ignores_a_config_files_exclude_list() {
    let repo = TempRepo::new("gate-ignores-config-file");
    repo.write(
        "vendor/generated.md",
        &format!("{}\n", coauthor_trailer("Someone", "someone@example.com")),
    );
    repo.write("osf.toml", "exclude = [\"vendor/**\"]\n");
    repo.commit("add a vendor file and a loosened config");
    let home = isolated_home("gate-ignores-config-file");

    let without_gate = run_osf(
        &repo.dir,
        &home,
        &["--config", "osf.toml", "scan", "--format", "json"],
    );
    assert_eq!(without_gate.status.code(), Some(0), "{without_gate:?}");

    let with_gate = run_osf(
        &repo.dir,
        &home,
        &["--config", "osf.toml", "scan", "--format", "json", "--gate"],
    );
    assert_eq!(with_gate.status.code(), Some(1), "{with_gate:?}");
}

// --- suppression markers over a scan finding ------------------------------

#[test]
fn a_secret_finding_with_a_line_marker_and_a_reason_is_suppressed() {
    let repo = TempRepo::new("scan-suppress-secret-line");
    let line = format!(
        "{} {}\n",
        fake_secret_assignment("TOKEN"),
        suppress_marker("disable-line", "scan-secret", Some("test fixture"))
    );
    repo.write("config.txt", &line);
    repo.commit("add a suppressed secret");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    let secret = findings
        .iter()
        .find(|f| f.rule == "scan-secret")
        .expect("scan-secret fires");
    assert_eq!(secret.suppressed.as_deref(), Some("test fixture"));
}

#[test]
fn a_secret_finding_is_suppressed_by_a_file_marker() {
    let repo = TempRepo::new("scan-suppress-secret-file");
    let text = format!(
        "{}\n{}\n",
        suppress_marker("disable-file", "scan-secret", Some("test fixture")),
        fake_secret_assignment("KEY")
    );
    repo.write("config.txt", &text);
    repo.commit("add a file-suppressed secret");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    let secret = findings
        .iter()
        .find(|f| f.rule == "scan-secret")
        .expect("scan-secret fires");
    assert!(secret.suppressed.is_some(), "{findings:?}");
}

#[test]
fn a_marker_with_no_reason_does_not_suppress_a_secret_finding() {
    let repo = TempRepo::new("scan-suppress-secret-no-reason");
    let line = format!(
        "{} {}\n",
        fake_secret_assignment("SECRET"),
        suppress_marker("disable-line", "scan-secret", None)
    );
    repo.write("config.txt", &line);
    repo.commit("add an unreasoned marker");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    let secret = findings
        .iter()
        .find(|f| f.rule == "scan-secret")
        .expect("scan-secret fires");
    assert!(secret.suppressed.is_none(), "{findings:?}");
    assert!(findings
        .iter()
        .any(|f| f.rule == "suppression-without-reason"));
}

#[test]
fn a_marker_for_a_different_rule_does_not_suppress_a_secret_finding() {
    let repo = TempRepo::new("scan-suppress-secret-other-rule");
    let line = format!(
        "{} {}\n",
        fake_secret_assignment("PASSWORD"),
        suppress_marker("disable-line", "scan-local-path", Some("test fixture"))
    );
    repo.write("config.txt", &line);
    repo.commit("add a marker naming a different rule");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    let secret = findings
        .iter()
        .find(|f| f.rule == "scan-secret")
        .expect("scan-secret fires");
    assert!(secret.suppressed.is_none(), "{findings:?}");
}

/// A marker for a writing-lint rule, sitting in a file scan also reads,
/// must not read as unused: scan does not run the writing lint, so it
/// cannot say whether that marker ever matched anything.
#[test]
fn a_writing_only_marker_in_a_scanned_file_gives_zero_scan_findings() {
    let repo = TempRepo::new("scan-ignores-a-writing-only-marker");
    let text = format!(
        "Nothing sensitive here. {}\n",
        suppress_marker(
            "disable-line",
            "long-sentence",
            Some("quoting a specification verbatim")
        )
    );
    repo.write("notes.md", &text);
    repo.commit("add a writing-only marker");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    assert!(findings.is_empty(), "{findings:?}");
}

/// A scan-secret marker that never matches a real finding is scan's own to
/// judge, so it still warns as unused.
#[test]
fn an_unused_scan_secret_marker_still_warns() {
    let repo = TempRepo::new("scan-unused-secret-marker-warns");
    let text = format!(
        "Nothing sensitive here. {}\n",
        suppress_marker("disable-line", "scan-secret", Some("test fixture"))
    );
    repo.write("notes.md", &text);
    repo.commit("add an unused scan-secret marker");

    let found = scan_paths(&repo.dir, &[], &rules(&repo), &no_exclude(), false).expect("scan runs");
    let (_, findings) = found.files.first().expect("one file scanned");
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "suppression-unused" && f.excerpt == "scan-secret"),
        "{findings:?}"
    );
}

/// `--no-suppress` ignores a marker the same way it already does for
/// `osf lint writing`: continuous integration's own secret scan must not be
/// bypassable by a reasoned marker added alongside the secret.
#[test]
fn no_suppress_flag_ignores_a_reasoned_marker() {
    let repo = TempRepo::new("scan-no-suppress");
    repo.write(
        "config.txt",
        &format!(
            "{} {}\n",
            fake_secret_assignment("TOKEN"),
            suppress_marker("disable-line", "scan-secret", Some("test fixture"))
        ),
    );
    repo.commit("add a suppressed secret");
    let home = isolated_home("scan-no-suppress");

    let plain = run_osf(&repo.dir, &home, &["scan", "--format", "json"]);
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");

    let strict = run_osf(
        &repo.dir,
        &home,
        &["scan", "--format", "json", "--no-suppress"],
    );
    assert_eq!(strict.status.code(), Some(1), "{strict:?}");
}
