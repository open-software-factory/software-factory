//! `osf review run`: selects the lenses a change earns, runs reviewers from
//! the roster over each one, checks their findings against the files,
//! decides a verdict, and journals every answer and the decision.
//!
//! A reviewer is whatever coding-agent tool is already installed and
//! logged in on this machine; this module never reads or holds a model
//! provider's key. A reviewer that fails to run counts as could-not-run
//! for that lens, and the run moves on to the next roster entry;
//! could-not-run never passes. Nothing journalled here carries a prompt or
//! a raw answer: `review_context` has already redacted the prompt, and
//! `transcript` is a path or nothing, never the text itself.

use crate::answer::AnswerFinding;
use crate::journal::{Journal, Payload, ReviewAnswer, ReviewDecision};
use crate::lenses::{self, Depth, Lens};
use crate::quotes;
use crate::reducer::{self, LensAnswer, LensVerdict, Verdict};
use crate::review_context::{self, Sources};
use crate::reviewers::{self, Outcome, Reviewer};
use crate::{config, git, risk};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many distinct model families a lens needs among its answers before
/// reviewers stop being tried for it, matching [`reducer::decide_lens`]'s
/// own quorum.
const QUORUM_FAMILIES: usize = 2;

const ACTOR: &str = "osf";

/// What one run of `osf review run` needs: the repository, what to diff
/// against, and the work item file the caller saved, if any.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub root: &'a Path,
    pub base: &'a str,
    pub work_item: Option<&'a Path>,
}

/// One finding that survived quote verification, with the lens it came from.
#[derive(Debug, Clone)]
pub struct KeptFinding {
    pub lens: String,
    pub finding: AnswerFinding,
}

/// The whole run's result: the verdict, one printable line per lens plus
/// the verdict itself, every finding that survived verification, and
/// whether the journal itself had a problem.
#[derive(Debug)]
pub struct RunOutcome {
    pub verdict: Verdict,
    pub lines: Vec<String>,
    pub findings: Vec<KeptFinding>,
    pub journal_error: Option<String>,
}

/// Runs the review: loads the catalogue, selects lenses for the change,
/// runs reviewers over each selected lens, decides a verdict, and journals
/// every answer plus the decision to `state_dir`.
///
/// # Errors
/// Returns an error, naming the file, when the lens catalogue fails to
/// load; naming the reason when the risk assessment or the changed-file
/// list cannot be produced; or when the reviewer roster or the review
/// threshold cannot be read from `osf.toml`. None of these leaves a lens
/// to blame, so the caller reports could-not-configure rather than
/// picking one lens to fail.
pub fn run(req: &Request, state_dir: &Path) -> Result<RunOutcome, String> {
    let catalogue = lenses::load(req.root, None)?;
    let report = risk::assess(req.root, req.base)?;
    let changed = git::changed_files(req.root, req.base).map_err(|e| e.to_string())?;
    let signals = report.signals();
    let selected = lenses::select(&catalogue, &changed, &signals, report.tier);
    let depth = lenses::depth(report.tier);

    let roster = reviewers::roster(req.root)?;
    let enabled: Vec<&Reviewer> = roster.iter().filter(|r| r.enabled).collect();
    let review_config = config::review_config(req.root).map_err(|e| e.to_string())?;
    let threshold = review_config.threshold;
    let timeout = Duration::from_secs(review_config.timeout_seconds);

    let sources = Sources {
        root: req.root,
        base: req.base,
        work_item: req.work_item,
    };

    let run_id = format!("review-{}-{}", now_millis(), std::process::id());
    let mut journal_error = None;
    let mut journal = match Journal::open(state_dir, &run_id) {
        Ok(j) => Some(j),
        Err(e) => {
            journal_error = Some(e);
            None
        }
    };

    let mut decided: Vec<(&Lens, LensVerdict)> = Vec::with_capacity(selected.len());
    let mut lines = Vec::with_capacity(selected.len() + 1);
    let mut findings = Vec::new();

    for selected_lens in &selected {
        let lens = selected_lens.lens;
        let (verdict, attempts) = run_lens(req.root, lens, depth, &sources, &enabled, timeout);
        for attempt in attempts {
            if let Some(journal) = journal.as_mut() {
                let event = attempt.into_event(&lens.name);
                if let Err(e) = journal.append(ACTOR, now_millis(), Payload::ReviewAnswer(event.0))
                {
                    journal_error.get_or_insert(e);
                }
                findings.extend(event.1);
            } else {
                findings.extend(attempt.kept_findings(&lens.name));
            }
        }
        lines.push(format!("{}: {}", lens.name, verdict_label(&verdict)));
        decided.push((lens, verdict));
    }

    let verdict = reducer::decide(&decided, threshold);
    let score = reducer::weighted_mean(&decided);
    let reported_threshold = score.map(|_| threshold);
    let lens_summaries: Vec<(String, String)> = decided
        .iter()
        .map(|(lens, verdict)| (lens.name.clone(), verdict_label(verdict)))
        .collect();

    if let Some(journal) = journal.as_mut() {
        if let Err(e) = journal.append(
            ACTOR,
            now_millis(),
            Payload::ReviewDecision(ReviewDecision {
                verdict: verdict_word(verdict).to_string(),
                lenses: lens_summaries,
                score,
                threshold: reported_threshold,
            }),
        ) {
            journal_error.get_or_insert(e);
        }
    }

    lines.push(match score {
        Some(score) => format!(
            "verdict: {} (score {score:.2}, threshold {threshold:.2})",
            verdict_word(verdict)
        ),
        None => format!("verdict: {}", verdict_word(verdict)),
    });

    Ok(RunOutcome {
        verdict,
        lines,
        findings,
        journal_error,
    })
}

/// One reviewer's attempt at answering for one lens, kept just long enough
/// to build both the journal event and the reducer's own input from it.
struct Attempt {
    lens_answer: LensAnswer,
    result: &'static str,
    findings_kept: u32,
    findings_dropped: u32,
    kept: Vec<AnswerFinding>,
}

impl Attempt {
    /// The journal event this attempt writes, and the kept findings it
    /// contributes, named by `lens`.
    fn into_event(self, lens: &str) -> (ReviewAnswer, Vec<KeptFinding>) {
        let scores = self
            .lens_answer
            .answer
            .as_ref()
            .map(|a| a.scores().clone())
            .unwrap_or_default();
        let found = self.kept_findings(lens);
        let event = ReviewAnswer {
            lens: lens.to_string(),
            reviewer: self.lens_answer.reviewer,
            family: self.lens_answer.family,
            result: self.result.to_string(),
            scores,
            findings_kept: self.findings_kept,
            findings_dropped: self.findings_dropped,
            transcript: None,
            reason: self.lens_answer.reason,
        };
        (event, found)
    }

    /// This attempt's kept findings, named by `lens`, without consuming `self`.
    fn kept_findings(&self, lens: &str) -> Vec<KeptFinding> {
        self.kept
            .iter()
            .cloned()
            .map(|finding| KeptFinding {
                lens: lens.to_string(),
                finding,
            })
            .collect()
    }
}

/// Builds `lens`'s context, then runs enabled reviewers over it in roster
/// order until two families have answered or the roster runs out. A
/// context failure makes the whole lens could-not-run, naming the reason,
/// with no reviewer ever asked.
fn run_lens(
    root: &Path,
    lens: &Lens,
    depth: Depth,
    sources: &Sources,
    enabled: &[&Reviewer],
    timeout: Duration,
) -> (LensVerdict, Vec<Attempt>) {
    let context = match review_context::build(lens, depth, sources) {
        Ok(context) => context,
        Err(reason) => return (LensVerdict::CouldNotRun(reason), Vec::new()),
    };
    let prompt = build_prompt(lens, &context);

    let mut attempts = Vec::new();
    let mut families: BTreeSet<String> = BTreeSet::new();
    for reviewer in enabled {
        if families.len() >= QUORUM_FAMILIES {
            break;
        }
        let attempt = attempt_reviewer(root, reviewer, &prompt, lens, timeout);
        if attempt.lens_answer.answer.is_some() {
            families.insert(attempt.lens_answer.family.clone());
        }
        attempts.push(attempt);
    }

    let lens_answers: Vec<LensAnswer> = attempts.iter().map(|a| a.lens_answer.clone()).collect();
    let verdict = reducer::decide_lens(lens, &lens_answers);
    (verdict, attempts)
}

/// Runs one reviewer for one lens, and turns its outcome into the shape
/// both the journal and the reducer need: an answered reviewer's findings
/// go through [`quotes::check`] first, so a lens can never be scored from a
/// finding nothing has verified.
fn attempt_reviewer(
    root: &Path,
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    timeout: Duration,
) -> Attempt {
    match reviewers::run_one(reviewer, prompt, lens, root, timeout) {
        Outcome::Answered(answer) => {
            let checked = quotes::check(root, answer);
            let findings_kept = as_u32(checked.kept.findings().len());
            let findings_dropped = as_u32(checked.dropped.len());
            let kept = checked.kept.findings().to_vec();
            Attempt {
                lens_answer: LensAnswer {
                    reviewer: reviewer.name.clone(),
                    family: reviewer.family.clone(),
                    answer: Some(checked.kept),
                    reason: None,
                },
                result: "answered",
                findings_kept,
                findings_dropped,
                kept,
            }
        }
        Outcome::Invalid(reason) => Attempt {
            lens_answer: LensAnswer {
                reviewer: reviewer.name.clone(),
                family: reviewer.family.clone(),
                answer: None,
                reason: Some(reason),
            },
            result: "invalid",
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
        },
        Outcome::CouldNotRun(reason) => Attempt {
            lens_answer: LensAnswer {
                reviewer: reviewer.name.clone(),
                family: reviewer.family.clone(),
                answer: None,
                reason: Some(reason),
            },
            result: "could-not-run",
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
        },
    }
}

/// The full prompt sent to a reviewer for `lens`: its own questions and
/// severity guide, then `context` (the diff and whatever else the lens
/// declared it needs, already assembled and redacted by
/// [`review_context::build`]).
fn build_prompt(lens: &Lens, context: &str) -> String {
    let mut criteria = String::new();
    for criterion in &lens.criteria {
        let _ = writeln!(criteria, "- {}: {}", criterion.id, criterion.question);
    }
    format!(
        "You are reviewing a change through the \"{name}\" lens: {summary}\n\n\
         Score each criterion from 0 to 1:\n{criteria}\n\
         Severity guide - blocker: {blocker}; major: {major}; minor: {minor}.\n\n\
         Answer only with JSON matching the review-answer schema for lens \"{name}\": an \
         object with \"lens\", \"scores\" (one entry per criterion id above) and \"findings\" \
         (each with \"path\", \"line\", \"quote\", \"severity\", \"action\" and \"body\"). A \
         finding's \"quote\" must be the exact text at its \"path\" and \"line\".\n\n{context}",
        name = lens.name,
        summary = lens.summary,
        blocker = lens.severity_guide.blocker,
        major = lens.severity_guide.major,
        minor = lens.severity_guide.minor,
    )
}

/// One lens's verdict, rendered for a journal event's `lenses` list and for
/// the printed per-lens line.
fn verdict_label(verdict: &LensVerdict) -> String {
    match verdict {
        LensVerdict::Pass { score } => format!("pass (score {score:.2})"),
        LensVerdict::Fail { score, blockers } => {
            format!("fail (score {score:.2}, {blockers} blocker(s))")
        }
        LensVerdict::CouldNotRun(reason) => format!("could-not-run: {reason}"),
    }
}

/// The whole review's verdict, as the word the journal and the exit code agree on.
fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Fail => "fail",
        Verdict::CouldNotRun => "could-not-run",
    }
}

/// `n` as a `u32`, saturating rather than panicking on a count this module
/// never expects to see in practice.
fn as_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The current wall-clock time in whole milliseconds since the epoch, for
/// journal timestamps only: [`crate::journal::event_hash`] never reads it.
fn now_millis() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}
