//! Consumer-level policy tests. Raw detector fixtures remain independent
//! of enforcement so a disabled check can still be evaluated and improved.

mod common;

use common::{isolated_home, run_osf, TempDir, TempRepo};
use osf::config::WritingConfig;
use osf::lints::{load_known_names, Context};
use std::path::Path;

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn ordinary_writing_does_not_enforce_audited_detectors_but_raw_fixtures_do() {
    let repo = TempRepo::new("audit-writing-policy");
    let home = isolated_home("audit-writing-policy");
    repo.write("osf.toml", "[writing]\nmust_explain_names = [\"Fable\"]\n");
    let mut cases = vec![
        ("undefined-name", "Use Fable for this.\n".to_string()),
        (
            "undefined-name-at-start",
            "Deploy with FableDb today.\n".to_string(),
        ),
        ("arrow", "Input -> output.\n".to_string()),
        ("filler", "Let me know if that helps.\n".to_string()),
        (
            "numbers-in-prose",
            "It ran 12 axes over 3 rounds in 41 minutes.\n".to_string(),
        ),
        (
            "bold-sentence",
            "**Run the full suite before every release, without exception.**\n".to_string(),
        ),
    ];
    for rule in [
        "contrast-tail",
        "aphorism",
        "rhetorical-setup",
        "universal-pronoun",
        "colon-reveal",
        "short-kicker",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/writing/{rule}-positive.md"));
        cases.push((rule, std::fs::read_to_string(path).expect("fixture reads")));
    }
    let known = load_known_names(&[], None).expect("names load");
    let cfg = WritingConfig {
        must_explain_names: vec!["Fable".to_string()],
        ..WritingConfig::default()
    };
    for (rule, text) in cases {
        let raw =
            osf::lints::writing::lint_writing(&text, &known, &cfg, Context::Document, false, true);
        assert!(
            raw.iter().any(|f| f.rule == rule),
            "raw detector lost: {rule}: {raw:?}"
        );
        // A kept finding proves the command ran and printed its findings.
        repo.write("ordinary.md", &format!("{text}\n\nFixed in #125 today.\n"));
        let out = run_osf(
            &repo.dir,
            &home,
            &["lint", "writing", "--format", "json", "ordinary.md"],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let findings: Vec<serde_json::Value> = stdout
            .lines()
            .map(|line| serde_json::from_str(line).expect("finding JSON"))
            .collect();
        assert!(
            findings.iter().any(|f| f["rule"] == "bare-reference"),
            "the kept finding is missing, so the output proves nothing: {stdout}"
        );
        assert!(
            findings.iter().all(|f| f["rule"] != rule),
            "disabled {rule} enforced: {stdout}"
        );
    }
    repo.write("ordinary.md", "Fixed in #125 today.\n");
    let out = run_osf(&repo.dir, &home, &["lint", "writing", "ordinary.md"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "retained rule must enforce: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("bare-reference"));
}

#[test]
fn skill_enforcement_uses_both_audited_families_without_hiding_syntax_errors() {
    let repo = TempRepo::new("audit-skill-policy");
    let home = isolated_home("audit-skill-policy");
    for fixture in ["no-trigger", "first-person", "manual-shaped", "no-done"] {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/skills/{fixture}/SKILL.md")),
        )
        .expect("fixture reads");
        repo.write("skills/demo/SKILL.md", &text);
        let out = run_osf(
            &repo.dir,
            &home,
            &["lint", "skill", "--format", "human", "skills/demo"],
        );
        let printed = output_text(&out);
        assert!(
            printed.contains("osf lint skill:") && printed.contains("disabled from enforcement:"),
            "{fixture}: the command printed no result: {printed}"
        );
        for rule in [
            "skill-description-no-trigger",
            "skill-first-person",
            "skill-first-section-is-overview",
            "skill-descriptive-over-imperative",
            "skill-no-done-condition",
        ] {
            assert!(
                !String::from_utf8_lossy(&out.stdout).contains(&format!("[{rule}]")),
                "disabled {rule} enforced: {printed}"
            );
        }
    }
    repo.write("skills/demo/SKILL.md", "---\nname: demo\ndescription: Use when checking a folder.\n---\n\n## Steps\n\n1. Count 12 axes over 3 rounds in 41 minutes.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["lint", "skill", "--format", "human", "skills/demo"],
    );
    assert!(
        out.status.success(),
        "writing policy missing from skill: {}",
        output_text(&out)
    );
    assert!(
        output_text(&out).contains("osf lint skill: 0 error(s)"),
        "the command printed no result: {}",
        output_text(&out)
    );
    repo.write("skills/demo/SKILL.md", "---\nname: demo\nname: demo\ndescription: Use when checking a folder.\n---\n\n1. Run the check.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["lint", "skill", "--format", "human", "skills/demo"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "syntax check lost: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("agnix:AS-016"));
}

#[test]
fn commit_gate_does_not_enforce_audited_quantity_rule() {
    let repo = TempRepo::new("audit-commit-policy");
    let home = isolated_home("audit-commit-policy");
    repo.write("MSG", "Count 12 axes over 3 rounds in 41 minutes.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "lint",
            "writing",
            "--context",
            "commit",
            "--message",
            "MSG",
            "--gate",
            "--format",
            "human",
        ],
    );
    assert!(
        out.status.success(),
        "disabled rule blocked commit: {}",
        output_text(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("[numbers-in-prose]"),
        "disabled warning leaked: {}",
        output_text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("osf lint writing: 0 error(s)"),
        "the command printed no result: {}",
        output_text(&out)
    );
    // The same message with a kept finding must report it, and still not the quantity rule.
    repo.write(
        "MSG",
        "Count 12 axes over 3 rounds in 41 minutes.\n\nFixed in #125 today.\n",
    );
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "lint",
            "writing",
            "--context",
            "commit",
            "--message",
            "MSG",
            "--gate",
            "--format",
            "human",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    assert!(stdout.contains("[bare-reference]"), "{}", output_text(&out));
    assert!(!stdout.contains("[numbers-in-prose]"), "{stdout}");
}

#[test]
fn ci_gate_cannot_reenable_disabled_rules_or_disable_retained_rules() {
    let repo = TempRepo::new("audit-ci-policy");
    let home = isolated_home("audit-ci-policy");
    repo.write("base.txt", "base\n");
    repo.commit("Add base file");
    repo.write("guide.md", "Count 12 axes over 3 rounds in 41 minutes.\n");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Checks a folder for problems.\n---\n\n1. Run the check.\n",
    );
    repo.write("poison.toml", "[writing.levels]\nnumbers-in-prose = \"error\"\nbare-reference = \"off\"\n[skill.levels]\nskill-description-no-trigger = \"error\"\n");
    repo.commit("Add checked prose");
    let writing = [
        "--config",
        "poison.toml",
        "check",
        "lint-writing",
        "--checkpoint",
        "pre-push",
        "--gate",
        "guide.md",
    ];
    let skill = [
        "--config",
        "poison.toml",
        "check",
        "lint-skill",
        "--checkpoint",
        "pre-push",
        "--gate",
        "skills/demo/SKILL.md",
    ];
    for args in [&writing, &skill] {
        let out = run_osf(&repo.dir, &home, args);
        assert!(
            out.status.success(),
            "gate reenabled a disabled rule: {}",
            output_text(&out)
        );
        assert!(
            output_text(&out).contains("0 error(s), 0 warning(s)"),
            "disabled finding leaked: {}",
            output_text(&out)
        );
    }
    repo.write("guide.md", "Fixed in #125 today.\n");
    repo.commit("Add ordinary reference");
    let out = run_osf(&repo.dir, &home, &writing);
    assert_eq!(
        out.status.code(),
        Some(1),
        "gate accepted poisoned retained rule: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("bare-reference"));
}

/// Sets two audited rules to error in both sections, raises one kept rule
/// for prose, and turns the same kept rule off for skills.
const POISON: &str = "[writing.levels]\narrow = \"error\"\nskill-first-person = \"error\"\nsemicolon = \"error\"\n[skill.levels]\nskill-first-person = \"error\"\narrow = \"error\"\nsemicolon = \"off\"\n";
const ARROW_DOC: &str = "Input -> output.\n";
const SEMICOLON_DOC: &str = "Run the check; then stop.\n";
const ARROW_SKILL: &str = "---\nname: demo\ndescription: I can check a folder when the user wants a health check.\n---\n\nRun this skill to check a folder.\n\n1. Map input -> output for each file.\n\nStop when every file has been mapped.\n";
const SEMICOLON_SKILL: &str = "---\nname: demo\ndescription: Use when checking a folder.\n---\n\nRun this skill to check a folder.\n\n1. Check the folder; note any large file.\n\nStop when every file has been checked.\n";

fn run(repo: &TempRepo, home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = run_osf(&repo.dir, home, args);
    (out.status.code(), output_text(&out))
}

/// One moon task per checkpoint and gate state, each running the built binary.
fn write_lint_tasks(repo: &TempRepo) {
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    let task = |name: &str, check: &str, checkpoint: &str, gate: &str, inputs: &str, tag: &str| {
        format!(
            "  {name}:\n    command: '\"{bin}\" check {check} --checkpoint {checkpoint}{gate}'\n    inputs: [{inputs}]\n    tags: [{tag}]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
        )
    };
    let skill_inputs = "'/**/SKILL.md', '/**/skills/**/*'";
    let tasks = [
        task(
            "lint-writing",
            "lint-writing",
            "pre-push",
            "",
            "'/**/*.md'",
            "osf-pre-push",
        ),
        task(
            "lint-skill",
            "lint-skill",
            "pre-push",
            "",
            skill_inputs,
            "osf-pre-push",
        ),
        task(
            "lint-writing-gate",
            "lint-writing",
            "pull-request",
            " --gate",
            "'/**/*.md'",
            "osf-pull-request",
        ),
        task(
            "lint-skill-gate",
            "lint-skill",
            "pull-request",
            " --gate",
            skill_inputs,
            "osf-pull-request",
        ),
    ]
    .concat();
    repo.write(".osf/moon.yml", &format!("language: rust\ntasks:\n{tasks}"));
}

#[test]
fn a_config_cannot_reenable_an_audited_rule_on_an_ungated_path() {
    let repo = TempRepo::new("audit-ungated-off");
    let home = isolated_home("audit-ungated-off");
    repo.write("osf.toml", POISON);
    repo.write("guide.md", ARROW_DOC);
    repo.write("skills/demo/SKILL.md", ARROW_SKILL);
    repo.commit("Add prose and a skill");
    let raw = osf::lints::skill::lint_skill(
        &repo.dir.join("skills/demo"),
        &osf::config::SkillConfig::default(),
        &load_known_names(&[], None).expect("names load"),
        &WritingConfig::default(),
    )
    .expect("skill reads");
    assert!(
        raw.iter().any(|f| f.finding.rule == "skill-first-person"),
        "the fixture no longer trips the raw detector: {:?}",
        raw.iter().map(|f| f.finding.rule).collect::<Vec<_>>()
    );
    let cases: [(&str, &[&str]); 4] = [
        (
            "osf lint writing: 0 error(s)",
            &["lint", "writing", "--format", "human", "guide.md"],
        ),
        (
            "osf lint skill: 0 error(s)",
            &["lint", "skill", "--format", "human", "skills/demo"],
        ),
        (
            "osf check lint-writing: 0 error(s)",
            &[
                "check",
                "lint-writing",
                "--checkpoint",
                "pre-push",
                "guide.md",
            ],
        ),
        (
            "osf check lint-skill: 0 error(s)",
            &[
                "check",
                "lint-skill",
                "--checkpoint",
                "pre-push",
                "skills/demo/SKILL.md",
            ],
        ),
    ];
    for (summary, args) in cases {
        let (code, text) = run(&repo, &home, args);
        assert_eq!(code, Some(0), "{args:?}: {text}");
        assert!(
            text.contains(summary),
            "{args:?} printed no summary: {text}"
        );
        assert!(
            text.contains("disabled from enforcement:") && text.contains("arrow"),
            "{args:?} did not report the enforced policy: {text}"
        );
        for rule in ["[arrow]", "[skill-first-person]"] {
            assert!(!text.contains(rule), "{args:?} enforced {rule}: {text}");
        }
    }
}

#[test]
fn a_config_cannot_reenable_an_audited_rule_through_the_checkpoint_runner() {
    let repo = TempRepo::new("audit-verify-off");
    let home = isolated_home("audit-verify-off");
    repo.write("osf.toml", POISON);
    write_lint_tasks(&repo);
    let base = repo.commit("Add the checks");
    repo.write("guide.md", ARROW_DOC);
    repo.write("skills/demo/SKILL.md", ARROW_SKILL);
    repo.commit("Add prose and a skill");
    let (code, text) = run(
        &repo,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(code, Some(0), "{text}");
    // A skipped or not-run task prints its name too, so the word after it decides.
    for task in ["osf:lint-writing", "osf:lint-skill"] {
        assert!(
            text.contains(&format!("{task}: passed")),
            "{task} did not pass: {text}"
        );
    }
}

#[test]
fn a_kept_rule_raised_to_error_fails_its_task_in_the_checkpoint_runner() {
    let repo = TempRepo::new("audit-verify-raise");
    let home = isolated_home("audit-verify-raise");
    repo.write("osf.toml", POISON);
    write_lint_tasks(&repo);
    let base = repo.commit("Add the checks");
    repo.write("guide.md", SEMICOLON_DOC);
    repo.commit("Add prose");
    let (code, text) = run(
        &repo,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("osf:lint-writing: failed"),
        "the raised rule did not fail its task: {text}"
    );
}

#[test]
fn a_config_that_raises_a_kept_rule_applies_on_ungated_writing_paths_and_not_on_gated_ones() {
    let repo = TempRepo::new("audit-kept-raise");
    let home = isolated_home("audit-kept-raise");
    repo.write("osf.toml", POISON);
    write_lint_tasks(&repo);
    let base = repo.commit("Add the checks");
    repo.write("guide.md", SEMICOLON_DOC);
    repo.commit("Add prose");
    let ungated: [&[&str]; 2] = [
        &["lint", "writing", "--format", "human", "guide.md"],
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pre-push",
            "guide.md",
        ],
    ];
    for args in ungated {
        let (code, text) = run(&repo, &home, args);
        assert_eq!(
            code,
            Some(1),
            "{args:?} ignored the kept rule's level: {text}"
        );
        assert!(text.contains("[semicolon]"), "{args:?}: {text}");
    }
    let gated: [&[&str]; 2] = [
        &["lint", "writing", "--gate", "--format", "human", "guide.md"],
        &[
            "check",
            "lint-writing",
            "--checkpoint",
            "pull-request",
            "--gate",
            "guide.md",
        ],
    ];
    for args in gated {
        let (code, text) = run(&repo, &home, args);
        assert_eq!(code, Some(0), "{args:?} honoured the config: {text}");
        assert!(
            text.contains("[semicolon]") && text.contains("0 error(s)"),
            "{args:?} lost the kept finding at its compiled level: {text}"
        );
    }
    let args = ["verify", "--checkpoint", "pre-push", "--base", &base];
    let (code, text) = run(&repo, &home, &args);
    assert_eq!(
        code,
        Some(1),
        "pre-push ignored the kept rule's level: {text}"
    );
    let args = ["verify", "--checkpoint", "pull-request", "--base", &base];
    let (code, text) = run(&repo, &home, &args);
    assert_eq!(code, Some(0), "the gate honoured the config: {text}");
    assert!(
        text.contains("osf:lint-writing-gate: passed"),
        "gate did not pass: {text}"
    );
}

#[test]
fn a_config_that_lowers_a_kept_rule_applies_on_ungated_skill_paths_and_not_on_gated_ones() {
    let repo = TempRepo::new("audit-kept-lower");
    let home = isolated_home("audit-kept-lower");
    repo.write("osf.toml", POISON);
    repo.write("skills/demo/SKILL.md", SEMICOLON_SKILL);
    repo.commit("Add a skill");
    let ungated: [&[&str]; 2] = [
        &["lint", "skill", "--format", "human", "skills/demo"],
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "pre-push",
            "skills/demo/SKILL.md",
        ],
    ];
    for args in ungated {
        let (code, text) = run(&repo, &home, args);
        assert_eq!(code, Some(0), "{args:?} ignored the lowered level: {text}");
        assert!(text.contains(": 0 error(s)"), "{args:?}: {text}");
        assert!(!text.contains("[semicolon]"), "{args:?}: {text}");
    }
    let gated: [&[&str]; 2] = [
        &[
            "lint",
            "skill",
            "--gate",
            "--format",
            "human",
            "skills/demo",
        ],
        &[
            "check",
            "lint-skill",
            "--checkpoint",
            "pull-request",
            "--gate",
            "skills/demo/SKILL.md",
        ],
    ];
    for args in gated {
        let (code, text) = run(&repo, &home, args);
        assert_eq!(code, Some(1), "{args:?} honoured the config: {text}");
        assert!(text.contains("[semicolon]"), "{args:?}: {text}");
    }
}

/// A session id no other run, test or process shares.
fn unique_session(name: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    format!("{name}-{}-{nanos}", std::process::id())
}

/// Runs a hook with its bounce counter and advice file under `state`, the
/// test's own folder, instead of the shared system temp folder.
fn run_hook(
    repo: &TempRepo,
    home: &Path,
    state: &Path,
    args: &[&str],
    event: &serde_json::Value,
) -> (Option<i32>, String, String) {
    let state = state.to_str().expect("state folder is UTF-8");
    let env = [("TMPDIR", state), ("TEMP", state), ("TMP", state)];
    let out = common::run_osf_stdin(&repo.dir, home, &env, args, &event.to_string());
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_stop_check_names_dropped_audited_findings_only_when_the_message_passes() {
    let repo = TempRepo::new("audit-stop-output");
    let home = isolated_home("audit-stop-output");
    let state = TempDir::new("audit-stop-state");
    let stop = |session: &str, message: &str| {
        let session = unique_session(session);
        let event = serde_json::json!({ "session_id": session, "last_assistant_message": message });
        run_hook(&repo, &home, &state, &["hook", "stop"], &event)
    };
    let (code, _, stderr) = stop("audit-stop-pass", "Input -> output.");
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stderr.contains("osf hook writing policy: dropped 1 finding(s) from audited rules: arrow"),
        "the drop was not reported: {stderr}"
    );
    let (code, _, stderr) = stop("audit-stop-clean", "The check passed.");
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        !stderr.contains("writing policy"),
        "a clean check printed a policy line: {stderr}"
    );
    let (code, _, stderr) = stop("audit-stop-refuse", "Fixed in #125 today. Input -> output.");
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("bare-reference"), "{stderr}");
    assert!(
        !stderr.contains("writing policy"),
        "a refusal carried a policy line: {stderr}"
    );
}

#[test]
fn the_prompt_hook_gives_the_concrete_writing_instructions() {
    let repo = TempRepo::new("audit-prompt-output");
    let home = isolated_home("audit-prompt-output");
    let state = TempDir::new("audit-prompt-state");
    let event = serde_json::json!({ "session_id": unique_session("audit-prompt") });
    let (code, stdout, stderr) = run_hook(&repo, &home, &state, &["hook", "prompt"], &event);
    assert_eq!(code, Some(0), "{stderr}");
    for phrase in [
        "`, not X` or `, never X` tail",
        "owner/repo#N",
        "`nobody` or `everyone`",
    ] {
        assert!(stdout.contains(phrase), "reminder lost {phrase}: {stdout}");
    }
    assert!(!stdout.contains("disabled"), "{stdout}");
}

#[test]
fn lint_skill_gate_refuses_the_flags_that_loosen_it() {
    let repo = TempRepo::new("audit-skill-gate-flags");
    let home = isolated_home("audit-skill-gate-flags");
    repo.write("skills/demo/SKILL.md", SEMICOLON_SKILL);
    repo.write("names.txt", "Demo\n");
    for flag in [
        &["--known-names", "names.txt"][..],
        &["--overview-max-words", "10"][..],
        &["--overview-max-paragraphs", "1"][..],
    ] {
        let mut args = vec!["lint", "skill", "--gate"];
        args.extend_from_slice(flag);
        args.push("skills/demo");
        let (code, text) = run(&repo, &home, &args);
        assert_eq!(code, Some(2), "{args:?}: {text}");
        assert!(text.contains("cannot be used with"), "{args:?}: {text}");
    }
}

#[test]
fn gate_fixtures_still_evaluate_disabled_raw_detectors() {
    let repo = TempRepo::new("audit-fixture-policy");
    let home = isolated_home("audit-fixture-policy");
    repo.write("base.txt", "base\n");
    repo.commit("Add base file");
    repo.write(
        "tests/fixtures/quantity.md",
        "Count 12 axes over 3 rounds in 41 minutes.\n\n<!-- osf-expect numbers-in-prose -->\n",
    );
    repo.commit("Add detector fixture");
    let args = [
        "check",
        "lint-writing",
        "--checkpoint",
        "pre-push",
        "--gate",
        "tests/fixtures/quantity.md",
    ];
    let out = run_osf(&repo.dir, &home, &args);
    assert!(
        out.status.success(),
        "raw fixture not evaluated: {}",
        output_text(&out)
    );
    repo.write(
        "tests/fixtures/quantity.md",
        "Count the axes.\n\n<!-- osf-expect numbers-in-prose -->\n",
    );
    repo.commit("Break detector fixture");
    let out = run_osf(&repo.dir, &home, &args);
    assert_eq!(
        out.status.code(),
        Some(1),
        "fixture mismatch hidden: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("expectation-missing"));
}

#[test]
fn explain_preserves_disabled_rule_purpose_and_reports_enforcement_status() {
    let repo = TempRepo::new("audit-explain-policy");
    let home = isolated_home("audit-explain-policy");
    for rule in ["numbers-in-prose", "skill-description-no-trigger"] {
        let out = run_osf(&repo.dir, &home, &["explain", rule]);
        assert!(out.status.success(), "disabled rule definition lost");
        let text = output_text(&out);
        assert!(
            text.contains("disabled by default"),
            "disabled status missing: {text}"
        );
        assert!(text.contains("### Why it is bad"), "purpose lost: {text}");
    }
}

#[test]
fn clean_enforcement_reports_what_was_disabled() {
    let repo = TempRepo::new("audit-coverage-policy");
    let home = isolated_home("audit-coverage-policy");
    repo.write("ordinary.md", "The check passed.\n");
    for args in [
        vec!["lint", "writing", "--format", "human", "ordinary.md"],
        vec!["lint", "writing", "--format", "json", "ordinary.md"],
    ] {
        let out = run_osf(&repo.dir, &home, &args);
        let text = output_text(&out);
        assert!(out.status.success(), "{text}");
        assert!(
            text.contains("disabled from enforcement:"),
            "silent coverage omission: {text}"
        );
        assert!(
            text.contains("numbers-in-prose"),
            "disabled coverage missing: {text}"
        );
    }
}

#[test]
fn required_reference_description_is_not_a_parenthetical_violation() {
    let known = load_known_names(&[], None).expect("names load");
    let text = "See [repo#125 (fix the slow loading path)](https://example.com/125).";
    let raw = osf::lints::writing::lint_writing(
        text,
        &known,
        &WritingConfig::default(),
        Context::Document,
        false,
        true,
    );
    assert!(
        !raw.iter().any(|f| f.rule == "parenthetical"),
        "contradictory label finding: {raw:?}"
    );
    assert!(osf::lints::rule_meta("parenthetical").is_none());
}

#[test]
fn manual_shape_retains_individual_detectors_without_the_redundant_vote() {
    let known = load_known_names(&[], None).expect("names load");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills/manual-shaped");
    let findings = osf::lints::skill::lint_skill(
        &dir,
        &osf::config::SkillConfig::default(),
        &known,
        &WritingConfig::default(),
    )
    .expect("skill reads");
    assert!(
        !findings
            .iter()
            .any(|f| f.finding.rule == "skill-reads-as-manual"),
        "redundant composite still reported"
    );
    for id in [
        "skill-first-section-is-overview",
        "skill-descriptive-over-imperative",
    ] {
        assert!(
            findings.iter().any(|f| f.finding.rule == id),
            "disabled detector lost: {id}"
        );
    }
    assert!(osf::lints::rule_meta("skill-reads-as-manual").is_none());
}
