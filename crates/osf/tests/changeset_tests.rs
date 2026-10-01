//! Integration tests for `osf changeset tests`: a throwaway git repository
//! with a base and a head commit, and the test summary read by parsing
//! both, never by building or running either one.

mod common;

use common::TempRepo;
use osf::changeset_tests::{render, summarize, GroupBody};

fn base_repo(name: &str) -> TempRepo {
    TempRepo::new(name)
}

#[test]
fn added_changed_and_removed_tests_are_counted_and_named() {
    let repo = base_repo("added-changed-removed");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[cfg(test)]\n\
         mod tests {\n\
         \x20   #[test]\n\
         \x20   fn a_kept_test() { assert_eq!(1, 1); }\n\
         \n\
         \x20   #[test]\n\
         \x20   fn a_removed_test() {}\n\
         }\n",
    );
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[cfg(test)]\n\
         mod tests {\n\
         \x20   #[test]\n\
         \x20   fn a_kept_test() { assert_eq!(2, 2); }\n\
         \n\
         \x20   #[test]\n\
         \x20   fn an_added_test() {}\n\
         }\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.added, 1);
    assert_eq!(summary.changed, 1);
    assert_eq!(summary.removed, 1);

    let group = summary.groups.first().expect("one group");
    assert_eq!(group.crate_name, "osf");
    assert_eq!(group.file, "crates/osf/src/thing.rs");
    let GroupBody::Tests {
        removed,
        added,
        changed,
    } = &group.body
    else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    assert_eq!(
        removed.first().map(|t| t.name.as_str()),
        Some("tests::a_removed_test")
    );
    assert_eq!(
        added.first().map(|t| t.name.as_str()),
        Some("tests::an_added_test")
    );
    assert_eq!(
        changed.first().map(|t| t.description.as_str()),
        Some("a kept test")
    );
}

#[test]
fn a_doc_comment_is_the_description() {
    let repo = base_repo("doc-comment");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "/// Checks that doc comments become the description.\n\
         #[test]\n\
         fn t() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { added, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    let test = added.first().expect("one added test");
    assert_eq!(
        test.description,
        "Checks that doc comments become the description."
    );
}

#[test]
fn with_no_doc_comment_the_name_is_turned_into_words() {
    let repo = base_repo("name-only");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_missing_block_is_started() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { added, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    let test = added.first().expect("one added test");
    assert_eq!(test.name, "a_missing_block_is_started");
    assert_eq!(test.description, "a missing block is started");
}

#[test]
fn a_test_inside_a_cfg_test_module_is_found_by_its_module_path() {
    let repo = base_repo("cfg-test-module");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn nested() {}\n}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { added, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    let test = added.first().expect("one added test");
    assert_eq!(test.name, "tests::nested");
}

#[test]
fn an_async_test_attribute_counts_as_a_test() {
    let repo = base_repo("async-test");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[tokio::test]\nasync fn an_async_test() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.added, 1);
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { added, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    assert_eq!(
        added.first().map(|t| t.name.as_str()),
        Some("an_async_test")
    );
}

#[test]
fn adding_an_ignore_attribute_with_no_other_edit_counts_as_changed() {
    let repo = base_repo("added-ignore-attribute");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_test() { assert!(true); }\n",
    );
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\n#[ignore]\nfn a_test() { assert!(true); }\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.added, 0, "{:?}", summary.groups);
    assert_eq!(summary.removed, 0, "{:?}", summary.groups);
    assert_eq!(summary.changed, 1, "{:?}", summary.groups);
}

#[test]
fn editing_only_the_doc_comment_counts_as_changed() {
    let repo = base_repo("edited-doc-comment");
    repo.write(
        "crates/osf/src/thing.rs",
        "/// Checks the first thing.\n#[test]\nfn a_test() {}\n",
    );
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "/// Checks the second thing.\n#[test]\nfn a_test() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.added, 0, "{:?}", summary.groups);
    assert_eq!(summary.removed, 0, "{:?}", summary.groups);
    assert_eq!(summary.changed, 1, "{:?}", summary.groups);
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { changed, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    assert_eq!(
        changed.first().map(|t| t.description.as_str()),
        Some("Checks the second thing.")
    );
}

#[test]
fn an_unparsable_file_is_reported_unparsed_not_guessed_at() {
    let repo = base_repo("unparsable");
    repo.write("crates/osf/src/thing.rs", "#[test]\nfn ok_fn() {}\n");
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "fn broken( {{{ not rust at all\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    assert!(matches!(group.body, GroupBody::Unparsed));
    assert_eq!(summary.added, 0);
    assert_eq!(summary.changed, 0);
    assert_eq!(summary.removed, 0);
}

#[test]
fn a_file_under_tests_fixtures_is_skipped_entirely() {
    let repo = base_repo("fixture-skip");
    repo.write(
        "crates/osf/tests/fixtures/sample.rs",
        "#[test]\nfn f() {}\n",
    );
    let base = repo.commit("base");
    repo.write(
        "crates/osf/tests/fixtures/sample.rs",
        "#[test]\nfn g() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert!(summary.groups.is_empty(), "{:?}", summary.groups);
    assert_eq!((summary.added, summary.changed, summary.removed), (0, 0, 0));
}

#[test]
fn a_test_added_only_on_the_base_branch_is_not_reported_by_this_change() {
    let repo = base_repo("merge-base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_shared_test() {}\n",
    );
    repo.commit("common ancestor");

    repo.git(&["checkout", "-b", "feature"]);
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_shared_test() {}\n\n#[test]\nfn a_pr_test() {}\n",
    );
    let head = repo.commit("adds a test on the pull request branch");

    repo.git(&["checkout", "main"]);
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_shared_test() {}\n\n#[test]\nfn a_base_only_test() {}\n",
    );
    let base = repo.commit("adds an unrelated test on the base branch");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.added, 1, "{:?}", summary.groups);
    assert_eq!(summary.removed, 0, "{:?}", summary.groups);
    let group = summary.groups.first().expect("one group");
    let GroupBody::Tests { added, removed, .. } = &group.body else {
        panic!("expected a Tests body, got {:?}", group.body);
    };
    assert_eq!(added.first().map(|t| t.name.as_str()), Some("a_pr_test"));
    assert!(removed.is_empty(), "{:?}", group.body);
}

#[test]
fn a_renamed_test_file_is_compared_as_one_file_not_a_delete_plus_an_add() {
    let repo = base_repo("renamed-test-file");
    let original = "#[test]\nfn a_kept_test() { assert_eq!(1, 1); }\n";
    repo.write("crates/osf/src/old_name.rs", original);
    let base = repo.commit("base");

    std::fs::remove_file(repo.dir.join("crates/osf/src/old_name.rs")).expect("remove old file");
    let extended = format!("{original}\n#[test]\nfn an_added_test() {{}}\n");
    repo.write("crates/osf/src/new_name.rs", &extended);
    let head = repo.commit("rename and extend");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    assert_eq!(summary.removed, 0, "{:?}", summary.groups);
    assert_eq!(summary.added, 1, "{:?}", summary.groups);
    assert_eq!(summary.changed, 0, "{:?}", summary.groups);
    let group = summary.groups.first().expect("one group");
    assert_eq!(group.file, "crates/osf/src/new_name.rs");
}

#[test]
fn a_non_rust_test_file_is_named_not_yet_supported() {
    let repo = base_repo("non-rust");
    repo.write("service/tests/test_thing.py", "def test_old(): pass\n");
    let base = repo.commit("base");
    repo.write("service/tests/test_thing.py", "def test_new(): pass\n");
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    assert_eq!(group.file, "service/tests/test_thing.py");
    assert!(matches!(group.body, GroupBody::Unsupported));
}

#[test]
fn a_prefix_named_test_file_at_the_repository_root_is_named_not_yet_supported() {
    let repo = base_repo("root-prefix-test-file");
    repo.write("test_thing.py", "def test_old(): pass\n");
    let base = repo.commit("base");
    repo.write("test_thing.py", "def test_new(): pass\n");
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    assert_eq!(group.file, "test_thing.py");
    assert!(matches!(group.body, GroupBody::Unsupported));
}

#[test]
fn a_file_outside_crates_groups_under_the_workspace_root() {
    let repo = base_repo("workspace-root");
    repo.write("src/lib.rs", "\n");
    let base = repo.commit("base");
    repo.write("src/lib.rs", "#[test]\nfn t() {}\n");
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let group = summary.groups.first().expect("one group");
    assert_eq!(group.crate_name, "(workspace root)");
}

#[test]
fn render_lists_removed_then_added_then_changed_by_description_alone() {
    let repo = base_repo("render-order");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_kept_test() { assert_eq!(1, 1); }\n\n#[test]\nfn a_removed_test() {}\n",
    );
    let base = repo.commit("base");
    repo.write(
        "crates/osf/src/thing.rs",
        "#[test]\nfn a_kept_test() { assert_eq!(2, 2); }\n\n#[test]\nfn an_added_test() {}\n",
    );
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let text = render(&summary);
    assert!(text.starts_with("**Tests**: 1 added, 1 changed, 1 removed"));
    let removed_at = text.find("removed `a_removed_test`").expect("removed line");
    let added_at = text.find("added: an added test").expect("added line");
    let changed_at = text.find("changed: a kept test").expect("changed line");
    assert!(removed_at < added_at, "{text}");
    assert!(added_at < changed_at, "{text}");
    assert!(!text.contains("an_added_test"), "{text}");
    assert!(!text.contains("changed: `a_kept_test`"), "{text}");
}

#[test]
fn an_added_test_with_no_readable_description_names_the_file_instead_of_the_test() {
    let repo = base_repo("added-no-description");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write("crates/osf/src/thing.rs", "#[test]\nfn ___() {}\n");
    let head = repo.commit("head");

    let summary = summarize(&repo.dir, &base, &head).expect("summarize runs");
    let text = render(&summary);
    assert!(text.contains("added: no description (in crates/osf/src/thing.rs)"));
    assert!(!text.contains("___"), "{text}");
}
