//! Every rule's own doc text is what `osf explain <rule>` shows to a
//! person, and what a finding quotes back in its message, so the text must
//! pass the very lint it describes. This test runs the writing lint over
//! each rule's `doc` string, in the `document` context, with the default
//! config and the built-in known names.
//!
//! An `### Example` section carries one deliberately bad line, marked
//! `Bad:`. This test skips any line that starts with `Bad:` and lints
//! every other line as ordinary prose.

use osf::config::WritingConfig;
use osf::lints::writing::lint_writing;
use osf::lints::{load_known_names, Context, KnownNames, Level, RULE_META};

fn known() -> KnownNames {
    load_known_names(&[], None).expect("built-in names load")
}

fn doc_without_bad_example(doc: &str) -> String {
    doc.lines()
        .filter(|line| !line.trim_start().starts_with("Bad:"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn blocking_findings(
    doc: &str,
    known: &KnownNames,
    cfg: &WritingConfig,
) -> Vec<osf::lints::Finding> {
    let text = doc_without_bad_example(doc);
    lint_writing(&text, known, cfg, Context::Document, false, false)
        .into_iter()
        .filter(|f| matches!(f.level, Level::Error | Level::Warning))
        .collect()
}

#[test]
fn every_writing_rule_doc_passes_its_own_lint() {
    let known = known();
    let cfg = WritingConfig::default();
    let failures: Vec<String> = RULE_META
        .iter()
        .filter_map(|meta| {
            let findings = blocking_findings(meta.doc, &known, &cfg);
            (!findings.is_empty()).then(|| format!("{}: {findings:?}", meta.id))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
