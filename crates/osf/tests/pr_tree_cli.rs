//! CLI-level cases for `osf pr tree render`: the change is read from git, so
//! added, modified, deleted and renamed paths all show with their counts.

mod common;

use common::{isolated_home, run_osf, TempRepo};

#[test]
fn render_reads_status_and_counts_from_git() {
    let repo = TempRepo::new("pr-tree-render");
    repo.write_lines("src/keep.rs", 10);
    repo.write("src/gone.rs", "a\nb\nc\nd\ne\n");
    repo.write_lines("docs/old-name.md", 20);
    let base = repo.commit("base");
    repo.write_lines("src/keep.rs", 14);
    repo.write("src/new.rs", "n1\nn2\nn3\nn4\nn5\nn6\nn7\n");
    repo.git(&["rm", "-q", "src/gone.rs"]);
    repo.git(&["mv", "docs/old-name.md", "docs/new-name.md"]);
    repo.commit("head");

    let home = isolated_home("pr-tree-render");
    let output = run_osf(
        &repo.dir,
        &home,
        &["pr", "tree", "render", "--base", &base, "--head", "HEAD"],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("<summary><b>All 4 files</b>: 3 code, 1 docs, build and infra</summary>"),
        "{stdout}"
    );
    assert!(stdout.contains("`keep.rs` | M |"), "{stdout}");
    assert!(stdout.contains("`new.rs` | A |"), "{stdout}");
    assert!(stdout.contains("`gone.rs` | D |"), "{stdout}");
    assert!(stdout.contains("`new-name.md` | R |"), "{stdout}");
    assert!(stdout.contains(r"\texttt{+7}"), "{stdout}");
    assert!(stdout.contains(r"\texttt{-5}"), "{stdout}");
}

#[test]
fn render_with_no_difference_says_so_and_fails() {
    let repo = TempRepo::new("pr-tree-render-empty");
    repo.write("a.rs", "x\n");
    let base = repo.commit("only commit");
    let home = isolated_home("pr-tree-render-empty");
    let output = run_osf(&repo.dir, &home, &["pr", "tree", "render", "--base", &base]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no files differ"), "{stderr}");
}

#[test]
fn render_with_an_unknown_base_fails() {
    let repo = TempRepo::new("pr-tree-render-bad-base");
    repo.write("a.rs", "x\n");
    repo.commit("only commit");
    let home = isolated_home("pr-tree-render-bad-base");
    let output = run_osf(
        &repo.dir,
        &home,
        &["pr", "tree", "render", "--base", "no-such-ref"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
