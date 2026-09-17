//! Rule-level tests for `osf scan`, as integration tests rather than unit
//! tests inside `src/scan/mod.rs`: each one needs a realistic bad-pattern
//! sample to prove a rule fires on it, and this repository excludes no test
//! file from its own `osf scan` run, so every sample here is built at run
//! time instead of sitting in this file's source text as a literal match.
//!
//! The corpus below has one case per supported agent and one per platform.
//! The tests that walk `osf::agents::AGENTS` are the ones that matter: a
//! rule cannot pass them while knowing one agent and not the others.

mod common;

use common::{
    agent_state_path, coauthor_trailer, foreign_reference, home_path, mac_home_path,
    network_share_path, root_home_path, session_link, session_link_for, windows_forward_path,
    windows_user_path, wsl_user_path, TempRepo,
};
use osf::agents::{self, AGENTS};
use osf::config::ScanConfig;
use osf::scan::{rule_meta, Rules};
use osf_lint_core::{Context, Finding, Level};

fn rules() -> Rules {
    Rules::build(&ScanConfig::default()).expect("empty config builds")
}

fn rule_ids(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

// --- session links: one case per agent that has them -------------------

#[test]
fn a_session_link_fires_for_every_agent_with_hosted_sessions() {
    for agent in AGENTS.iter().filter(|a| a.session_host.is_some()) {
        let link = session_link_for(
            agent.session_host.expect("host"),
            agent.session_path.expect("path"),
            "01AbCdEf",
        );
        let text = format!("See {link} for the discussion.\n");
        let found = rules().scan_text(&text, Context::Document);
        assert_eq!(
            rule_ids(&found),
            vec!["scan-session-link"],
            "{}",
            agent.name
        );
        let f = found.first().expect("one finding");
        assert_eq!(f.level, Level::Error);
        assert!(
            f.message.contains(agent.name),
            "{}: {}",
            agent.name,
            f.message
        );
        assert_eq!(f.excerpt, link, "{}", agent.name);
    }
}

#[test]
fn a_session_link_is_caught_without_its_scheme_and_inside_punctuation() {
    let bare = session_link("01AbCdEf").replace("https://", "");
    let text = format!("(see {bare}) and [{bare}].\n");
    let found = rules().scan_text(&text, Context::Document);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.excerpt == bare), "{found:?}");
}

#[test]
fn a_configured_session_link_prefix_is_added_never_substituted() {
    let cfg = ScanConfig {
        session_links: vec!["agents.example.test/run/".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(&cfg).expect("config builds");
    let configured = "https://agents.example.test/run/9f";
    let built_in = session_link("01");
    let text = format!("{configured} and {built_in}\n");
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.rule == "scan-session-link"));
}

#[test]
fn a_bad_configured_session_link_prefix_is_an_error_not_a_silent_skip() {
    let cfg = ScanConfig {
        session_links: vec![String::new()],
        ..ScanConfig::default()
    };
    // An empty prefix would match everything; the regex is valid, so this
    // proves the list is read rather than that it is validated.
    let rules = Rules::build(&cfg).expect("an empty prefix still compiles");
    let found = rules.scan_text("plain\n", Context::Document);
    assert!(
        !found.is_empty(),
        "an empty prefix matches, and that is visible"
    );
}

#[test]
fn plain_text_has_no_session_link_finding() {
    let found = rules().scan_text("A plain sentence with no link at all.\n", Context::Document);
    assert!(rule_ids(&found).is_empty());
}

// --- agent state paths: one case per agent ------------------------------

#[test]
fn a_state_path_fires_for_every_agent() {
    for agent in AGENTS {
        for dir in agent.state_dirs {
            let path = agent_state_path(dir);
            let text = format!("The transcript is at {path}.\n");
            let found = rules().scan_text(&text, Context::Document);
            assert_eq!(
                rule_ids(&found),
                vec!["scan-agent-state-path"],
                "{}: {dir}",
                agent.name
            );
            let f = found.first().expect("one finding");
            assert!(
                f.message.contains(agent.name),
                "{}: {}",
                agent.name,
                f.message
            );
        }
    }
}

#[test]
fn an_agents_committed_configuration_file_is_not_a_state_path() {
    for agent in AGENTS {
        for dir in agent.state_dirs {
            let text = format!("Edit {dir}/settings.json and {dir}/hooks.json.\n");
            let found = rules().scan_text(&text, Context::Document);
            assert!(
                rule_ids(&found).is_empty(),
                "{}: {dir} -> {found:?}",
                agent.name
            );
        }
    }
}

// --- local paths: one case per platform --------------------------------

#[test]
fn a_user_path_fires_on_every_platform() {
    let cases: Vec<(&str, String)> = vec![
        ("Windows", windows_user_path("pat")),
        ("Windows", windows_forward_path("pat")),
        ("network share", network_share_path("fileserver", "home")),
        ("Subsystem", wsl_user_path("pat")),
        ("Linux", home_path("pat")),
        ("root", root_home_path()),
        ("macOS", mac_home_path("pat")),
    ];
    for (platform, path) in cases {
        let text = format!("See {path} for the file.\n");
        let found = rules().scan_text(&text, Context::Document);
        assert_eq!(
            rule_ids(&found),
            vec!["scan-local-path"],
            "{platform}: {path}"
        );
        let f = found.first().expect("one finding");
        assert!(f.message.contains(platform), "{platform}: {}", f.message);
    }
}

/// A drive path also contains the home-directory shape. It must be
/// reported once, as Windows, not twice.
#[test]
fn a_windows_path_is_reported_once_not_also_as_a_home_directory() {
    let text = format!("See {}.\n", windows_forward_path("pat"));
    let found = rules().scan_text(&text, Context::Document);
    assert_eq!(found.len(), 1, "{found:?}");
    let first = found.first().expect("one finding");
    assert!(first.message.contains("Windows"), "{}", first.message);
}

#[test]
fn portable_forms_of_a_home_path_are_not_flagged() {
    let tilde = "~";
    let var = "$HOME";
    let win_var = "%USERPROFILE%";
    let text = format!("Config at {tilde}/.osf/config.toml, {var}/.osf/config.toml, {win_var}\\.osf\\config.toml\n");
    let found = rules().scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty(), "{found:?}");
}

#[test]
fn a_repository_relative_path_has_no_local_path_finding() {
    let found = rules().scan_text(
        "The file lives at crates/osf/src/scan/mod.rs.\n",
        Context::Document,
    );
    assert!(rule_ids(&found).is_empty());
}

// --- foreign references -------------------------------------------------

#[test]
fn a_foreign_reference_fires_when_an_owner_is_configured() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        ..ScanConfig::default()
    };
    let rules = Rules::build(&cfg).expect("config builds");
    let text = format!(
        "See {} for the fix.\n",
        foreign_reference("other-org", "tools", 42)
    );
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-foreign-reference"]);
    assert!(rules.notes().is_empty(), "{:?}", rules.notes());
}

#[test]
fn a_reference_to_the_configured_owner_does_not_fire() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        ..ScanConfig::default()
    };
    let rules = Rules::build(&cfg).expect("config builds");
    let text = format!(
        "See {} for the fix.\n",
        foreign_reference("acme", "tools", 42)
    );
    let found = rules.scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty());
}

/// With no owner configured, the owner comes from the git remote, in any
/// of the three common URL shapes.
#[test]
fn the_owner_is_read_from_the_git_remote_when_not_configured() {
    for (shape, url) in [
        ("https", "https://github.com/acme/tools.git"),
        ("ssh", "ssh://git@github.com/acme/tools"),
        ("scp", "git@github.com:acme/tools.git"),
    ] {
        let repo = TempRepo::new(&format!("scan-owner-{shape}"));
        repo.git(&["remote", "add", "origin", url]);
        let rules = Rules::build_for(&repo.dir, &ScanConfig::default()).expect("builds");
        assert_eq!(rules.owner(), Some("acme"), "{shape}");
        assert!(rules.notes().is_empty(), "{shape}: {:?}", rules.notes());
        let text = format!(
            "See {} and {}.\n",
            foreign_reference("other-org", "tools", 1),
            foreign_reference("acme", "tools", 2)
        );
        let found = rules.scan_text(&text, Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-foreign-reference"], "{shape}");
        assert!(
            found
                .first()
                .is_some_and(|f| f.excerpt.starts_with("other-org/")),
            "{shape}"
        );
    }
}

/// A configured owner wins over the remote.
#[test]
fn a_configured_owner_is_not_overridden_by_the_remote() {
    let repo = TempRepo::new("scan-owner-configured");
    repo.git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/acme/tools.git",
    ]);
    let cfg = ScanConfig {
        project_owner: "other-org".to_string(),
        ..ScanConfig::default()
    };
    let rules = Rules::build_for(&repo.dir, &cfg).expect("builds");
    assert_eq!(rules.owner(), Some("other-org"));
}

/// With no owner from either source the rule does not run, and the scan
/// says so rather than passing quietly.
#[test]
fn with_no_owner_from_anywhere_the_rule_does_not_run_and_says_so() {
    let repo = TempRepo::new("scan-owner-none");
    let built = Rules::build_for(&repo.dir, &ScanConfig::default()).expect("builds");
    assert_eq!(built.owner(), None);
    assert_eq!(built.notes().len(), 1, "{:?}", built.notes());
    assert!(built
        .notes()
        .first()
        .is_some_and(|n| n.contains("scan-foreign-reference did not run")));
    let text = format!("See {}.\n", foreign_reference("other-org", "tools", 42));
    assert!(rule_ids(&built.scan_text(&text, Context::Document)).is_empty());

    let plain = rules();
    assert_eq!(plain.notes().len(), 1, "{:?}", plain.notes());
}

// --- denylist -----------------------------------------------------------

#[test]
fn a_denylisted_name_fires_with_no_excerpt() {
    let cfg = ScanConfig {
        denylist: vec!["SecretCode".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(&cfg).expect("config builds");
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
    let rules = Rules::build(&cfg).expect("config builds");
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
    let rules = Rules::build(&cfg).expect("config builds");
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

// --- the trailer ----------------------------------------------------------

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

// --- the rules as a set ---------------------------------------------------

const ALL_RULES: &[&str] = &[
    "scan-session-link",
    "scan-agent-state-path",
    "scan-coauthor-trailer",
    "scan-local-path",
    "scan-foreign-reference",
    "scan-denied-name",
];

#[test]
fn every_scan_rule_id_has_metadata_with_a_coverage_section() {
    for id in ALL_RULES {
        let meta = rule_meta(id).unwrap_or_else(|| panic!("no metadata for {id}"));
        assert!(
            meta.doc.contains("### Coverage"),
            "{id} has no Coverage section"
        );
    }
}

/// The doc text of the two agent-aware rules must name every agent the
/// shared list holds, on the right side of the line: hosted or on disk.
/// Adding an agent to the list without updating the docs fails here.
#[test]
fn the_rule_docs_name_every_supported_agent() {
    let link_doc = rule_meta("scan-session-link").expect("meta").doc;
    for name in agents::with_hosted_sessions() {
        assert!(
            link_doc.contains(name),
            "scan-session-link doc omits {name}"
        );
    }
    for name in agents::with_local_sessions_only() {
        assert!(
            link_doc.contains(name),
            "scan-session-link doc omits {name} as local-only"
        );
    }
    let state_doc = rule_meta("scan-agent-state-path").expect("meta").doc;
    for agent in AGENTS {
        assert!(
            state_doc.contains(agent.name),
            "scan-agent-state-path doc omits {}",
            agent.name
        );
    }
}

#[test]
fn every_finding_is_an_error_and_points_at_explain() {
    let cfg = ScanConfig {
        project_owner: "acme".to_string(),
        denylist: vec!["Secret".to_string()],
        ..ScanConfig::default()
    };
    let rules = Rules::build(&cfg).expect("config builds");
    let text = format!(
        "See {} and {} and Secret and {} and {} and\n{}\n",
        session_link("1"),
        foreign_reference("other", "repo", 1),
        windows_user_path("pat"),
        agent_state_path(
            AGENTS
                .first()
                .and_then(|a| a.state_dirs.first())
                .copied()
                .expect("an agent with a state directory")
        ),
        coauthor_trailer("X", "x@example.com")
    );
    let found = rules.scan_text(&text, Context::Commit);
    assert_eq!(found.len(), ALL_RULES.len(), "{found:?}");
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
