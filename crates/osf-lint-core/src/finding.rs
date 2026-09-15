//! What one rule reports: a level, a location and a one-line message.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
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

    #[must_use]
    pub fn render(&self, name: &str, level: Level) -> String {
        let level = match level {
            Level::Error => "error",
            Level::Warning => "warning",
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
