//! Rule-level tests for `osf scan`, as integration tests rather than unit
//! tests inside `src/scan/mod.rs`: each one needs a realistic bad-pattern
//! sample to prove a rule fires on it, and this repository excludes no test
//! file from its own `osf scan` run, so every sample here is built at run
//! time instead of sitting in this file's source text as a literal match.
//!
//! The corpus below has one case per supported agent, per session place,
//! and per platform. The tests that walk `osf::agents::AGENTS` are the
//! ones that matter: a rule cannot pass them while knowing one agent and
//! not the others. No test here reaches the network: every repository\n//! states its own visibility in its configuration, and the visibility\n//! changes no finding.

mod common;

use common::{
    agent_state_path, coauthor_trailer, file_url, foreign_reference, home_path, mac_home_path,
    network_share_path, public_config, public_repo, root_home_path, session_link, session_link_for,
    windows_forward_path, windows_user_path, wsl_user_path, TempRepo,
};
use osf::agents::{self, AGENTS};
use osf::config::ScanConfig;
use osf::repository::Visibility;
use osf::scan::{rule_meta, Rules};
use osf_lint_core::{Context, Finding, Level};

/// Rules for a public repository with a remote, the common case. Each
/// call gets its own repository, since tests run in parallel and a shared
/// directory would be removed by one test while another still used it.
fn rules() -> (TempRepo, Rules) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (repo, cfg) = public_repo(&format!("scan-rules-{n}"), "acme");
    let rules = Rules::build(&repo.dir, &cfg).expect("builds");
    (repo, rules)
}

fn rule_ids(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

// --- session links: one case per agent that has them -------------------

#[test]
fn a_session_link_fires_for_every_agent_with_hosted_sessions() {
    let (_repo, rules) = rules();
    for agent in AGENTS {
        let Some((host, path)) = agent.hosted() else {
            continue;
        };
        let link = session_link_for(host, path, "01AbCdEf");
        let text = format!("See {link} for the discussion.\n");
        let found = rules.scan_text(&text, Context::Document);
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
fn a_session_link_is_caught_without_its_scheme_inside_punctuation_and_with_a_query() {
    let (_repo, rules) = rules();
    let bare = session_link("01AbCdEf").replace("https://", "");
    let with_query = format!("{}?tab=files#top", session_link("01AbCdEf"));
    let text = format!("(see {bare}) and [{bare}] and {with_query}.\n");
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.iter().all(|f| f.rule == "scan-session-link"));
}

/// The same path on another host is not a session link. Comparing hosts
/// is what parsing the URL buys over matching a substring.
#[test]
fn the_same_path_on_another_host_is_not_a_session_link() {
    let (_repo, rules) = rules();
    let (_, path) = AGENTS
        .iter()
        .find_map(osf::agents::Agent::hosted)
        .expect("an agent has hosted sessions");
    let text = format!("See https://example.com{path}01AbCdEf and example.org{path}2.\n");
    let found = rules.scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty(), "{found:?}");
}

#[test]
fn a_configured_session_link_prefix_is_added_never_substituted() {
    let (repo, mut cfg) = public_repo("scan-links-configured", "acme");
    cfg.session_links = vec!["agents.example.test/run/".to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("config builds");
    let configured = "https://agents.example.test/run/9f";
    let built_in = session_link("01");
    let text = format!("{configured} and {built_in}\n");
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.rule == "scan-session-link"));
}

#[test]
fn a_configured_prefix_without_a_path_is_refused_not_ignored() {
    let (repo, mut cfg) = public_repo("scan-links-bad", "acme");
    cfg.session_links = vec!["agents.example.test".to_string()];
    let err = Rules::build(&repo.dir, &cfg)
        .err()
        .expect("a prefix with no path is an error");
    assert!(err.contains("agents.example.test"), "{err}");
}

#[test]
fn plain_text_has_no_session_link_finding() {
    let (_repo, rules) = rules();
    let found = rules.scan_text("A plain sentence with no link at all.\n", Context::Document);
    assert!(rule_ids(&found).is_empty());
}

// --- agent state paths: one case per agent, per session place -----------

#[test]
fn a_state_path_fires_for_every_agent_at_every_session_place() {
    let (_repo, rules) = rules();
    for agent in AGENTS {
        for dir in agent.state_dirs {
            for place in agent.session_paths {
                let path = agent_state_path(dir, place);
                let text = format!("The transcript is at {path}.\n");
                let found = rules.scan_text(&text, Context::Document);
                assert_eq!(
                    rule_ids(&found),
                    vec!["scan-agent-state-path"],
                    "{}: {dir}/{place}",
                    agent.name
                );
                let f = found.first().expect("one finding");
                assert!(
                    f.message.contains(agent.name),
                    "{}: {}",
                    agent.name,
                    f.message
                );
                assert_eq!(f.excerpt, path, "{}: {dir}/{place}", agent.name);
            }
        }
    }
}

#[test]
fn an_agents_committed_configuration_file_is_not_a_state_path() {
    let (_repo, rules) = rules();
    for agent in AGENTS {
        for dir in agent.state_dirs {
            let text = format!("Edit {dir}/settings.json and {dir}/hooks.json.\n");
            let found = rules.scan_text(&text, Context::Document);
            assert!(
                rule_ids(&found).is_empty(),
                "{}: {dir} -> {found:?}",
                agent.name
            );
        }
    }
}

// --- local paths: one case per platform, plain and as a file address ----

fn platform_cases() -> Vec<(&'static str, String)> {
    vec![
        ("Windows", windows_user_path("pat")),
        ("Windows", windows_forward_path("pat")),
        ("network share", network_share_path("fileserver", "home")),
        ("Subsystem", wsl_user_path("pat")),
        ("Linux", home_path("pat")),
        ("root", root_home_path()),
        ("macOS", mac_home_path("pat")),
    ]
}

#[test]
fn a_user_path_fires_on_every_platform() {
    let (_repo, rules) = rules();
    for (platform, path) in platform_cases() {
        let text = format!("See {path} for the file.\n");
        let found = rules.scan_text(&text, Context::Document);
        assert_eq!(
            rule_ids(&found),
            vec!["scan-local-path"],
            "{platform}: {path}"
        );
        let f = found.first().expect("one finding");
        assert!(f.message.contains(platform), "{platform}: {}", f.message);
        assert_eq!(f.excerpt, path, "{platform}");
    }
}

#[test]
fn a_user_path_inside_a_file_address_fires_on_every_platform() {
    let (_repo, rules) = rules();
    for (platform, path) in platform_cases() {
        if platform == "network share" {
            continue;
        }
        let address = file_url(&path);
        let text = format!("Open {address} in a browser.\n");
        let found = rules.scan_text(&text, Context::Document);
        assert_eq!(
            rule_ids(&found),
            vec!["scan-local-path"],
            "{platform}: {address}"
        );
        assert!(
            found.first().is_some_and(|f| f.message.contains(platform)),
            "{platform}: {found:?}"
        );
    }
    let share = format!("{}{}", "file://", "fileserver/home/pat/notes.md");
    let found = rules.scan_text(&format!("Open {share}.\n"), Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-local-path"], "{found:?}");
    assert!(
        found
            .first()
            .is_some_and(|f| f.message.contains("network share")),
        "{found:?}"
    );
}

/// A drive path also contains the home-directory shape. It must be
/// reported once, as Windows, not twice.
#[test]
fn a_windows_path_is_reported_once_not_also_as_a_home_directory() {
    let (_repo, rules) = rules();
    let text = format!("See {}.\n", windows_forward_path("pat"));
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 1, "{found:?}");
    let first = found.first().expect("one finding");
    assert!(first.message.contains("Windows"), "{}", first.message);
}

#[test]
fn portable_forms_a_system_folder_and_a_web_path_are_not_flagged() {
    let (_repo, rules) = rules();
    let tilde = "~";
    let var = "$HOME";
    let win_var = "%USERPROFILE%";
    let programs = format!("{}{}{}", "C:", r"\", r"Program Files\tool\tool.exe");
    let web = format!("https://example.com/{}/guide/", "Users");
    let text = format!(
        "Config at {tilde}/.osf/config.toml, {var}/.osf/config.toml, {win_var}\\.osf\\config.toml, {programs}, {web}\n"
    );
    let found = rules.scan_text(&text, Context::Document);
    assert!(rule_ids(&found).is_empty(), "{found:?}");
}

#[test]
fn a_repository_relative_path_has_no_local_path_finding() {
    let (_repo, rules) = rules();
    let found = rules.scan_text(
        "The file lives at crates/osf/src/scan/mod.rs.\n",
        Context::Document,
    );
    assert!(rule_ids(&found).is_empty());
}

// --- the repository: owner and visibility, with no configuration --------

/// With nothing configured, the owner comes from the git remote, in any
/// of the three common URL shapes.
#[test]
fn the_owner_is_read_from_the_git_remote_with_no_configuration() {
    for (shape, url) in [
        ("https", "https://github.com/acme/tools.git"),
        ("ssh", "ssh://git@github.com/acme/tools"),
        ("scp", "git@github.com:acme/tools.git"),
    ] {
        let repo = TempRepo::new(&format!("scan-owner-{shape}"));
        repo.git(&["remote", "add", "origin", url]);
        let rules = Rules::build(&repo.dir, &public_config()).expect("builds");
        assert_eq!(rules.owner(), Some("acme"), "{shape}");
        assert_eq!(
            rules.repository().host.as_deref(),
            Some("github.com"),
            "{shape}"
        );
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

#[test]
fn a_configured_owner_wins_over_the_remote() {
    let (repo, mut cfg) = public_repo("scan-owner-configured", "acme");
    cfg.project_owner = "other-org".to_string();
    let rules = Rules::build(&repo.dir, &cfg).expect("builds");
    assert_eq!(rules.owner(), Some("other-org"));
}

/// With no owner from either source the rule does not run, and the scan
/// says so rather than passing quietly.
#[test]
fn with_no_owner_from_anywhere_the_rule_does_not_run_and_says_so() {
    let repo = TempRepo::new("scan-owner-none");
    let built = Rules::build(&repo.dir, &public_config()).expect("builds");
    assert_eq!(built.owner(), None);
    assert!(
        built
            .notes()
            .iter()
            .any(|n| n.contains("scan-foreign-reference did not run")),
        "{:?}",
        built.notes()
    );
    let text = format!("See {}.\n", foreign_reference("other-org", "tools", 42));
    assert!(rule_ids(&built.scan_text(&text, Context::Document)).is_empty());
}

/// A private repository changes nothing. None of this belongs in a private
/// repository either, and a private one becomes public more often than
/// anyone plans, so every finding keeps its full weight.
#[test]
fn a_private_repository_lowers_no_finding() {
    let (repo, mut cfg) = public_repo("scan-private", "acme");
    cfg.repository_visibility = "private".to_string();
    cfg.denylist = vec!["Secret".to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("builds");
    assert_eq!(rules.repository().visibility, Visibility::Private);
    let text = format!(
        "See {} and {} and Secret.\n",
        session_link("1"),
        windows_user_path("pat")
    );
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 3, "{found:?}");
    for f in &found {
        assert_eq!(f.level, Level::Error, "{}", f.rule);
        assert!(!f.message.contains("public repository"), "{}", f.message);
    }
}
/// An unknown visibility is reported in a note and changes nothing: the
/// findings are the same errors they would be anywhere.
#[test]
fn an_unknown_visibility_is_reported_and_changes_nothing() {
    let repo = TempRepo::new("scan-visibility-unknown");
    let rules = Rules::build(&repo.dir, &ScanConfig::default()).expect("builds");
    assert_eq!(rules.repository().visibility, Visibility::Unknown);
    assert!(
        rules
            .notes()
            .iter()
            .any(|n| n.contains("visibility is unknown")),
        "{:?}",
        rules.notes()
    );
    let text = format!("See {}.\n", session_link("1"));
    let found = rules.scan_text(&text, Context::Document);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found.first().map(|f| f.level), Some(Level::Error));
}
#[test]
fn a_visibility_word_the_tool_does_not_know_is_reported_not_accepted() {
    let (repo, mut cfg) = public_repo("scan-visibility-bad", "acme");
    cfg.repository_visibility = "secret".to_string();
    let rules = Rules::build(&repo.dir, &cfg).expect("builds");
    assert_eq!(rules.repository().visibility, Visibility::Unknown);
    assert!(
        rules.notes().iter().any(|n| n.contains("secret")),
        "{:?}",
        rules.notes()
    );
}

// --- denylist -----------------------------------------------------------

#[test]
fn a_denylisted_name_fires_with_no_excerpt() {
    let (repo, mut cfg) = public_repo("scan-deny", "acme");
    cfg.denylist = vec!["SecretCode".to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("config builds");
    let found = rules.scan_text("The plan mentions SecretCode by name.\n", Context::Document);
    assert_eq!(rule_ids(&found), vec!["scan-denied-name"]);
    let f = found.first().expect("one finding");
    assert_eq!(f.excerpt, "");
}

#[test]
fn text_with_no_denylisted_name_has_no_finding() {
    let (repo, mut cfg) = public_repo("scan-deny-clean", "acme");
    cfg.denylist = vec!["SecretCode".to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("config builds");
    let found = rules.scan_text("Nothing sensitive here.\n", Context::Document);
    assert!(rule_ids(&found).is_empty());
}

/// The denylist match must never appear in the message, the excerpt, the
/// rendered human line, the JSON line, or the SARIF report: every one of
/// this crate's output formats.
#[test]
fn the_denylisted_text_never_appears_in_any_output_format() {
    let secret = "TotallyASecretOrgName";
    let (repo, mut cfg) = public_repo("scan-deny-output", "acme");
    cfg.denylist = vec![secret.to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("config builds");
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
    let (_repo, rules) = rules();
    let text = format!(
        "Fix the bug.\n\n{}\n",
        coauthor_trailer("Someone", "someone@example.com")
    );
    let found = rules.scan_text(&text, Context::Commit);
    assert_eq!(rule_ids(&found), vec!["scan-coauthor-trailer"]);
    let f = found.first().expect("one finding");
    assert_eq!(f.line, 3);
}

#[test]
fn a_normal_commit_message_has_no_coauthor_finding() {
    let (_repo, rules) = rules();
    let found = rules.scan_text("Fix the bug.\n", Context::Commit);
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
    let (repo, mut cfg) = public_repo("scan-all-rules", "acme");
    cfg.denylist = vec!["Secret".to_string()];
    let rules = Rules::build(&repo.dir, &cfg).expect("config builds");
    let first = AGENTS.first().expect("an agent");
    let (first_dir, first_place) = (
        first.state_dirs.first().expect("a state dir"),
        first.session_paths.first().expect("a session place"),
    );
    let text = format!(
        "See {} and {} and Secret and {} and {} and\n{}\n",
        session_link("1"),
        foreign_reference("other", "repo", 1),
        windows_user_path("pat"),
        agent_state_path(first_dir, first_place),
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
