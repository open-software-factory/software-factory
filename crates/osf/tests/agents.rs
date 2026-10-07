//! `osf agents list`, and `osf hooks install --agents`: both read the one
//! agent list and the `[agents]` table of the current folder's `osf.toml`.

mod common;
use common::{isolated_home, run_osf, TempDir, TempRepo};

fn repo_with(name: &str, osf_toml: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write("osf.toml", osf_toml);
    repo
}

type Row = serde_json::Value;

/// `row`'s field `key`, never indexing.
fn field<'a>(row: &'a Row, key: &str) -> &'a Row {
    row.get(key)
        .unwrap_or_else(|| panic!("no field {key} in {row}"))
}

fn named<'a>(rows: &'a [Row], name: &str) -> &'a Row {
    rows.iter()
        .find(|r| field(r, "name") == name)
        .unwrap_or_else(|| panic!("no row for {name}"))
}

fn list_json(repo: &TempRepo, home: &std::path::Path) -> Vec<serde_json::Value> {
    let out = run_osf(&repo.dir, home, &["agents", "list", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON output");
    rows.as_array().expect("an array").clone()
}

#[test]
fn list_json_names_the_five_agents_with_the_defaults_from_the_list() {
    let home = isolated_home("agents-list-defaults");
    let repo = repo_with("agents-list-defaults", "");
    let rows = list_json(&repo, &home);
    let names: Vec<&str> = rows
        .iter()
        .map(|r| field(r, "name").as_str().expect("name"))
        .collect();
    assert_eq!(names, vec!["dsh", "omp", "opencode", "codex", "claude"]);
    assert!(rows
        .iter()
        .all(|r| field(r, "enabled") == true && field(r, "reviewer") == false));
    let builders: Vec<&str> = rows
        .iter()
        .filter(|r| field(r, "builder") == true)
        .map(|r| field(r, "name").as_str().expect("name"))
        .collect();
    assert_eq!(builders, vec!["dsh"]);
    let codex = named(&rows, "codex");
    assert_eq!(field(codex, "family"), "openai");
    assert_eq!(field(codex, "hooks"), ".codex/hooks.json");
    assert_eq!(
        field(codex, "command"),
        &serde_json::json!(["codex", "exec"])
    );
    assert!(field(codex, "model").is_null());
}

#[test]
fn list_shows_what_osf_toml_selects() {
    let home = isolated_home("agents-list-selected");
    let repo = repo_with(
        "agents-list-selected",
        "[agents]\nenabled = [\"dsh\", \"claude\"]\nreviewers = [\"claude\"]\n\n[agents.models]\nclaude = \"claude-sonnet-5\"\n",
    );
    let rows = list_json(&repo, &home);
    assert_eq!(field(named(&rows, "dsh"), "enabled"), true);
    assert_eq!(field(named(&rows, "codex"), "enabled"), false);
    assert_eq!(field(named(&rows, "claude"), "reviewer"), true);
    assert_eq!(field(named(&rows, "claude"), "model"), "claude-sonnet-5");
}

#[test]
fn list_gives_a_many_family_agent_the_family_of_its_model() {
    let home = isolated_home("agents-list-model-family");
    let repo = repo_with(
        "agents-list-model-family",
        "[agents]\nreviewers = [\"opencode\"]\n\n[agents.models]\nopencode = \"openrouter/qwen/qwen3-coder-next\"\n",
    );
    let rows = list_json(&repo, &home);
    assert_eq!(field(named(&rows, "opencode"), "family"), "qwen");
    assert_eq!(field(named(&rows, "omp"), "family"), "unknown");
    assert_eq!(field(named(&rows, "claude"), "family"), "anthropic");
}

#[test]
fn list_prints_a_table_with_one_row_per_agent() {
    let home = isolated_home("agents-list-human");
    let repo = repo_with("agents-list-human", "[agents]\nreviewers = [\"codex\"]\n");
    let out = run_osf(&repo.dir, &home, &["agents", "list"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 6, "{text}");
    let header = lines.first().expect("a header");
    assert!(
        header.starts_with("agent") && header.contains("hook settings"),
        "{text}"
    );
    let codex = lines
        .iter()
        .find(|l| l.starts_with("codex"))
        .expect("codex row");
    assert!(
        codex.contains("openai") && codex.contains(".codex/hooks.json"),
        "{text}"
    );
}

#[test]
fn an_unknown_agent_in_osf_toml_is_a_clear_error() {
    let home = isolated_home("agents-list-unknown");
    let repo = repo_with("agents-list-unknown", "[agents]\nreviewers = [\"pi\"]\n");
    let out = run_osf(&repo.dir, &home, &["agents", "list"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown agent \"pi\""), "{stderr}");
    assert!(
        stderr.contains("dsh, omp, opencode, codex, claude"),
        "{stderr}"
    );
}

#[test]
fn hooks_install_agents_writes_the_settings_of_each_enabled_agent_only() {
    let home = isolated_home("agents-hooks-install");
    let repo = repo_with(
        "agents-hooks-install",
        "[agents]\nenabled = [\"dsh\", \"claude\"]\n",
    );
    let root = TempDir::new("agents-hooks-install-root");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "hooks",
            "install",
            "--agents",
            "--root",
            &root.to_string_lossy(),
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let settings =
        std::fs::read_to_string(root.join(".claude/settings.json")).expect("claude file");
    let value: serde_json::Value = serde_json::from_str(&settings).expect("valid JSON");
    assert_eq!(
        value.pointer("/hooks/Stop/0/hooks/0/command"),
        Some(&serde_json::json!("osf hook stop"))
    );
    assert_eq!(
        value.pointer("/hooks/UserPromptSubmit/0/hooks/0/command"),
        Some(&serde_json::json!("osf hook prompt"))
    );
    assert!(root.join(".dsh/cordis.patch.yml").is_file());
    assert!(!root.join(".codex").exists(), "codex is not enabled");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(".claude/settings.json"), "{stdout}");
}

#[test]
fn hooks_install_agents_needs_a_root() {
    let home = isolated_home("agents-hooks-install-no-root");
    let repo = repo_with("agents-hooks-install-no-root", "");
    let out = run_osf(&repo.dir, &home, &["hooks", "install", "--agents"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--root"));
}
