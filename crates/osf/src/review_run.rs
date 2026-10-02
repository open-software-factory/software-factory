//! `osf review run`: selects the lenses a change earns, runs reviewers from
//! the roster over each one, checks their findings against the files,
//! decides a verdict, and journals every answer and the decision.
//!
//! The work splits in two so each reviewer can run alone, with only its own
//! provider's key. [`run_reviewer`] runs one reviewer over every selected
//! lens and returns a [`ReviewerRun`], which can be saved as a file. [`reduce`]
//! takes the saved runs of every reviewer, checks the findings against the
//! files, journals, and decides. [`run`] is both in one process: every
//! roster reviewer in turn, then [`reduce`].
//!
//! A reviewer is whatever coding-agent tool is already installed and
//! logged in on this machine; this module never reads or holds a model
//! provider's key. A reviewer starts in a clean copy of the change, with no
//! coding agent's settings in it ([`CleanCopy`]). A reviewer that fails to
//! run counts as could-not-run for that lens; could-not-run never passes.
//! Nothing journalled here carries a prompt or a raw answer:
//! `review_context` has already redacted the metadata, and `transcript` is a
//! path or nothing, never the text itself. A reviewer's findings are
//! reviewer-written text too, so the exact values of the secrets the job
//! holds are removed from their path, quote and body, and the same pattern
//! redaction is applied, before they leave a reviewer run, and again before
//! they reach SARIF, a printed line, or a later posted review.

use crate::answer::{Answer, AnswerFinding};
use crate::builder;
use crate::clean_copy::CleanCopy;
use crate::journal::{Journal, Payload, ReviewAnswer, ReviewDecision};
use crate::lenses::{self, Catalogue, Lens};
use crate::quotes;
use crate::reducer::{self, LensAnswer, LensVerdict, Verdict};
use crate::review_context::{self, ChangeFolder, PullRequest, Sources};
use crate::review_prompt;
use crate::reviewers::{self, Outcome, Reviewer};
use crate::secret_values;
use crate::{changeset_risk, config, git};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many independent rounds a reviewer is asked for once chosen: more
/// than one, so a single answer's own noise never alone decides a family's
/// judgment.
const ROUNDS_PER_FAMILY: usize = 2;

/// The interim policy's critical round. Every reviewer that answered asks
/// once more beyond [`ROUNDS_PER_FAMILY`] and saves it marked as critical,
/// because it cannot know whether another family answered. [`reduce`] counts a
/// family's critical round only when exactly one family answered. Remove this,
/// and the policy that goes with it, once a second family is a real
/// requirement rather than a goal.
const CRITICAL_ROUND: u32 = 3;

const ACTOR: &str = "osf";

/// What one run of `osf review run` needs: the repository, what to diff
/// against, the work item file the caller saved, if any, the pull request,
/// if any, and where the trusted configuration lives.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub root: &'a Path,
    /// Where the lens catalogue, the prompt file and the `[review]` table of
    /// `osf.toml` (roster, threshold, timeout, cost ceiling) are read from.
    /// A pull request under review must not be able to weaken its own review
    /// by editing a lens, the prompt or the threshold, so this is the base
    /// tree when the caller passes one, never `root` in that case. Defaults
    /// to `root` when the caller has no separate trusted tree.
    pub config_root: &'a Path,
    pub base: &'a str,
    pub work_item: Option<&'a Path>,
    pub pull_request: Option<&'a PullRequest>,
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

/// One attempt of one reviewer at one lens, as saved. The family and model
/// are not saved: [`reduce`] reads them from the trusted roster.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Attempt {
    /// `answered`, `invalid` or `could-not-run`.
    pub result: String,
    pub reason: Option<String>,
    /// What the run noted, such as a declared login path that was missing.
    pub notes: Vec<String>,
    /// The reviewer's own attempt number for this lens, one-based.
    pub round: u32,
    /// Whether this is the critical round. [`reduce`] counts it only when
    /// exactly one family answered.
    #[serde(default)]
    pub critical: bool,
    /// The validated answer, its findings not yet checked against the files.
    pub answer: Option<Answer>,
}

/// One reviewer's attempts at one lens.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensRun {
    pub lens: String,
    /// Why this lens's metadata could not be built, when it could not.
    pub context_error: Option<String>,
    pub attempts: Vec<Attempt>,
}

/// Everything one reviewer did over a change, saved as one file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewerRun {
    pub reviewer: String,
    pub lenses: Vec<LensRun>,
}

impl ReviewerRun {
    /// Writes this run to `path` as JSON.
    ///
    /// # Errors
    /// Names the file and the reason when it cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Reads a run saved by [`ReviewerRun::save`].
    ///
    /// # Errors
    /// Names the file and the reason when it cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Everything a run reads from the trusted config root and the repository
/// before any reviewer starts.
struct Setup {
    catalogue: Catalogue,
    /// The names of the selected lenses, in catalogue order.
    selected: Vec<String>,
    roster: Vec<Reviewer>,
    threshold: f64,
    timeout: Duration,
    prompt: String,
    builder_families: Vec<String>,
    skip_families: BTreeSet<String>,
}

impl Setup {
    fn selected_lenses(&self) -> Vec<&Lens> {
        self.catalogue
            .lenses
            .iter()
            .filter(|lens| self.selected.contains(&lens.name))
            .collect()
    }
}

/// # Errors
/// Returns an error, naming the file, when the lens catalogue or the prompt
/// file fails to load; naming the reason when the risk assessment, the
/// changed-file list, or the reviewed range's own commits cannot be read; or
/// when the reviewer roster or the review settings cannot be read from
/// `osf.toml`. None of these leaves a lens to blame, so the caller reports
/// could-not-configure rather than picking one lens to fail.
fn setup(req: &Request) -> Result<Setup, String> {
    let catalogue = lenses::load(req.config_root, None)?;
    let report = changeset_risk::assess(req.root, req.config_root, req.base)?;
    let changed = git::changed_files(req.root, req.base).map_err(|e| e.to_string())?;
    let signals = report.signals();
    let selected: Vec<String> = lenses::select(&catalogue, &changed, &signals, report.tier)
        .iter()
        .map(|s| s.lens.name.clone())
        .collect();

    let roster = reviewers::roster(req.config_root)?;
    let review_config = config::review_config(req.config_root).map_err(|e| e.to_string())?;
    let prompt = review_prompt::load(req.config_root, review_config.prompt_file.as_deref())?;
    let (builder_families, skip_families) = detect_builder_families(req, &review_config)?;
    Ok(Setup {
        catalogue,
        selected,
        roster,
        threshold: review_config.threshold,
        timeout: Duration::from_secs(review_config.timeout_seconds),
        prompt,
        builder_families,
        skip_families,
    })
}

/// Runs every roster reviewer in turn, then decides: [`run_reviewer`] for
/// each, then [`reduce`], journaling every answer plus the decision to
/// `state_dir`.
///
/// # Errors
/// Returns an error when [`setup`] fails; see there.
pub fn run(req: &Request, state_dir: &Path) -> Result<RunOutcome, String> {
    let setup = setup(req)?;
    let runs: Vec<ReviewerRun> = setup
        .roster
        .iter()
        .map(|reviewer| run_reviewer_with(req, &setup, reviewer))
        .collect();
    Ok(reduce_with(req, &setup, &runs, state_dir))
}

/// Runs the roster reviewer called `name` over every selected lens, and
/// returns what it did, unchecked and unjournaled. A reviewer whose family
/// built the change, or whose family is unknown, is not started: its run
/// holds no attempts.
///
/// # Errors
/// Returns an error when [`setup`] fails, or the roster has no reviewer
/// called `name`.
pub fn run_reviewer(req: &Request, name: &str) -> Result<ReviewerRun, String> {
    let setup = setup(req)?;
    let reviewer = setup
        .roster
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("reviewer \"{name}\" is not in the roster"))?;
    Ok(run_reviewer_with(req, &setup, reviewer))
}

/// What one reviewer's run works in: the folder that holds the diff, and the
/// clean copy of the change it starts in. Both are removed when this is dropped.
struct Workspace {
    change: ChangeFolder,
    copy: CleanCopy,
}

impl Workspace {
    fn create(sources: &Sources) -> Result<Self, String> {
        Ok(Self {
            change: ChangeFolder::create(sources)?,
            copy: CleanCopy::create(sources.root)?,
        })
    }
}

fn run_reviewer_with(req: &Request, setup: &Setup, reviewer: &Reviewer) -> ReviewerRun {
    let sources = Sources {
        root: req.root,
        config_root: req.config_root,
        base: req.base,
        work_item: req.work_item,
        pull_request: req.pull_request,
    };
    let idle = reviewer.family_error.is_some() || reviewer.is_excluded_by(&setup.skip_families);
    // Written once, before any reviewer starts, and removed when this run ends.
    let change = if idle {
        None
    } else {
        Some(Workspace::create(&sources))
    };
    let mut reviewer = reviewer.clone();
    if let Some(Ok(workspace)) = &change {
        reviewer.review_dir = Some(workspace.change.dir().to_path_buf());
    }
    let reviewer = &reviewer;
    let lenses = setup
        .selected_lenses()
        .into_iter()
        .map(|lens| {
            let mut run = LensRun {
                lens: lens.name.clone(),
                context_error: None,
                attempts: Vec::new(),
            };
            let workspace = match &change {
                None => return run,
                Some(Err(reason)) => {
                    run.context_error = Some(reason.clone());
                    return run;
                }
                Some(Ok(workspace)) => workspace,
            };
            match review_context::build(lens, &sources) {
                Err(reason) => run.context_error = Some(reason),
                Ok(metadata) => {
                    let file = workspace.change.file().to_string_lossy().into_owned();
                    let prompt = review_prompt::render(&setup.prompt, lens, &metadata, &file);
                    let attempts = Attempts {
                        root: req.root,
                        config_root: req.config_root,
                        workdir: workspace.copy.dir(),
                        reviewer,
                        prompt: &prompt,
                        lens,
                        timeout: setup.timeout,
                    };
                    run.attempts = attempts.run();
                }
            }
            run
        })
        .collect();
    ReviewerRun {
        reviewer: reviewer.name.clone(),
        lenses,
    }
}

/// One reviewer's attempts at one lens, run in `workdir`.
struct Attempts<'a> {
    root: &'a Path,
    config_root: &'a Path,
    /// The clean copy of the change the reviewer starts in.
    workdir: &'a Path,
    reviewer: &'a Reviewer,
    prompt: &'a str,
    lens: &'a Lens,
    timeout: Duration,
}

impl Attempts<'_> {
    /// [`ROUNDS_PER_FAMILY`] independent rounds, and, when the reviewer
    /// answered at all, the critical round, saved marked as critical.
    fn run(&self) -> Vec<Attempt> {
        let mut attempts: Vec<Attempt> = (1..=ROUNDS_PER_FAMILY)
            .map(|round| self.once(as_u32(round), false))
            .collect();
        if attempts.iter().any(|a| a.answer.is_some()) {
            attempts.push(self.once(CRITICAL_ROUND, true));
        }
        attempts
    }

    /// Runs the reviewer once. An answer's findings are redacted here, so
    /// nothing a reviewer wrote is saved as it was written.
    fn once(&self, round: u32, critical: bool) -> Attempt {
        let (outcome, notes) = reviewers::run_one_noted(
            self.reviewer,
            self.prompt,
            self.lens,
            self.workdir,
            self.timeout,
        );
        let (result, reason, answer) = match outcome {
            Outcome::Answered(answer) => (
                "answered",
                None,
                Some(redact_answer(self.root, self.config_root, answer)),
            ),
            Outcome::Invalid(reason) => ("invalid", Some(reason), None),
            Outcome::CouldNotRun(reason) => ("could-not-run", Some(reason), None),
        };
        Attempt {
            result: result.to_string(),
            reason,
            notes,
            round,
            critical,
            answer,
        }
    }
}

/// Decides the review from the saved `runs` of the roster's reviewers,
/// journaling every answer plus the decision to `state_dir`. A roster
/// reviewer with no run, or no run for a lens, counts as could-not-run for
/// it, naming that.
///
/// # Errors
/// Returns an error when [`setup`] fails; see there.
pub fn reduce(req: &Request, runs: &[ReviewerRun], state_dir: &Path) -> Result<RunOutcome, String> {
    let setup = setup(req)?;
    Ok(reduce_with(req, &setup, runs, state_dir))
}

fn reduce_with(req: &Request, setup: &Setup, runs: &[ReviewerRun], state_dir: &Path) -> RunOutcome {
    let run_id = format!("review-{}-{}", now_millis(), std::process::id());
    let mut journal_error = None;
    let mut journal = match Journal::open(state_dir, &run_id) {
        Ok(j) => Some(j),
        Err(e) => {
            journal_error = Some(e);
            None
        }
    };

    let selected = setup.selected_lenses();
    let mut decided: Vec<(&Lens, LensVerdict)> = Vec::with_capacity(selected.len());
    let mut lens_summaries: Vec<(String, String)> = Vec::with_capacity(selected.len());
    let mut lines = Vec::with_capacity(selected.len() + 1);
    let mut findings = Vec::new();

    for lens in selected {
        let (verdict, judged, interim) = judge_lens(req, setup, runs, lens);
        for attempt in judged {
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
        let label = verdict_label(&verdict, interim);
        lines.push(format!("{}: {label}", lens.name));
        lens_summaries.push((lens.name.clone(), label));
        decided.push((lens, verdict));
    }

    let verdict = reducer::decide(&decided, setup.threshold);
    let score = reducer::weighted_mean(&decided);
    let reported_threshold = score.map(|_| setup.threshold);

    if let Some(journal) = journal.as_mut() {
        if let Err(e) = journal.append(
            ACTOR,
            now_millis(),
            Payload::ReviewDecision(ReviewDecision {
                verdict: verdict_word(verdict).to_string(),
                lenses: lens_summaries,
                score,
                threshold: reported_threshold,
                builder_families: setup.builder_families.clone(),
                grade: "reported".to_string(),
            }),
        ) {
            journal_error.get_or_insert(e);
        }
    }

    lines.push(match score {
        Some(score) => format!(
            "verdict: {} (score {score:.2}, threshold {:.2})",
            verdict_word(verdict),
            setup.threshold
        ),
        None => format!("verdict: {}", verdict_word(verdict)),
    });

    RunOutcome {
        verdict,
        lines,
        findings,
        journal_error,
    }
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

/// One attempt after its findings are checked against the files, ready for
/// the journal and for the reducer.
struct Judged {
    lens_answer: LensAnswer,
    /// The model the reviewer's roster entry pinned, if any.
    model: Option<String>,
    result: String,
    findings_kept: u32,
    findings_dropped: u32,
    kept: Vec<AnswerFinding>,
    round: u32,
    notes: Vec<String>,
}

impl Judged {
    /// An attempt that never ran, or never answered: `answer` stays `None`,
    /// so [`reducer::decide_lens`] counts it as missing.
    fn missing(reviewer: &Reviewer, result: &str, reason: String) -> Self {
        Judged {
            lens_answer: LensAnswer {
                reviewer: reviewer.name.clone(),
                family: reviewer.family.clone(),
                answer: None,
                reason: Some(reason),
            },
            model: reviewer.model.clone(),
            result: result.to_string(),
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round: 1,
            notes: Vec::new(),
        }
    }

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
        let reason = self
            .lens_answer
            .reason
            .into_iter()
            .chain(self.notes)
            .collect::<Vec<_>>();
        let event = ReviewAnswer {
            lens: lens.to_string(),
            reviewer: self.lens_answer.reviewer,
            family: self.lens_answer.family,
            model: self.model,
            result: self.result,
            scores,
            findings_kept: self.findings_kept,
            findings_dropped: self.findings_dropped,
            transcript: None,
            reason: (!reason.is_empty()).then(|| reason.join("; ")),
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

/// One lens's verdict from every roster reviewer's attempts at it, and
/// whether the interim policy decided it: exactly one family answered, so
/// that family's critical round counts. With no family or two or more, the
/// critical rounds are ignored. A reviewer whose family built this change is
/// never counted: it is not an independent second opinion on its own change.
fn judge_lens(
    req: &Request,
    setup: &Setup,
    runs: &[ReviewerRun],
    lens: &Lens,
) -> (LensVerdict, Vec<Judged>, bool) {
    let mut judged: Vec<Judged> = Vec::new();
    let mut context_error: Option<String> = None;
    let mut ran: Vec<(&Reviewer, &LensRun)> = Vec::new();
    for reviewer in &setup.roster {
        if let Some(reason) = &reviewer.family_error {
            judged.push(Judged::missing(reviewer, "could-not-run", reason.clone()));
            continue;
        }
        if reviewer.is_excluded_by(&setup.skip_families) {
            let reason = format!("the builder's own family ({})", reviewer.family);
            judged.push(Judged::missing(reviewer, "skipped", reason));
            continue;
        }
        let lens_run = runs
            .iter()
            .find(|r| r.reviewer == reviewer.name)
            .and_then(|r| r.lenses.iter().find(|l| l.lens == lens.name));
        match lens_run {
            Some(run) if run.context_error.is_some() => {
                context_error = context_error.or_else(|| run.context_error.clone());
            }
            Some(run) if !run.attempts.is_empty() => ran.push((reviewer, run)),
            _ => judged.push(Judged::missing(
                reviewer,
                "could-not-run",
                "its run left no answer for this lens".to_string(),
            )),
        }
    }
    if let Some(reason) = context_error {
        return (LensVerdict::CouldNotRun(reason), Vec::new(), false);
    }
    let answered: BTreeSet<&str> = ran
        .iter()
        .filter(|(_, run)| {
            run.attempts
                .iter()
                .any(|a| !a.critical && a.result == "answered" && a.answer.is_some())
        })
        .map(|(reviewer, _)| reviewer.family.as_str())
        .collect();
    let interim = answered.len() == 1;
    for (reviewer, run) in ran {
        let counts_critical = interim && answered.contains(reviewer.family.as_str());
        judged.extend(
            run.attempts
                .iter()
                .filter(|saved| !saved.critical || counts_critical)
                .map(|saved| judge_attempt(req, reviewer, saved)),
        );
    }
    let answers: Vec<LensAnswer> = judged.iter().map(|a| a.lens_answer.clone()).collect();
    let verdict = name_builder_exclusion(
        reducer::decide_lens(lens, &answers, interim),
        &setup.skip_families,
        &judged,
    );
    (verdict, judged, interim)
}

/// `saved`, with an answer's findings checked against the files under
/// `req.root` through [`quotes::check`], so a lens can never be scored from
/// a finding nothing has verified, whoever wrote the file.
fn judge_attempt(req: &Request, reviewer: &Reviewer, saved: &Attempt) -> Judged {
    let (answer, reason, result) = match (&saved.answer, saved.result.as_str()) {
        (Some(answer), "answered") => (Some(answer.clone()), None, "answered"),
        (None, "answered") => (
            None,
            Some("the saved run holds no answer".to_string()),
            "could-not-run",
        ),
        (_, "invalid") => (None, saved.reason.clone(), "invalid"),
        _ => (None, saved.reason.clone(), "could-not-run"),
    };
    let mut judged = Judged::missing(reviewer, result, String::new());
    judged.lens_answer.reason = reason;
    judged.round = saved.round;
    judged.notes.clone_from(&saved.notes);
    if let Some(answer) = answer {
        let checked = quotes::check(req.root, answer);
        judged.findings_kept = as_u32(checked.kept.findings().len());
        judged.findings_dropped = as_u32(checked.dropped.len());
        judged.kept = checked
            .kept
            .findings()
            .iter()
            .map(|finding| redact_finding(req.root, req.config_root, finding))
            .collect();
        judged.lens_answer.answer = Some(checked.kept);
    }
    judged
}

/// `verdict`, with its could-not-run reason naming the builder families when
/// at least one reviewer was left out of `judged` for building this change:
/// a reader seeing too few families answer should learn why, not just that
/// it happened. Every other verdict, and a could-not-run one with no
/// builder-family skip behind it, passes through unchanged.
fn name_builder_exclusion(
    verdict: LensVerdict,
    skip_families: &BTreeSet<String>,
    judged: &[Judged],
) -> LensVerdict {
    let any_skipped = judged.iter().any(|a| a.result == "skipped");
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

/// `answer`, with every finding's path, quote and body redacted.
fn redact_answer(root: &Path, config_root: &Path, answer: Answer) -> Answer {
    Answer {
        findings: answer
            .findings
            .iter()
            .map(|finding| redact_finding(root, config_root, finding))
            .collect(),
        ..answer
    }
}

/// `finding`, with its path, quote and body redacted the same way
/// [`review_context::build`] redacts a prompt: a quote is real text the
/// reviewer copied out of the repository, and the path and body are the
/// reviewer's own unverified prose, so none is trusted just because the
/// finding survived quote verification.
///
/// Falls back to a fixed placeholder, never the original text, if the
/// redaction rules themselves cannot be built.
fn redact_finding(root: &Path, config_root: &Path, finding: &AnswerFinding) -> AnswerFinding {
    AnswerFinding {
        path: redact_reviewer_text(root, config_root, &finding.path),
        quote: redact_reviewer_text(root, config_root, &finding.quote),
        body: redact_reviewer_text(root, config_root, &finding.body),
        line: finding.line,
        severity: finding.severity,
        action: finding.action,
    }
}

/// `text`, with the exact secrets this job holds removed, then redacted
/// through [`review_context::redact_secrets`], the same function a
/// reviewer's own prompt is redacted through.
fn redact_reviewer_text(root: &Path, config_root: &Path, text: &str) -> String {
    let text = secret_values::scrub_held(text);
    review_context::redact_secrets(root, config_root, &text).map_or_else(
        |_| "[could not verify this text is safe to show]".to_string(),
        |(redacted, _)| redacted,
    )
}

/// One lens's verdict, rendered for a journal event's `lenses` list and for
/// the printed per-lens line. `interim` names the interim policy on a
/// `Pass` or `Fail`: it is true when exactly one family answered, so that
/// family's own critical round counted toward the lens (see [`CRITICAL_ROUND`]).
fn verdict_label(verdict: &LensVerdict, interim: bool) -> String {
    let interim = if interim {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn judged(reason: Option<&str>, notes: &[&str]) -> Judged {
        Judged {
            lens_answer: LensAnswer {
                reviewer: "r".to_string(),
                family: "f".to_string(),
                answer: None,
                reason: reason.map(str::to_string),
            },
            model: None,
            result: "answered".to_string(),
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round: 1,
            notes: notes.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn the_journal_event_carries_the_runs_notes_in_its_reason() {
        let (event, _) = judged(None, &["login path \"x\" is missing"]).into_event("lens");
        assert_eq!(event.reason.as_deref(), Some("login path \"x\" is missing"));
        let (event, _) = judged(Some("bad"), &["note"]).into_event("lens");
        assert_eq!(event.reason.as_deref(), Some("bad; note"));
        let (event, _) = judged(None, &[]).into_event("lens");
        assert_eq!(event.reason, None);
    }

    #[test]
    fn a_saved_run_reads_back_as_it_was_written() {
        let dir = crate::test_support::TempDir::new("osf-review-run-save");
        let run = ReviewerRun {
            reviewer: "codex".to_string(),
            lenses: vec![LensRun {
                lens: "correctness".to_string(),
                context_error: None,
                attempts: vec![Attempt {
                    result: "could-not-run".to_string(),
                    reason: Some("timed out".to_string()),
                    notes: vec!["note".to_string()],
                    round: 2,
                    critical: false,
                    answer: None,
                }],
            }],
        };
        let path = dir.join("codex.json");
        run.save(&path).expect("saves");
        let back = ReviewerRun::load(&path).expect("loads");
        assert_eq!(back.reviewer, "codex");
        let attempt = back
            .lenses
            .first()
            .and_then(|l| l.attempts.first())
            .expect("one attempt");
        assert_eq!(attempt.round, 2);
        assert_eq!(attempt.reason.as_deref(), Some("timed out"));
    }

    #[test]
    fn a_file_that_is_not_a_saved_run_names_itself() {
        let dir = crate::test_support::TempDir::new("osf-review-run-bad-file");
        let path = dir.join("bad.json");
        std::fs::write(&path, "not json").expect("writes");
        let e = ReviewerRun::load(&path).expect_err("refused");
        assert!(e.contains("bad.json"), "{e}");
    }
}
