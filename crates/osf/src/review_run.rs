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
use crate::failure;
use crate::journal::{Actor, Journal, Payload, ReviewAnswer, ReviewDecision};
use crate::lenses::{self, Catalogue, Lens};
use crate::quotes;
use crate::reducer::{self, LensAnswer, LensVerdict, Verdict};
use crate::review_context::{self, ChangeFolder, PullRequest, Sources};
use crate::review_prompt;
use crate::reviewers::{self, Outcome, Reviewer};
use crate::secret_values;
use crate::{changeset_risk, config, git};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// The pre-push review budget: how many failed rounds have already tried to
/// stop this push. Keyed by the worktree path, so each develop run has its
/// own count, and kept in the journal state so the builder cannot edit it.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RoundCounts {
    #[serde(default)]
    pub review: u32,
    #[serde(default)]
    pub could_not_run: u32,
}

/// What one finished round found, for the pre-push budget.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundKind {
    /// Nothing to fix: the push may go and the budget resets.
    Pass,
    /// A kept must-fix or should-fix finding.
    Findings,
    /// The review could not run.
    CouldNotRun,
}

impl RoundCounts {
    fn path(root: &Path) -> Result<PathBuf, String> {
        use std::hash::{Hash as _, Hasher as _};
        // The count decides whether a push may proceed, so it lives in the
        // journal state, outside the worktree the builder can edit. The
        // worktree path is the key, so each develop run has its own count.
        let dir = crate::journal::state_dir()?;
        let rooted = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        rooted.hash(&mut hasher);
        let name = format!("{:016x}.json", hasher.finish());
        Ok(dir.join("review-rounds").join(name))
    }

    fn load(root: &Path) -> Result<Self, String> {
        let path = Self::path(root)?;
        match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn save(&self, root: &Path) -> Result<(), String> {
        let path = Self::path(root)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn clear(root: &Path) -> Result<(), String> {
        let path = Self::path(root)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// Applies the pre-push budget and returns whether the push may proceed.
///
/// A pass clears the counts. A failed round increments its own count, and
/// lets the push through once that count is past its limit. When it is not,
/// the push keeps failing, and the next round fixes the findings.
///
/// # Errors
/// Returns an error when the journal state directory cannot be resolved, when
/// the count file cannot be read, written or removed, or when a could-not-run
/// streak passes its limit and the push must stop.
pub fn round_allows_push(
    root: &Path,
    kind: RoundKind,
    review_limit: u32,
    could_not_run_limit: u32,
) -> Result<bool, String> {
    match kind {
        RoundKind::Pass => {
            RoundCounts::clear(root)?;
            Ok(true)
        }
        RoundKind::Findings => {
            let mut counts = RoundCounts::load(root)?;
            counts.review = counts.review.saturating_add(1);
            let allowed = counts.review > review_limit;
            counts.save(root)?;
            Ok(allowed)
        }
        RoundKind::CouldNotRun => {
            let mut counts = RoundCounts::load(root)?;
            counts.could_not_run = counts.could_not_run.saturating_add(1);
            let over = counts.could_not_run > could_not_run_limit;
            counts.save(root)?;
            if over {
                // A review that did not happen is never a pass. Past the
                // limit the gate names the block and stops, instead of
                // silently retrying the same broken reviewer.
                return Err(format!(
                    "the review could not run {} rounds, past the limit of {could_not_run_limit}; fix the reviewer before pushing",
                    counts.could_not_run
                ));
            }
            Ok(false)
        }
    }
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
    /// What a failed attempt's output said about its cause. The run artifact
    /// holds it for a person to read; the journal, the summary and the
    /// posted review never do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<AttemptCause>,
}

/// The evidence a failed attempt leaves in the run artifact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttemptCause {
    /// What made the classifier choose its category: a phrase from the
    /// classifier's fixed list, an HTTP status such as `http status 429`, or
    /// a JSON field such as `json status 429`. Never text the provider wrote.
    pub basis: Option<String>,
    /// The last [`failure::TAIL_LINES`] lines of the reviewer's standard
    /// output then standard error, after the job's secret redaction, each
    /// line cut to [`failure::TAIL_LINE_CHARS`] characters.
    pub tail: String,
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
    /// Why this reviewer made no attempt at all, when it was left out on
    /// purpose: its family built the change, or another reviewer is the one
    /// that runs without independence. An empty `attempts` list without this reads as "nothing was
    /// looked at" and "looked at, clean" alike.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_run: Option<String>,
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

/// Each saved reviewer file named in `paths`, in order. A folder stands for
/// the `.json` files directly inside it, sorted by name.
///
/// # Errors
/// Names the folder and the reason when one cannot be read.
pub fn expand_saved_runs(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        if !path.is_dir() {
            files.push(path.clone());
            continue;
        }
        let mut inside = Vec::new();
        let entries = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            let file = entry.path();
            if file.is_file() && file.extension().is_some_and(|ext| ext == "json") {
                inside.push(file);
            }
        }
        inside.sort();
        files.extend(inside);
    }
    Ok(files)
}

/// Everything a run reads from the trusted config root and the repository
/// before any reviewer starts.
struct Setup {
    catalogue: Catalogue,
    /// The names of the selected lenses, in catalogue order.
    selected: Vec<String>,
    roster: Vec<Reviewer>,
    threshold: f64,
    /// How long one reviewer attempt may run before it is stopped.
    per_attempt: Duration,
    /// How long this reviewer run may take, counted from the first lens start.
    total: Duration,
    /// How many lenses may run at once.
    concurrency: usize,
    /// The whole-run cost ceiling, when one is set.
    cost_ceiling: Option<f64>,
    prompt: String,
    builder_families: Vec<String>,
    skip_families: BTreeSet<String>,
    /// Why the builder's family is unknown, when it is. Every reviewer is
    /// then tried, and the note says that independence was not achieved.
    unknown_builder: Option<String>,
}

/// The reason a reviewer sits out because its family built the change.
fn same_family_reason(family: &str) -> String {
    format!("skipped: same model family as the builder ({family})")
}

/// The note for a run whose builder family is unknown. `detail` names each
/// unknown commit by its short hash and a fixed cause.
fn unknown_builder_note(detail: &str) -> String {
    format!(
        "builder family unknown, independence not achieved ({detail}); name the \
         family with --builder-family or fix the Code-Generator trailer"
    )
}

/// What a lens note says when a reviewer of the builder's family counted.
const SAME_FAMILY_NOTE: &str = "reviewer family same as builder, independence not achieved";

/// What one configured reviewer of another family did on one lens.
struct Other {
    name: String,
    family: String,
    /// Whether the reviewer made at least one attempt. A reviewer that was
    /// never tried is no evidence that it was unavailable.
    tried: bool,
    /// The error text of each attempt that did not answer.
    reasons: Vec<String>,
}

/// The evidence that a reviewer of the builder's family may count as the last
/// resort: one line for each configured reviewer of another family, saying why
/// it could not answer, from what was actually tried. An empty list is valid
/// when no reviewer of another family is configured.
///
/// # Errors
/// Names the first reviewer that was never tried: it is no evidence.
fn last_resort_evidence(others: &[Other]) -> Result<Vec<String>, String> {
    if let Some(untried) = others.iter().find(|o| !o.tried) {
        return Err(format!(
            "reviewer {} of another family was never tried, so it is no evidence that another family was unavailable",
            plain(&untried.name)
        ));
    }
    Ok(others
        .iter()
        .map(|o| {
            let why = if o.reasons.is_empty() {
                "tried, no error text recorded".to_string()
            } else {
                o.reasons.join("; ")
            };
            format!(
                "reviewer {} ({}) did not answer: {why}",
                plain(&o.name),
                o.family
            )
        })
        .collect())
}

impl Setup {
    /// Why `reviewer` is not started in the first pass: its family built the
    /// change. A reviewer of another family runs first; this one runs only as
    /// the last resort, see [`last_resort_evidence`].
    fn skip_reason(&self, reviewer: &Reviewer) -> Option<String> {
        reviewer
            .is_excluded_by(&self.skip_families)
            .then(|| same_family_reason(&reviewer.family))
    }

    /// Whether a reviewer of the builder's family may start in a single
    /// reviewer job: only when the roster holds no reviewer of another family
    /// that could run first.
    fn last_resort_only(&self) -> bool {
        !self
            .roster
            .iter()
            .any(|r| r.family_error.is_none() && !r.is_excluded_by(&self.skip_families))
    }

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
    // The review's own output under .osf/out never reaches this list:
    // git::changed_files reads tracked, committed paths, and .osf/out is
    // ignored. A tracked change under .osf/, such as the moon task or the
    // lens overrides, stays in the list so a lens that triggers on it fires.
    let changed = git::changed_files(req.root, req.base).map_err(|e| e.to_string())?;
    let signals = report.signals();
    let selected: Vec<String> = lenses::select(&catalogue, &changed, &signals, report.tier)
        .iter()
        .map(|s| s.lens.name.clone())
        .collect();

    let roster = reviewers::roster(req.config_root)?;
    let review_config = config::review_config(req.config_root).map_err(|e| e.to_string())?;
    let prompt = review_prompt::load(req.config_root, review_config.prompt_file.as_deref())?;
    let BuilderFamilies {
        all: builder_families,
        skip,
        unknown,
    } = detect_builder_families(req, &review_config)?;
    Ok(Setup {
        catalogue,
        selected,
        roster,
        threshold: review_config.threshold,
        per_attempt: Duration::from_secs(review_config.timeout_seconds),
        total: Duration::from_secs(review_config.total_timeout_seconds),
        concurrency: review_config.concurrency,
        cost_ceiling: review_config.cost_ceiling,
        prompt,
        builder_families,
        skip_families: skip,
        unknown_builder: unknown,
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
    // Reviewers of another family go first, in the configured order. A
    // reviewer of the builder's family starts only when some lens got no
    // answer from them: the last resort.
    let mut runs: Vec<ReviewerRun> = setup
        .roster
        .iter()
        .map(|reviewer| run_reviewer_with(req, &setup, reviewer, false))
        .collect();
    if setup.selected.iter().any(|lens| !answered_on(&runs, lens)) {
        for (run, reviewer) in runs.iter_mut().zip(&setup.roster) {
            if setup.skip_reason(reviewer).is_some() {
                *run = run_reviewer_with(req, &setup, reviewer, true);
            }
        }
    }
    Ok(reduce_with(
        req,
        &setup,
        &runs,
        &BTreeMap::new(),
        Vec::new(),
        state_dir,
    ))
}

/// Whether some reviewer in `runs` answered at least one attempt on `lens`.
fn answered_on(runs: &[ReviewerRun], lens: &str) -> bool {
    runs.iter()
        .flat_map(|run| &run.lenses)
        .filter(|l| l.lens == lens)
        .flat_map(|l| &l.attempts)
        .any(|a| a.result == "answered")
}

/// Runs the roster reviewer called `name` over every selected lens, and
/// returns what it did, unchecked and unjournaled. A reviewer whose family
/// built the change, while another family is available, is not started: its
/// run holds no attempts, unless the roster holds no reviewer of another
/// family to try first.
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
    let last_resort = setup.last_resort_only();
    Ok(run_reviewer_with(req, &setup, reviewer, last_resort))
}

/// Runs one lens in its own worker: a [`CleanCopy`] of the change per lens
/// and the attempts made inside it, dropped when the lens ends.
fn run_lens(
    req: &Request,
    setup: &Setup,
    reviewer: &Reviewer,
    lens: &Lens,
    sources: &Sources,
    change: &ChangeFolder,
    deadline: Instant,
) -> LensRun {
    let mut run = LensRun {
        lens: lens.name.clone(),
        context_error: None,
        attempts: Vec::new(),
    };
    if Instant::now() >= deadline {
        return could_not_run_lens(lens, total_ran_out_before_lens(setup.total.as_secs()));
    }
    // One read-only copy per lens, dropped at the end of this function.
    match CleanCopy::create_read_only(req.root) {
        Err(reason) => run.context_error = Some(reason),
        Ok(copy) => match review_context::build(lens, sources) {
            Err(reason) => run.context_error = Some(reason),
            Ok(metadata) => {
                let file = change.file().to_string_lossy().into_owned();
                let prompt = review_prompt::render(&setup.prompt, lens, &metadata, &file);
                let attempts = Attempts {
                    root: req.root,
                    config_root: req.config_root,
                    workdir: copy.dir(),
                    reviewer,
                    prompt: &prompt,
                    lens,
                    per_attempt: setup.per_attempt,
                    total: setup.total,
                    deadline,
                };
                run.attempts = attempts.run();
            }
        },
    }
    run
}

/// A lens recorded as could-not-run before it started. `context_error` stays
/// `None`: setting it would suppress every reviewer's journal event for the
/// lens, and this record exists so one complete event still carries the name.
fn could_not_run_lens(lens: &Lens, reason: String) -> LensRun {
    LensRun {
        lens: lens.name.clone(),
        context_error: None,
        attempts: vec![Attempt {
            result: "could-not-run".to_string(),
            reason: Some(reason),
            notes: Vec::new(),
            round: 1,
            critical: false,
            answer: None,
            cause: None,
        }],
    }
}

/// Runs `body`, turning a panic into a could-not-run record so one lens never stops the run.
fn contain_panic(lens: &Lens, body: impl FnOnce() -> LensRun) -> LensRun {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(run) => run,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "no message".to_string());
            could_not_run_lens(lens, format!("lens '{}' panicked: {message}", lens.name))
        }
    }
}

/// The reason a lens never started because the whole run's limit ran out.
fn total_ran_out_before_lens(seconds: u64) -> String {
    format!("the total limit (total_timeout_seconds = {seconds}s) ran out before this lens started")
}

/// The reason a running attempt was stopped by the whole run's limit, or
/// could not be stopped when the kill failed.
fn total_ran_out_while_running(reviewer: &str, seconds: u64, kill_failed: bool) -> String {
    let outcome = if kill_failed {
        "it could not be stopped"
    } else {
        "it was stopped"
    };
    format!(
        "the total limit (total_timeout_seconds = {seconds}s) ran out while reviewer '{reviewer}' was running; {outcome}"
    )
}

/// The reason a running attempt was stopped by the per-attempt limit, or
/// could not be stopped when the kill failed.
fn per_attempt_limit(reviewer: &str, seconds: u64, kill_failed: bool) -> String {
    let outcome = if kill_failed {
        "could not be stopped"
    } else {
        "was stopped"
    };
    format!(
        "reviewer '{reviewer}' timed out at the per-attempt limit (timeout_seconds = {seconds}s) and {outcome}"
    )
}

/// The reason a lens never started because the cost ceiling was reached.
fn cost_ceiling_reached(ceiling: f64) -> String {
    format!("the cost ceiling of {ceiling} was reached before this lens started")
}

/// The lens records for a reviewer that is idle or whose shared folder
/// failed: no worker starts, exactly as before this run could parallelise.
fn lens_records(lenses: &[&Lens], context_error: Option<&str>) -> Vec<LensRun> {
    lenses
        .iter()
        .map(|lens| LensRun {
            lens: lens.name.clone(),
            context_error: context_error.map(ToString::to_string),
            attempts: Vec::new(),
        })
        .collect()
}

/// Runs `count` pieces of work over `workers` threads, keeping their results in index order.
fn run_pool(
    count: usize,
    workers: usize,
    work: &(dyn Fn(usize) -> LensRun + Sync),
) -> Vec<LensRun> {
    let queue = Mutex::new(VecDeque::from_iter(0..count));
    let results: Mutex<Vec<Option<LensRun>>> = Mutex::new((0..count).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = queue
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop_front();
                let Some(index) = index else { break };
                let run = work(index);
                if let Some(slot) = results
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_mut(index)
                {
                    *slot = Some(run);
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .map(|run| run.expect("every item is taken from the queue once"))
        .collect()
}

/// The instant the whole-run limit ends, or a reason when it is too far away.
fn deadline_after(total: Duration) -> Result<Instant, String> {
    Instant::now()
        .checked_add(total)
        .ok_or_else(|| "[review]: total_timeout_seconds is too large to count from now".to_string())
}

/// `allow_same_family` lets a reviewer of the builder's family start: it is
/// the last resort, and the caller has the reason to take it.
fn run_reviewer_with(
    req: &Request,
    setup: &Setup,
    reviewer: &Reviewer,
    allow_same_family: bool,
) -> ReviewerRun {
    let mut run = run_reviewer_unscrubbed(req, setup, reviewer, allow_same_family);
    scrub_run(req, &mut run);
    run
}

/// Passes every reason and context error in `run` through the secret
/// redaction, whatever made it: a limit, a panic, a context error or a
/// reviewer failure. The reasons are built from fixed wording and osf's own
/// names, so this removes nothing in the ordinary case.
fn scrub_run(req: &Request, run: &mut ReviewerRun) {
    for lens in &mut run.lenses {
        if let Some(error) = &mut lens.context_error {
            *error = redact_reviewer_text(req.root, req.config_root, error);
        }
        for attempt in &mut lens.attempts {
            if let Some(reason) = &mut attempt.reason {
                *reason = redact_reviewer_text(req.root, req.config_root, reason);
            }
            // The tail is raw reviewer text. It is redacted whole, then cut,
            // so a cut never splits a secret.
            if let Some(cause) = &mut attempt.cause {
                let redacted = redact_reviewer_text(req.root, req.config_root, &cause.tail);
                cause.tail = failure::cap_line_lengths(&redacted);
            }
        }
    }
}

/// The credential preflight and why this reviewer may sit the change out.
fn preflight_and_not_run(
    reviewer: &Reviewer,
    allow_same_family: bool,
    setup: &Setup,
) -> (Option<String>, Option<String>) {
    let skipped = if allow_same_family {
        None
    } else {
        setup.skip_reason(reviewer)
    };
    // A reviewer already left out for a more specific reason, such as no
    // read-only mode, is not judged again by the credential preflight.
    // A reviewer with no read-only mode is left out for that reason. Its
    // worker reports it, so the preflight must not take its place.
    let preflight =
        if reviewer.family_error.is_some() || skipped.is_some() || reviewer.read_only.is_none() {
            None
        } else {
            reviewer.missing_credentials(crate::reviewers::real_home().as_deref())
        };
    let not_run = if let Some(reason) = &reviewer.family_error {
        Some(format!("could not run: {reason}"))
    } else if skipped.is_some() {
        skipped
    } else {
        preflight
            .as_ref()
            .map(|reason| format!("could not run: {reason}"))
    };
    (preflight, not_run)
}

/// One reviewer that no attempt can help: every lens is recorded as
/// could-not-run with the same reason, and no worker starts.
fn preflight_reviewer_run(
    reviewer: &Reviewer,
    lenses: &[&Lens],
    reason: &str,
    binding: Option<Binding>,
) -> ReviewerRun {
    let reason = format!("could not run: {reason}");
    ReviewerRun {
        reviewer: reviewer.name.clone(),
        lenses: lenses
            .iter()
            .map(|lens| could_not_run_lens(lens, reason.clone()))
            .collect(),
        binding,
        not_run: None,
    }
}

fn run_reviewer_unscrubbed(
    req: &Request,
    setup: &Setup,
    reviewer: &Reviewer,
    allow_same_family: bool,
) -> ReviewerRun {
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
    let (preflight, not_run) = preflight_and_not_run(reviewer, allow_same_family, setup);
    let idle = not_run.is_some();
    // Written once per reviewer run, and removed when this run ends.
    let change = if idle {
        None
    } else {
        Some(ChangeFolder::create(&sources))
    };
    let mut reviewer = reviewer.clone();
    if let Some(Ok(change)) = &change {
        reviewer.review_dir = Some(change.dir().to_path_buf());
    }
    let reviewer = &reviewer;
    let lenses = setup.selected_lenses();
    // An idle reviewer, or a shared folder that failed, starts no worker.
    let change = match change {
        None if preflight.is_some() => {
            return preflight_reviewer_run(
                reviewer,
                &lenses,
                preflight.as_deref().unwrap_or("no credential"),
                req.binding.cloned(),
            );
        }
        None => {
            return ReviewerRun {
                reviewer: reviewer.name.clone(),
                lenses: lens_records(&lenses, None),
                binding: req.binding.cloned(),
                not_run,
            };
        }
        Some(Err(reason)) => {
            return ReviewerRun {
                reviewer: reviewer.name.clone(),
                lenses: lens_records(&lenses, Some(&reason)),
                binding: req.binding.cloned(),
                not_run: None,
            };
        }
        Some(Ok(change)) => change,
    };

    // Taken once, just before the workers start: it bounds every lens.
    let deadline = match deadline_after(setup.total) {
        Ok(deadline) => deadline,
        Err(reason) => {
            return ReviewerRun {
                reviewer: reviewer.name.clone(),
                lenses: lenses
                    .iter()
                    .map(|lens| could_not_run_lens(lens, reason.clone()))
                    .collect(),
                binding: req.binding.cloned(),
                not_run: None,
            };
        }
    };
    let count = lenses.len();
    let workers = setup.concurrency.min(count);
    // No reviewer reports cost yet, so nothing adds to this counter.
    let spent = Mutex::new(0.0_f64);
    let lenses = run_pool(count, workers, &|index| {
        let lens = *lenses.get(index).expect("index comes from the pool");
        match setup.cost_ceiling {
            Some(ceiling) => {
                let spent = *spent.lock().unwrap_or_else(PoisonError::into_inner);
                if spent >= ceiling {
                    could_not_run_lens(lens, cost_ceiling_reached(ceiling))
                } else {
                    contain_panic(lens, || {
                        run_lens(req, setup, reviewer, lens, &sources, &change, deadline)
                    })
                }
            }
            None => contain_panic(lens, || {
                run_lens(req, setup, reviewer, lens, &sources, &change, deadline)
            }),
        }
    });
    ReviewerRun {
        reviewer: reviewer.name.clone(),
        lenses,
        binding: req.binding.cloned(),
        not_run: None,
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
    per_attempt: Duration,
    total: Duration,
    deadline: Instant,
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

    /// Runs the reviewer once with whatever time the total limit leaves, or
    /// records the attempt as not started once that limit has run out. An
    /// answer's findings are redacted here, so nothing a reviewer wrote is
    /// saved as it was written.
    fn once(&self, round: u32, critical: bool) -> Attempt {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Attempt {
                result: "could-not-run".to_string(),
                reason: Some(self.scrub(&total_ran_out_while_running(
                    &self.reviewer.name,
                    self.total.as_secs(),
                    false,
                ))),
                notes: Vec::new(),
                round,
                critical,
                answer: None,
                cause: None,
            };
        }
        let given = self.per_attempt.min(remaining);
        let (outcome, notes, cause) =
            reviewers::run_one_noted(self.reviewer, self.prompt, self.lens, self.workdir, given);
        let (result, reason, answer) = match outcome {
            Outcome::Answered(answer) => (
                "answered",
                None,
                Some(redact_answer(self.root, self.config_root, answer)),
            ),
            Outcome::Invalid(reason) => ("invalid", Some(self.scrub(&reason)), None),
            Outcome::CouldNotRun(reason) => ("could-not-run", Some(self.scrub(&reason)), None),
            Outcome::TimedOut(timeout) => (
                "could-not-run",
                Some(self.scrub(&self.timeout_reason(&timeout, given))),
                None,
            ),
        };
        Attempt {
            result: result.to_string(),
            reason,
            notes,
            round,
            critical,
            answer,
            cause: cause.map(|c| AttemptCause {
                basis: c.basis,
                tail: c.tail,
            }),
        }
    }

    /// The reason for an attempt that ran past its time; see
    /// [`timeout_reason`].
    fn timeout_reason(&self, timeout: &reviewers::Timeout, given: Duration) -> String {
        timeout_reason(
            &self.reviewer.name,
            self.per_attempt,
            self.total,
            given,
            timeout,
        )
    }
}

/// The reason for an attempt that ran past its time. The binding limit, the
/// kill result and the category all come from `timeout`'s data, and `given`
/// says whether the per-attempt or the total limit bound it. A run whose
/// time ran out before it could start again says so and is never reported
/// as stopped.
fn timeout_reason(
    reviewer: &str,
    per_attempt: Duration,
    total: Duration,
    given: Duration,
    timeout: &reviewers::Timeout,
) -> String {
    let per_attempt_bound = given == per_attempt;
    if timeout.before_start {
        return if per_attempt_bound {
            format!(
                "the per-attempt limit (timeout_seconds = {}s) ran out before reviewer '{reviewer}' could start again",
                per_attempt.as_secs()
            )
        } else {
            format!(
                "the total limit (total_timeout_seconds = {}s) ran out before reviewer '{reviewer}' could start again",
                total.as_secs()
            )
        };
    }
    let core = if per_attempt_bound {
        per_attempt_limit(reviewer, per_attempt.as_secs(), timeout.kill_failed)
    } else {
        total_ran_out_while_running(reviewer, total.as_secs(), timeout.kill_failed)
    };
    format!("{core}{}", timeout.hint())
}

impl Attempts<'_> {
    /// A reason with the exact secret values and the secret patterns removed,
    /// as a last guard. The reasons themselves are built from fixed wording
    /// and names osf owns, so this removes nothing in the ordinary case.
    fn scrub(&self, reason: &str) -> String {
        redact_reviewer_text(self.root, self.config_root, reason)
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
    mut notes: Vec<String>,
    state_dir: &Path,
) -> RunOutcome {
    notes.extend(left_out_notes(setup));
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
        let (verdict, judged, interim, lens_notes) = judge_lens(req, setup, runs, refused, lens);
        for note in lens_notes {
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        for attempt in judged {
            if let Some(journal) = journal.as_mut() {
                let event = attempt.into_event(&lens.name);
                if let Err(e) = journal.append(
                    &Actor::system(ACTOR),
                    now_millis(),
                    Payload::ReviewAnswer(event.0),
                ) {
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
            &Actor::system(ACTOR),
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

/// One note per roster reviewer the run left out on purpose, so a posted
/// summary says who did not look and why. An unknown builder family leads
/// with its own note, so the reader learns that the review is not
/// independent of the builder.
fn left_out_notes(setup: &Setup) -> Vec<String> {
    let independence = setup
        .unknown_builder
        .iter()
        .map(|d| unknown_builder_note(d));
    let left_out = setup
        .roster
        .iter()
        .filter(|r| r.family_error.is_none())
        .filter_map(|r| {
            setup
                .skip_reason(r)
                .map(|reason| format!("reviewer {} {reason}", plain(&r.name)))
        });
    independence.chain(left_out).collect()
}

/// How many unknown-builder reasons the note names before counting the rest.
const UNKNOWN_REASONS_SHOWN: usize = 3;

/// The reasons the builder family is unknown as one short line: up to
/// [`UNKNOWN_REASONS_SHOWN`] of them, then a count of the rest. Each reason
/// is a commit's short hash and a fixed cause, so the line holds no text an
/// author wrote and needs no redaction.
fn unknown_detail(reasons: &[String]) -> String {
    if reasons.is_empty() {
        return "no family named".to_string();
    }
    let shown = reasons.iter().take(UNKNOWN_REASONS_SHOWN);
    let list = shown.cloned().collect::<Vec<_>>().join("; ");
    match reasons.len().checked_sub(UNKNOWN_REASONS_SHOWN) {
        Some(rest) if rest > 0 => format!(
            "{list}; and {rest} more commit{} with an unknown builder family",
            if rest == 1 { "" } else { "s" }
        ),
        _ => list,
    }
}

/// What [`detect_builder_families`] found.
struct BuilderFamilies {
    all: Vec<String>,
    skip: BTreeSet<String>,
    unknown: Option<String>,
}

/// The builder families for `req`'s own reviewed range (see
/// [`builder::detect_detailed`]), the subset of them a roster entry is left
/// out for (every detected family other than [`builder::UNKNOWN`], which
/// names no real family to leave a reviewer out for), and, when any family
/// is unknown, the commits that name none. An unknown family does not stop
/// the run: see [`Setup::fallback`].
///
/// # Errors
/// Returns an error when git cannot read the reviewed range's own commits.
fn detect_builder_families(
    req: &Request,
    review_config: &config::ReviewConfig,
) -> Result<BuilderFamilies, String> {
    let range = format!("{}..HEAD", req.base);
    let found = builder::detect_detailed(
        req.root,
        &range,
        &review_config.builder_family_aliases,
        req.builder_family_overrides,
    )?;
    let skip: BTreeSet<String> = found
        .families
        .iter()
        .filter(|family| family.as_str() != builder::UNKNOWN)
        .cloned()
        .collect();
    let unknown = if found.families.iter().any(|f| f == builder::UNKNOWN) {
        // The trailer text is the author's own, so it is cut to one short line.
        let detail = unknown_detail(&found.unknown);
        // Commit-message text reaches saved files, the journal, the console and
        // the posted review, so it goes through the same redaction as any
        // other text from the change.
        Some(detail)
    } else {
        None
    };
    Ok(BuilderFamilies {
        all: found.families,
        skip,
        unknown,
    })
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
) -> (LensVerdict, Vec<Judged>, bool, Vec<String>) {
    let mut judged: Vec<Judged> = Vec::new();
    let mut context_error: Option<String> = None;
    let mut ran: Vec<(&Reviewer, Vec<Judged>)> = Vec::new();
    let mut others: Vec<Other> = Vec::new();
    let mut last_resort: Vec<&Reviewer> = Vec::new();
    for reviewer in &setup.roster {
        if let Some(reason) = &reviewer.family_error {
            judged.push(Judged::missing(reviewer, "could-not-run", reason.clone()));
            continue;
        }
        if reviewer.is_excluded_by(&setup.skip_families) {
            last_resort.push(reviewer);
            continue;
        }
        let before = judged.len();
        let ctx = LensCtx {
            req,
            runs,
            refused,
            lens,
        };
        let attempts = judge_reviewer(&ctx, reviewer, &mut judged, &mut context_error);
        let reasons: Vec<String> = match &attempts {
            Some(attempts) => attempts
                .iter()
                .filter(|a| a.lens_answer.answer.is_none())
                .filter_map(|a| a.lens_answer.reason.clone())
                .collect(),
            None => judged
                .get(before..)
                .unwrap_or_default()
                .iter()
                .filter_map(|a| a.lens_answer.reason.clone())
                .collect(),
        };
        others.push(Other {
            name: reviewer.name.clone(),
            family: reviewer.family.clone(),
            tried: attempts.is_some(),
            reasons,
        });
        if let Some(attempts) = attempts {
            ran.push((reviewer, attempts));
        }
    }
    let lens_notes = settle_last_resort(
        &LensCtx {
            req,
            runs,
            refused,
            lens,
        },
        &last_resort,
        &others,
        &mut ran,
        &mut judged,
        &mut context_error,
    );
    if let Some(reason) = context_error {
        return (LensVerdict::CouldNotRun(reason), judged, false, lens_notes);
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
    (verdict, judged, interim, lens_notes)
}

/// What judging one lens reads: the run's request, the saved runs, the
/// reviewers refused at binding, and the lens.
struct LensCtx<'a> {
    req: &'a Request<'a>,
    runs: &'a [ReviewerRun],
    refused: &'a BTreeMap<String, String>,
    lens: &'a Lens,
}

/// Decides whether the reviewers of the builder's family count on this lens as
/// the last resort. They count only when no reviewer of another family
/// answered and [`last_resort_evidence`] holds. Returns the notes that say
/// so, for the summary.
fn settle_last_resort<'a>(
    ctx: &LensCtx<'_>,
    last_resort: &[&'a Reviewer],
    others: &[Other],
    ran: &mut Vec<(&'a Reviewer, Vec<Judged>)>,
    judged: &mut Vec<Judged>,
    context_error: &mut Option<String>,
) -> Vec<String> {
    let mut lens_notes: Vec<String> = Vec::new();
    if last_resort.is_empty() {
        return lens_notes;
    }
    let others_answered = ran
        .iter()
        .any(|(_, attempts)| attempts.iter().any(|a| a.lens_answer.answer.is_some()));
    let evidence = if others_answered {
        Err(String::new())
    } else {
        last_resort_evidence(others)
    };
    let lens_name = plain(&ctx.lens.name);
    for &reviewer in last_resort {
        let skipped = |why: &str| {
            let mut reason = same_family_reason(&reviewer.family);
            if !why.is_empty() {
                reason = format!("{reason}; {why}");
            }
            Judged::missing(reviewer, "skipped", reason)
        };
        match &evidence {
            Err(why) => {
                if !why.is_empty() {
                    lens_notes.push(format!(
                        "{lens_name}: reviewer {} not counted as the last resort: {why}",
                        plain(&reviewer.name)
                    ));
                }
                judged.push(skipped(why));
            }
            Ok(evidence) => {
                let mut scratch = Vec::new();
                match judge_reviewer(ctx, reviewer, &mut scratch, context_error) {
                    Some(attempts) => {
                        let proof = if evidence.is_empty() {
                            "no reviewer of another family is configured".to_string()
                        } else {
                            format!("other families tried first: {}", evidence.join("; "))
                        };
                        lens_notes.push(format!(
                            "{lens_name}: reviewer {} counted as the last resort. {SAME_FAMILY_NOTE}; {proof}",
                            plain(&reviewer.name)
                        ));
                        ran.push((reviewer, attempts));
                    }
                    None => judged.push(skipped("it was not run as the last resort")),
                }
            }
        }
    }
    lens_notes
}

/// Judges what the roster reviewer `reviewer` saved for `lens`. Returns its
/// attempts when it made any. Otherwise it pushes the reason onto `judged` and
/// returns `None`: the reviewer was not tried.
fn judge_reviewer(
    ctx: &LensCtx<'_>,
    reviewer: &Reviewer,
    judged: &mut Vec<Judged>,
    context_error: &mut Option<String>,
) -> Option<Vec<Judged>> {
    let LensCtx {
        req,
        runs,
        refused,
        lens,
    } = *ctx;
    if let Some(reason) = refused.get(&reviewer.name) {
        judged.push(Judged::missing(reviewer, "could-not-run", reason.clone()));
        return None;
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
        return None;
    }
    match lens_run {
        Some(run) if run.context_error.is_some() => {
            let reason = run.context_error.clone().unwrap_or_default();
            judged.push(Judged::missing(reviewer, "could-not-run", reason.clone()));
            *context_error = context_error.take().or(Some(reason));
            None
        }
        Some(run) if run.attempts.len() > MAX_ATTEMPTS => {
            judged.push(Judged::missing(
                reviewer,
                "could-not-run",
                "its saved run holds more attempts than a run makes".to_string(),
            ));
            None
        }
        Some(run) if !run.attempts.is_empty() => {
            Some(judge_attempts(req, reviewer, lens, &run.attempts))
        }
        _ => {
            judged.push(Judged::missing(
                reviewer,
                "could-not-run",
                "its run left no answer for this lens".to_string(),
            ));
            None
        }
    }
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
    #[test]
    fn a_folder_of_saved_runs_expands_to_its_json_files_in_name_order() {
        let dir = std::env::temp_dir().join(format!("osf-expand-runs-{}", std::process::id())); // osf: temp-dir allowed, a test folder named by this process id and removed at the end
        std::fs::create_dir_all(dir.join("sub")).expect("temp folder");
        for name in ["b.json", "a.json", "note.txt"] {
            std::fs::write(dir.join(name), "{}").expect("write");
        }
        let single = std::path::PathBuf::from("single.json");
        let files = super::expand_saved_runs(&[single.clone(), dir.clone()]).expect("expands");
        assert_eq!(files, vec![single, dir.join("a.json"), dir.join("b.json")]);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    use super::*;
    use crate::lenses::{Runs, SeverityGuide, Trigger};

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
            not_run: None,
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
                    cause: None,
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
                    cause: None,
                }],
            }],
            binding: Some(binding()),
            not_run: None,
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
                not_run: None,
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

    fn test_lens(name: &str) -> Lens {
        Lens {
            name: name.to_string(),
            summary: "a lens these tests build by hand".to_string(),
            criteria: Vec::new(),
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

    fn answered(lens: &Lens) -> LensRun {
        LensRun {
            lens: lens.name.clone(),
            context_error: None,
            attempts: vec![Attempt {
                result: "answered".to_string(),
                reason: None,
                notes: Vec::new(),
                round: 1,
                critical: false,
                answer: None,
                cause: None,
            }],
        }
    }

    #[test]
    fn a_panicking_lens_is_recorded_as_could_not_run_naming_the_panic() {
        let lens = test_lens("correctness");

        let run = contain_panic(&lens, || panic!("fake command blew up"));
        assert_eq!(run.attempts.len(), 1);
        assert_eq!(run.context_error, None);
        let attempt = run.attempts.first().expect("one attempt");
        assert_eq!(attempt.result, "could-not-run");
        assert!(attempt.answer.is_none());
        let reason = attempt.reason.as_deref().expect("a reason");
        assert!(reason.contains("fake command blew up"), "{reason}");
        assert!(reason.contains("correctness"), "{reason}");

        let run = contain_panic(&lens, || {
            std::panic::panic_any(String::from("owned message"));
        });
        assert_eq!(run.context_error, None);
        let attempt = run.attempts.first().expect("one attempt");
        assert_eq!(attempt.result, "could-not-run");
        let reason = attempt.reason.as_deref().expect("a reason");
        assert!(reason.contains("owned message"), "{reason}");

        let run = contain_panic(&lens, || answered(&lens));
        assert_eq!(run.attempts.len(), 1);
        assert_eq!(run.context_error, None);
        let attempt = run.attempts.first().expect("one attempt");
        assert_eq!(attempt.result, "answered");
        assert_eq!(attempt.reason, None);
    }

    #[test]
    fn a_run_s_limit_panic_and_context_reasons_all_go_through_the_scrub() {
        let tmp = crate::test_support::TempDir::new("osf-scrub-run");
        let req = Request {
            root: &tmp,
            config_root: &tmp,
            base: "HEAD",
            work_item: None,
            pull_request: None,
            builder_family_overrides: &[],
            binding: None,
        };
        let secret = format!("{}{}", "sk-", "ABCDEFGHIJKLMNOPQRSTUVWX0123456789");
        let secret = secret.as_str();
        let lens = test_lens("correctness");
        let mut run = ReviewerRun {
            reviewer: "synthetic".to_string(),
            lenses: vec![
                contain_panic(&lens, || panic!("boom {secret}")),
                LensRun {
                    lens: "x".to_string(),
                    context_error: Some(format!("cannot build context {secret}")),
                    attempts: Vec::new(),
                },
            ],
            binding: None,
            not_run: None,
        };
        scrub_run(&req, &mut run);
        let text = serde_json::to_string(&run).expect("serialises");
        assert!(!text.contains(secret), "{text}");
    }

    #[test]
    fn a_run_whose_time_ran_out_before_it_could_start_is_never_worded_as_stopped() {
        let timeout = reviewers::Timeout {
            reviewer: "synthetic".to_string(),
            waited: Duration::from_secs(5),
            kill_failed: false,
            category: crate::failure::Category::Quota,
            before_start: true,
        };
        let per_attempt = Duration::from_secs(5);
        let total = Duration::from_secs(60);
        let at_per_attempt = timeout_reason("synthetic", per_attempt, total, per_attempt, &timeout);
        assert!(
            at_per_attempt.contains("timeout_seconds = 5s"),
            "{at_per_attempt}"
        );
        let at_total = timeout_reason(
            "synthetic",
            per_attempt,
            total,
            Duration::from_secs(3),
            &timeout,
        );
        assert!(
            at_total.contains("total_timeout_seconds = 60s"),
            "{at_total}"
        );
        for reason in [at_per_attempt, at_total] {
            assert!(
                reason.contains("before reviewer 'synthetic' could start again"),
                "{reason}"
            );
            assert!(!reason.contains("stopped"), "{reason}");
        }
    }

    #[test]
    fn a_failed_kill_is_never_worded_as_stopped() {
        let per_attempt = per_attempt_limit("synthetic", 5, true);
        assert!(
            per_attempt.contains("could not be stopped"),
            "{per_attempt}"
        );
        assert!(!per_attempt.contains("was stopped"), "{per_attempt}");
        let total = total_ran_out_while_running("synthetic", 5, true);
        assert!(total.contains("could not be stopped"), "{total}");
        assert!(!total.contains("it was stopped"), "{total}");
        assert!(per_attempt_limit("synthetic", 5, false).contains("was stopped"));
        assert!(total_ran_out_while_running("synthetic", 5, false).contains("it was stopped"));
    }

    #[test]
    fn one_panicking_lens_does_not_stop_the_pool() {
        let lenses = [test_lens("first"), test_lens("second"), test_lens("third")];
        let work = |index: usize| {
            let lens = lenses.get(index).expect("index comes from the pool");
            contain_panic(lens, || {
                assert!(index != 1, "fake command blew up");
                answered(lens)
            })
        };

        for workers in [2, 1] {
            let runs = run_pool(3, workers, &work);
            assert_eq!(runs.len(), 3, "workers = {workers}");
            let first = runs.first().expect("first");
            assert_eq!(first.lens, "first");
            assert_eq!(
                first.attempts.first().map(|a| a.result.as_str()),
                Some("answered")
            );
            let second = runs.get(1).expect("second");
            assert_eq!(second.lens, "second");
            assert_eq!(
                second.attempts.first().map(|a| a.result.as_str()),
                Some("could-not-run")
            );
            let reason = second
                .attempts
                .first()
                .and_then(|a| a.reason.as_deref())
                .expect("a reason");
            assert!(reason.contains("fake command blew up"), "{reason}");
            assert!(reason.contains("second"), "{reason}");
            let third = runs.get(2).expect("third");
            assert_eq!(third.lens, "third");
            assert_eq!(
                third.attempts.first().map(|a| a.result.as_str()),
                Some("answered")
            );
        }
    }

    #[test]
    fn many_unknown_reasons_are_counted_not_listed() {
        let reasons: Vec<String> = (0..50)
            .map(|n| format!("commit {n:012}: no Code-Generator trailer"))
            .collect();
        let detail = unknown_detail(&reasons);
        assert!(
            detail.ends_with("and 47 more commits with an unknown builder family"),
            "{detail}"
        );
        assert!(detail.len() < 400, "{}", detail.len());
        assert_eq!(unknown_detail(&[]), "no family named");
        let four: Vec<String> = reasons.iter().take(4).cloned().collect();
        assert!(unknown_detail(&four).ends_with("and 1 more commit with an unknown builder family"));
    }

    fn other(name: &str, tried: bool, reasons: &[&str]) -> Other {
        Other {
            name: name.to_string(),
            family: "f2".to_string(),
            tried,
            reasons: reasons.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn the_last_resort_needs_evidence_from_reviewers_that_were_tried() {
        let failed = last_resort_evidence(&[other("a", true, &["401 rejected"])])
            .expect("a tried reviewer is evidence");
        let line = failed.first().expect("one line");
        assert!(line.contains("reviewer a (f2)") && line.contains("401 rejected"));

        let untried = last_resort_evidence(&[other("a", true, &["x"]), other("b", false, &[])])
            .expect_err("a reviewer never tried is no evidence");
        assert!(
            untried.contains("reviewer b") && untried.contains("never tried"),
            "{untried}"
        );

        assert_eq!(last_resort_evidence(&[]), Ok(Vec::new()));
        let silent = last_resort_evidence(&[other("a", true, &[])]).expect("tried");
        assert!(
            silent
                .first()
                .is_some_and(|l| l.contains("no error text recorded")),
            "{silent:?}"
        );
    }

    #[test]
    fn a_total_too_large_to_count_is_an_error_naming_the_setting() {
        let err = deadline_after(Duration::MAX).expect_err("no instant is that far away");
        assert!(err.contains("total_timeout_seconds"), "{err}");
        assert!(deadline_after(Duration::from_secs(60)).is_ok());
    }
}
