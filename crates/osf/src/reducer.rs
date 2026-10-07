//! Turns reviewer answers into one verdict per lens, then one verdict for
//! the whole review, by fixed rules with no model call.
//!
//! Every rule here is order-independent: it sums or counts over the
//! answers it is given, so the same answers in a different order give the
//! same verdict. A lens can only ever be scored from a [`VerifiedAnswer`],
//! never a raw [`Answer`], so there is no way to hand this reducer a
//! finding whose quote was never checked against the files.

use crate::answer::{Action, Severity};
use crate::lenses::Lens;
use crate::quotes::VerifiedAnswer;
use std::collections::{BTreeMap, BTreeSet};

/// How many distinct model families a lens needs among its answers before it can run at all.
const REQUIRED_FAMILIES: usize = 2;

/// How many answered independent rounds a family needs before it counts
/// toward a lens's quorum. A critical round is never one of them.
pub const ROUNDS_REQUIRED: usize = 2;

/// One reviewer's outcome for one lens: its verified answer, when it gave one, or the reason it did not.
#[derive(Debug, Clone)]
pub struct LensAnswer {
    pub reviewer: String,
    pub family: String,
    pub answer: Option<VerifiedAnswer>,
    pub reason: Option<String>,
    /// Whether this is the critical round of the one-family fallback, not an independent round.
    pub critical: bool,
}

/// Which of a lens's answers count toward its verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    /// One flag per answer, in the order given: whether that answer counts.
    pub counts: Vec<bool>,
    /// Whether exactly one family qualified, so its critical answer decided the lens.
    pub interim: bool,
    /// Why the lens could not run, when it could not.
    pub could_not_run: Option<String>,
}

/// Works out which of `answers` count toward a lens's verdict.
///
/// A family qualifies with at least [`ROUNDS_REQUIRED`] answered independent
/// rounds. With [`REQUIRED_FAMILIES`] or more qualified families, every
/// answered independent round of a qualified family counts, and a critical
/// round counts for no one. With exactly one qualified family, the interim
/// policy applies: the lens runs only when that family also gave an answered
/// critical round, and its independent rounds and its critical round all
/// count. Otherwise the lens could not run. A family with fewer answered
/// rounds never counts, whatever else it gave.
#[must_use]
pub fn count(answers: &[LensAnswer]) -> Counted {
    let mut independent: BTreeMap<&str, usize> = BTreeMap::new();
    let mut critical: BTreeMap<&str, usize> = BTreeMap::new();
    for answer in answers.iter().filter(|a| a.answer.is_some()) {
        let tally = if answer.critical {
            &mut critical
        } else {
            &mut independent
        };
        *tally.entry(answer.family.as_str()).or_default() += 1;
    }
    let qualified: BTreeSet<&str> = independent
        .iter()
        .filter(|(_, rounds)| **rounds >= ROUNDS_REQUIRED)
        .map(|(family, _)| *family)
        .collect();
    let none_count = |reason: String| Counted {
        counts: vec![false; answers.len()],
        interim: false,
        could_not_run: Some(reason),
    };
    if qualified.len() >= REQUIRED_FAMILIES {
        return Counted {
            counts: answers
                .iter()
                .map(|a| !a.critical && a.answer.is_some() && qualified.contains(a.family.as_str()))
                .collect(),
            interim: false,
            could_not_run: None,
        };
    }
    let Some(family) = qualified.iter().next().copied() else {
        return none_count(quorum_reason(&independent));
    };
    if critical.get(family).copied().unwrap_or(0) == 0 {
        return none_count(format!(
            "only one family answered {ROUNDS_REQUIRED} rounds ({family}), and it gave no critical answer"
        ));
    }
    Counted {
        counts: answers
            .iter()
            .map(|a| a.answer.is_some() && a.family == family)
            .collect(),
        interim: true,
        could_not_run: None,
    }
}

/// One lens's verdict: a score out of 1, a score with the blockers that
/// vetoed it, or that it never reached quorum to run at all.
#[derive(Debug, Clone, PartialEq)]
pub enum LensVerdict {
    Pass { score: f64 },
    Fail { score: f64, blockers: usize },
    CouldNotRun(String),
}

/// The whole review's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    CouldNotRun,
}

/// Decides one lens's verdict from every reviewer's outcome for it.
///
/// Only the answers [`count`] says count are used. When none can decide the
/// lens, it is [`LensVerdict::CouldNotRun`]. A finding with severity
/// blocker, or action must-fix, among the counted answers' findings vetoes
/// the lens whatever its score. Otherwise the lens score is the mean, over
/// the counted answers, of the mean of each answer's own criterion scores.
#[must_use]
pub fn decide_lens(lens: &Lens, answers: &[LensAnswer]) -> LensVerdict {
    let counted = count(answers);
    if let Some(reason) = counted.could_not_run {
        return LensVerdict::CouldNotRun(reason);
    }
    let present: Vec<&VerifiedAnswer> = answers
        .iter()
        .zip(&counted.counts)
        .filter(|(_, counts)| **counts)
        .filter_map(|(a, _)| a.answer.as_ref())
        .collect();

    let blockers = present
        .iter()
        .flat_map(|answer| answer.findings().iter())
        .filter(|finding| {
            finding.severity == Severity::Blocker || finding.action == Action::MustFix
        })
        .count();

    let score = mean_of_means(lens, &present);

    if blockers > 0 {
        LensVerdict::Fail { score, blockers }
    } else {
        LensVerdict::Pass { score }
    }
}

/// Why no family qualified, naming each family's answered independent rounds.
fn quorum_reason(independent: &BTreeMap<&str, usize>) -> String {
    if independent.is_empty() {
        return "no reviewer answered".to_string();
    }
    let parts: Vec<String> = independent
        .iter()
        .map(|(family, rounds)| format!("{family} answered {rounds}"))
        .collect();
    format!(
        "no family answered {ROUNDS_REQUIRED} rounds: {}",
        parts.join(", ")
    )
}

/// The mean, over `answers`, of each answer's own mean criterion score for `lens`.
fn mean_of_means(lens: &Lens, answers: &[&VerifiedAnswer]) -> f64 {
    if answers.is_empty() {
        return 0.0;
    }
    let sum: f64 = answers.iter().map(|answer| mean_score(lens, answer)).sum();
    sum / as_f64(answers.len())
}

/// `answer`'s mean score over `lens`'s own criteria, a missing score counting as 0.
fn mean_score(lens: &Lens, answer: &VerifiedAnswer) -> f64 {
    if lens.criteria.is_empty() {
        return 0.0;
    }
    let sum: f64 = lens
        .criteria
        .iter()
        .filter_map(|criterion| answer.scores().get(&criterion.id))
        .sum();
    sum / as_f64(lens.criteria.len())
}

/// `n`, as an `f64`, with no precision lost for any count this reducer sees.
fn as_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The weighted mean of every lens's own score, weighted by each lens's own
/// `weight`, or `None` when any lens could not run: a could-not-run lens
/// has no score to weigh in, and the review it belongs to was never
/// actually scored against a threshold. The sole source of this formula;
/// [`decide`] and a review run's own reporting both call it rather than
/// each keeping their own copy.
#[must_use]
pub fn weighted_mean(lenses: &[(&Lens, LensVerdict)]) -> Option<f64> {
    if lenses
        .iter()
        .any(|(_, verdict)| matches!(verdict, LensVerdict::CouldNotRun(_)))
    {
        return None;
    }

    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    for (lens, verdict) in lenses {
        let score = match verdict {
            LensVerdict::Pass { score } | LensVerdict::Fail { score, .. } => *score,
            LensVerdict::CouldNotRun(_) => unreachable!("could-not-run lenses returned above"),
        };
        weighted_sum += lens.weight * score;
        weight_total += lens.weight;
    }
    Some(if weight_total > 0.0 {
        weighted_sum / weight_total
    } else {
        0.0
    })
}

/// Decides the whole review from every lens's verdict.
///
/// Any lens still could-not-run makes the whole review
/// [`Verdict::CouldNotRun`], whatever the others say: a could-not-run lens
/// is never folded into a pass. Otherwise, any lens that failed, or
/// [`weighted_mean`] of the lens scores under `threshold`, is
/// [`Verdict::Fail`]. A weighted mean equal to `threshold` still passes,
/// when nothing else fails. A weighted mean that is not finite, or a NaN
/// threshold, fails, so a bad setting never passes a review. Otherwise the
/// review passes.
#[must_use]
pub fn decide(lenses: &[(&Lens, LensVerdict)], threshold: f64) -> Verdict {
    let Some(mean) = weighted_mean(lenses) else {
        return Verdict::CouldNotRun;
    };

    let any_fail = lenses
        .iter()
        .any(|(_, verdict)| matches!(verdict, LensVerdict::Fail { .. }));

    if any_fail || !mean.is_finite() || threshold.is_nan() || mean < threshold {
        Verdict::Fail
    } else {
        Verdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{Action, Answer, AnswerFinding, Severity};
    use crate::lenses::{Criterion, Runs, SeverityGuide, Trigger};
    use crate::test_support::TempDir;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn test_lens() -> Lens {
        Lens {
            name: "correctness".to_string(),
            summary: "does the change do what it claims".to_string(),
            criteria: vec![
                Criterion {
                    id: "c1".to_string(),
                    question: "q1".to_string(),
                },
                Criterion {
                    id: "c2".to_string(),
                    question: "q2".to_string(),
                },
            ],
            severity_guide: SeverityGuide {
                blocker: "loses data".to_string(),
                major: "a wrong behaviour a user can hit".to_string(),
                minor: "a small nit".to_string(),
            },
            weight: 1.0,
            runs: Runs::Always,
            trigger: Trigger::default(),
            context: Vec::new(),
        }
    }

    /// A verified answer with no findings, built through [`crate::quotes::check`] like any other, since it is the only way to get one.
    fn scored(lens: &Lens, root: &Path, c1: f64, c2: f64) -> VerifiedAnswer {
        verified(lens, root, c1, c2, Vec::new())
    }

    /// A verified answer carrying `findings`, built through [`crate::quotes::check`].
    fn verified(
        lens: &Lens,
        root: &Path,
        c1: f64,
        c2: f64,
        findings: Vec<AnswerFinding>,
    ) -> VerifiedAnswer {
        let mut scores = BTreeMap::new();
        scores.insert("c1".to_string(), c1);
        scores.insert("c2".to_string(), c2);
        let answer = Answer {
            lens: lens.name.clone(),
            scores,
            findings,
        };
        crate::quotes::check(root, answer).kept
    }

    fn answered(reviewer: &str, family: &str, answer: VerifiedAnswer) -> LensAnswer {
        LensAnswer {
            reviewer: reviewer.to_string(),
            family: family.to_string(),
            answer: Some(answer),
            reason: None,
            critical: false,
        }
    }

    fn critical(reviewer: &str, family: &str, answer: VerifiedAnswer) -> LensAnswer {
        LensAnswer {
            critical: true,
            ..answered(reviewer, family, answer)
        }
    }

    fn missing(reviewer: &str, family: &str, reason: &str) -> LensAnswer {
        LensAnswer {
            reviewer: reviewer.to_string(),
            family: family.to_string(),
            answer: None,
            reason: Some(reason.to_string()),
            critical: false,
        }
    }

    /// Two answered independent rounds from one family, both scoring `s`.
    fn two_rounds(
        lens: &Lens,
        root: &Path,
        reviewer: &str,
        family: &str,
        s: f64,
    ) -> Vec<LensAnswer> {
        vec![
            answered(reviewer, family, scored(lens, root, s, s)),
            answered(reviewer, family, scored(lens, root, s, s)),
        ]
    }

    /// A blocker finding whose quote is exactly the whole third line of the file `verified`'s `root` must hold: `"fn broken"`.
    fn blocker_finding() -> AnswerFinding {
        AnswerFinding {
            path: "src/lib.rs".to_string(),
            line: 3,
            quote: "fn broken".to_string(),
            severity: Severity::Blocker,
            action: Action::MustFix,
            body: "loses data".to_string(),
        }
    }

    #[test]
    fn one_family_with_two_rounds_and_no_critical_round_is_could_not_run() {
        let lens = test_lens();
        let root = TempDir::new("reducer-one-family");
        let answers = vec![
            answered("codex", "openai", scored(&lens, &root, 0.9, 0.9)),
            answered("dsh", "openai", scored(&lens, &root, 0.8, 0.8)),
        ];
        match decide_lens(&lens, &answers) {
            LensVerdict::CouldNotRun(reason) => {
                assert!(reason.contains("no critical answer"), "{reason}");
            }
            other => panic!("expected CouldNotRun, got {other:?}"),
        }
    }

    /// The interim policy: when only one family answered both independent
    /// rounds, its critical round must have answered too, and all three
    /// answers decide the lens.
    #[test]
    fn a_single_family_with_two_rounds_and_a_critical_answer_passes() {
        let lens = test_lens();
        let root = TempDir::new("reducer-interim-single-family");
        let mut answers = vec![
            answered("codex", "openai", scored(&lens, &root, 0.9, 0.9)),
            answered("codex", "openai", scored(&lens, &root, 0.8, 0.8)),
        ];
        answers.push(critical("codex", "openai", scored(&lens, &root, 1.0, 1.0)));
        let counted = count(&answers);
        assert!(counted.interim);
        assert_eq!(counted.counts, vec![true, true, true]);
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 0.9).abs() < 1e-9, "{score}"),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn a_single_family_whose_critical_round_did_not_answer_is_could_not_run() {
        let lens = test_lens();
        let root = TempDir::new("reducer-interim-missing-critical");
        for failed in [
            missing("codex", "openai", "timed out"),
            LensAnswer {
                critical: true,
                ..missing("codex", "openai", "the answer did not validate")
            },
        ] {
            let mut answers = two_rounds(&lens, &root, "codex", "openai", 1.0);
            answers.push(LensAnswer {
                critical: true,
                ..failed
            });
            assert!(
                matches!(decide_lens(&lens, &answers), LensVerdict::CouldNotRun(_)),
                "a critical round that did not answer must not pass the lens"
            );
        }
    }

    #[test]
    fn a_family_with_one_answered_round_does_not_count_toward_the_quorum() {
        let lens = test_lens();
        let root = TempDir::new("reducer-truncated-round");
        let mut answers = two_rounds(&lens, &root, "claude", "anthropic", 1.0);
        answers.push(answered("codex", "openai", scored(&lens, &root, 1.0, 1.0)));
        answers.push(missing("codex", "openai", "the second round timed out"));
        let counted = count(&answers);
        assert_eq!(
            counted.counts,
            vec![false; 4],
            "nothing counts when no quorum forms"
        );
        assert!(
            !counted.interim && counted.could_not_run.is_some(),
            "one family with two rounds is not a quorum without a critical round"
        );
        assert!(matches!(
            decide_lens(&lens, &answers),
            LensVerdict::CouldNotRun(_)
        ));
    }

    #[test]
    fn an_invalid_second_round_leaves_a_family_short_of_the_quorum() {
        let lens = test_lens();
        let root = TempDir::new("reducer-invalid-round");
        let mut answers = two_rounds(&lens, &root, "claude", "anthropic", 1.0);
        answers.push(answered("codex", "openai", scored(&lens, &root, 1.0, 1.0)));
        answers.push(missing("codex", "openai", "invalid: no scores"));
        answers.push(critical(
            "claude",
            "anthropic",
            scored(&lens, &root, 1.0, 1.0),
        ));
        let counted = count(&answers);
        assert!(counted.interim, "only anthropic qualified");
        assert_eq!(counted.counts, vec![true, true, false, false, true]);
    }

    #[test]
    fn a_critical_answer_never_stands_in_for_a_missing_independent_round() {
        let lens = test_lens();
        let root = TempDir::new("reducer-critical-only");
        let answers = vec![
            answered("codex", "openai", scored(&lens, &root, 1.0, 1.0)),
            missing("codex", "openai", "the second round timed out"),
            critical("codex", "openai", scored(&lens, &root, 1.0, 1.0)),
        ];
        match decide_lens(&lens, &answers) {
            LensVerdict::CouldNotRun(reason) => {
                assert!(reason.contains("openai answered 1"), "{reason}");
            }
            other => panic!("expected CouldNotRun, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_reviewer_is_could_not_run_even_with_a_critical_answer_elsewhere() {
        let lens = test_lens();
        let answers = vec![missing("codex", "openai", "timed out")];
        assert!(matches!(
            decide_lens(&lens, &answers),
            LensVerdict::CouldNotRun(_)
        ));
    }

    /// Two qualified families score on their independent rounds; a critical
    /// round from either is ignored.
    #[test]
    fn two_qualified_families_ignore_a_critical_round() {
        let lens = test_lens();
        let root = TempDir::new("reducer-two-families-critical");
        let mut answers = two_rounds(&lens, &root, "codex", "openai", 0.9);
        answers.extend(two_rounds(&lens, &root, "claude", "anthropic", 1.0));
        answers.push(critical("codex", "openai", scored(&lens, &root, 0.0, 0.0)));
        let counted = count(&answers);
        assert!(!counted.interim);
        assert_eq!(counted.counts, vec![true, true, true, true, false]);
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 0.95).abs() < 1e-9, "{score}"),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn no_answers_at_all_is_could_not_run() {
        let lens = test_lens();
        let answers = vec![
            missing("codex", "openai", "timed out"),
            missing("claude", "anthropic", "disabled"),
        ];
        assert!(matches!(
            decide_lens(&lens, &answers),
            LensVerdict::CouldNotRun(_)
        ));
    }

    #[test]
    fn a_blocker_vetoes_whatever_the_scores() {
        let lens = test_lens();
        let root = TempDir::new("reducer-blocker");
        std::fs::create_dir_all(root.join("src")).expect("dirs");
        std::fs::write(root.join("src/lib.rs"), "one\ntwo\nfn broken\n").expect("write");

        let with_blocker = verified(&lens, &root, 1.0, 1.0, vec![blocker_finding()]);
        let mut answers = vec![
            answered("codex", "openai", with_blocker),
            answered("codex", "openai", scored(&lens, &root, 1.0, 1.0)),
        ];
        answers.extend(two_rounds(&lens, &root, "claude", "anthropic", 1.0));
        match decide_lens(&lens, &answers) {
            LensVerdict::Fail { score, blockers } => {
                assert_eq!(blockers, 1);
                assert!((score - 1.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn a_blocker_whose_quote_fails_verification_does_not_veto_the_lens() {
        let lens = test_lens();
        let root = TempDir::new("reducer-invented-blocker");
        std::fs::create_dir_all(root.join("src")).expect("dirs");
        std::fs::write(root.join("src/lib.rs"), "one\ntwo\nthree\n").expect("write");

        let with_invented_blocker = verified(&lens, &root, 1.0, 1.0, vec![blocker_finding()]);
        let mut answers = vec![
            answered("codex", "openai", with_invented_blocker),
            answered("codex", "openai", scored(&lens, &root, 1.0, 1.0)),
        ];
        answers.extend(two_rounds(&lens, &root, "claude", "anthropic", 1.0));
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 1.0).abs() < f64::EPSILON),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn a_blocker_in_a_family_that_did_not_qualify_is_not_counted() {
        let lens = test_lens();
        let root = TempDir::new("reducer-unqualified-blocker");
        std::fs::create_dir_all(root.join("src")).expect("dirs");
        std::fs::write(root.join("src/lib.rs"), "one\ntwo\nfn broken\n").expect("write");

        let with_blocker = verified(&lens, &root, 1.0, 1.0, vec![blocker_finding()]);
        let mut answers = two_rounds(&lens, &root, "claude", "anthropic", 1.0);
        answers.extend(two_rounds(&lens, &root, "dsh", "deepseek", 1.0));
        answers.push(answered("codex", "openai", with_blocker));
        answers.push(missing("codex", "openai", "the second round timed out"));
        assert!(matches!(
            decide_lens(&lens, &answers),
            LensVerdict::Pass { .. }
        ));
    }

    #[test]
    fn two_families_with_high_scores_and_no_blocker_pass() {
        let lens = test_lens();
        let root = TempDir::new("reducer-pass");
        let mut answers = two_rounds(&lens, &root, "codex", "openai", 0.9);
        answers.extend(two_rounds(&lens, &root, "claude", "anthropic", 1.0));
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 0.95).abs() < 1e-9, "{score}"),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn the_same_answers_in_a_different_order_give_the_same_lens_verdict() {
        let lens = test_lens();
        let root = TempDir::new("reducer-order-independent");
        let mut forward = two_rounds(&lens, &root, "codex", "openai", 0.75);
        forward.extend(two_rounds(&lens, &root, "claude", "anthropic", 0.5));
        let mut backward = two_rounds(&lens, &root, "claude", "anthropic", 0.5);
        backward.extend(two_rounds(&lens, &root, "codex", "openai", 0.75));
        assert_eq!(decide_lens(&lens, &forward), decide_lens(&lens, &backward));
    }

    #[test]
    fn a_nan_threshold_or_score_never_passes_a_review() {
        let lens = test_lens();
        let passing: Vec<(&Lens, LensVerdict)> = vec![(&lens, LensVerdict::Pass { score: 0.9 })];
        assert_eq!(decide(&passing, f64::NAN), Verdict::Fail);
        let nan_score: Vec<(&Lens, LensVerdict)> =
            vec![(&lens, LensVerdict::Pass { score: f64::NAN })];
        assert_eq!(decide(&nan_score, 0.5), Verdict::Fail);
    }

    #[test]
    fn a_could_not_run_lens_makes_the_review_could_not_run() {
        let lens_a = test_lens();
        let mut lens_b = test_lens();
        lens_b.name = "security".to_string();
        let lenses: Vec<(&Lens, LensVerdict)> = vec![
            (&lens_a, LensVerdict::Pass { score: 1.0 }),
            (
                &lens_b,
                LensVerdict::CouldNotRun("only one family answered: openai".to_string()),
            ),
        ];
        assert_eq!(decide(&lenses, 0.7), Verdict::CouldNotRun);
    }

    #[test]
    fn a_could_not_run_lens_wins_over_a_failed_lens() {
        let lens_a = test_lens();
        let mut lens_b = test_lens();
        lens_b.name = "security".to_string();
        let lenses: Vec<(&Lens, LensVerdict)> = vec![
            (
                &lens_a,
                LensVerdict::Fail {
                    score: 0.1,
                    blockers: 1,
                },
            ),
            (
                &lens_b,
                LensVerdict::CouldNotRun("no reviewer answered".to_string()),
            ),
        ];
        assert_eq!(decide(&lenses, 0.7), Verdict::CouldNotRun);
    }

    #[test]
    fn a_weighted_score_under_the_threshold_fails() {
        let mut lens_a = test_lens();
        lens_a.weight = 1.0;
        let mut lens_b = test_lens();
        lens_b.name = "security".to_string();
        lens_b.weight = 3.0;
        let lenses: Vec<(&Lens, LensVerdict)> = vec![
            (&lens_a, LensVerdict::Pass { score: 0.9 }),
            (&lens_b, LensVerdict::Pass { score: 0.4 }),
        ];
        assert_eq!(decide(&lenses, 0.7), Verdict::Fail);
    }

    #[test]
    fn a_weighted_score_at_or_above_the_threshold_with_no_failed_lens_passes() {
        let mut lens_a = test_lens();
        lens_a.weight = 1.0;
        let mut lens_b = test_lens();
        lens_b.name = "security".to_string();
        lens_b.weight = 3.0;
        let lenses: Vec<(&Lens, LensVerdict)> = vec![
            (&lens_a, LensVerdict::Pass { score: 0.9 }),
            (&lens_b, LensVerdict::Pass { score: 0.8 }),
        ];
        assert_eq!(decide(&lenses, 0.7), Verdict::Pass);
    }

    #[test]
    fn a_weighted_score_exactly_at_the_threshold_passes() {
        let mut lens_a = test_lens();
        lens_a.weight = 1.0;
        let lenses: Vec<(&Lens, LensVerdict)> = vec![(&lens_a, LensVerdict::Pass { score: 0.7 })];
        assert_eq!(decide(&lenses, 0.7), Verdict::Pass);
    }

    #[test]
    fn the_same_lenses_in_a_different_order_give_the_same_review_verdict() {
        let mut lens_a = test_lens();
        lens_a.weight = 1.0;
        let mut lens_b = test_lens();
        lens_b.name = "security".to_string();
        lens_b.weight = 3.0;
        let forward: Vec<(&Lens, LensVerdict)> = vec![
            (&lens_a, LensVerdict::Pass { score: 0.9 }),
            (&lens_b, LensVerdict::Pass { score: 0.4 }),
        ];
        let backward: Vec<(&Lens, LensVerdict)> = vec![
            (&lens_b, LensVerdict::Pass { score: 0.4 }),
            (&lens_a, LensVerdict::Pass { score: 0.9 }),
        ];
        assert_eq!(decide(&forward, 0.7), decide(&backward, 0.7));
    }
}
