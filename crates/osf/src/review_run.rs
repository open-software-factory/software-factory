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
//!
//! [`reduce`] checks every saved reviewer run: each answer again against
//! the schema and the lens, each file against the reviewer it is named for,
//! and the round and critical flags against the order of the attempts.

use crate::answer::{self, Answer, AnswerFinding};
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
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many independent rounds a reviewer is asked for once chosen: more
/// than one, so a single answer's own noise never alone decides a family's
/// judgment.
const ROUNDS_PER_FAMILY: usize = 2;

/// The interim policy's critical round. Every reviewer that answered asks
/// once more beyond [`ROUNDS_PER_FAMILY`] and saves it marked as critical,
/// because it cannot know whether another family answered. [`reduce`] counts a
/// family's critical round only when exactly one family answered both
/// independent rounds, and then it requires that critical round to have
/// answered. Remove this,
/// and the policy that goes with it, once a second family is a real
/// requirement rather than a goal.
const CRITICAL_ROUND: u32 = 3;

/// The most attempts one reviewer makes at one lens: the independent rounds and the critical round.
const MAX_ATTEMPTS: usize = ROUNDS_PER_FAMILY + 1;

const ACTOR: &str = "osf";

/// What a saved reviewer run belongs to: the repository, the pull request,
/// the base and head commits, and the CI run. [`reduce_saved`] accepts a saved
/// run only when its binding equals the one it was given, field for field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Binding {
    pub repository: String,
    pub pull_request: u64,
    pub base: String,
    pub head: String,
    pub run_id: String,
}

impl Binding {
    /// The binding for the checkout at `root` reviewed against `base`: the
    /// base is resolved to a commit, and the head is the commit checked out.
    /// A `head` the caller names must be that commit.
    ///
    /// # Errors
    /// Names what is missing or wrong: a repository that is not `owner/name`,
    /// a missing pull request number or CI run id, a base that does not
    /// resolve, or a `head` other than the checked-out commit.
    pub fn for_checkout(
        root: &Path,
        base: &str,
        repository: Option<&str>,
        pull_request: Option<u64>,
        head: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<Self, String> {
        let repository = repository
            .filter(|r| !r.is_empty())
            .ok_or("a repository is required: pass --repository or set GITHUB_REPOSITORY")?;
        let plain = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !repository
            .split_once('/')
            .is_some_and(|(owner, name)| plain(owner) && plain(name))
        {
            return Err(format!("the repository \"{repository}\" is not owner/name"));
        }
        let pull_request =
            pull_request.ok_or("a pull request number is required: pass --pull-request-number")?;
        let run_id = run_id
            .filter(|r| !r.is_empty())
            .ok_or("a CI run id is required: pass --ci-run-id or set GITHUB_RUN_ID")?;
        let resolved =
            git::resolve_rev(root, base).map_err(|e| format!("the base \"{base}\": {e}"))?;
        let checked_out = git::head_sha(root).map_err(|e| e.to_string())?;
        if let Some(head) = head {
            if !head.eq_ignore_ascii_case(&checked_out) {
                return Err(format!(
                    "the head {head} is not the commit checked out ({checked_out})"
                ));
            }
        }
        Ok(Self {
            repository: repository.to_string(),
            pull_request,
            base: resolved,
            head: checked_out,
            run_id: run_id.to_string(),
        })
    }
}

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
    /// What a saved run is bound to. [`run_reviewer`] writes it into the run
    /// it saves, and [`reduce_saved`] refuses a run whose binding differs.
    pub binding: Option<&'a Binding>,
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
    /// What the reduce step left out or could not use, such as a saved file
    /// for a reviewer outside the roster. Empty when nothing was dropped.
    pub notes: Vec<String>,
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
    /// exactly one family answered both independent rounds.
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
    /// What this run belongs to. [`reduce_saved`] refuses a run without the
    /// binding it was given.
    #[serde(default)]
    pub binding: Option<Binding>,
}

impl ReviewerRun {
    /// Whether this reviewer tried and never answered: it made at least one
    /// attempt, or could not build a lens's context, and no attempt on any
    /// lens answered. A reviewer that was idle, such as one left out for the
    /// builder's family, tried nothing and has not failed.
    #[must_use]
    pub fn never_answered(&self) -> bool {
        let tried = self
            .lenses
            .iter()
            .any(|l| l.context_error.is_some() || !l.attempts.is_empty());
        let answered = self
            .lenses
            .iter()
            .flat_map(|l| &l.attempts)
            .any(|a| a.result == "answered");
        tried && !answered
    }

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
    Ok(reduce_with(
        req,
        &setup,
        &runs,
        &BTreeMap::new(),
        Vec::new(),
        state_dir,
    ))
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
        work_item_binding: req.binding.map(|binding| review_context::WorkItemBinding {
            repository: &binding.repository,
            head: &binding.head,
        }),
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
        binding: req.binding.cloned(),
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

/// A reviewer's saved run, with the name of the file or artifact it came from.
#[derive(Debug, Clone)]
pub struct SavedRun {
    /// The file or artifact name without its extension. It must be the name
    /// of the reviewer the run holds.
    pub name: String,
    pub run: ReviewerRun,
}

/// Whether `name` is a plain reviewer name: letters, digits, `-` and `_`,
/// starting with a letter or digit. Checked before a file or artifact name is used.
#[must_use]
pub fn valid_artifact_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What [`bind`] made of the saved runs: the ones that may be used, the
/// reason each reviewer's file was refused, and what it dropped without a reviewer to blame.
#[derive(Debug, Default)]
struct Bound {
    accepted: Vec<ReviewerRun>,
    refused: BTreeMap<String, String>,
    notes: Vec<String>,
}

/// The runs `saved` holds that may be used. A file counts only when its name
/// is a plain name, it is the reviewer's own name, no other file carries it,
/// and its binding equals `expected`. With no `expected` binding, no file
/// counts, so a run is never taken on trust.
fn bind(saved: &[SavedRun], expected: Option<&Binding>) -> Bound {
    let mut bound = Bound::default();
    let mut copies: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in saved {
        *copies.entry(entry.name.as_str()).or_default() += 1;
    }
    let mut unnamed = 0_usize;
    for entry in saved {
        let reason = if !valid_artifact_name(&entry.name) {
            "a saved run came from a file whose name is not a plain reviewer name"
        } else if entry.name != entry.run.reviewer {
            "its saved run names another reviewer"
        } else if copies.get(entry.name.as_str()).copied().unwrap_or(0) > 1 {
            "more than one saved run carries its name"
        } else if expected.is_none() {
            "no binding was given to check its saved run against"
        } else if entry.run.binding.as_ref() != expected {
            "its saved run belongs to another pull request, commit or CI run"
        } else {
            bound.accepted.push(entry.run.clone());
            continue;
        };
        if valid_artifact_name(&entry.name) {
            bound.refused.insert(entry.name.clone(), reason.to_string());
        } else {
            unnamed += 1;
        }
    }
    if unnamed > 0 {
        bound.notes.push(format!(
            "ignored {unnamed} saved run(s) from a file whose name is not a plain reviewer name"
        ));
    }
    bound
}

/// `name` when it is plain text safe to print, else a fixed stand-in.
fn plain(name: &str) -> &str {
    if valid_artifact_name(name) {
        name
    } else {
        "(a name that is not plain text)"
    }
}

/// What the saved runs hold that the reduce step never reads: a run for a
/// reviewer outside `setup`'s roster, and a lens entry the change did not select.
fn dropped_entries(setup: &Setup, bound: &mut Bound) {
    let roster: BTreeSet<&str> = setup.roster.iter().map(|r| r.name.as_str()).collect();
    for name in bound
        .refused
        .keys()
        .filter(|n| !roster.contains(n.as_str()))
    {
        bound.notes.push(format!(
            "ignored the saved run for \"{name}\": that reviewer is not in the roster"
        ));
    }
    for run in &bound.accepted {
        if !roster.contains(run.reviewer.as_str()) {
            bound.notes.push(format!(
                "ignored the saved run for \"{}\": that reviewer is not in the roster",
                plain(&run.reviewer)
            ));
            continue;
        }
        for lens in &run.lenses {
            if !setup.selected.contains(&lens.lens) {
                bound.notes.push(format!(
                    "ignored the lens \"{}\" in the saved run for \"{}\": this change did not select it",
                    plain(&lens.lens),
                    plain(&run.reviewer)
                ));
            }
        }
    }
}

/// Decides the review from the saved `runs` of the roster's reviewers,
/// journaling every answer plus the decision to `state_dir`. A roster
/// reviewer with no run, or no run for a lens, counts as could-not-run for
/// it, naming that. Each run is taken to come from a file named for its reviewer.
///
/// # Errors
/// Returns an error when [`setup`] fails; see there.
pub fn reduce(req: &Request, runs: &[ReviewerRun], state_dir: &Path) -> Result<RunOutcome, String> {
    let saved: Vec<SavedRun> = runs
        .iter()
        .map(|run| SavedRun {
            name: run.reviewer.clone(),
            run: run.clone(),
        })
        .collect();
    reduce_saved(req, &saved, state_dir)
}

/// [`reduce`] for runs read from files or artifacts, each with the name it
/// came from. A run whose name is not its reviewer's, or that shares a name
/// with another, is refused, and that reviewer is could-not-run.
///
/// # Errors
/// Returns an error when [`setup`] fails; see there.
pub fn reduce_saved(
    req: &Request,
    saved: &[SavedRun],
    state_dir: &Path,
) -> Result<RunOutcome, String> {
    let setup = setup(req)?;
    let mut bound = bind(saved, req.binding);
    dropped_entries(&setup, &mut bound);
    Ok(reduce_with(
        req,
        &setup,
        &bound.accepted,
        &bound.refused,
        bound.notes,
        state_dir,
    ))
}

fn reduce_with(
    req: &Request,
    setup: &Setup,
    runs: &[ReviewerRun],
    refused: &BTreeMap<String, String>,
    notes: Vec<String>,
    state_dir: &Path,
) -> RunOutcome {
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
        let (verdict, judged, interim) = judge_lens(req, setup, runs, refused, lens);
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
        notes,
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
    /// Whether this is the critical round, worked out from the attempt's place in the run.
    critical: bool,
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
                critical: false,
            },
            model: reviewer.model.clone(),
            result: result.to_string(),
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round: 1,
            critical: false,
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
/// whether the interim policy decided it: exactly one family answered
/// [`reducer::ROUNDS_REQUIRED`] independent rounds, and its critical round
/// answered too, so that round counts. With two or more such families, the
/// critical rounds are ignored. A family with fewer answered rounds counts for
/// nothing, and the attempts that do not count lose their findings. A reviewer
/// whose family built this change is never counted: it is not an independent
/// second opinion on its own change.
fn judge_lens(
    req: &Request,
    setup: &Setup,
    runs: &[ReviewerRun],
    refused: &BTreeMap<String, String>,
    lens: &Lens,
) -> (LensVerdict, Vec<Judged>, bool) {
    let mut judged: Vec<Judged> = Vec::new();
    let mut context_error: Option<String> = None;
    let mut ran: Vec<(&Reviewer, Vec<Judged>)> = Vec::new();
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
        if let Some(reason) = refused.get(&reviewer.name) {
            judged.push(Judged::missing(reviewer, "could-not-run", reason.clone()));
            continue;
        }
        let mut lens_runs = runs
            .iter()
            .filter(|r| r.reviewer == reviewer.name)
            .flat_map(|r| r.lenses.iter())
            .filter(|l| l.lens == lens.name);
        let lens_run = lens_runs.next();
        if lens_runs.next().is_some() {
            judged.push(Judged::missing(
                reviewer,
                "could-not-run",
                "its saved run holds this lens more than once".to_string(),
            ));
            continue;
        }
        match lens_run {
            Some(run) if run.context_error.is_some() => {
                context_error = context_error.or_else(|| run.context_error.clone());
            }
            Some(run) if run.attempts.len() > MAX_ATTEMPTS => judged.push(Judged::missing(
                reviewer,
                "could-not-run",
                "its saved run holds more attempts than a run makes".to_string(),
            )),
            Some(run) if !run.attempts.is_empty() => {
                ran.push((reviewer, judge_attempts(req, reviewer, lens, &run.attempts)));
            }
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
    for (_, attempts) in ran {
        judged.extend(attempts);
    }
    let answers: Vec<LensAnswer> = judged.iter().map(|a| a.lens_answer.clone()).collect();
    let counted = reducer::count(&answers);
    let verdict = reducer::decide_lens(lens, &answers);
    let mut kept_attempts = Vec::with_capacity(judged.len());
    for (mut attempt, counts) in judged.into_iter().zip(&counted.counts) {
        if attempt.critical && !counts {
            continue;
        }
        if !counts && attempt.lens_answer.answer.is_some() {
            attempt.kept.clear();
            attempt
                .notes
                .push("not counted toward the lens verdict".to_string());
        }
        kept_attempts.push(attempt);
    }
    let judged = kept_attempts;
    let verdict = name_builder_exclusion(verdict, &setup.skip_families, &judged);
    let interim = counted.interim && !matches!(verdict, LensVerdict::CouldNotRun(_));
    (verdict, judged, interim)
}

/// Every attempt of one reviewer at `lens`, judged. The round and the
/// critical flag come from an attempt's place in the run, never from the
/// saved file: the first [`ROUNDS_PER_FAMILY`] attempts are the independent
/// rounds, and the one after them is the critical round, which exists only
/// when an independent round answered.
fn judge_attempts(
    req: &Request,
    reviewer: &Reviewer,
    lens: &Lens,
    attempts: &[Attempt],
) -> Vec<Judged> {
    let independent_answered = attempts
        .iter()
        .take(ROUNDS_PER_FAMILY)
        .any(|a| a.result == "answered" && a.answer.is_some());
    attempts
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < ROUNDS_PER_FAMILY || independent_answered)
        .map(|(index, saved)| {
            let mut judged = judge_attempt(req, reviewer, lens, saved);
            judged.round = as_u32(index + 1);
            judged.critical = index >= ROUNDS_PER_FAMILY;
            judged.lens_answer.critical = judged.critical;
            judged
        })
        .collect()
}

/// `saved`, with its answer checked again against the schema and `lens`, and
/// its findings checked against the files under `req.root` through
/// [`quotes::check`], so a lens can never be scored from an answer or a
/// finding nothing has verified, whoever wrote the file.
fn judge_attempt(req: &Request, reviewer: &Reviewer, lens: &Lens, saved: &Attempt) -> Judged {
    let (answer, reason, result) = match (&saved.answer, saved.result.as_str()) {
        (Some(answer), "answered") => match answer::revalidate(answer, lens) {
            Ok(valid) => (Some(valid), None, "answered"),
            Err(why) => (
                None,
                Some(format!("the saved answer does not validate: {why}")),
                "could-not-run",
            ),
        },
        (None, "answered") => (
            None,
            Some("the saved run holds no answer".to_string()),
            "could-not-run",
        ),
        (_, "invalid") => (None, saved.reason.clone(), "invalid"),
        _ => (None, saved.reason.clone(), "could-not-run"),
    };
    let text = |text: &str| redact_reviewer_text(req.root, req.config_root, text);
    let mut judged = Judged::missing(reviewer, result, String::new());
    judged.lens_answer.reason = reason.map(|reason| text(&reason));
    judged.notes = saved.notes.iter().map(|note| text(note)).collect();
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
                critical: false,
            },
            model: None,
            result: "answered".to_string(),
            findings_kept: 0,
            findings_dropped: 0,
            kept: Vec::new(),
            round: 1,
            critical: false,
            notes: notes.iter().map(ToString::to_string).collect(),
        }
    }

    fn run_with(lenses: Vec<LensRun>) -> ReviewerRun {
        ReviewerRun {
            reviewer: "x".to_string(),
            lenses,
            binding: None,
        }
    }

    fn lens_run(results: &[&str], context_error: Option<&str>) -> LensRun {
        LensRun {
            lens: "correctness".to_string(),
            context_error: context_error.map(str::to_string),
            attempts: results
                .iter()
                .map(|r| Attempt {
                    result: (*r).to_string(),
                    reason: None,
                    notes: Vec::new(),
                    round: 1,
                    critical: false,
                    answer: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_reviewer_never_answered_only_when_it_tried_and_nothing_answered() {
        assert!(run_with(vec![lens_run(&["could-not-run", "invalid"], None)]).never_answered());
        assert!(run_with(vec![lens_run(&[], Some("no context"))]).never_answered());
        assert!(!run_with(vec![
            lens_run(&["could-not-run"], None),
            lens_run(&["answered"], None)
        ])
        .never_answered());
        assert!(!run_with(vec![lens_run(&[], None)]).never_answered());
        assert!(!run_with(Vec::new()).never_answered());
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

    fn binding() -> Binding {
        Binding {
            repository: "owner/repo".to_string(),
            pull_request: 7,
            base: "b".repeat(40),
            head: "c".repeat(40),
            run_id: "1001".to_string(),
        }
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
            binding: Some(binding()),
        };
        let path = dir.join("codex.json");
        run.save(&path).expect("saves");
        let back = ReviewerRun::load(&path).expect("loads");
        assert_eq!(back.reviewer, "codex");
        assert_eq!(back.binding, Some(binding()));
        let attempt = back
            .lenses
            .first()
            .and_then(|l| l.attempts.first())
            .expect("one attempt");
        assert_eq!(attempt.round, 2);
        assert_eq!(attempt.reason.as_deref(), Some("timed out"));
    }

    fn saved_with(name: &str, reviewer: &str, binding: Option<Binding>) -> SavedRun {
        SavedRun {
            name: name.to_string(),
            run: ReviewerRun {
                reviewer: reviewer.to_string(),
                lenses: Vec::new(),
                binding,
            },
        }
    }

    fn saved(name: &str, reviewer: &str) -> SavedRun {
        saved_with(name, reviewer, Some(binding()))
    }

    #[test]
    fn a_saved_run_is_used_only_under_the_name_of_the_reviewer_it_holds() {
        let bound = bind(
            &[saved("codex", "codex"), saved("claude", "claude")],
            Some(&binding()),
        );
        assert_eq!(bound.accepted.len(), 2);
        assert!(bound.refused.is_empty(), "{:?}", bound.refused);
    }

    #[test]
    fn a_file_named_for_one_reviewer_that_claims_another_is_refused() {
        let bound = bind(&[saved("codex", "claude")], Some(&binding()));
        assert!(bound.accepted.is_empty());
        assert_eq!(
            bound.refused.get("codex").map(String::as_str),
            Some("its saved run names another reviewer")
        );
        assert!(!bound.refused.contains_key("claude"));
    }

    #[test]
    fn two_saved_runs_with_one_name_are_both_refused() {
        let bound = bind(
            &[saved("claude", "claude"), saved("claude", "claude")],
            Some(&binding()),
        );
        assert!(bound.accepted.is_empty());
        assert_eq!(
            bound.refused.get("claude").map(String::as_str),
            Some("more than one saved run carries its name")
        );
    }

    /// Each field of the binding is checked: a run from another repository,
    /// pull request, base, head or CI run is refused, and so is one with no binding.
    #[test]
    fn a_saved_run_is_used_only_with_the_exact_binding_it_was_given() {
        let expected = binding();
        let same = bind(&[saved("codex", "codex")], Some(&expected));
        assert_eq!(same.accepted.len(), 1);
        let others: Vec<(&str, Binding)> = vec![
            (
                "repository",
                Binding {
                    repository: "owner/other".to_string(),
                    ..binding()
                },
            ),
            (
                "pull request",
                Binding {
                    pull_request: 8,
                    ..binding()
                },
            ),
            (
                "base",
                Binding {
                    base: "d".repeat(40),
                    ..binding()
                },
            ),
            (
                "head",
                Binding {
                    head: "e".repeat(40),
                    ..binding()
                },
            ),
            (
                "run",
                Binding {
                    run_id: "1002".to_string(),
                    ..binding()
                },
            ),
        ];
        for (what, other) in others {
            let bound = bind(
                &[saved_with("codex", "codex", Some(other))],
                Some(&expected),
            );
            assert!(bound.accepted.is_empty(), "a different {what} was accepted");
            assert_eq!(
                bound.refused.get("codex").map(String::as_str),
                Some("its saved run belongs to another pull request, commit or CI run"),
                "{what}"
            );
        }
        let unbound = bind(&[saved_with("codex", "codex", None)], Some(&expected));
        assert!(
            unbound.accepted.is_empty(),
            "a run with no binding was accepted"
        );
    }

    #[test]
    fn with_no_binding_to_check_against_no_saved_run_is_used() {
        let bound = bind(&[saved("codex", "codex")], None);
        assert!(bound.accepted.is_empty());
        assert_eq!(
            bound.refused.get("codex").map(String::as_str),
            Some("no binding was given to check its saved run against")
        );
    }

    #[test]
    fn a_reviewer_name_is_checked_before_it_is_used() {
        for good in ["codex", "opencode", "a", "review-run_2"] {
            assert!(valid_artifact_name(good), "{good}");
        }
        for bad in [
            "",
            "../codex",
            "a/b",
            "a\\b",
            ".hidden",
            "-lead",
            "has space",
            "semi;colon",
            "new\nline",
            &"x".repeat(65),
        ] {
            assert!(!valid_artifact_name(bad), "{bad:?}");
        }
        let bound = bind(&[saved("../codex", "codex")], Some(&binding()));
        assert!(bound.accepted.is_empty());
        assert!(
            bound.refused.is_empty(),
            "a bad name is never used as a key"
        );
        assert_eq!(
            bound.notes,
            vec![
                "ignored 1 saved run(s) from a file whose name is not a plain reviewer name"
                    .to_string()
            ]
        );
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
