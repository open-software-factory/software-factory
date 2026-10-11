//! The `adoption_mode` key of `osf.toml`, checked through the command line.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use osf::config::AdoptionMode;

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn an_unsupported_adoption_mode_stops_osf_naming_the_key_and_every_supported_value() {
    let repo = TempRepo::new("adoption-mode-bad");
    let home = isolated_home("adoption-mode-bad");
    repo.write("osf.toml", "adoption_mode = \"vendored\"\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["--config", "osf.toml", "config", "show"],
    );
    assert!(!out.status.success(), "{}", output_text(&out));
    let text = output_text(&out);
    assert!(text.contains(AdoptionMode::KEY), "{text}");
    for mode in AdoptionMode::ALL {
        assert!(text.contains(mode.name()), "{text}");
    }
}

#[test]
fn a_supported_adoption_mode_loads() {
    let repo = TempRepo::new("adoption-mode-good");
    let home = isolated_home("adoption-mode-good");
    repo.write("osf.toml", "adoption_mode = \"private-factory-repo\"\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["--config", "osf.toml", "config", "show"],
    );
    assert!(out.status.success(), "{}", output_text(&out));
}

#[test]
fn an_unsupported_adoption_mode_stops_the_gate_path_too() {
    let repo = TempRepo::new("adoption-mode-gate");
    let home = isolated_home("adoption-mode-gate");
    repo.write("osf.toml", "adoption_mode = \"vendored\"\n");
    repo.write("clean.md", "Run the tests.\n");
    let out = run_osf(&repo.dir, &home, &["lint", "writing", "--gate", "clean.md"]);
    assert!(!out.status.success(), "{}", output_text(&out));
    let text = output_text(&out);
    assert!(text.contains(AdoptionMode::KEY), "{text}");
    for mode in AdoptionMode::ALL {
        assert!(text.contains(mode.name()), "{text}");
    }
    repo.write("osf.toml", "adoption_mode = \"private-factory-repo\"\n");
    let ok = run_osf(&repo.dir, &home, &["lint", "writing", "--gate", "clean.md"]);
    assert!(ok.status.success(), "{}", output_text(&ok));
}
