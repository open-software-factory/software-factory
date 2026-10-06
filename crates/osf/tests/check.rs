//! Integration tests for `osf check <name>`: each existing verify check,
//! run on its own over an explicit file list instead of a computed diff.

mod common;
use common::{
    fake_secret_assignment, isolated_home, run_osf, run_osf_with_env, session_link,
    suppress_marker, TempRepo,
};

/// The length of a SARIF report's first run's `results` array, read without
/// the panicking index operator this workspace's clippy settings deny.
fn sarif_results_len(v: &serde_json::Value) -> Option<usize> {
    v.get("runs")?
        .get(0)?
        .get("results")?
        .as_array()
        .map(Vec::len)
}

#[test]
fn check_scan_reports_a_leak_in_a_named_file_and_writes_sarif() {
    let repo = TempRepo::new("check-scan");
    repo.write(
        "leak.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.commit("add a leak");
    let home = isolated_home("check-scan");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan",
            "--checkpoint",
            "pre-push",
            "--sarif-out",
            ".osf/out/scan.sarif",
            "leak.md",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let sarif =
        std::fs::read_to_string(repo.dir.join(".osf/out/scan.sarif")).expect("sarif written");
    let v: serde_json::Value = serde_json::from_str(&sarif).expect("sarif is json");
    assert_eq!(sarif_results_len(&v), Some(1));
}

#[test]
fn check_with_no_files_says_nothing_to_check_and_writes_an_empty_sarif() {
    let repo = TempRepo::new("check-empty");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");
    let home = isolated_home("check-empty");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pre-push",
            "--sarif-out",
            "o.sarif",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to check"));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo.dir.join("o.sarif")).expect("written"))
            .expect("json");
    assert_eq!(sarif_results_len(&v), Some(0));
}

#[test]
fn check_scan_staged_reads_the_index_not_the_working_tree() {
    let repo = TempRepo::new("check-staged");
    repo.write("notes.md", "Clean for now.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.stage("notes.md");
    repo.write("notes.md", "Clean again in the working tree.\n");
    let home = isolated_home("check-staged");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan-staged",
            "--checkpoint",
            "pre-commit",
            "notes.md",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// Moved from `tests/verify.rs`: a fixture under `tests/fixtures`
/// that declares an `osf-expect` marker is checked against that
/// declaration, rather than against the usual level rules, the same way
/// `osf lint writing` treats one.
#[test]
fn check_lint_writing_matches_a_fixture_s_declared_rule() {
    let repo = TempRepo::new("check-fixture-declared");
    repo.write(
        "crates/osf/tests/fixtures/writing/demo.md",
        "A thing — another thing.\n\n<!-- osf-expect\nem-dash\n-->\n",
    );
    repo.commit("add a declared writing fixture");
    let home = isolated_home("check-fixture-declared");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pre-push",
            "crates/osf/tests/fixtures/writing/demo.md",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

/// The mirror case: a fixture that no longer produces a rule it declares
/// is a failure, the alarm for a rule that silently stopped firing.
#[test]
fn check_lint_writing_fails_a_fixture_missing_its_declared_rule() {
    let repo = TempRepo::new("check-fixture-missing");
    repo.write(
        "crates/osf/tests/fixtures/writing/demo.md",
        "Nothing wrong here.\n\n<!-- osf-expect\nem-dash\n-->\n",
    );
    repo.commit("add a fixture missing its declared rule");
    let home = isolated_home("check-fixture-missing");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pre-push",
            "crates/osf/tests/fixtures/writing/demo.md",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("expectation-missing"),
        "{out:?}"
    );
}

#[test]
fn check_lint_writing_gate_ignores_a_suppression_marker() {
    let repo = TempRepo::new("check-gate");
    repo.write(
        "n.md",
        &format!(
            "Fixed in #125 today. {}\n",
            suppress_marker("disable-line", "bare-reference", Some("tracked"))
        ),
    );
    repo.commit("suppressed");
    let home = isolated_home("check-gate");
    let plain = run_osf(
        &repo.dir,
        &home,
        &["check", "lint-writing", "--checkpoint", "pre-push", "n.md"],
    );
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");
    let gated = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pull-request",
            "--gate",
            "n.md",
        ],
    );
    assert_eq!(gated.status.code(), Some(1), "{gated:?}");
}

/// A suppression marker on a `scan` finding works the same way it does for
/// `lint-writing`: `osf-disable-line <rule> -- <reason>` on the offending
/// line drops the exit code to 0.
#[test]
fn check_scan_honours_a_suppression_marker_with_a_reason() {
    let repo = TempRepo::new("check-scan-suppress");
    repo.write(
        "config.txt",
        &format!(
            "{} {}\n",
            fake_secret_assignment("TOKEN"),
            suppress_marker("disable-line", "scan-secret", Some("test fixture"))
        ),
    );
    repo.commit("add a suppressed secret");
    let home = isolated_home("check-scan-suppress");
    let out = run_osf(
        &repo.dir,
        &home,
        &["check", "scan", "--checkpoint", "pre-push", "config.txt"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

/// The same marker with no reason after `--` does not suppress: the finding
/// still fails the check.
#[test]
fn check_scan_ignores_a_suppression_marker_with_no_reason() {
    let repo = TempRepo::new("check-scan-suppress-no-reason");
    repo.write(
        "config.txt",
        &format!(
            "{} {}\n",
            fake_secret_assignment("SECRET"),
            suppress_marker("disable-line", "scan-secret", None)
        ),
    );
    repo.commit("add an unreasoned marker");
    let home = isolated_home("check-scan-suppress-no-reason");
    let out = run_osf(
        &repo.dir,
        &home,
        &["check", "scan", "--checkpoint", "pre-push", "config.txt"],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// `--gate` ignores a suppression marker for `scan`, the same as it already
/// does for `lint-writing`: a secret with a reasoned marker must not pass
/// the pull-request gate just because the change under review added the
/// marker itself.
#[test]
fn check_scan_gate_ignores_a_suppression_marker() {
    let repo = TempRepo::new("check-scan-gate");
    repo.write(
        "config.txt",
        &format!(
            "{} {}\n",
            fake_secret_assignment("TOKEN"),
            suppress_marker("disable-line", "scan-secret", Some("test fixture"))
        ),
    );
    repo.commit("add a suppressed secret");
    let home = isolated_home("check-scan-gate");
    let plain = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan",
            "--checkpoint",
            "pull-request",
            "config.txt",
        ],
    );
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");
    let gated = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan",
            "--checkpoint",
            "pull-request",
            "--gate",
            "config.txt",
        ],
    );
    assert_eq!(gated.status.code(), Some(1), "{gated:?}");
}

/// The same gate behaviour for `scan-staged`.
#[test]
fn check_scan_staged_gate_ignores_a_suppression_marker() {
    let repo = TempRepo::new("check-scan-staged-gate");
    repo.write("config.txt", "Clean for now.\n");
    repo.commit("clean");
    repo.write(
        "config.txt",
        &format!(
            "{} {}\n",
            fake_secret_assignment("SECRET"),
            suppress_marker("disable-line", "scan-secret", Some("test fixture"))
        ),
    );
    repo.stage("config.txt");
    let home = isolated_home("check-scan-staged-gate");
    let plain = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan-staged",
            "--checkpoint",
            "pre-commit",
            "config.txt",
        ],
    );
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");
    let gated = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan-staged",
            "--checkpoint",
            "pre-commit",
            "--gate",
            "config.txt",
        ],
    );
    assert_eq!(gated.status.code(), Some(1), "{gated:?}");
}

/// `scan-staged` reads the index, so a file staged then deleted from disk
/// (the deletion itself not staged) is still there to check: existence for
/// this check means present in the index, never the working tree.
#[test]
fn check_scan_staged_finds_a_leak_staged_then_deleted_from_disk() {
    let repo = TempRepo::new("check-staged-deleted");
    repo.write("notes.md", "Clean for now.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.stage("notes.md");
    std::fs::remove_file(repo.dir.join("notes.md")).expect("file removes");
    let home = isolated_home("check-staged-deleted");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan-staged",
            "--checkpoint",
            "pre-commit",
            "notes.md",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// A file that is in neither the index nor the working tree is still a
/// could-not-run error naming it, never a silent drop.
#[test]
fn check_scan_staged_reports_could_not_run_for_a_file_in_neither_place() {
    let repo = TempRepo::new("check-staged-missing");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");
    let home = isolated_home("check-staged-missing");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "scan-staged",
            "--checkpoint",
            "pre-commit",
            "ghost.md",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ghost.md"), "{out:?}");
    assert!(
        stderr.contains("does not exist"),
        "git's own error text must survive, not just the file name: {out:?}"
    );
}

/// Moon cannot pass changed file paths to these tasks, so each check falls
/// back to `OSF_FILES_FROM`: a file naming one repository-relative path per
/// line, used only when no `FILES` are given on the command line.
#[test]
fn check_scan_reads_files_from_the_env_var_list_when_no_files_are_given() {
    let repo = TempRepo::new("check-files-from-env");
    repo.write(
        "leak.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.commit("add a leak");
    let list_path = repo.dir.join("files-from.txt");
    std::fs::write(&list_path, "leak.md\n").expect("list file writes");
    let home = isolated_home("check-files-from-env");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_FILES_FROM", list_path.to_str().expect("utf8 path"))],
        &["check", "scan", "--checkpoint", "pre-push"],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// `osf check scan` with no `--checkpoint` exits 2 and names every valid
/// value, so a broken moon task fails loudly instead of silently reading
/// the wrong content.
#[test]
fn check_scan_with_no_checkpoint_flag_exits_two_and_names_the_valid_values() {
    let repo = TempRepo::new("check-no-checkpoint");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");
    let home = isolated_home("check-no-checkpoint");
    let out = run_osf(&repo.dir, &home, &["check", "scan"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    for value in ["hook", "pre-commit", "pre-push", "pull-request", "schedule"] {
        assert!(stderr.contains(value), "{value} missing from: {stderr}");
    }
}

/// The hook checkpoint, selected by the flag, reads a file as it is on
/// disk right now, not its last committed content.
#[test]
fn check_scan_at_the_hook_checkpoint_reads_the_file_just_written_to_disk() {
    let repo = TempRepo::new("check-hook-reads-disk");
    repo.write("notes.md", "Clean.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    let home = isolated_home("check-hook-reads-disk");
    let out = run_osf(
        &repo.dir,
        &home,
        &["check", "scan", "--checkpoint", "hook", "notes.md"],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// The pre-push checkpoint, selected by the same flag, reads `HEAD`
/// instead, so the same uncommitted leak on disk is invisible to it.
#[test]
fn check_scan_at_the_pre_push_checkpoint_reads_head_not_the_working_tree() {
    let repo = TempRepo::new("check-pre-push-reads-head");
    repo.write("notes.md", "Clean.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    let home = isolated_home("check-pre-push-reads-head");
    let out = run_osf(
        &repo.dir,
        &home,
        &["check", "scan", "--checkpoint", "pre-push", "notes.md"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

/// `lint-skill` follows the same rule as `scan` and `lint-writing`: the
/// pre-push checkpoint reads `SKILL.md` as committed at `HEAD`, not an
/// uncommitted working-tree fix.
#[test]
fn check_lint_skill_at_the_pre_push_checkpoint_reads_head_not_the_working_tree() {
    let repo = TempRepo::new("check-lint-skill-pre-push-head");
    let first_person = "---\nname: demo\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n";
    repo.write("skills/demo/SKILL.md", first_person);
    repo.commit("add a first-person skill description");
    let third_person = "---\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n";
    repo.write("skills/demo/SKILL.md", third_person);
    let home = isolated_home("check-lint-skill-pre-push-head");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "pre-push",
            "skills/demo/SKILL.md",
        ],
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("AS-016"),
        "the committed, duplicate-name frontmatter must still be read at pre-push: {out:?}"
    );
}

/// The mirror case: the hook checkpoint reads `SKILL.md` as it is on disk
/// right now, so the same uncommitted fix is visible to it.
#[test]
fn check_lint_skill_at_the_hook_checkpoint_reads_the_working_tree_fix() {
    let repo = TempRepo::new("check-lint-skill-hook-disk");
    let first_person = "---\nname: demo\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n";
    repo.write("skills/demo/SKILL.md", first_person);
    repo.commit("add a first-person skill description");
    let third_person = "---\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n";
    repo.write("skills/demo/SKILL.md", third_person);
    let home = isolated_home("check-lint-skill-hook-disk");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "hook",
            "skills/demo/SKILL.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        stdout.contains("osf check lint-skill: 0 error(s)"),
        "the check printed no result: {out:?}"
    );
    assert!(
        !stdout.contains("AS-016"),
        "the on-disk fix must be what the hook checkpoint reads: {out:?}"
    );
}

const CLEAN_SKILL: &str = "---\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n";

/// A skill's scripts come from the same place as its `SKILL.md`: at pre-push
/// the committed script, so an unstaged fix cannot hide a committed unpinned install.
#[test]
fn check_lint_skill_at_the_pre_push_checkpoint_reads_committed_scripts() {
    let repo = TempRepo::new("check-lint-skill-script-head");
    repo.write("skills/demo/SKILL.md", CLEAN_SKILL);
    repo.write("skills/demo/scripts/setup.sh", "npm install -g left-pad\n");
    repo.commit("add a skill with an unpinned install");
    repo.write(
        "skills/demo/scripts/setup.sh",
        "npm install -g left-pad@1.3.0\n",
    );
    let home = isolated_home("check-lint-skill-script-head");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "pre-push",
            "skills/demo/SKILL.md",
        ],
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("skill-script-unpinned"),
        "the committed unpinned install must still be read at pre-push: {out:?}"
    );
}

/// The mirror case: the hook checkpoint reads the script on disk right now.
#[test]
fn check_lint_skill_at_the_hook_checkpoint_reads_the_working_tree_script() {
    let repo = TempRepo::new("check-lint-skill-script-hook");
    repo.write("skills/demo/SKILL.md", CLEAN_SKILL);
    repo.write("skills/demo/scripts/setup.sh", "npm install -g left-pad\n");
    repo.commit("add a skill with an unpinned install");
    repo.write(
        "skills/demo/scripts/setup.sh",
        "npm install -g left-pad@1.3.0\n",
    );
    let home = isolated_home("check-lint-skill-script-hook");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "hook",
            "skills/demo/SKILL.md",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        stdout.contains("osf check lint-skill: 0 error(s)"),
        "the check printed no result: {out:?}"
    );
    assert!(
        !stdout.contains("skill-script-unpinned"),
        "the on-disk fix must be what the hook checkpoint reads: {out:?}"
    );
}

/// `OSF_CHECKPOINT` is no longer read at all. An inherited value of `hook`
/// must not turn a `--checkpoint pre-push` run into a disk read.
#[test]
fn an_inherited_osf_checkpoint_env_var_no_longer_changes_what_pre_push_reads() {
    let repo = TempRepo::new("check-env-var-ignored");
    repo.write("notes.md", "Clean.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    let home = isolated_home("check-env-var-ignored");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_CHECKPOINT", "hook")],
        &["check", "scan", "--checkpoint", "pre-push", "notes.md"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

/// Config cannot turn off a skill fixture's own declared-rule contract.
#[test]
fn config_cannot_disable_a_skill_fixture_contract_failure_in_check() {
    let repo = TempRepo::new("check-skill-fixture-policy-is-fixed");
    repo.write(
        "crates/osf/tests/fixtures/skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n\n<!-- osf-expect-skill\nskill-first-person\n-->\n",
    );
    repo.write(
        "osf.toml",
        "[skill.levels]\nexpectation-missing = \"off\"\n",
    );
    repo.commit("add a declared skill fixture and weaken its contract policy");
    let home = isolated_home("check-skill-fixture-policy-is-fixed");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "--config",
            "osf.toml",
            "check",
            "lint-skill",
            "--checkpoint",
            "pre-push",
            "crates/osf/tests/fixtures/skills/demo/SKILL.md",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("expectation-missing"),
        "{out:?}"
    );
}

/// The out-of-fixtures marker warning stays configurable.
#[test]
fn check_honors_config_for_a_skill_marker_outside_fixtures_warning() {
    let repo = TempRepo::new("check-skill-outside-fixtures-warning-configurable");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Use this skill when the user wants a quick health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n## Checks\n\n1. Read the folder listing.\n2. Check that a license file exists.\n3. Check that a readme file exists.\n4. Write one line per problem found, with the file path.\n\nStop when every check has run once.\n\n<!-- osf-expect-skill\n-->\n",
    );
    repo.write(
        "osf.toml",
        "[skill.levels]\nexpectation-outside-fixtures = \"off\"\n",
    );
    repo.commit("add an out-of-fixtures marker and configure its warning off");
    let home = isolated_home("check-skill-outside-fixtures-warning-configurable");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "--config",
            "osf.toml",
            "check",
            "lint-skill",
            "--checkpoint",
            "pre-push",
            "skills/demo/SKILL.md",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("expectation-outside-fixtures"),
        "{out:?}"
    );
}
