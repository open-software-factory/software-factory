//! The `osf forge pr open` contract without a host: the builder token is
//! required before any read, and the help names the flags the develop skill
//! passes.

use std::process::Command;

fn osf(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.args(args);
    cmd.env_remove("GH_TOKEN");
    cmd.env_remove("GITHUB_TOKEN");
    cmd
}

#[test]
fn opening_without_a_token_is_a_usage_error() {
    let output = osf(&[
        "forge",
        "pr",
        "open",
        "--head",
        "work/item-1",
        "--title",
        "A change",
        "--body",
        "body.md",
    ])
    .output()
    .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("builder app token"), "{stderr}");
}

#[test]
fn the_help_lists_the_develop_skill_flags() {
    let output = osf(&["forge", "pr", "open", "--help"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    for flag in ["--head", "--base", "--title", "--body", "--work-item"] {
        assert!(stdout.contains(flag), "{stdout}");
    }
}
