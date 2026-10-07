//! Inline suppression markers, the same convention as `markdownlint`: an
//! HTML comment that silences one or more rules for a line, a block, or a
//! whole file. The scanner reads the raw source, before the Markdown parse
//! strips comments, so it still sees every marker.

use crate::finding::{Finding, Level};
use crate::segment::{line_at, line_starts};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    NextLine,
    File,
    BlockStart,
    BlockEnd,
}

struct Marker {
    kind: Kind,
    line: usize,
    ids: Option<Vec<String>>,
    reason: Option<String>,
}

struct OpenBlock {
    start_line: usize,
    ids: Option<Vec<String>>,
    reason: Option<String>,
}

struct Span {
    start_line: usize,
    end_line: usize,
    rules: Option<Vec<String>>,
    reason: Option<String>,
    used: bool,
}

impl Span {
    fn new(
        start_line: usize,
        end_line: usize,
        rules: Option<Vec<String>>,
        reason: Option<String>,
    ) -> Self {
        Span {
            start_line,
            end_line,
            rules,
            reason,
            used: false,
        }
    }

    fn covers(&self, finding: &Finding) -> bool {
        finding.line >= self.start_line
            && finding.line <= self.end_line
            && self
                .rules
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|id| id == finding.rule))
    }

    /// Whether this call owns this span: it names at least one rule this
    /// call's own checker runs, or names none at all, the bare form, which
    /// covers every rule. A checker that runs none of a span's named rules
    /// cannot say whether that span ever matched anything.
    fn owned_by(&self, owned_rules: &[&str]) -> bool {
        self.rules
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| owned_rules.contains(&id.as_str())))
    }
}

fn marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"<!--\s*osf-(disable-line|disable-next-line|disable-file|disable|enable)\b(.*?)-->",
        )
        .expect("suppression marker pattern compiles")
    })
}

fn split_ids(s: &str) -> Option<Vec<String>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn parse_marker(kind_str: &str, rest: &str, line: usize) -> Marker {
    let kind = match kind_str {
        "disable-line" => Kind::Line,
        "disable-next-line" => Kind::NextLine,
        "disable-file" => Kind::File,
        "disable" => Kind::BlockStart,
        _ => Kind::BlockEnd,
    };
    if kind == Kind::BlockEnd {
        return Marker {
            kind,
            line,
            ids: split_ids(rest),
            reason: None,
        };
    }
    let separator = rest.find(" -- ");
    let ids_part = separator.map_or(rest, |i| rest.get(..i).unwrap_or(rest));
    let reason = separator
        .and_then(|i| rest.get(i + 4..))
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    Marker {
        kind,
        line,
        ids: split_ids(ids_part),
        reason,
    }
}

fn scan_markers(source: &str, doc_lines: &[usize]) -> Vec<Marker> {
    marker_regex()
        .captures_iter(source)
        .filter_map(|c| {
            let kind = c.get(1)?.as_str();
            let rest = c.get(2).map_or("", |m| m.as_str());
            let start = c.get(0)?.start();
            Some(parse_marker(kind, rest, line_at(doc_lines, start)))
        })
        .collect()
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Line => "osf-disable-line",
        Kind::NextLine => "osf-disable-next-line",
        Kind::File => "osf-disable-file",
        Kind::BlockStart => "osf-disable",
        Kind::BlockEnd => "osf-enable",
    }
}

fn require_reason(marker: &Marker, diagnostics: &mut Vec<Finding>) {
    if marker.kind != Kind::BlockEnd && marker.reason.is_none() {
        diagnostics.push(Finding::new(
            "suppression-without-reason",
            Level::Error,
            marker.line,
            "say why, after -- ; a suppression with no reason is not a guardrail".to_string(),
            kind_label(marker.kind).to_string(),
        ));
    }
}

/// Report every id not in `known`, and return only the known ones. `None`
/// (the bare form) passes through unchanged: it means every rule.
fn validate_ids(
    ids: Option<Vec<String>>,
    known: &[&str],
    line: usize,
    diagnostics: &mut Vec<Finding>,
) -> Option<Vec<String>> {
    let ids = ids?;
    for id in &ids {
        if !known.contains(&id.as_str()) {
            diagnostics.push(Finding::new(
                "suppression-unknown-rule",
                Level::Error,
                line,
                format!("'{id}' is not a rule id"),
                id.clone(),
            ));
        }
    }
    Some(
        ids.into_iter()
            .filter(|id| known.contains(&id.as_str()))
            .collect(),
    )
}

fn close_blocks(
    open: &mut Vec<OpenBlock>,
    spans: &mut Vec<Span>,
    end_line: usize,
    rules: Option<Vec<String>>,
) {
    match rules {
        None => {
            for block in open.drain(..) {
                spans.push(Span::new(
                    block.start_line,
                    end_line,
                    block.ids,
                    block.reason,
                ));
            }
        }
        Some(ids) => {
            for id in ids {
                let matched = open
                    .iter()
                    .rposition(|b| b.ids.as_ref().is_none_or(|list| list.contains(&id)));
                if let Some(pos) = matched {
                    let block = open.remove(pos);
                    spans.push(Span::new(
                        block.start_line,
                        end_line,
                        block.ids,
                        block.reason,
                    ));
                }
            }
        }
    }
}

fn build(markers: Vec<Marker>, known: &[&str]) -> (Vec<Span>, Vec<Finding>) {
    let mut diagnostics = Vec::new();
    let mut open: Vec<OpenBlock> = Vec::new();
    let mut spans = Vec::new();
    for marker in markers {
        require_reason(&marker, &mut diagnostics);
        let rules = validate_ids(marker.ids, known, marker.line, &mut diagnostics);
        if marker.kind != Kind::BlockEnd && marker.reason.is_none() {
            continue;
        }
        match marker.kind {
            Kind::Line => spans.push(Span::new(marker.line, marker.line, rules, marker.reason)),
            Kind::NextLine => spans.push(Span::new(
                marker.line + 1,
                marker.line + 1,
                rules,
                marker.reason,
            )),
            Kind::File => spans.push(Span::new(1, usize::MAX, rules, marker.reason)),
            Kind::BlockStart => open.push(OpenBlock {
                start_line: marker.line,
                ids: rules,
                reason: marker.reason,
            }),
            Kind::BlockEnd => close_blocks(&mut open, &mut spans, marker.line, rules),
        }
    }
    for block in open {
        spans.push(Span::new(
            block.start_line,
            usize::MAX,
            block.ids,
            block.reason,
        ));
    }
    (spans, diagnostics)
}

fn unused_warning(span: &Span) -> Finding {
    let target = span
        .rules
        .as_ref()
        .map_or_else(|| "every rule".to_string(), |ids| ids.join(", "));
    Finding::new(
        "suppression-unused",
        Level::Warning,
        span.start_line,
        "this suppression matches no finding".to_string(),
        target,
    )
}

/// Apply every suppression marker in `source` to `findings`. A covered
/// finding is kept, not dropped, with its reason recorded. `known_rules` is
/// every rule id in the tool, for validating a marker's ids: raw text is
/// often read by more than one checker, so a marker naming a rule from a
/// different one is not unknown. `owned_rules` is this call's own checker's
/// rules, and decides which markers this call can judge as unused: a
/// marker naming only a rule this call does not run is invisible to it,
/// neither suppressing anything nor reported unused, since this call
/// cannot say whether that other checker ever matched it. The result also
/// carries the suppression engine's own diagnostics: a marker with no
/// reason, an unknown rule id, and an owned marker that matched nothing.
#[must_use]
pub fn apply_suppressions(
    source: &str,
    mut findings: Vec<Finding>,
    known_rules: &[&str],
    owned_rules: &[&str],
) -> Vec<Finding> {
    let doc_lines = line_starts(source);
    let markers = scan_markers(source, &doc_lines);
    let (mut spans, diagnostics) = build(markers, known_rules);
    for finding in &mut findings {
        let mut reason = None;
        for span in &mut spans {
            if span.covers(finding) {
                span.used = true;
                reason.get_or_insert_with(|| span.reason.clone().unwrap_or_default());
            }
        }
        finding.suppressed = reason;
    }
    findings.extend(diagnostics);
    findings.extend(
        spans
            .iter()
            .filter(|s| !s.used && s.owned_by(owned_rules))
            .map(unused_warning),
    );
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &[&str] = &["bare-reference", "long-sentence"];

    /// Builds an `osf-<directive>` marker at run time: `osf scan` reads its
    /// own tracked source for markers too, so the shape must never sit
    /// whole in this file. `rest` is everything after the directive word,
    /// including its own leading space when there is one.
    fn marker(directive: &str, rest: &str) -> String {
        let open = ["<!--", "osf-"].join(" ");
        format!("{open}{directive}{rest} -->")
    }

    /// A probe finding, as if `bare-reference` had fired on `line`. This
    /// call owns every rule in `RULES`, the same as `known_rules`, so it
    /// behaves exactly as it did before ownership was tracked separately.
    fn run(source: &str, line: usize) -> Vec<Finding> {
        let finding = Finding::new(
            "bare-reference",
            Level::Error,
            line,
            "write the repository before the number".to_string(),
            "#125".to_string(),
        );
        apply_suppressions(source, vec![finding], RULES, RULES)
    }

    fn bare_reference(found: &[Finding]) -> &Finding {
        found
            .iter()
            .find(|f| f.rule == "bare-reference")
            .expect("the probe finding is kept, suppressed or not")
    }

    #[test]
    fn disable_line_covers_only_that_line() {
        let t = format!(
            "One.\nFixed in #125 today. {}\n",
            marker("disable-line", " bare-reference -- tracked")
        );
        let found = run(&t, 2);
        assert_eq!(
            bare_reference(&found).suppressed.as_deref(),
            Some("tracked")
        );
    }

    #[test]
    fn disable_next_line_covers_the_line_after() {
        let t = format!(
            "{}\nFixed in #125 today.\n",
            marker("disable-next-line", " bare-reference -- tracked")
        );
        let found = run(&t, 2);
        assert!(bare_reference(&found).suppressed.is_some());
    }

    #[test]
    fn disable_and_enable_bound_a_block() {
        let t = format!(
            "{}\nFixed in #125 today.\n{}\n",
            marker("disable", " bare-reference -- tracked"),
            marker("enable", " bare-reference")
        );
        let found = run(&t, 2);
        assert!(bare_reference(&found).suppressed.is_some());
    }

    #[test]
    fn a_block_never_enabled_covers_to_end_of_file() {
        let t = format!(
            "{}\nFixed in #125 today.\n",
            marker("disable", " bare-reference -- tracked")
        );
        let found = run(&t, 2);
        assert!(bare_reference(&found).suppressed.is_some());
        assert!(found.iter().all(|f| f.rule != "suppression-unused"));
    }

    #[test]
    fn disable_file_covers_the_whole_file() {
        let t = format!(
            "{}\nOne.\nFixed in #125 today.\n",
            marker("disable-file", " bare-reference -- tracked")
        );
        let found = run(&t, 3);
        assert!(bare_reference(&found).suppressed.is_some());
    }

    #[test]
    fn the_bare_form_covers_every_rule() {
        let t = format!(
            "Fixed in #125 today. {}\n",
            marker("disable-line", " -- tracked")
        );
        let found = run(&t, 1);
        assert!(bare_reference(&found).suppressed.is_some());
    }

    #[test]
    fn a_missing_reason_is_an_error() {
        let t = format!(
            "Fixed in #125 today. {}\n",
            marker("disable-line", " bare-reference")
        );
        let found = run(&t, 5);
        assert_eq!(
            found
                .iter()
                .filter(|f| f.rule == "suppression-without-reason")
                .count(),
            1
        );
    }

    #[test]
    fn a_missing_reason_does_not_suppress_the_finding() {
        let t = format!(
            "Fixed in #125 today. {}\n",
            marker("disable-line", " bare-reference")
        );
        let found = run(&t, 1);
        assert!(bare_reference(&found).suppressed.is_none());
    }

    #[test]
    fn an_unknown_rule_id_is_an_error() {
        let t = format!(
            "Fixed in #125 today. {}\n",
            marker("disable-line", " not-a-rule -- tracked")
        );
        let found = run(&t, 1);
        assert!(found
            .iter()
            .any(|f| f.rule == "suppression-unknown-rule" && f.excerpt == "not-a-rule"));
        assert!(bare_reference(&found).suppressed.is_none());
    }

    #[test]
    fn an_unused_suppression_is_a_warning() {
        let t = format!(
            "One. Two.\n{}\n",
            marker("disable-line", " long-sentence -- tracked")
        );
        let found = run(&t, 1);
        assert!(found
            .iter()
            .any(|f| f.rule == "suppression-unused" && f.level == Level::Warning));
    }

    #[test]
    fn a_marker_naming_only_an_unowned_rule_is_not_reported_unused() {
        let t = format!(
            "One.\n{}\n",
            marker("disable-line", " long-sentence -- tracked")
        );
        let found = apply_suppressions(&t, Vec::new(), RULES, &["bare-reference"]);
        assert!(
            found.iter().all(|f| f.rule != "suppression-unused"),
            "{found:?}"
        );
    }

    #[test]
    fn a_marker_naming_an_owned_rule_is_still_reported_unused() {
        let t = format!(
            "One.\n{}\n",
            marker("disable-line", " bare-reference -- tracked")
        );
        let found = apply_suppressions(&t, Vec::new(), RULES, &["bare-reference"]);
        assert!(
            found.iter().any(|f| f.rule == "suppression-unused"),
            "{found:?}"
        );
    }
}
