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
use std::collections::BTreeSet;

/// How many distinct model families a lens needs among its answers before it can run at all.
const REQUIRED_FAMILIES: usize = 2;

/// One reviewer's outcome for one lens: its verified answer, when it gave one, or the reason it did not.
#[derive(Debug, Clone)]
pub struct LensAnswer {
    pub reviewer: String,
    pub family: String,
    pub answer: Option<VerifiedAnswer>,
    pub reason: Option<String>,
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
/// Fewer than [`REQUIRED_FAMILIES`] distinct families among the answers
/// that are present is [`LensVerdict::CouldNotRun`]: too few reviewers
/// answered, or they all came from the same family. A finding with
/// severity blocker, or action must-fix, among those answers' findings
/// vetoes the lens whatever its score. Otherwise the lens score is the
/// mean, over the answers, of the mean of each answer's own criterion
/// scores.
#[must_use]
pub fn decide_lens(lens: &Lens, answers: &[LensAnswer]) -> LensVerdict {
    let present: Vec<&VerifiedAnswer> = answers.iter().filter_map(|a| a.answer.as_ref()).collect();
    let families: BTreeSet<&str> = answers
        .iter()
        .filter(|a| a.answer.is_some())
        .map(|a| a.family.as_str())
        .collect();

    if families.len() < REQUIRED_FAMILIES {
        return LensVerdict::CouldNotRun(quorum_reason(&families));
    }

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

/// Why a lens could not run, naming the families it did get answers from.
fn quorum_reason(families: &BTreeSet<&str>) -> String {
    if families.is_empty() {
        "no reviewer answered".to_string()
    } else {
        let names: Vec<&str> = families.iter().copied().collect();
        format!("only one family answered: {}", names.join(", "))
    }
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
/// when nothing else fails. Otherwise the review passes.
#[must_use]
pub fn decide(lenses: &[(&Lens, LensVerdict)], threshold: f64) -> Verdict {
    let Some(mean) = weighted_mean(lenses) else {
        return Verdict::CouldNotRun;
    };

    let any_fail = lenses
        .iter()
        .any(|(_, verdict)| matches!(verdict, LensVerdict::Fail { .. }));

    if any_fail || mean < threshold {
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
        }
    }

    fn missing(reviewer: &str, family: &str, reason: &str) -> LensAnswer {
        LensAnswer {
            reviewer: reviewer.to_string(),
            family: family.to_string(),
            answer: None,
            reason: Some(reason.to_string()),
        }
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
    fn one_family_only_is_could_not_run() {
        let lens = test_lens();
        let root = TempDir::new("reducer-one-family");
        let answers = vec![
            answered("codex", "openai", scored(&lens, &root, 0.9, 0.9)),
            answered("dsh", "openai", scored(&lens, &root, 0.8, 0.8)),
        ];
        assert!(matches!(
            decide_lens(&lens, &answers),
            LensVerdict::CouldNotRun(_)
        ));
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
        let answers = vec![
            answered("codex", "openai", with_blocker),
            answered("claude", "anthropic", scored(&lens, &root, 1.0, 1.0)),
        ];
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
        let answers = vec![
            answered("codex", "openai", with_invented_blocker),
            answered("claude", "anthropic", scored(&lens, &root, 1.0, 1.0)),
        ];
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 1.0).abs() < f64::EPSILON),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn two_families_with_high_scores_and_no_blocker_pass() {
        let lens = test_lens();
        let root = TempDir::new("reducer-pass");
        let answers = vec![
            answered("codex", "openai", scored(&lens, &root, 0.9, 0.9)),
            answered("claude", "anthropic", scored(&lens, &root, 1.0, 1.0)),
        ];
        match decide_lens(&lens, &answers) {
            LensVerdict::Pass { score } => assert!((score - 0.95).abs() < 1e-9, "{score}"),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn the_same_answers_in_a_different_order_give_the_same_lens_verdict() {
        let lens = test_lens();
        let root = TempDir::new("reducer-order-independent");
        let forward = decide_lens(
            &lens,
            &[
                answered("codex", "openai", scored(&lens, &root, 0.9, 0.7)),
                answered("claude", "anthropic", scored(&lens, &root, 0.6, 0.8)),
            ],
        );
        let backward = decide_lens(
            &lens,
            &[
                answered("claude", "anthropic", scored(&lens, &root, 0.6, 0.8)),
                answered("codex", "openai", scored(&lens, &root, 0.9, 0.7)),
            ],
        );
        assert_eq!(forward, backward);
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
