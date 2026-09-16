//! What one rule reports: a level, a location and a one-line message.

use serde::Serialize;

/// How much a finding weighs. An error fails the run. A warning is
/// reported, and becomes an error under `--strict`. An informational
/// finding is reported and never blocks anything, under any flag: it is
/// the level another engine's notes arrive at, and a gate that promoted
/// them would refuse work over a remark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Info,
}

/// How much to trust a finding: a fixed rule, or a statistical analyser.
/// A policy can refuse to gate on the statistical kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Evidence {
    #[default]
    Deterministic,
    Statistical,
}

/// What the author should do about a finding, from cheapest to most costly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Remediation {
    /// Fix and produce the whole text again.
    #[default]
    Rewrite,
    /// Add or correct the named part only; keep the rest.
    Clarify,
    /// Nothing now. Stored and delivered before the next turn.
    Advise,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub level: Level,
    pub line: usize,
    pub message: String,
    pub excerpt: String,
    pub source: &'static str,
    pub evidence: Evidence,
    pub remediation: Remediation,
    /// `Some(reason)` once a suppression marker covers this finding. The
    /// engine keeps a suppressed finding rather than dropping it; the
    /// caller decides whether to show it.
    pub suppressed: Option<String>,
}

impl Finding {
    /// A finding from a fixed rule. `source` defaults to the rule id,
    /// `evidence` defaults to deterministic, and `remediation` defaults to
    /// rewrite; a caller with a context-aware policy overrides it.
    #[must_use]
    pub fn new(
        rule: &'static str,
        level: Level,
        line: usize,
        message: String,
        excerpt: String,
    ) -> Self {
        Finding {
            rule,
            level,
            line,
            message,
            excerpt,
            source: rule,
            evidence: Evidence::Deterministic,
            remediation: Remediation::default(),
            suppressed: None,
        }
    }

    /// Mark this finding as the output of a named analyser, with its evidence.
    #[must_use]
    pub fn from_analyser(mut self, source: &'static str, evidence: Evidence) -> Self {
        self.source = source;
        self.evidence = evidence;
        self
    }

    /// Sets this finding's evidence without touching `source`, for a rule
    /// that mixes deterministic and statistical findings under one id.
    #[must_use]
    pub fn with_evidence(mut self, evidence: Evidence) -> Self {
        self.evidence = evidence;
        self
    }

    #[must_use]
    pub fn render(&self, name: &str, level: Level) -> String {
        let level = match level {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
        };
        format!(
            "{}:{}: {} [{}] {}: \"{}\"",
            name, self.line, level, self.rule, self.message, self.excerpt
        )
    }

    /// # Panics
    /// Panics if the finding cannot be serialised. Its field types always can.
    #[must_use]
    pub fn to_json(&self, name: &str, level: Level) -> String {
        #[derive(Serialize)]
        struct Row<'a> {
            file: &'a str,
            line: usize,
            level: Level,
            rule: &'a str,
            message: &'a str,
            excerpt: &'a str,
        }
        serde_json::to_string(&Row {
            file: name,
            line: self.line,
            level,
            rule: self.rule,
            message: &self.message,
            excerpt: &self.excerpt,
        })
        .expect("a finding serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn finding(level: Level) -> Finding {
        Finding::new("probe", level, 1, "m".to_string(), "x".to_string())
    }

    #[test]
    fn every_level_renders_under_its_own_name() {
        assert!(finding(Level::Error)
            .render("f", Level::Error)
            .contains(" error "));
        assert!(finding(Level::Warning)
            .render("f", Level::Warning)
            .contains(" warning "));
        assert!(finding(Level::Info)
            .render("f", Level::Info)
            .contains(" info "));
    }

    #[test]
    fn the_json_row_carries_the_level_name() {
        let row = finding(Level::Info).to_json("f", Level::Info);
        assert!(row.contains("\"level\":\"info\""), "{row}");
    }

    /// The report format has a level below warning, and an informational
    /// finding lands there rather than being rounded up.
    #[test]
    fn an_informational_finding_is_a_note_in_the_report_format() {
        let tool = crate::ToolInfo {
            name: "osf",
            version: "0.0.0",
            information_uri: "https://example.com",
        };
        let report = crate::to_sarif(&[("f".to_string(), vec![finding(Level::Info)])], &tool);
        let text = serde_json::to_string(&report).expect("serialises");
        assert!(text.contains("\"level\":\"note\""), "{text}");
    }

    #[test]
    fn a_rule_can_be_set_to_info_in_configuration() {
        let mut levels = BTreeMap::new();
        levels.insert("probe".to_string(), crate::LevelSetting::Info);
        let out = crate::apply_level_overrides(vec![finding(Level::Error)], &levels);
        assert_eq!(out.len(), 1);
        assert_eq!(out.first().map(|f| f.level), Some(Level::Info));
    }
}
