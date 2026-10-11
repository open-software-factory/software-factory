//! Keeps the devcontainer's moon install matched to the build's processor
//! architecture, the same way its git-town install already is: a hardcoded
//! `x86_64` download breaks any `TARGETARCH=arm64` build.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    root.canonicalize()
        .unwrap_or_else(|e| panic!("cannot canonicalise {}: {e}", root.display()))
}

fn dockerfile_text() -> String {
    let path = repo_root().join(".devcontainer").join("Dockerfile");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The moon install, like the git-town install beside it, must branch on
/// `TARGETARCH` rather than naming one architecture's download outright.
#[test]
fn the_moon_install_branches_on_targetarch_for_both_known_architectures() {
    let text = dockerfile_text();
    let moon_block_start = text
        .find("# moon, the task runner")
        .expect("the moon install comment block is present");
    let moon_block_end = text[moon_block_start..]
        .find("# GitHub CLI")
        .expect("the GitHub CLI install follows the moon install");
    let moon_block = &text[moon_block_start..moon_block_start + moon_block_end];
    assert!(
        moon_block.contains("${TARGETARCH}"),
        "the moon install must branch on TARGETARCH: {moon_block}"
    );
    assert!(
        moon_block.contains("amd64") && moon_block.contains("arm64"),
        "the moon install must cover both amd64 and arm64: {moon_block}"
    );
    assert!(
        !moon_block.contains("moon_cli-x86_64-unknown-linux-gnu.tar.xz\""),
        "the moon download URL must not hardcode the x86_64 artifact name: {moon_block}"
    );
}

/// One moon install only, so no second copy can shadow the first on `PATH`.
#[test]
fn moon_is_installed_once() {
    let text = dockerfile_text();
    assert_eq!(text.matches("moonrepo/moon/releases/download").count(), 1);
    assert_eq!(text.matches("ARG MOON_VERSION=").count(), 1);
}
