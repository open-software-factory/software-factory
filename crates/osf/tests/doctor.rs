//! The doctor command's argument handling: a bad check name is a usage
//! error, one check can run alone, JSON is available, and a broken config
//! exits non-zero.

use std::process::Command;

fn osf(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.args(args);
    // The CI sets OSF_CONFIG=osf.toml relative to the repository root, but a
    // test runs from the package root, so that path would not resolve.
    cmd.env_remove("OSF_CONFIG");
    cmd
}

#[test]
fn an_unknown_check_name_exits_two_and_names_the_choices() {
    let output = osf(&["doctor", "--check", "nope"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no check named nope"), "{stderr}");
}

#[test]
fn the_osf_check_runs_alone() {
    let output = osf(&["doctor", "--check", "osf"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(": osf - "), "{stdout}");
    for other in ["settings", "agents", "agent-binaries", "hooks", "container"] {
        assert!(!stdout.contains(&format!(": {other} - ")), "{stdout}");
    }
}

#[test]
fn json_prints_one_array_for_the_named_check() {
    let config = std::env::temp_dir().join(format!("osf-doctor-clean-{}", std::process::id())); // osf: temp-dir allowed, an empty config the settings check can load
    std::fs::write(&config, "").expect("write config");
    let output = osf(&[
        "--config",
        config.to_str().expect("path is text"),
        "doctor",
        "--format",
        "json",
        "--check",
        "settings",
    ])
    .output()
    .expect("runs osf");
    std::fs::remove_file(&config).ok();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_start().starts_with('['), "{stdout}");
    assert!(stdout.contains("\"name\": \"settings\""), "{stdout}");
    assert!(stdout.contains("\"status\": \"ok\""), "{stdout}");
}

#[test]
fn a_broken_config_fails_the_settings_check() {
    let path = std::env::temp_dir().join(format!("osf-doctor-config-{}", std::process::id())); // osf: temp-dir allowed, a throwaway config that must fail to load
    std::fs::write(&path, "[nope]\nkey = 1\n").expect("write config");
    let output = osf(&[
        "--config",
        path.to_str().expect("path is text"),
        "doctor",
        "--check",
        "settings",
    ])
    .output()
    .expect("runs osf");
    std::fs::remove_file(&path).ok();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fail: settings"), "{stdout}");
}
