//! Rule-level tests for `osf scan`, as integration tests rather than unit
//! tests inside `src/scan/mod.rs`: each one needs a realistic bad-pattern
//! sample to prove a rule fires on it, and this repository excludes no test
//! file from its own `osf scan` run, so every sample here is built at run
//! time instead of sitting in this file's source text as a literal match.

mod common;

use common::{coauthor_trailer, foreign_reference, home_path, session_link, windows_user_path};
use osf::config::ScanConfig;
use osf::scan::{rule_meta, Rules};
use osf_lint_core::{Context, Finding, Level};

fn rules() -> Rules {
    Rules::build(ScanConfig::default()).expect("empty config builds")
}

fn rule_ids(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

#[test]
fn a_session_link_fires() {
    let text = format!("See {} for the discussion.\n", session_link("01AbCdEf"));
    let found = rules().scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-session-link"]);
    let f = found.first().expect("one finding");
    assert_eq!(f.level, Level::Error);
}

#[test]
fn plain_text_has_no_session_link_finding() {
    let found = rules().scan_text("A plain sentence with no link at all.\n", Context::Document);
    assert!(rule_ids(&found).is_empty());
}

#[test]
fn a_coauthor_trailer_fires() {
    let text = format!(
        "Fix the bug.\n\n{}\n",
        coauthor_trailer("Someone", "someone@example.com")
    );
    let found = rules().scan_text(&text, Context::Commit);
    assert_eq!(rule_ids(&found), vec!["scan-coauthor-trailer"]);
    let f = found.first().expect("one finding");
    assert_eq!(f.line, 3);
}

#[test]
fn a_normal_commit_message_has_no_coauthor_finding() {
    let found = rules().scan_text("Fix the bug.\n", Context::Commit);
    assert!(rule_ids(&found).is_empty());
}

#[test]
fn a_windows_user_path_fires() {
    let text = format!("See {} for the file.", windows_user_path("pat"));
    let found = rules().scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-local-path"]);
}

#[test]
fn a_home_path_fires() {
    let text = format!("The file lives at {}.\n", home_path("pat"));
    let found = rules().scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-local-path"]);
}

#[test]
fn a_repository_relative_path_has_no_local_path_finding() {
    let found = rules().scan_text(
        "The file lives at crates/osf/src/scan/mod.rs.\n",
        Context::Document,
    );
    assert!(rule_ids(&found).is_empty());
}

#[test]
fn a_foreign_reference_fires_when_an_owner_is_configured() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let text = format!(
        "See {} for the fix.\n",
        foreign_reference("other-org", "tools", 42)
    );
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-foreign-reference"]);
}

#[test]
fn a_reference_to_the_configured_owner_does_not_fire() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let text = format!(
        "See {} for the fix.\n",
        foreign_reference("acme", "tools", 42)
    );
    let found = rules.scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty());
}

/// With no project owner configured, the rule must never fire: the
/// tool has no way to tell a foreign reference from the project's own.
#[test]
fn foreign_reference_never_fires_with_no_project_owner_configured() {
    let text = format!(
        "See {} for the fix.\n",
        foreign_reference("other-org", "tools", 42)
    );
    let found = rules().scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty());
}

#[test]
fn a_denylisted_name_fires_with_no_excerpt() {
    let cfg = ScanConfig {
        denylist: vec!["SecretCode".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let found = rules.scan_text("The plan mentions SecretCode by name.\n", Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-denied-name"]);
    let f = found.first().expect("one finding");
    assert_eq!(f.excerpt, "");
}

#[test]
fn text_with_no_denylisted_name_has_no_finding() {
    let cfg = ScanConfig {
        denylist: vec!["SecretCode".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let found = rules.scan_text("Nothing sensitive here.\n", Context::Document);
    assert!(rule_ids(&found).is_empty());
}

/// The denylist match must never appear in the message, the excerpt, the
/// rendered human line, the JSON line, or the SARIF report: every one of
/// this crate's output formats.
#[test]
fn the_denylisted_text_never_appears_in_any_output_format() {
    let secret = "TotallyASecretOrgName";
    let cfg = ScanConfig {
        denylist: vec![secret.to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let text = format!("Some notes mention {secret} in passing.\n");
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-denied-name"]);
    let finding = found.first().expect("one finding").clone();

    assert!(!finding.excerpt.contains(secret));
    assert!(!finding.message.contains(secret));

    let rendered = finding.render("notes.txt", finding.level);
    assert!(!rendered.contains(secret), "{rendered}");

    let json = finding.to_json("notes.txt", finding.level);
    assert!(!json.contains(secret), "{json}");

    let tool = osf_lint_core::ToolInfo {
        name: "osf",
        version: "0.1.0",
        information_uri: "https://example.com",
    };
    let sarif = osf_lint_core::to_sarif(&[("notes.txt".to_string(), found)], &tool);
    let sarif_text = serde_json::to_string(&sarif).expect("sarif serialises");
    assert!(!sarif_text.contains(secret), "{sarif_text}");
}

#[test]
fn every_scan_rule_id_has_metadata() {
    for id in [
        "scan-session-link",
        "scan-coauthor-trailer",
        "scan-local-path",
        "scan-foreign-reference",
        "scan-denied-name",
    ] {
        assert!(rule_meta(id).is_some(), "no metadata for {id}");
    }
}

#[test]
fn every_finding_is_an_error_and_points_at_explain() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        denylist: vec!["Secret".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(cfg).expect("config builds");
    let text = format!(
        "See {} and {} and Secret and {} and\n{}\n",
        session_link("1"),
        foreign_reference("other", "repo", 1),
        windows_user_path("pat"),
        coauthor_trailer("X", "x@example.com")
    );
    let found = rules.scan_text(&text, Context::Commit);
    assert_eq!(found.len(), 5, "{found:?}");
    for f in &found {
        assert_eq!(f.level, Level::Error, "{}", f.rule);
        assert!(f.message.contains("osf explain"), "{}", f.message);
    }
}

#[test]
fn a_binary_file_is_recognised() {
    assert!(osf::scan::is_binary(b"\x00\x01\x02"));
    assert!(!osf::scan::is_binary(b"plain text"));
}
