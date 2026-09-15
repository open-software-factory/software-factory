//! Orchestration: pick the units a rule's scope asks for, run the rule or
//! the analyser over them, and merge the findings. No rule lives here.

use crate::rule::{Analyser, Rule, Scope, Tier};
use crate::segment::{Doc, TextUnit};
use crate::Finding;

fn units_for(doc: &Doc, scope: Scope) -> &[TextUnit] {
    match scope {
        Scope::Sentence => &doc.sentences,
        Scope::Paragraph => &doc.paragraphs,
        Scope::Document => std::slice::from_ref(&doc.whole),
    }
}

/// Run every rule over the units its scope calls for, passing `config` to
/// each check. With `fast_only`, only the fast tier runs. The stop hook
/// uses this, since it runs on every turn end and must stay fast.
#[must_use]
pub fn run_rules<C>(
    doc: &Doc,
    rules: &[&dyn Rule<C>],
    config: &C,
    fast_only: bool,
) -> Vec<Finding> {
    rules
        .iter()
        .filter(|r| !fast_only || r.tier() == Tier::Fast)
        .flat_map(|r| {
            units_for(doc, r.scope())
                .iter()
                .flat_map(|u| r.check(u, config))
        })
        .collect()
}

/// Run every analyser over the batch of units its scope calls for.
#[must_use]
pub fn run_analysers(doc: &Doc, analysers: &[&dyn Analyser]) -> Vec<Finding> {
    analysers
        .iter()
        .flat_map(|a| a.analyse(units_for(doc, a.scope())))
        .collect()
}

pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by_key(|f| (f.line, f.rule));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::FnRule;
    use crate::segment::parse;
    use crate::{Finding, Level};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn one(unit: &TextUnit, _config: &()) -> Vec<Finding> {
        vec![Finding::new(
            "probe",
            Level::Warning,
            unit.line,
            unit.text.clone(),
            String::new(),
        )]
    }

    #[test]
    fn a_sentence_rule_runs_once_per_sentence() {
        let doc = parse("One. Two.\n");
        let rule = FnRule::sentence("probe", one);
        let found = run_rules(&doc, &[&rule], &(), false);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_document_rule_sees_the_whole_text_once() {
        let doc = parse("One. Two.\n");
        let rule = FnRule::document("probe", one);
        let found = run_rules(&doc, &[&rule], &(), false);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found.first().map(|f| f.message.as_str()),
            Some("One. Two.\n")
        );
    }

    #[test]
    fn a_paragraph_rule_sees_one_unit_per_paragraph() {
        let doc = parse("First paragraph.\n\nSecond paragraph.\n");
        let rule = FnRule::paragraph("probe", one);
        let found = run_rules(&doc, &[&rule], &(), false);
        assert_eq!(found.len(), 2);
    }

    struct SlowProbe;

    impl Rule for SlowProbe {
        fn id(&self) -> &'static str {
            "slow-probe"
        }
        fn scope(&self) -> Scope {
            Scope::Sentence
        }
        fn tier(&self) -> Tier {
            Tier::Slow
        }
        fn check(&self, unit: &TextUnit, config: &()) -> Vec<Finding> {
            one(unit, config)
        }
    }

    #[test]
    fn fast_only_skips_the_slow_tier() {
        let doc = parse("One. Two.\n");
        let fast = FnRule::sentence("fast-probe", one);
        let slow = SlowProbe;
        let rules: Vec<&dyn Rule> = vec![&fast, &slow];
        assert_eq!(run_rules(&doc, &rules, &(), true).len(), 2);
        assert_eq!(run_rules(&doc, &rules, &(), false).len(), 4);
    }

    struct CountingAnalyser;

    impl Analyser for CountingAnalyser {
        fn id(&self) -> &'static str {
            "counting-analyser"
        }
        fn scope(&self) -> Scope {
            Scope::Sentence
        }
        fn analyse(&self, units: &[TextUnit]) -> Vec<Finding> {
            vec![Finding::new(
                self.id(),
                Level::Warning,
                0,
                units.len().to_string(),
                String::new(),
            )
            .from_analyser("counting-analyser", crate::Evidence::Statistical)]
        }
    }

    #[test]
    fn an_analyser_sees_every_unit_in_one_batch() {
        let doc = parse("One. Two. Three.\n");
        let analyser = CountingAnalyser;
        let found = run_analysers(&doc, &[&analyser]);
        assert_eq!(found.len(), 1);
        assert_eq!(found.first().map(|f| f.message.as_str()), Some("3"));
        assert_eq!(
            found.first().map(|f| f.evidence),
            Some(crate::Evidence::Statistical)
        );
    }
}
