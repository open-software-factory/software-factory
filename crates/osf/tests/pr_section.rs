//! End-to-end case for `osf pr section write`, driven against a fake `gh`
//! script placed first on `PATH`. Nothing here reaches the network.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const FAKE_GH_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  case "$*" in
    *headRefOid*) echo "cafe1234cafe1234" ; exit 0 ;;
  esac
  cat "$PR_SECTION_BODY_FILE"
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "edit" ]; then
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "--body-file" ]; then
      cp "$arg" "$PR_SECTION_OUT_FILE"
    fi
    prev="$arg"
  done
  exit 0
fi
echo "fake gh: unrecognized args: $*" >&2
exit 1
"#;

/// A fake `gh` on its own `PATH` entry, standing in for the code host: `pr
/// view` prints a body fixed ahead of time, and `pr edit` copies whatever
/// `--body-file` names to a path the test can read back.
struct FakeGh {
    dir: PathBuf,
    body_file: PathBuf,
    out_file: PathBuf,
}

impl FakeGh {
    fn new(name: &str, initial_body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osf-pr-section-fakegh-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fake gh dir creates");
        let body_file = dir.join("body.md");
        let out_file = dir.join("out.md");
        fs::write(&body_file, initial_body).expect("writes the initial body");
        let script = dir.join("gh");
        fs::write(&script, FAKE_GH_SCRIPT).expect("writes the fake gh script");
        let mut perms = fs::metadata(&script)
            .expect("fake gh metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod the fake gh script");
        FakeGh {
            dir,
            body_file,
            out_file,
        }
    }

    fn run_osf(&self, args: &[&str]) -> std::process::Output {
        let existing_path = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{existing_path}", self.dir.display());
        Command::new(env!("CARGO_BIN_EXE_osf"))
            .current_dir(&self.dir)
            .env("PATH", path)
            .env("PR_SECTION_BODY_FILE", &self.body_file)
            .env("PR_SECTION_OUT_FILE", &self.out_file)
            .args(args)
            .output()
            .expect("osf runs")
    }

    fn written_body(&self) -> String {
        fs::read_to_string(&self.out_file).expect("gh pr edit wrote a body file")
    }
}

impl Drop for FakeGh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn assert_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn writes_a_new_section_through_a_fake_gh() {
    let fake = FakeGh::new("write-new", "Intro.\n\nTail.\n");
    let content = fake.dir.join("content.md");
    fs::write(&content, "The outline.\n").expect("writes the content file");

    let output = fake.run_osf(&[
        "pr",
        "section",
        "write",
        "--pr",
        "7",
        "--name",
        "outline",
        "--file",
        content.to_str().expect("a utf-8 path"),
    ]);
    assert_ok(&output);
    assert_eq!(
        fake.written_body(),
        "Intro.\n\nTail.\n\n<!-- osf:outline:start head=cafe1234cafe1234 -->\nThe outline.\n<!-- osf:outline:end -->\n"
    );
}

#[test]
fn replaces_an_existing_section_through_a_fake_gh() {
    let fake = FakeGh::new(
        "write-replace",
        "Intro.\n\n<!-- osf:outline:start -->\nold\n<!-- osf:outline:end -->\n\nTail.\n",
    );
    let content = fake.dir.join("content.md");
    fs::write(&content, "new outline\n").expect("writes the content file");

    let output = fake.run_osf(&[
        "pr",
        "section",
        "write",
        "--pr",
        "9",
        "--repo",
        "acme/example",
        "--name",
        "outline",
        "--head",
        "1234567",
        "--file",
        content.to_str().expect("a utf-8 path"),
    ]);
    assert_ok(&output);
    assert_eq!(
        fake.written_body(),
        "Intro.\n\n<!-- osf:outline:start head=1234567 -->\nnew outline\n<!-- osf:outline:end -->\n\nTail.\n"
    );
}

#[test]
fn replaces_an_old_style_pr_lens_block_without_leaving_a_duplicate() {
    let fake = FakeGh::new(
        "write-migrate",
        "Intro.\n\n<!-- osf:pr-lens:start -->\nold diagram\n<!-- osf:pr-lens:end -->\n\nTail.\n",
    );
    let content = fake.dir.join("content.md");
    fs::write(&content, "new diagram\n").expect("writes the content file");

    let output = fake.run_osf(&[
        "pr",
        "section",
        "write",
        "--pr",
        "11",
        "--name",
        "pr-lens",
        "--head",
        "abcdef0",
        "--file",
        content.to_str().expect("a utf-8 path"),
    ]);
    assert_ok(&output);
    assert_eq!(
        fake.written_body(),
        "Intro.\n\n<!-- osf:pr-lens:start head=abcdef0 -->\nnew diagram\n<!-- osf:pr-lens:end -->\n\nTail.\n"
    );
}
