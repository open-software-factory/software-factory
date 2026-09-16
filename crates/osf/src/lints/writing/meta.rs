//! Metadata for every writing rule: why we believe it (its [`Class`]),
//! whether a reader can work out the meaning without asking (its
//! [`Group`]), and its doc text for `osf explain` and for every finding's
//! pointer back to that text.
//!
//! Every rule here is `House`. The ASD-STE100 standard could not be
//! obtained, so no rule cites it; where a doc text mentions it, the text
//! says plainly that the limit comes from a secondary summary and is
//! unverified.

use osf_lint_core::{Class, Context, Exception, Group, Level, Remediation};

pub struct RuleMeta {
    pub id: &'static str,
    pub class: Class,
    pub group: Group,
    pub citation: &'static str,
    pub doc: &'static str,
    /// Overrides the level the context matrix would otherwise choose.
    pub exception: Option<Exception>,
}

impl RuleMeta {
    /// The level and remediation for this rule in `context`, from the
    /// class-and-group matrix, the [`Exception`] above, and nothing else;
    /// a caller's own config may still override the level afterwards.
    #[must_use]
    pub fn resolve(&self, context: Context) -> (Level, Remediation) {
        osf_lint_core::resolve(self.class, self.group, context, self.exception)
    }
}

macro_rules! rule_meta {
    ($id:literal, $class:ident, $group:ident, $citation:literal, $doc:literal) => {
        RuleMeta {
            id: $id,
            class: Class::$class,
            group: Group::$group,
            citation: $citation,
            doc: $doc,
            exception: None,
        }
    };
    ($id:literal, $class:ident, $group:ident, $citation:literal, $doc:literal, $exception:expr) => {
        RuleMeta {
            id: $id,
            class: Class::$class,
            group: Group::$group,
            citation: $citation,
            doc: $doc,
            exception: Some($exception),
        }
    };
}

pub const RULE_META: &[RuleMeta] = &[
    rule_meta!(
        "bare-reference",
        House,
        Comprehension,
        "house",
        "### What it does\n\
         Flags `#123` written with no `owner/repo` in front of it.\n\
         ### Why it is bad\n\
         A reader outside this one repository cannot open a bare number. They \
         do not know which project it points to.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: Fixed in #125 today.\n\
         Good: Fixed in open-software-factory/software-factory#125 (the login crash) today."
    ),
    rule_meta!(
        "reference-without-label",
        House,
        Comprehension,
        "house",
        "### What it does\n\
         Flags `open-software-factory/software-factory#123` (or `repo#123`) with no bracketed description \
         straight after it.\n\
         ### Why it is bad\n\
         A bare reference number tells the reader nothing about what it is. \
         They must go and look it up before they can follow the text.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: The fix landed in open-software-factory/software-factory#125 today.\n\
         Good: The fix landed in open-software-factory/software-factory#125 (the login crash) today."
    ),
    rule_meta!(
        "reference-without-link",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a labelled `open-software-factory/software-factory#123 (the thing)` reference that is not \
         wrapped in a Markdown link. Always a warning, in every context: a \
         labelled reference is already resolvable without the link.\n\
         ### Why it is bad\n\
         The reader cannot click through. They must open a browser tab and \
         search for the reference by hand.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: open-software-factory/software-factory#125 (the login crash) is now fixed.\n\
         Good: [open-software-factory/software-factory#125 (the login crash)](https://example.com/125) is now fixed.",
        Exception::FixedLevel(Level::Warning)
    ),
    rule_meta!(
        "chat-local-reference",
        House,
        Comprehension,
        "house",
        "### What it does\n\
         Flags phrases and labels that only make sense inside one \
         conversation: \"as discussed\", \"Phase 2\", \"Step 3.\" with nothing \
         after it.\n\
         ### Why it is bad\n\
         A reader who was not in that conversation cannot resolve the \
         reference. The document must stand on its own.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: Do Phase 2 next.\n\
         Good: Run the database migration next."
    ),
    rule_meta!(
        "undefined-name",
        House,
        Comprehension,
        "house",
        "### What it does\n\
         Flags a capitalised name on the repository's own curated \
         must-explain list (`writing.must_explain_names` in its config \
         file), on its first use, when neither that sentence nor the next \
         one explains what it is.\n\
         ### Why it is bad\n\
         A reader who does not already know the name cannot follow the rest \
         of the text. The must-explain list names, one by one, the terms \
         this repository has decided are worth that certainty.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: Use Vale for this.\n\
         Good: Use Vale, a prose checker, for this."
    ),
    rule_meta!(
        "undefined-name-at-start",
        House,
        Style,
        "house",
        "### What it does\n\
         The weaker twin of undefined-name, for a name not on the \
         must-explain list. Fires when a capitalised word or run merely \
         looks like a name: an internal capital such as \"GitHub\" or \
         \"DuckDB\", a digit in one of its words, a domain-like suffix such \
         as \".dev\", or a multi-word run repeated more than once in the \
         document. None of these prove a name; a bare capital letter with \
         none of them, such as a word that only opens a sentence, is never \
         reported at all.\n\
         ### Why it is bad\n\
         Same reason as undefined-name, but the evidence only suggests a \
         name rather than confirming one, so it warns instead of erring, \
         and the finding carries statistical evidence rather than \
         deterministic.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: DuckDB runs fast.\n\
         Good: DuckDB, our embedded database, runs fast."
    ),
    rule_meta!(
        "long-sentence",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a sentence over the configured error limit (25 words by \
         default), and over the configured warning limit, when one is set.\n\
         ### Why it is bad\n\
         A long sentence usually carries more than one idea. The reader has \
         to hold every clause in mind before the sentence resolves.\n\
         ### Class\n\
         house. A secondary summary of the ASD-STE100 standard (not the \
         primary text, which could not be obtained) states a similar idea: \
         no more than 20 words in a procedure, 25 in description. This rule \
         copies the general idea, not a verified clause, so it is house, \
         not spec, and the exact numbers are unverified.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: The build failed because the test step timed out after the \
         runner ran out of disk space, which happened because the cache grew \
         past the volume limit set last month.\n\
         Good: The build failed. The test step timed out. The runner ran out \
         of disk space, because the cache grew past last month's volume limit."
    ),
    rule_meta!(
        "em-dash",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags an em dash, an en dash, or a spaced double hyphen.\n\
         ### Why it is bad\n\
         An em dash often joins two ideas that would read better as two \
         sentences. It also reads, to many people, as a sign the text was \
         written by a language model rather than a person.\n\
         ### Class\n\
         house: a common tell of machine-written text, with no measurement \
         behind it.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: The fix shipped — it closed the ticket.\n\
         Good: The fix shipped. It closed the ticket."
    ),
    rule_meta!(
        "arrow",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags an arrow character or `->` or `=>` in prose text.\n\
         ### Why it is bad\n\
         The reader must guess whether the arrow means \"becomes\", \"leads \
         to\", \"maps to\", or something else. A plain verb says which one.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: Bad input -> a crash.\n\
         Good: Bad input causes a crash."
    ),
    rule_meta!(
        "semicolon",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a semicolon followed by a space, outside a table cell.\n\
         ### Why it is bad\n\
         A semicolon usually joins two sentences that should be separate. \
         Two short sentences are easier to read than one joined by a \
         semicolon.\n\
         ### Class\n\
         house: our own taste, no external standard requires this shape.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: It ran; it passed.\n\
         Good: It ran. It passed."
    ),
    rule_meta!(
        "filler",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a configured list of words and phrases, such as \"leverage\", \
         \"robust\", and \"let me know if\".\n\
         ### Why it is bad\n\
         These words add length without adding meaning. Most can be cut, or \
         replaced with the plain thing they stand in for.\n\
         ### Class\n\
         house: our own taste, no external standard requires this list.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: We should leverage the cache to streamline the build.\n\
         Good: We should use the cache to make the build faster."
    ),
    rule_meta!(
        "numbers-in-prose",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a sentence with more than the configured number of separate \
         number tokens (2 by default), outside a table cell.\n\
         ### Why it is bad\n\
         Three or more numbers in one sentence are hard to scan. A table or \
         a list holds them better.\n\
         ### Class\n\
         house: our own taste, no external standard requires this limit.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: It ran 12 axes over 3 rounds in 41 minutes.\n\
         Good: It ran 12 axes. See the table for the round count and the time."
    ),
    rule_meta!(
        "bold-sentence",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a whole sentence set in bold, when it runs past six words. \
         A bold phrase inside an otherwise plain sentence does not fire it.\n\
         ### Why it is bad\n\
         Bolding an entire sentence removes the emphasis. If everything is \
         bold, nothing stands out.\n\
         ### Class\n\
         house: our own taste, no external standard requires this limit.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: **Run the full suite before every release, without exception.**\n\
         Good: **Run the tests.** Every release needs a clean run first."
    ),
    rule_meta!(
        "parenthetical",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a parenthetical aside of four or more words.\n\
         ### Why it is bad\n\
         A long aside interrupts the main sentence. Reading it as its own \
         sentence is usually easier.\n\
         ### Class\n\
         house: our own taste, no external standard requires this limit.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: The fix (which took three days because the failure only showed \
         up under load) shipped today.\n\
         Good: The fix shipped today. It took three days, because the \
         failure only showed up under load."
    ),
    rule_meta!(
        "heading-in-short-text",
        House,
        Style,
        "house",
        "### What it does\n\
         Flags a Markdown heading inside a short text (under the configured \
         word count, 500 by default). Off for the `document` and `skill` \
         contexts: both are structured text that is expected to have \
         headings.\n\
         ### Why it is bad\n\
         A heading in a short reply adds structure the reply does not need. \
         A sentence, or a short table, usually says the same thing.\n\
         ### Class\n\
         house: our own taste, no external standard requires this limit.\n\
         ### Citation\n\
         house\n\
         ### Example\n\
         Bad: ## Result\\n\\nIt passed.\n\
         Good: It passed."
    ),
];

#[must_use]
pub fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    RULE_META.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_id_has_metadata() {
        for id in crate::lints::writing::rules::rule_ids() {
            assert!(rule_meta(id).is_some(), "no metadata for {id}");
        }
    }

    #[test]
    fn every_rule_is_house_and_says_so() {
        for meta in RULE_META {
            assert_eq!(meta.class, Class::House, "{}", meta.id);
            assert_eq!(meta.citation, "house", "{}", meta.id);
            assert!(
                !meta.doc.to_lowercase().contains("asd-ste100 says"),
                "{}",
                meta.id
            );
        }
    }
}
