//! Golden cases for `osf pr tree render`: the chips, the size bars and the
//! layout come out exactly as in the description samples this format follows.

use osf::pr_tree::{render, FileChange, Status};

fn change(path: &str, status: Status, added: u64, removed: u64) -> FileChange {
    FileChange {
        path: path.to_string(),
        status,
        added,
        removed,
    }
}

const ADD_4: &str =
    r"$\rlap{\color{#033a16}{\rule[-5px]{26px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+4}}$";
const REMOVE_2: &str =
    r"$\rlap{\color{#67060c}{\rule[-5px]{26px}{18px}}}\hspace{5px}\color{#ffdcd7}{\texttt{-2}}$";
const SIZE_4_2: &str =
    r"$\color{#033a16}{\rule{53px}{8px}}\hspace{2px}\color{#67060c}{\rule{27px}{8px}}$";

fn four_workflows() -> Vec<FileChange> {
    [
        "build-deploy-core-api-beta.yml",
        "build-deploy-org-function-beta.yml",
        "build-deploy-project-function-beta.yml",
        "build-deploy-search-function-beta.yml",
    ]
    .iter()
    .map(|name| change(&format!(".github/workflows/{name}"), Status::Modified, 4, 2))
    .collect()
}

#[test]
fn a_small_change_is_one_flat_table_with_the_directory_shown_once() {
    let out = render(&four_workflows());
    let expected = format!(
        "<details>\n\
<summary><b>All 4 files</b>: 4 docs, build and infra</summary>\n\
\n\
**Docs, build and infra** (4 files, +16 −8)\n\
\n\
| File | Change | Added | Removed | Size |\n\
|:--|:-:|--:|--:|:--|\n\
| <sub>.github/workflows/</sub><br>`build-deploy-core-api-beta.yml` | M | {ADD_4} | {REMOVE_2} | {SIZE_4_2} |\n\
| `build-deploy-org-function-beta.yml` | M | {ADD_4} | {REMOVE_2} | {SIZE_4_2} |\n\
| `build-deploy-project-function-beta.yml` | M | {ADD_4} | {REMOVE_2} | {SIZE_4_2} |\n\
| `build-deploy-search-function-beta.yml` | M | {ADD_4} | {REMOVE_2} | {SIZE_4_2} |\n\
\n\
<sub>A added · M modified · D deleted · R renamed. Size: log scale of lines changed.</sub>\n\
\n\
</details>\n"
    );
    assert_eq!(out, expected);
}

fn sixteen_files() -> Vec<FileChange> {
    let m = Status::Modified;
    vec![
        change("Core/Infrastructure/A.cs", m, 300, 10),
        change("Core/Infrastructure/B.cs", m, 30, 1),
        change("Core/Infrastructure/C.cs", m, 5, 1),
        change("Core/Host/A.cs", m, 40, 19),
        change("Core/Host/B.cs", m, 4, 0),
        change("Core/Domain/A.cs", m, 10, 2),
        change("Core/Domain/B.cs", m, 6, 0),
        change("Core/Domain/C.cs", m, 6, 0),
        change("Core/Domain/D.cs", m, 6, 0),
        change("Core/Domain/E.cs", m, 6, 0),
        change("Core/Domain/F.cs", m, 6, 0),
        change("Core/Application/A.cs", m, 27, 0),
        change("Tests/Core/ATests.cs", m, 1000, 6),
        change("Tests/Core/BTests.cs", m, 300, 0),
        change("Tests/Core/CTests.cs", m, 100, 0),
        change("Tests/Core/DTests.cs", m, 42, 0),
    ]
}

#[test]
fn a_big_change_is_one_row_per_component_biggest_first_with_the_sample_widths() {
    let out = render(&sixteen_files());
    assert!(
        out.starts_with("<details>\n<summary><b>All 16 files</b>: 12 code, 4 tests</summary>\n\n**Code** (12 files, +446 −33)\n\n| Component | Files | Added | Removed | Size |\n|:--|--:|--:|--:|:--|\n"),
        "{out}"
    );
    let infra = r"| `Core/Infrastructure` | 3 | $\rlap{\color{#033a16}{\rule[-5px]{42px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+335}}$ | $\rlap{\color{#67060c}{\rule[-5px]{34px}{18px}}}\hspace{5px}\color{#ffdcd7}{\texttt{-12}}$ | $\color{#033a16}{\rule{62px}{8px}}\hspace{2px}\color{#67060c}{\rule{2px}{8px}}$ |";
    let host = r"| `Core/Host` | 2 | $\rlap{\color{#033a16}{\rule[-5px]{34px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+44}}$ | $\rlap{\color{#67060c}{\rule[-5px]{34px}{18px}}}\hspace{5px}\color{#ffdcd7}{\texttt{-19}}$ | $\color{#033a16}{\rule{32px}{8px}}\hspace{2px}\color{#67060c}{\rule{14px}{8px}}$ |";
    let domain = r"| `Core/Domain` | 6 | $\rlap{\color{#033a16}{\rule[-5px]{34px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+40}}$ | $\rlap{\color{#67060c}{\rule[-5px]{26px}{18px}}}\hspace{5px}\color{#ffdcd7}{\texttt{-2}}$ | $\color{#033a16}{\rule{39px}{8px}}\hspace{2px}\color{#67060c}{\rule{2px}{8px}}$ |";
    let application = r"| `Core/Application` | 1 | $\rlap{\color{#033a16}{\rule[-5px]{34px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+27}}$ | · | $\color{#033a16}{\rule{37px}{8px}}$ |";
    let tests = r"| `Tests/Core` | 4 | $\rlap{\color{#033a16}{\rule[-5px]{58px}{18px}}}\hspace{5px}\color{#aff5b4}{\texttt{+1,442}}$ | $\rlap{\color{#67060c}{\rule[-5px]{26px}{18px}}}\hspace{5px}\color{#ffdcd7}{\texttt{-6}}$ | $\color{#033a16}{\rule{80px}{8px}}$ |";
    let code_rows = format!("{infra}\n{host}\n{domain}\n{application}\n");
    assert!(out.contains(&code_rows), "{out}");
    assert!(
        out.contains(&format!(
            "**Tests** (4 files, +1,442 −6)\n\n| Component | Files | Added | Removed | Size |\n|:--|--:|--:|--:|:--|\n{tests}\n"
        )),
        "{out}"
    );
    assert!(
        out.contains("<sub>Grouped by component because the PR changes more than 15 files. Size: log scale of lines changed.</sub>"),
        "{out}"
    );
    assert!(
        !out.contains("`A.cs`"),
        "a grouped table lists no files: {out}"
    );
}

#[test]
fn fifteen_files_are_flat_and_sixteen_are_grouped() {
    let many = |n: usize| -> Vec<FileChange> {
        (0..n)
            .map(|i| change(&format!("src/f{i:02}.rs"), Status::Modified, 3, 1))
            .collect()
    };
    assert!(render(&many(15)).contains("| File | Change |"));
    assert!(render(&many(16)).contains("| Component | Files |"));
}

#[test]
fn files_fall_into_code_tests_and_docs_in_that_order() {
    let out = render(&[
        change("README.md", Status::Modified, 1, 0),
        change("tests/it.rs", Status::Added, 9, 0),
        change("src/lib.rs", Status::Modified, 2, 1),
    ]);
    let code = out.find("**Code** (1 file, +2 −1)").expect("code header");
    let tests = out.find("**Tests** (1 file, +9 −0)").expect("tests header");
    let docs = out
        .find("**Docs, build and infra** (1 file, +1 −0)")
        .expect("docs header");
    assert!(code < tests && tests < docs, "{out}");
    assert!(
        out.contains(
            "<summary><b>All 3 files</b>: 1 code, 1 tests, 1 docs, build and infra</summary>"
        ),
        "{out}"
    );
}

#[test]
fn a_chip_is_as_wide_as_its_text_and_a_zero_is_a_dot() {
    let out = render(&[change("a.rs", Status::Added, 1_234, 0)]);
    assert!(out.contains(r"\rule[-5px]{58px}{18px}"), "{out}");
    assert!(out.contains(r"\texttt{+1,234}"), "{out}");
    let row = out
        .lines()
        .find(|l| l.starts_with("| `a.rs`"))
        .expect("the file row");
    assert!(row.contains("| · |"), "{row}");
}

#[test]
fn a_rename_shows_its_new_path_and_the_letter_r() {
    let out = render(&[change("docs/new-name.md", Status::Renamed, 0, 0)]);
    assert!(
        out.contains("| <sub>docs/</sub><br>`new-name.md` | R | · | · | · |"),
        "{out}"
    );
}

#[test]
fn a_deleted_file_has_only_a_removed_bar() {
    let out = render(&[change("src/old.rs", Status::Deleted, 0, 40)]);
    let row = out
        .lines()
        .find(|l| l.contains("`old.rs`"))
        .expect("the file row");
    assert!(row.contains("| D | · |"), "{row}");
    assert!(
        row.ends_with(r"| $\color{#67060c}{\rule{80px}{8px}}$ |"),
        "{row}"
    );
}

#[test]
fn nothing_changed_renders_nothing() {
    assert_eq!(render(&[]), "");
}

#[test]
fn the_same_changes_give_the_same_bytes() {
    let first = render(&sixteen_files());
    let second = render(&sixteen_files());
    assert_eq!(first.as_bytes(), second.as_bytes());
}

#[test]
fn only_the_four_allowed_maths_commands_appear() {
    let out = render(&sixteen_files());
    let commands: std::collections::BTreeSet<&str> = out
        .split('\\')
        .skip(1)
        .map(|rest| {
            let end = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            rest.get(..end).unwrap_or_default()
        })
        .collect();
    let expected: std::collections::BTreeSet<&str> = ["color", "rule", "rlap", "hspace", "texttt"]
        .into_iter()
        .collect();
    assert_eq!(commands, expected, "{out}");
}
