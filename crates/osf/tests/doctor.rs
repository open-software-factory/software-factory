//! The doctor command's argument handling: a bad check name is a usage
//! error, and one check can run alone.

use std::process::Command;

#[test]
fn an_unknown_check_name_exits_two_and_names_the_choices() {
    let output = Command::new(env!("CARGO_BIN_EXE_osf"))
        .args(["doctor", "--check", "nope"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no check named nope"), "{stderr}");
}

#[test]
fn the_osf_check_runs_alone_and_succeeds() {
    let output = Command::new(env!("CARGO_BIN_EXE_osf"))
        .args(["doctor", "--check", "osf"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ok: osf"), "{stdout}");
}
