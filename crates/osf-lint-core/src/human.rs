//! Render findings as annotated source snippets: the way a compiler shows
//! a `file:line:col` diagnostic, with the source line and a caret under
//! the offending excerpt.

use crate::{Finding, Level};
use annotate_snippets::{AnnotationKind, Group, Level as SnippetLevel, Renderer, Snippet};
use std::ops::Range;

/// Render every finding for one named, in-memory source text.
#[must_use]
pub fn render_human(name: &str, source: &str, findings: &[Finding]) -> String {
    let renderer = Renderer::styled();
    let groups: Vec<Group<'static>> = findings
        .iter()
        .map(|finding| to_group(name, source, finding))
        .collect();
    renderer.render(&groups)
}

fn line_text(source: &str, line: usize) -> String {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .to_string()
}

/// Where the excerpt sits in its source line, or an empty span if it moved.
fn excerpt_span(line: &str, excerpt: &str) -> Range<usize> {
    line.find(excerpt)
        .map_or(0..0, |start| start..start + excerpt.len())
}

fn to_group(name: &str, source: &str, finding: &Finding) -> Group<'static> {
    let level = match finding.level {
        Level::Error => SnippetLevel::ERROR,
        Level::Warning => SnippetLevel::WARNING,
        Level::Info => SnippetLevel::INFO,
    };
    let text = line_text(source, finding.line);
    let span = excerpt_span(&text, &finding.excerpt);
    let title = format!("[{}] {}", finding.rule, finding.message);
    level.primary_title(title).element(
        Snippet::source(text)
            .line_start(finding.line)
            .path(name.to_string())
            .annotation(AnnotationKind::Primary.span(span).label(finding.rule)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offending_line_and_excerpt_are_both_shown() {
        let source = "Fixed in #125 today.\n";
        let findings = vec![Finding::new(
            "bare-reference",
            Level::Error,
            1,
            "write the repository before the number".to_string(),
            "#125".to_string(),
        )];
        let rendered = render_human("message.txt", source, &findings);
        assert!(rendered.contains("#125"));
        assert!(rendered.contains("bare-reference"));
        assert!(rendered.contains("message.txt"));
    }

    #[test]
    fn a_missing_excerpt_still_renders_without_panic() {
        let findings = vec![Finding::new(
            "heading-in-short-text",
            Level::Error,
            2,
            "a heading in a short text".to_string(),
            "heading on line 2".to_string(),
        )];
        let rendered = render_human("message.txt", "One line only.\n", &findings);
        assert!(rendered.contains("heading-in-short-text"));
    }
}
