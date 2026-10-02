//! The prompt a reviewer gets. osf ships a default prompt file, and a
//! repository can replace it with a file its trusted `osf.toml` names under
//! `[review] prompt_file`. The trusted config root is the base branch, so a
//! pull request cannot rewrite its own reviewer's instructions.
//!
//! The file holds plain placeholders in braces that osf fills in once, left
//! to right: `{lens_name}`, `{lens_summary}`, `{lens_questions}`,
//! `{severity_guide}`, `{metadata}` and `{change_file}`, the path of the file
//! that holds the diff and the commit log. Text that osf inserts is never read
//! again for placeholders. Only the answer format osf parses stays in code,
//! and osf appends it after the file's text, so no file can break parsing.

use crate::lenses::Lens;
use std::fmt::Write as _;
use std::path::{Component, Path};

const DEFAULT_PROMPT: &str = include_str!("../defaults/review-prompt.md");

/// The answer format osf parses. Always the last part of a prompt.
fn answer_format(lens: &Lens) -> String {
    format!(
        "Answer only with JSON matching the review-answer schema for lens \"{name}\": an \
         object with \"lens\", \"scores\" (one entry per criterion id in the lens questions) \
         and \"findings\" (each with \"path\", \"line\", \"quote\", \"severity\", \"action\" \
         and \"body\"). A finding's \"quote\" must be the exact text at its \"path\" and \
         \"line\" in the checkout.",
        name = lens.name
    )
}

/// The prompt file's text: the file `prompt_file` names under `config_root`,
/// or the shipped default when `prompt_file` is `None`.
///
/// # Errors
/// Names the file and the reason when `prompt_file` points outside
/// `config_root` or cannot be read.
pub fn load(config_root: &Path, prompt_file: Option<&str>) -> Result<String, String> {
    let Some(relative) = prompt_file else {
        return Ok(DEFAULT_PROMPT.to_string());
    };
    let inside = !relative.is_empty()
        && Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !inside {
        return Err(format!(
            "[review] prompt_file \"{relative}\" must be a path inside the config root"
        ));
    }
    let path = config_root.join(relative);
    let real_root = config_root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", config_root.display()))?;
    let real_path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !real_path.starts_with(&real_root) {
        return Err(format!(
            "[review] prompt_file \"{relative}\" must be a file inside the config root, not a link to one outside it"
        ));
    }
    std::fs::read_to_string(&real_path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `template` with its placeholders filled in for `lens` and `metadata`,
/// then the fixed answer format.
#[must_use]
pub fn render(template: &str, lens: &Lens, metadata: &str, change_file: &str) -> String {
    let mut questions = String::new();
    for criterion in &lens.criteria {
        let _ = writeln!(questions, "- {}: {}", criterion.id, criterion.question);
    }
    let guide = format!(
        "- blocker: {}\n- major: {}\n- minor: {}",
        lens.severity_guide.blocker, lens.severity_guide.major, lens.severity_guide.minor
    );
    let values = [
        ("lens_name", lens.name.as_str()),
        ("lens_summary", lens.summary.as_str()),
        ("lens_questions", questions.trim_end()),
        ("severity_guide", guide.as_str()),
        ("metadata", metadata),
        ("change_file", change_file),
    ];
    let mut out = String::new();
    let mut rest = template;
    while let Some((before, after)) = rest.split_once('{') {
        out.push_str(before);
        let named = after.split_once('}').and_then(|(key, tail)| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value, tail))
        });
        if let Some((value, tail)) = named {
            out.push_str(value);
            rest = tail;
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    let _ = write!(out, "\n{}", answer_format(lens));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenses::{Criterion, Runs, SeverityGuide, Trigger};
    use crate::test_support::TempDir;

    fn lens() -> Lens {
        Lens {
            name: "correctness".to_string(),
            summary: "does it work".to_string(),
            criteria: vec![Criterion {
                id: "c1".to_string(),
                question: "is it right?".to_string(),
            }],
            severity_guide: SeverityGuide {
                blocker: "loses data".to_string(),
                major: "wrong output".to_string(),
                minor: "a nit".to_string(),
            },
            weight: 1.0,
            runs: Runs::Always,
            trigger: Trigger::default(),
            context: Vec::new(),
        }
    }

    #[test]
    fn the_default_prompt_fills_every_placeholder() {
        let template = load(Path::new("."), None).expect("default loads");
        let prompt = render(&template, &lens(), "META-TEXT", "/x/change.diff");
        assert!(
            prompt.contains("\"correctness\" lens: does it work"),
            "{prompt}"
        );
        assert!(prompt.contains("- c1: is it right?"), "{prompt}");
        assert!(prompt.contains("- blocker: loses data"), "{prompt}");
        assert!(prompt.contains("META-TEXT"), "{prompt}");
        assert!(
            prompt.contains("in the file /x/change.diff"),
            "the prompt names the diff file: {prompt}"
        );
        assert!(!prompt.contains("git diff"), "{prompt}");
        assert!(!prompt.contains("{change_file}"));
        assert!(!prompt.contains("{lens_name}") && !prompt.contains("{metadata}"));
    }

    #[test]
    fn the_answer_format_follows_the_file_text_whatever_the_file_says() {
        let prompt = render("Say nothing about answers.", &lens(), "m", "f");
        assert!(prompt.starts_with("Say nothing about answers."));
        assert!(prompt.ends_with("in the checkout."), "{prompt}");
        assert!(prompt.contains("review-answer schema for lens \"correctness\""));
    }

    #[test]
    fn inserted_text_is_never_read_for_placeholders() {
        let prompt = render("{metadata}|{lens_name}", &lens(), "{lens_name}", "f");
        assert!(prompt.starts_with("{lens_name}|correctness"), "{prompt}");
    }

    #[test]
    fn braces_that_name_no_placeholder_stay_as_they_are() {
        let prompt = render("json {\"a\": 1} and { open", &lens(), "m", "f");
        assert!(prompt.starts_with("json {\"a\": 1} and { open"), "{prompt}");
    }

    #[test]
    fn a_named_file_under_the_config_root_replaces_the_default() {
        let root = TempDir::new("osf-review-prompt-override");
        std::fs::write(root.join("mine.md"), "Custom for {lens_name}.").expect("writes");
        let template = load(&root, Some("mine.md")).expect("override loads");
        assert_eq!(template, "Custom for {lens_name}.");
    }

    #[test]
    fn a_path_outside_the_config_root_is_refused() {
        for bad in ["../x.md", "/etc/hostname", ""] {
            let e = load(Path::new("."), Some(bad)).expect_err("refused");
            assert!(e.contains("inside the config root"), "{bad}: {e}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_prompt_file_that_links_outside_the_config_root_is_not_read() {
        let outside = TempDir::new("osf-review-prompt-outside");
        std::fs::write(outside.join("other.md"), "OUTSIDE-TEXT").expect("writes");
        let root = TempDir::new("osf-review-prompt-link");
        std::os::unix::fs::symlink(outside.join("other.md"), root.join("mine.md"))
            .expect("file link");
        std::os::unix::fs::symlink(&*outside, root.join("dir")).expect("dir link");
        for named in ["mine.md", "dir/other.md"] {
            let e = load(&root, Some(named)).expect_err("a link out is refused");
            assert!(e.contains("inside the config root"), "{named}: {e}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_prompt_file_that_links_to_another_file_inside_the_config_root_is_read() {
        let root = TempDir::new("osf-review-prompt-inner-link");
        std::fs::write(root.join("real.md"), "INSIDE-TEXT").expect("writes");
        std::os::unix::fs::symlink("real.md", root.join("mine.md")).expect("link");
        assert_eq!(load(&root, Some("mine.md")).expect("loads"), "INSIDE-TEXT");
    }

    #[test]
    fn a_missing_named_file_names_the_file() {
        let root = TempDir::new("osf-review-prompt-missing");
        let e = load(&root, Some("gone.md")).expect_err("missing");
        assert!(e.contains("gone.md"), "{e}");
    }
}
