//! Integration tests for assembling a review lens's context: one throwaway
//! git repository per case, so `osf::review_context::build` sees exactly
//! what a real change under review would look like.

mod common;

use common::{fake_forge_token, TempRepo};
use osf::lenses::{ContextInput, Criterion, Depth, Lens, Runs, SeverityGuide, Trigger};
use osf::review_context::{build, Sources};

/// A lens built only to carry a `context` list; its criteria and severity
/// guide are never read by `build`.
fn lens_with(context: Vec<ContextInput>) -> Lens {
    Lens {
        name: "test-lens".to_string(),
        summary: "a lens built for a review-context test".to_string(),
        criteria: vec![Criterion {
            id: "c1".to_string(),
            question: "q".to_string(),
        }],
        severity_guide: SeverityGuide {
            blocker: "b".to_string(),
            major: "m".to_string(),
            minor: "n".to_string(),
        },
        weight: 1.0,
        runs: Runs::Always,
        trigger: Trigger::default(),
        context,
    }
}

/// A repository with one committed change ready to diff against `origin/main`.
fn changed_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write("src/lib.rs", "fn one() {}\n");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/lib.rs", "fn one() {}\nfn two() {}\n");
    repo.commit("add a function");
    repo
}

#[test]
fn a_lens_with_no_work_item_file_fails_naming_work_item() {
    let repo = TempRepo::new("no-work-item");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    let lens = lens_with(vec![
        ContextInput::WorkItem,
        ContextInput::AcceptanceCriteria,
    ]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let err = build(&lens, Depth::Diff, &sources).expect_err("must fail");
    assert!(err.contains("work-item"), "{err}");
}

#[test]
fn a_work_item_with_no_acceptance_heading_fails_naming_acceptance_criteria() {
    let repo = TempRepo::new("no-acceptance-heading");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    let work_item = repo.dir.join("issue.md");
    std::fs::write(
        &work_item,
        "# Some work item\n\nA description with no acceptance section at all.\n",
    )
    .expect("work item writes");
    let lens = lens_with(vec![
        ContextInput::WorkItem,
        ContextInput::AcceptanceCriteria,
    ]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: Some(&work_item),
    };
    let err = build(&lens, Depth::Diff, &sources).expect_err("must fail");
    assert!(err.contains("acceptance-criteria"), "{err}");
}

#[test]
fn a_work_item_with_an_acceptance_heading_is_read() {
    let repo = TempRepo::new("acceptance-heading-present");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    let work_item = repo.dir.join("issue.md");
    std::fs::write(
        &work_item,
        "# Some work item\n\n## Done when\n- the thing happens\n\n## Notes\nirrelevant\n",
    )
    .expect("work item writes");
    let lens = lens_with(vec![ContextInput::AcceptanceCriteria]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: Some(&work_item),
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("the thing happens"), "{ctx}");
    assert!(!ctx.contains("irrelevant"), "{ctx}");
}

#[test]
fn diff_depth_includes_only_the_diff() {
    let repo = changed_repo("diff-only");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("## diff"), "{ctx}");
    assert!(
        !ctx.contains("## other files in the changed modules"),
        "{ctx}"
    );
    assert!(
        !ctx.contains("## files that name a changed symbol"),
        "{ctx}"
    );
}

#[test]
fn module_depth_includes_a_sibling_file() {
    let repo = TempRepo::new("module-depth-sibling");
    repo.write("src/lib.rs", "fn one() {}\n");
    repo.write("src/sibling.rs", "fn sibling_marker_content() {}\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/lib.rs", "fn one() {}\nfn two() {}\n");
    repo.commit("change lib");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Module, &sources).expect("builds");
    assert!(ctx.contains("src/sibling.rs"), "{ctx}");
    assert!(ctx.contains("sibling_marker_content"), "{ctx}");
}

#[test]
fn diff_and_callers_depth_includes_a_file_that_calls_a_changed_function() {
    let repo = TempRepo::new("diff-and-callers");
    repo.write("src/lib.rs", "pub fn helper_call_target() {}\n");
    repo.write("src/caller.rs", "fn uses_it() { helper_call_target(); }\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "src/lib.rs",
        "pub fn helper_call_target() { let _ = 1 + 1; }\n",
    );
    repo.commit("change the helper's body");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::DiffAndCallers, &sources).expect("builds");
    assert!(ctx.contains("src/caller.rs"), "{ctx}");
}

#[test]
fn entry_points_input_includes_a_caller_regardless_of_depth() {
    let repo = TempRepo::new("entry-points-caller");
    repo.write("src/lib.rs", "pub fn entry_target() {}\n");
    repo.write("src/caller.rs", "fn uses_it() { entry_target(); }\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/lib.rs", "pub fn entry_target() { let _ = 2 + 2; }\n");
    repo.commit("change the target's body");
    let lens = lens_with(vec![ContextInput::EntryPoints]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("src/caller.rs"), "{ctx}");
}

#[test]
fn a_diff_that_references_a_missing_decision_record_fails_naming_decision_records() {
    let repo = TempRepo::new("missing-decision-record");
    repo.write("src/lib.rs", "// no reference yet\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "src/lib.rs",
        "// see docs/architecture/decisions/0099-does-not-exist.md\n",
    );
    repo.commit("reference a missing decision record");
    let lens = lens_with(vec![ContextInput::DecisionRecords]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let err = build(&lens, Depth::Diff, &sources).expect_err("must fail");
    assert!(err.contains("decision-records"), "{err}");
}

#[test]
fn a_diff_that_touches_no_decision_record_is_not_an_error() {
    let repo = changed_repo("no-decision-record-touched");
    let lens = lens_with(vec![ContextInput::DecisionRecords]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("none touched or linked"), "{ctx}");
}

#[test]
fn a_diff_that_touches_an_existing_decision_record_includes_its_content() {
    let repo = TempRepo::new("touched-decision-record");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "docs/architecture/decisions/0001-example.md",
        "# 0001: an example decision\n\nBody text unique to this record.\n",
    );
    repo.commit("add a decision record");
    let lens = lens_with(vec![ContextInput::DecisionRecords]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("Body text unique to this record"), "{ctx}");
}

#[test]
fn no_architecture_docs_directory_fails_naming_architecture_docs() {
    let repo = changed_repo("no-architecture-docs");
    let lens = lens_with(vec![ContextInput::ArchitectureDocs]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let err = build(&lens, Depth::Diff, &sources).expect_err("must fail");
    assert!(err.contains("architecture-docs"), "{err}");
}

#[test]
fn architecture_docs_present_are_included() {
    let repo = TempRepo::new("architecture-docs-present");
    repo.write("README.md", "base\n");
    repo.write(
        "docs/architecture/overview.md",
        "# Overview\n\nA unique marker sentence for this test.\n",
    );
    repo.commit("base");
    repo.track_origin_main();
    repo.write("README.md", "base\nmore\n");
    repo.commit("touch the readme");
    let lens = lens_with(vec![ContextInput::ArchitectureDocs]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(
        ctx.contains("A unique marker sentence for this test"),
        "{ctx}"
    );
}

#[test]
fn every_rendered_path_is_forward_slash() {
    let repo = TempRepo::new("forward-slash-paths");
    repo.write("src/nested/dir/lib.rs", "fn one() {}\n");
    repo.write("src/nested/dir/sibling.rs", "fn two() {}\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/nested/dir/lib.rs", "fn one() {}\nfn changed() {}\n");
    repo.commit("change nested file");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Module, &sources).expect("builds");
    assert!(ctx.contains("src/nested/dir/sibling.rs"), "{ctx}");
    assert!(!ctx.contains('\\'), "{ctx}");
}

#[test]
fn a_fake_secret_in_a_changed_file_is_redacted_and_never_reaches_the_context() {
    let repo = TempRepo::new("redact-fake-secret");
    repo.write(
        "osf.toml",
        "[scan]\ndenylist = [\"sk-fake-secret-[0-9]+\"]\n",
    );
    repo.write("src/config.rs", "// nothing sensitive yet\n");
    repo.commit("base");
    repo.track_origin_main();
    let secret = "sk-fake-secret-90210";
    repo.write("src/config.rs", &format!("let leaked = \"{secret}\";\n"));
    repo.commit("accidentally add a secret");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(!ctx.contains(secret), "{ctx}");
    assert!(ctx.contains("redacted"), "{ctx}");
}

#[test]
fn a_very_large_diff_is_capped_with_a_marker_naming_what_was_left_out() {
    use std::fmt::Write as _;
    let repo = TempRepo::new("size-cap");
    repo.write("src/big.rs", "// base\n");
    repo.commit("base");
    repo.track_origin_main();
    let mut huge = String::new();
    for i in 0..20_000 {
        let _ = writeln!(huge, "// line {i}");
    }
    repo.write("src/big.rs", &huge);
    repo.commit("a very large change");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.len() < huge.len(), "{}", ctx.len());
    assert!(ctx.contains("truncated"), "{ctx}");
}

#[test]
fn a_built_in_secret_shape_is_redacted_with_no_configuration_at_all() {
    let repo = TempRepo::new("redact-built-in-secret");
    repo.write("src/config.rs", "// nothing sensitive yet\n");
    repo.commit("base");
    repo.track_origin_main();
    let secret = fake_forge_token("ghp_");
    repo.write("src/config.rs", &format!("let leaked = \"{secret}\";\n"));
    repo.commit("accidentally add a real-shaped token");
    let lens = lens_with(vec![ContextInput::Diff]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(!ctx.contains(&secret), "{ctx}");
    assert!(ctx.contains("redacted by scan-secret"), "{ctx}");
}

#[test]
fn a_huge_diff_is_cut_but_the_spec_and_acceptance_work_item_survives_whole() {
    use std::fmt::Write as _;
    let repo = TempRepo::new("size-cap-work-item-survives");
    repo.write("src/big.rs", "// base\n");
    repo.commit("base");
    repo.track_origin_main();

    let mut huge = String::new();
    while huge.len() < 780_000 {
        let _ = writeln!(huge, "// a line of a very large diff, repeated on purpose");
    }
    repo.write("src/big.rs", &huge);
    repo.commit("a very large change");

    let work_item = repo.dir.join("issue.md");
    let marker = "the exact thing this test looks for is present";
    std::fs::write(
        &work_item,
        format!("# A work item\n\n## Done when\n- {marker}\n"),
    )
    .expect("work item writes");

    let catalogue = osf::lenses::load(&repo.dir, None).expect("loads");
    let lens = catalogue
        .lenses
        .iter()
        .find(|l| l.name == "spec-and-acceptance")
        .expect("spec-and-acceptance is shipped");

    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: Some(&work_item),
    };
    let ctx = build(lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains(marker), "{ctx}");
    assert!(ctx.contains("truncated"), "{ctx}");
    assert!(ctx.len() < huge.len());
}

#[test]
fn a_work_item_bigger_than_the_cap_alone_is_could_not_run() {
    use std::fmt::Write as _;
    let repo = TempRepo::new("size-cap-required-too-big");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();

    let work_item = repo.dir.join("issue.md");
    let mut body = String::from("# A work item\n\n## Done when\n");
    while body.len() < 70_000 {
        let _ = writeln!(
            body,
            "- another acceptance line, padding this out on purpose"
        );
    }
    std::fs::write(&work_item, &body).expect("work item writes");

    let lens = lens_with(vec![
        ContextInput::WorkItem,
        ContextInput::AcceptanceCriteria,
    ]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: Some(&work_item),
    };
    let err = build(&lens, Depth::Diff, &sources).expect_err("must fail");
    assert!(
        err.contains("work-item") || err.contains("acceptance-criteria"),
        "{err}"
    );
    assert!(err.contains("cap"), "{err}");
}

#[test]
fn a_typescript_caller_is_found_for_entry_points() {
    let repo = TempRepo::new("entry-points-typescript");
    repo.write(
        "src/service.ts",
        "export function chargeCard() { return true; }\n",
    );
    repo.write(
        "src/caller.ts",
        "import { chargeCard } from './service';\nfunction run() { chargeCard(); }\n",
    );
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "src/service.ts",
        "export function chargeCard() { return false; }\n",
    );
    repo.commit("change the function body");
    let lens = lens_with(vec![ContextInput::EntryPoints]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("src/caller.ts"), "{ctx}");
}

#[test]
fn a_python_caller_is_found_for_entry_points() {
    let repo = TempRepo::new("entry-points-python");
    repo.write("app/billing.py", "def charge_card(): return True\n");
    repo.write(
        "app/caller.py",
        "from app.billing import charge_card\n\ndef run():\n    charge_card()\n",
    );
    repo.commit("base");
    repo.track_origin_main();
    repo.write("app/billing.py", "def charge_card(): return False\n");
    repo.commit("change the function body");
    let lens = lens_with(vec![ContextInput::EntryPoints]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("app/caller.py"), "{ctx}");
}

#[test]
fn an_unknown_extension_states_the_caller_search_is_unavailable() {
    let repo = TempRepo::new("entry-points-unknown-extension");
    repo.write("script.rb", "def charge_card\n  true\nend\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("script.rb", "def charge_card\n  false\nend\n");
    repo.commit("change the ruby method");
    let lens = lens_with(vec![ContextInput::EntryPoints]);
    let sources = Sources {
        root: &repo.dir,
        base: "origin/main",
        work_item: None,
    };
    let ctx = build(&lens, Depth::Diff, &sources).expect("builds");
    assert!(ctx.contains("unavailable"), "{ctx}");
    assert!(ctx.contains("rb"), "{ctx}");
}
