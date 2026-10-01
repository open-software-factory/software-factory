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
//! `transcript` is a path or nothing, never the text itself. A verified
//! finding's own quote and body are reviewer-written text too, so both are
//! redacted the same way before they ever leave this module, whether they
//! end up in SARIF, a printed line, or a later posted review.

use crate::answer::AnswerFinding;
use crate::builder;
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

/// How many independent rounds a family's own reviewer is asked for once
/// chosen: more than one, so a single answer's own noise never alone
/// decides a family's judgment.
const ROUNDS_PER_FAMILY: usize = 2;

/// The interim policy's own extra round: when the roster ever offers only
/// one non-building family at all, that lone family is asked once more
/// beyond [`ROUNDS_PER_FAMILY`], as the critical round [`reducer::decide_lens`]
/// requires before it will let one family decide a lens on its own. Remove
/// this, and the policy that goes with it, once a second family is a real
/// requirement rather than a goal.
const INTERIM_EXTRA_ROUNDS: usize = 1;

const ACTOR: &str = "osf";

/// What one run of `osf review run` needs: the repository, what to diff
/// against, the work item file the caller saved, if any, and where the
/// trusted configuration lives.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub root: &'a Path,
    /// Where the lens catalogue and the `[review]` table of `osf.toml`
    /// (roster, threshold, timeout, cost ceiling) are read from. A pull
    /// request under review must not be able to weaken its own review by
    /// editing a lens or lowering the threshold, so this is the base tree
    /// when the caller passes one, never `root` in that case. Defaults to
    /// `root` when the caller has no separate trusted tree.
    pub config_root: &'a Path,
    pub base: &'a str,
    pub work_item: Option<&'a Path>,
    /// Builder families named with `--builder-family`, repeatable. Overrides
    /// detection from the reviewed range's own `Code-Generator:` trailers
    /// entirely when non-empty.
    pub builder_family_overrides: &'a [String],
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
/// load; naming the reason when the risk assessment, the changed-file
/// list, or the reviewed range's own commits cannot be read; or when the
/// reviewer roster or the review threshold cannot be read from
/// `osf.toml`. None of these leaves a lens to blame, so the caller
/// reports could-not-configure rather than picking one lens to fail.
pub fn run(req: &Request, state_dir: &Path) -> Result<RunOutcome, String> {
    let catalogue = lenses::load(req.config_root, None)?;
    let report = risk::assess(req.root, req.config_root, req.base)?;
    let changed = git::changed_files(req.root, req.base).map_err(|e| e.to_string())?;
    let signals = report.signals();
    let selected = lenses::select(&catalogue, &changed, &signals, report.tier);
    let depth = lenses::depth(report.tier);

    let roster = reviewers::roster(req.config_root)?;
    let enabled: Vec<&Reviewer> = roster.iter().filter(|r| r.enabled).collect();
    let review_config = config::review_config(req.config_root).map_err(|e| e.to_string())?;
    let threshold = review_config.threshold;
    let timeout = Duration::from_secs(review_config.timeout_seconds);

    let (builder_families, skip_families) = detect_builder_families(req, &review_config)?;

    // The interim policy (see [`INTERIM_EXTRA_ROUNDS`]): whether the roster
    // ever offers more than one non-building family to try, computed once
    // for the whole run since `enabled` and `skip_families` do not vary by
    // lens.
    let available_families: BTreeSet<&str> = enabled
        .iter()
        .filter(|r| !skip_families.contains(&r.family))
        .map(|r| r.family.as_str())
        .collect();
    let single_family_available = available_families.len() == 1;

    let sources = Sources {
        root: req.root,
        config_root: req.config_root,
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
        let (verdict, line) = run_lens_and_journal(
            lens,
            depth,
            &sources,
            &enabled,
            &skip_families,
            timeout,
            single_family_available,
            &mut journal,
            &mut journal_error,
            &mut findings,
        );
        lines.push(line);
        decided.push((lens, verdict));
    }

    let verdict = reducer::decide(&decided, threshold);
    let score = reducer::weighted_mean(&decided);
    let reported_threshold = score.map(|_| threshold);
    let lens_summaries: Vec<(String, String)> = decided
        .iter()
        .map(|(lens, verdict)| {
            (
                lens.name.clone(),
                verdict_label(verdict, single_family_available),
            )
        })
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
                builder_families,
                grade: "reported".to_string(),
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

/// Runs one lens, journals every attempt it made (or collects its kept
/// findings when the journal itself could not open), and returns its
/// verdict alongside the printable line for it. Pulled out of [`run`] only
/// to keep that function's own line count down; it owns no decision of its
/// own.
#[allow(clippy::too_many_arguments)]
fn run_lens_and_journal(
    lens: &Lens,
    depth: Depth,
    sources: &Sources,
    enabled: &[&Reviewer],
    skip_families: &BTreeSet<String>,
    timeout: Duration,
    single_family_available: bool,
    journal: &mut Option<Journal>,
    journal_error: &mut Option<String>,
    findings: &mut Vec<KeptFinding>,
) -> (LensVerdict, String) {
    let (verdict, attempts) = run_lens(
        lens,
        depth,
        sources,
        enabled,
        skip_families,
        timeout,
        single_family_available,
    );
    for attempt in attempts {
        if let Some(journal) = journal.as_mut() {
            let event = attempt.into_event(&lens.name);
            if let Err(e) = journal.append(ACTOR, now_millis(), Payload::ReviewAnswer(event.0)) {
                journal_error.get_or_insert(e);
            }
            findings.extend(event.1);
        } else {
            findings.extend(attempt.kept_findings(&lens.name));
        }
    }
    let line = format!(
        "{}: {}",
        lens.name,
        verdict_label(&verdict, single_family_available)
    );
    (verdict, line)
}

/// The builder families for `req`'s own reviewed range (see
/// [`builder::detect`]), and the subset of them a roster entry is left out
/// for: every detected family other than [`builder::UNKNOWN`], which names
/// no real family to leave a reviewer out for.
///
/// # Errors
/// Returns an error when git cannot read the reviewed range's own commits.
fn detect_builder_families(
    req: &Request,
    review_config: &config::ReviewConfig,
) -> Result<(Vec<String>, BTreeSet<String>), String> {
    let range = format!("{}..HEAD", req.base);
    let families = builder::detect(
        req.root,
        &range,
        &review_config.builder_family_aliases,
        req.builder_family_overrides,
    )?;
    let skip: BTreeSet<String> = families
        .iter()
        .filter(|family| family.as_str() != builder::UNKNOWN)
        .cloned()
        .collect();
    Ok((families, skip))
}

/// One reviewer's attempt at answering for one lens, kept just long enough
/// to build both the journal event and the reducer's own input from it.
struct Attempt {
    lens_answer: LensAnswer,
    /// The model the reviewer's roster entry pinned, if any, carried
    /// through to the journal untouched by anything this module decides.
    model: Option<String>,
    result: &'static str,
    findings_kept: u32,
    findings_dropped: u32,
    kept: Vec<AnswerFinding>,
    /// This reviewer's own attempt number for this lens, one-based.
    round: u32,
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
            model: self.model,
            result: self.result.to_string(),
            scores,
            findings_kept: self.findings_kept,
            findings_dropped: self.findings_dropped,
            transcript: None,
            reason: self.lens_answer.reason,
            grade: "reported".to_string(),
            round: self.round,
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
/// order: each family tried gets [`ROUNDS_PER_FAMILY`] independent rounds,
/// until two non-builder families have each had their rounds or the roster
/// runs out. When the roster ever offers only one non-building family at
/// all (`single_family_available`), that lone family is asked for
/// [`INTERIM_EXTRA_ROUNDS`] more, as the interim policy's own critical
/// round, rather than blocking the lens on a second family nobody has
/// configured yet. A reviewer whose family is in `skip_families` (the
/// families that built this change; see [`builder::detect`]) is never run:
/// it is not an independent second opinion on its own change. A context
/// failure makes the whole lens could-not-run, naming the reason, with no
/// reviewer ever asked.
fn run_lens(
    lens: &Lens,
    depth: Depth,
    sources: &Sources,
    enabled: &[&Reviewer],
    skip_families: &BTreeSet<String>,
    timeout: Duration,
    single_family_available: bool,
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
        if skip_families.contains(&reviewer.family) {
            attempts.push(skipped_attempt(reviewer));
            continue;
        }
        let mut rounds = ROUNDS_PER_FAMILY;
        if single_family_available {
            rounds += INTERIM_EXTRA_ROUNDS;
        }
        for round in 1..=rounds {
            let attempt = attempt_reviewer(
                sources.root,
                sources.config_root,
                reviewer,
                &prompt,
                lens,
                timeout,
                as_u32(round),
            );
            if attempt.lens_answer.answer.is_some() {
                families.insert(attempt.lens_answer.family.clone());
            }
            attempts.push(attempt);
        }
    }

    let lens_answers: Vec<LensAnswer> = attempts.iter().map(|a| a.lens_answer.clone()).collect();
    let verdict = name_builder_exclusion(
        reducer::decide_lens(lens, &lens_answers, single_family_available),
        skip_families,
        &attempts,
    );
    (verdict, attempts)
}

/// The attempt recorded for a reviewer never run because its family built
/// this change: no harness call, no finding, its `answer` left `None` so
/// [`reducer::decide_lens`] counts it as missing rather than a family that
/// answered.
fn skipped_attempt(reviewer: &Reviewer) -> Attempt {
    Attempt {
        lens_answer: LensAnswer {
            reviewer: reviewer.name.clone(),
            family: reviewer.family.clone(),
            answer: None,
            reason: Some(format!("the builder's own family ({})", reviewer.family)),
        },
        model: reviewer.model.clone(),
        result: "skipped",
        findings_kept: 0,
        findings_dropped: 0,
        kept: Vec::new(),
        round: 1,
    }
}

/// `verdict`, with its could-not-run reason naming `skip_families` when at
/// least one reviewer was left out of `attempts` for building this change:
/// a reader seeing too few families answer should learn why, not just that
/// it happened. Every other verdict, and a could-not-run one with no
/// builder-family skip behind it, passes through unchanged.
fn name_builder_exclusion(
    verdict: LensVerdict,
    skip_families: &BTreeSet<String>,
    attempts: &[Attempt],
) -> LensVerdict {
    let any_skipped = attempts.iter().any(|a| a.result == "skipped");
    match verdict {
        LensVerdict::CouldNotRun(reason) if any_skipped && !skip_families.is_empty() => {
            let names: Vec<&str> = skip_families.iter().map(String::as_str).collect();
            LensVerdict::CouldNotRun(format!(
                "{reason}; the builder's own family is left out: {}",
                names.join(", ")
            ))
        }
        other => other,
    }
}

/// Runs one reviewer for one lens, and turns its outcome into the shape
/// both the journal and the reducer need: an answered reviewer's findings
/// go through [`quotes::check`] first, so a lens can never be scored from a
/// finding nothing has verified.
fn attempt_reviewer(
    root: &Path,
    config_root: &Path,
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    timeout: Duration,
    round: u32,
) -> Attempt {
    match reviewers::run_one(reviewer, prompt, lens, root, timeout) {
        Outcome::Answered(answer) => {
            let checked = quotes::check(root, answer);
            let findings_kept = as_u32(checked.kept.findings().len());
            let findings_dropped = as_u32(checked.dropped.len());
            let kept = checked
                .kept
                .findings()
                .iter()
                .cloned()
                .map(|finding| redact_finding(root, config_root, finding))
                .collect();
            Attempt {
                lens_answer: LensAnswer {
                    reviewer: reviewer.name.clone(),
                    family: reviewer.family.clone(),
                    answer: Some(checked.kept),
                    reason: None,
                },
                model: reviewer.model.clone(),
                result: "answered",
                findings_kept,
                findings_dropped,
                kept,
                round,
            }
        }
        Outcome::Invalid(reason) => Attempt {
            lens_answer: LensAnswer {
                reviewer: reviewer.name.clone(),
                family: reviewer.family.clone(),
                answer: None,
                reason: Some(reason),
            },
            model: reviewer.model.clone(),
            result: "invalid",
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round,
        },
        Outcome::CouldNotRun(reason) => Attempt {
            lens_answer: LensAnswer {
                reviewer: reviewer.name.clone(),
                family: reviewer.family.clone(),
                answer: None,
                reason: Some(reason),
            },
            model: reviewer.model.clone(),
            result: "could-not-run",
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round,
        },
    }
}

/// `finding`, with its quote and body redacted the same way
/// [`review_context::build`] redacts a prompt: a verified quote is real
/// text the reviewer copied out of the repository, and a finding's body is
/// the reviewer's own unverified prose, so neither is trusted just because
/// the finding survived quote verification.
///
/// Falls back to a fixed placeholder, never the original text, if the
/// redaction rules themselves cannot be built; in practice this never
/// happens here, because [`review_context::build`] already built them once
/// for this same lens before any reviewer was ever asked.
fn redact_finding(root: &Path, config_root: &Path, finding: AnswerFinding) -> AnswerFinding {
    AnswerFinding {
        quote: redact_reviewer_text(root, config_root, &finding.quote),
        body: redact_reviewer_text(root, config_root, &finding.body),
        ..finding
    }
}

/// `text`, redacted through [`review_context::redact_secrets`], the same
/// function a reviewer's own prompt is redacted through.
fn redact_reviewer_text(root: &Path, config_root: &Path, text: &str) -> String {
    review_context::redact_secrets(root, config_root, text).map_or_else(
        |_| "[could not verify this text is safe to show]".to_string(),
        |(redacted, _)| redacted,
    )
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
/// the printed per-lens line. `single_family_available` names the interim
/// policy on a `Pass` or `Fail`: it can only be true there when the lone
/// family's own extra critical round is what let the lens decide at all
/// (see [`INTERIM_EXTRA_ROUNDS`]).
fn verdict_label(verdict: &LensVerdict, single_family_available: bool) -> String {
    let interim = if single_family_available {
        ", interim policy: one family available"
    } else {
        ""
    };
    match verdict {
        LensVerdict::Pass { score } => format!("pass (score {score:.2}{interim})"),
        LensVerdict::Fail { score, blockers } => {
            format!("fail (score {score:.2}, {blockers} blocker(s){interim})")
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
