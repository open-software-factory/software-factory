use osf::pr_status::GhClient;
use osf::{
    agents, answer, assets, changeset_risk, changeset_tests, check, checkpoint, config, exclude,
    forge, git, githooks, github, hook, journal, lints, pr_status, pr_tree, reducer, review,
    review_run, reviewers, scan, section, verify,
};

use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use std::fmt::Write as _;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const INFORMATION_URI: &str = "https://github.com/open-software-factory/software-factory";

/// How to print findings. Human reads well in a terminal; sarif is what
/// GitHub reads on a pull request; json is one finding per line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    Human,
    Sarif,
    Json,
}

/// Human in a terminal, sarif otherwise, unless `--format` says differently.
fn resolve_format(chosen: Option<Format>, json_alias: bool) -> Format {
    if json_alias {
        return Format::Json;
    }
    chosen.unwrap_or_else(|| {
        if std::io::stdout().is_terminal() {
            Format::Human
        } else {
            Format::Sarif
        }
    })
}

/// Where the text lives, for the CLI: mirrors [`lints::Context`], since that
/// type lives in the config-agnostic core crate and cannot derive `clap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ContextArg {
    Transcript,
    Commit,
    Document,
    Skill,
}

impl From<ContextArg> for lints::Context {
    fn from(value: ContextArg) -> Self {
        match value {
            ContextArg::Transcript => lints::Context::Transcript,
            ContextArg::Commit => lints::Context::Commit,
            ContextArg::Document => lints::Context::Document,
            ContextArg::Skill => lints::Context::Skill,
        }
    }
}

/// `--context` wins when given. Otherwise `--message` means transcript,
/// and a file with neither flag is a document.
fn resolve_context(context: Option<ContextArg>, message: bool) -> lints::Context {
    match context {
        Some(c) => c.into(),
        None if message => lints::Context::Transcript,
        None => lints::Context::Document,
    }
}

#[derive(Parser)]
#[command(name = "osf", version, about = "Open Software Factory checks")]
struct Cli {
    /// Config file. Else `OSF_CONFIG`, else `~/.osf/config.toml`, else the compiled defaults.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a lint over files or standard input.
    Lint {
        #[command(subcommand)]
        kind: LintKind,
    },
    /// Answer a coding-agent hook event read from standard input.
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Inspect the writing-lint limits and word lists in force.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print a rule's doc text: what it does, why it is bad, its class, and an example.
    Explain {
        /// A rule id, such as `long-sentence`.
        rule_id: String,
    },
    /// Find text that must never reach a public repository.
    Scan(ScanArgs),
    /// Run one check `osf verify` also runs, on its own file list.
    Check(CheckArgs),
    /// Run every check one gate needs, in one entry point every gate calls.
    Verify(VerifyArgs),
    /// Report on a changeset: its blast radius, or its Rust test summary.
    Changeset {
        #[command(subcommand)]
        action: ChangesetAction,
    },
    /// Publish a code review on a pull request.
    Review {
        #[command(subcommand)]
        action: ReviewAction,
    },
    /// Manage this repository's local git hooks.
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
    /// Read or change a pull request's description.
    Pr {
        #[command(subcommand)]
        action: PrAction,
    },
    /// Publish built files to a data branch.
    Assets {
        #[command(subcommand)]
        action: AssetsAction,
    },
    /// Show the agents osf can drive and which this repository uses.
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },
}

#[derive(Subcommand)]
enum AgentsAction {
    /// List every agent, with what `[agents]` in `osf.toml` selects.
    List(AgentsListArgs),
}

#[derive(Args)]
struct AgentsListArgs {
    /// Print one JSON array instead of a table.
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum HooksAction {
    /// Write osf's own hook scripts and point this repository at them, or
    /// with `--agents`, write each enabled agent's hook settings.
    Install(HooksInstallArgs),
}

#[derive(Args)]
struct HooksInstallArgs {
    /// Report whether this repository's hooks point at osf's own folder,
    /// instead of installing anything. Exits non-zero when they do not.
    #[arg(long)]
    check: bool,
    /// Write the stop and prompt hook settings of every enabled agent under
    /// `--root`, replacing a file already there, instead of the git hooks.
    #[arg(long, requires = "root", conflicts_with = "check")]
    agents: bool,
    /// The folder the agents' settings go under, such as a home directory.
    #[arg(long, requires = "agents")]
    root: Option<PathBuf>,
}

#[derive(Subcommand)]
enum AssetsAction {
    /// Push a folder's files to a path on a branch, creating the branch as
    /// an orphan the first time, and print the raw content web address.
    Publish(AssetsPublishArgs),
}

#[derive(Args)]
struct AssetsPublishArgs {
    /// The branch to publish to, created as an orphan if it does not exist.
    #[arg(long)]
    branch: String,
    /// The path prefix within that branch the folder's files land under.
    #[arg(long)]
    path: String,
    /// The local folder whose files (not subfolders) are published.
    #[arg(long)]
    dir: PathBuf,
    /// The repository, as `owner/name`. Read from the current directory's
    /// `origin` remote when not given.
    #[arg(long)]
    repo: Option<String>,
    /// Push to this remote instead of the repository's own origin, with no
    /// GitHub authentication. For a test against a local repository.
    #[arg(long, hide = true)]
    remote: Option<String>,
    /// How many times to retry the publish when it loses a race with
    /// another run pushing to the same branch.
    #[arg(long, default_value_t = 5)]
    max_attempts: u32,
}

#[derive(Subcommand)]
enum PrAction {
    /// Work with one marked section of a pull request description.
    Section {
        #[command(subcommand)]
        action: SectionAction,
    },
    /// Render or apply the status block at the top of a pull request description.
    Status {
        #[command(subcommand)]
        action: StatusAction,
    },
    /// Render the file table for the tree block of a pull request description.
    Tree {
        #[command(subcommand)]
        action: TreeAction,
    },
}

#[derive(Subcommand)]
enum TreeAction {
    /// Print the collapsed file table for a change, with count chips and
    /// size bars. Put it in a pull request with `osf pr section write
    /// --name tree`.
    Render(TreeRenderArgs),
}

#[derive(Args)]
struct TreeRenderArgs {
    /// What to diff against: a remote-tracking ref or any commit-ish.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// The other side of the diff.
    #[arg(long, default_value = "HEAD")]
    head: String,
}

#[derive(Subcommand)]
enum SectionAction {
    /// Replace one marked section of a pull request description, or
    /// append it when the markers are not there yet.
    Write(SectionWriteArgs),
}

#[derive(Args)]
struct SectionWriteArgs {
    /// The pull request number.
    #[arg(long)]
    pr: u64,
    /// The section's name: the markers are
    /// `<!-- osf:<name>:start head=<sha> -->` and `<!-- osf:<name>:end -->`.
    #[arg(long)]
    name: String,
    /// Path to a file holding the section's content, in Markdown.
    #[arg(long)]
    file: PathBuf,
    /// The commit the section is built for, written into the start marker.
    /// Read from the pull request's head with `gh` when not given.
    #[arg(long)]
    head: Option<String>,
    /// The repository, as `owner/name`. Left to `gh`'s own detection of the
    /// current repository when not given.
    #[arg(long)]
    repo: Option<String>,
}

#[derive(Subcommand)]
enum ChangesetAction {
    /// Report the blast radius of a change: low, normal, or high, with the reasons.
    Risk(RiskArgs),
    /// Print the Rust test summary alone: the same tests-added-changed-
    /// removed report the status block carries, for a person or another
    /// tool to read without going through a pull request at all.
    Tests(ChangesetTestsArgs),
}

#[derive(Args)]
struct RiskArgs {
    /// What to diff the change against. A remote-tracking ref, so a local
    /// commit ahead of it still counts as changed.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// How to print the report: human or json. Sarif has no natural shape
    /// for a single blast-radius answer, so it is refused.
    #[arg(long, value_enum)]
    format: Option<Format>,
}

#[derive(Args)]
struct ChangesetTestsArgs {
    /// What to diff against: a remote-tracking ref or any commit-ish.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// The other side of the diff.
    #[arg(long, default_value = "HEAD")]
    head: String,
}

#[derive(Subcommand)]
enum StatusAction {
    /// Print the status block for the given inputs.
    Render(StatusRenderArgs),
    /// Put the status block into a pull request description.
    Apply(StatusApplyArgs),
    /// Recompute the status block from the pull request's current state and
    /// write it back only when it changed.
    Refresh(StatusRefreshArgs),
}

#[derive(Args)]
struct StatusRenderArgs {
    /// A file holding a JSON object with a string "tier" field and a
    /// "reasons" array of strings.
    #[arg(long)]
    tier_json: PathBuf,
    /// Gate results, comma-separated: "name: passed" or "name: failed: reason".
    #[arg(long)]
    gates: String,
    /// The pull request's head commit, written into the block. Defaults to
    /// `HEAD` of the current directory.
    #[arg(long)]
    head: Option<String>,
    /// The repository, as `owner/name`. Needed with `--pr` when `--review-json` is absent.
    #[arg(long)]
    repo: Option<String>,
    /// The pull request number.
    #[arg(long)]
    pr: Option<String>,
    /// A file holding `gh pr view --json reviewDecision,reviews,comments`
    /// output, instead of fetching it.
    #[arg(long)]
    review_json: Option<PathBuf>,
    /// What to diff against for the Rust test summary, against `HEAD`.
    /// Left out, the block carries no test summary row, the same as
    /// before this existed. Given, computes the summary the same way
    /// `osf pr status refresh` does.
    #[arg(long)]
    base: Option<String>,
    /// One short line for the Automated review row's Details, for example
    /// the model family, the rounds and the fixing commit.
    #[arg(long)]
    automated_review: Option<String>,
    /// One short line for the Human review row's Details, for example the
    /// reviewers' display names.
    #[arg(long)]
    human_review: Option<String>,
}

#[derive(Args)]
struct StatusApplyArgs {
    /// The rendered block to put into the description.
    #[arg(long)]
    block: PathBuf,
    /// The repository, as `owner/name`. Needed with `--pr` when `--body-file` is absent.
    #[arg(long)]
    repo: Option<String>,
    /// The pull request number.
    #[arg(long)]
    pr: Option<String>,
    /// Print the result instead of updating the pull request.
    #[arg(long)]
    dry_run: bool,
    /// Read the description from this file instead of `gh pr view`.
    #[arg(long)]
    body_file: Option<PathBuf>,
    /// Write the result to this file instead of `gh pr edit`.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// The name of the check this whole workflow reports as, so a refresh never
/// treats its own still-running check as a gate to report on.
const STATUS_CHECK_NAME: &str = "status block";

#[derive(Args)]
struct StatusRefreshArgs {
    /// The repository, as `owner/name`.
    #[arg(long)]
    repo: String,
    /// The pull request number.
    #[arg(long)]
    pr: String,
    /// What to diff the change against, for the risk tier. Defaults to
    /// `origin/<the pull request's base branch>`, fetched first.
    #[arg(long)]
    base: Option<String>,
    /// A file holding `gh pr checks --json name,state,bucket` output,
    /// instead of fetching it.
    #[arg(long)]
    checks_json: Option<PathBuf>,
    /// A file holding `gh pr view --json reviewDecision,reviews,comments`
    /// output, instead of fetching it.
    #[arg(long)]
    review_json: Option<PathBuf>,
    /// Print the rendered block and whether it would update, without
    /// editing the pull request.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Subcommand)]
enum ReviewAction {
    /// Post review findings on a pull request as one native review, or as
    /// a fallback comment when the reviewer and the author share one
    /// GitHub identity.
    Post(ReviewPostArgs),
    /// Review a change through its selected lenses, and journal each
    /// answer and the decision. With `--reviewer`, run that one reviewer
    /// only and save what it did to `--out`.
    Run(ReviewRunArgs),
    /// Decide a review from the files `review run --reviewer` saved, one
    /// per reviewer: check the findings against the files, journal, and
    /// post.
    Reduce(ReviewReduceArgs),
    /// Find the work item a pull request is for, the issue on its `Issue:`
    /// line or one it closes, and save its text for the reviewer jobs.
    WorkItem(ReviewWorkItemArgs),
}

#[derive(Args)]
struct ReviewWorkItemArgs {
    /// A JSON file holding the pull request's `number`, `title` and `body`.
    #[arg(long = "pull-request")]
    pull_request: PathBuf,
    /// The repository the pull request lives in, as `owner/repo`. Falls back
    /// to the `GITHUB_REPOSITORY` environment variable.
    #[arg(long)]
    repository: Option<String>,
    /// The pull request's head commit, recorded with the work item.
    #[arg(long)]
    head: String,
    /// Where to save the work item, as JSON. A pull request that links no
    /// readable issue saves the reason instead, and the command still exits 0.
    #[arg(long)]
    out: PathBuf,
}

/// What a saved reviewer run is bound to, so `review reduce` takes only the
/// runs made for this pull request, these commits and this CI run.
#[derive(Args)]
struct ReviewBindingArgs {
    /// The repository, as `owner/repo`. Falls back to the `GITHUB_REPOSITORY`
    /// environment variable.
    #[arg(long)]
    repository: Option<String>,
    /// The pull request's number.
    #[arg(long = "pull-request-number")]
    pull_request_number: Option<u64>,
    /// The pull request's head commit. It must be the commit checked out.
    #[arg(long)]
    head: Option<String>,
    /// The CI run's id. Falls back to the `GITHUB_RUN_ID` environment variable.
    #[arg(long = "ci-run-id")]
    ci_run_id: Option<String>,
}

impl ReviewBindingArgs {
    /// The binding for the checkout at `root` reviewed against `base`.
    /// `event_number` is the pull request number read from the event file,
    /// which must agree with `--pull-request-number` when both are given.
    fn binding(
        &self,
        root: &Path,
        base: &str,
        event_number: Option<u64>,
    ) -> Result<review_run::Binding, String> {
        if let (Some(flag), Some(event)) = (self.pull_request_number, event_number) {
            if flag != event {
                return Err(format!(
                    "--pull-request-number {flag} differs from the pull request file's {event}"
                ));
            }
        }
        let repository = self
            .repository
            .clone()
            .or_else(|| std::env::var("GITHUB_REPOSITORY").ok());
        let run_id = self
            .ci_run_id
            .clone()
            .or_else(|| std::env::var("GITHUB_RUN_ID").ok());
        review_run::Binding::for_checkout(
            root,
            base,
            repository.as_deref(),
            self.pull_request_number.or(event_number),
            self.head.as_deref(),
            run_id.as_deref(),
        )
    }
}

#[derive(Args)]
struct ReviewReduceArgs {
    /// The saved reviewer files.
    files: Vec<PathBuf>,
    #[command(flatten)]
    binding: ReviewBindingArgs,
    /// What to diff the change against. Falls back to the `OSF_BASE`
    /// environment variable when omitted.
    #[arg(long)]
    base: Option<String>,
    /// Write the kept findings as SARIF to this path.
    #[arg(long = "sarif-out")]
    sarif_out: Option<PathBuf>,
    /// Where the lens catalogue, the prompt file and the `[review]` table
    /// of `osf.toml` are read from. Point this at a base tree.
    #[arg(long = "config-root")]
    config_root: Option<PathBuf>,
    /// Post the kept findings to a pull request as one review. Named
    /// `owner/repo#N`. A failed post makes the run could-not-run.
    #[arg(long = "post-to")]
    post_to: Option<String>,
    /// Never fail on the review's own verdict.
    #[arg(long = "warn-only")]
    warn_only: bool,
    /// A family that built this change, repeatable. Overrides detection.
    #[arg(long = "builder-family")]
    builder_family: Vec<String>,
}

#[derive(Args)]
struct ReviewRunArgs {
    /// What to diff the change against. Falls back to the `OSF_BASE`
    /// environment variable, the checkpoint runner's own base, when omitted.
    #[arg(long)]
    base: Option<String>,
    /// A file holding the work item body, for a lens that needs one.
    #[arg(long = "work-item")]
    work_item: Option<PathBuf>,
    /// Write the kept findings as SARIF to this path.
    #[arg(long = "sarif-out")]
    sarif_out: Option<PathBuf>,
    /// Runs only when `[agents]` selects at least one reviewer. With
    /// none selected, prints one line and exits 0 without opening the
    /// journal, so the moon review task skips cleanly on a checkout with no
    /// reviewer configured instead of failing its checkpoint.
    #[arg(long = "if-enabled")]
    if_enabled: bool,
    /// Where the lens catalogue and the `[review]` table of `osf.toml` are
    /// read from, instead of the repository under review. Point this at a
    /// base tree so a pull request cannot weaken its own review by editing
    /// a lens or lowering the threshold. Defaults to the repository under
    /// review, as before this flag existed.
    #[arg(long = "config-root")]
    config_root: Option<PathBuf>,
    /// Post the kept findings to a pull request as one review, reusing `osf
    /// review post`'s own posting machinery. Named `owner/repo#N`. A failed
    /// post makes the run could-not-run: the branch-protection gate must
    /// never see a clean exit code with no review evidence behind it.
    #[arg(long = "post-to")]
    post_to: Option<String>,
    /// Never fail on the review's own verdict: still prints the verdict and
    /// writes SARIF, but always exits 0. For a checkpoint that only warns,
    /// such as pre-push, where the exit code cannot be trusted to gate
    /// anything.
    #[arg(long = "warn-only")]
    warn_only: bool,
    /// A family that built this change, repeatable. Overrides detection
    /// from the reviewed range's own `Code-Generator:` trailers entirely; a
    /// roster entry from a named family is left out of every lens the same
    /// way a detected one would be.
    #[arg(long = "builder-family")]
    builder_family: Vec<String>,
    /// Run only this reviewer, one of the roster's, and save what it did
    /// to `--out`. Nothing is journaled or posted: `review reduce` does that.
    #[arg(long, requires = "out")]
    reviewer: Option<String>,
    /// Where `--reviewer` saves what the reviewer did.
    #[arg(long, requires = "reviewer")]
    out: Option<PathBuf>,
    /// A JSON file holding the pull request's `number`, `title` and `body`.
    #[arg(long = "pull-request")]
    pull_request: Option<PathBuf>,
    /// What `--out` binds the saved run to. Needed with `--reviewer`.
    #[command(flatten)]
    binding: ReviewBindingArgs,
}

#[derive(Args)]
struct ReviewPostArgs {
    /// The repository the pull request lives in, as `owner/repo`.
    repo: String,
    /// The pull request number.
    pr: u64,
    /// Path to a file holding a JSON array of findings. Each needs `id`,
    /// `path`, `severity`, `action` and `body`, and may carry a `line`. A
    /// finding with no line goes in the summary instead of against a line
    /// of the diff.
    findings: PathBuf,
    /// Path to a file holding the review's summary body, in Markdown.
    summary: PathBuf,
    /// Build the review and print it, but post nothing.
    #[arg(long)]
    dry_run: bool,
    /// Comma-separated actions that earn `REQUEST_CHANGES`. Else `REVIEW_BLOCK_ON`, else `must-fix,should-fix`.
    #[arg(long)]
    block_on: Option<String>,
}

#[derive(Args)]
struct ScanArgs {
    /// Paths to scan. With none, every file git tracks in the current repository.
    paths: Vec<PathBuf>,
    /// How to print findings: human, sarif, or json.
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// Scan commit messages in this git revision range instead of files.
    #[arg(long)]
    commits: Option<String>,
    /// A path pattern to skip, on top of the configured list. Repeatable.
    /// For a person running the tool by hand; a gate run does not accept it.
    #[arg(long = "exclude", conflicts_with = "gate")]
    exclude: Vec<String>,
    /// Ignore the exclude list entirely and check everything. For a person
    /// running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    no_exclude: bool,
    /// Runs as a gate over a change that has not yet been approved: the exclude
    /// list is the compiled defaults only, never the config file or the
    /// environment, so that change cannot loosen this check by editing its
    /// own configuration.
    #[arg(long)]
    gate: bool,
    /// Ignore every osf-disable marker and report everything. Continuous
    /// integration uses this.
    #[arg(long)]
    no_suppress: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// Which check to run: scan, scan-staged, lint-writing, lint-skill, or scan-commits.
    #[arg(value_enum)]
    name: check::CheckName,
    /// Which checkpoint is calling: hook, pre-commit, pre-push, pull-request, or schedule. Required.
    #[arg(long, value_enum)]
    checkpoint: Option<checkpoint::Checkpoint>,
    /// What to diff commits against, for `scan-commits`. Else `OSF_BASE`, else the default branch.
    #[arg(long)]
    base: Option<String>,
    /// Ignores a suppression marker and uses the compiled exclude list, the
    /// same way `--gate` already does for `osf verify`.
    #[arg(long)]
    gate: bool,
    /// Write the findings as SARIF 2.1.0 to this path, creating parent folders.
    #[arg(long = "sarif-out")]
    sarif_out: Option<PathBuf>,
    /// Files to check. Else one repository-relative path per line in the
    /// file named by `OSF_FILES_FROM`. Else nothing to check.
    files: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum StageArg {
    PreCommit,
    PrePush,
    Ci,
}

impl From<StageArg> for checkpoint::Checkpoint {
    fn from(value: StageArg) -> Self {
        match value {
            StageArg::PreCommit => checkpoint::Checkpoint::PreCommit,
            StageArg::PrePush => checkpoint::Checkpoint::PrePush,
            StageArg::Ci => checkpoint::Checkpoint::PullRequest,
        }
    }
}

#[derive(Args)]
struct VerifyArgs {
    /// Which checkpoint is calling: hook, pre-commit, pre-push, pull-request, or schedule.
    #[arg(long, value_enum, conflicts_with = "stage")]
    checkpoint: Option<checkpoint::Checkpoint>,
    /// Deprecated alias for `--checkpoint`: pre-commit, pre-push, or ci (mapped to pull-request).
    #[arg(long, value_enum)]
    stage: Option<StageArg>,
    /// What to diff changed files against. Defaults to the repository's default branch.
    #[arg(long)]
    base: Option<String>,
    /// Reads the hook checkpoint's file list from standard input, one repository-relative path per line.
    #[arg(long)]
    files_from_stdin: bool,
    /// How long to let moon run before it is killed, in seconds. No limit when absent.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Extra positional arguments a git hook passes: pre-push's remote name
    /// (its first entry, read as the remote `verify_cmd` resolves a
    /// fallback base against) and URL, so a real hook call is never refused.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    hook_args: Vec<String>,
}

#[derive(Subcommand)]
enum LintKind {
    /// Check prose for references, chat-local phrases, sentence length, and style shapes.
    Writing(WritingArgs),
    /// Check a skill folder's SKILL.md and scripts for structural and safety problems.
    Skill(SkillLintArgs),
}

#[derive(Args)]
struct SkillLintArgs {
    /// Skill folders to check. Each must hold a `SKILL.md` file.
    paths: Vec<PathBuf>,
    /// How to print findings: human, sarif, or json.
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,
    /// A first section that is not step-shaped may hold this many paragraphs. Overrides the config file.
    /// For a person running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    overview_max_paragraphs: Option<usize>,
    /// A first section that is not step-shaped may hold this many words. Overrides the config file.
    /// For a person running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    overview_max_words: Option<usize>,
    /// Extra names that need no description, one per line. For a person
    /// running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    known_names: Option<PathBuf>,
    /// Runs as a gate over a change that has not yet been approved: every
    /// setting is the compiled default, never the config file or the
    /// environment, so that change cannot loosen this check by editing its
    /// own configuration.
    #[arg(long)]
    gate: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct WritingArgs {
    /// Files to check. With no files, standard input is checked.
    paths: Vec<PathBuf>,
    /// How to print findings: human, sarif, or json.
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// Report findings as JSON lines. Deprecated: use `--format json`.
    #[arg(long, hide = true)]
    json: bool,
    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,
    /// Extra names that need no description, one per line. For a person
    /// running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    known_names: Option<PathBuf>,
    /// The text is a reply to a person: shorthand for `--context transcript`.
    #[arg(long)]
    message: bool,
    /// Where the text lives. Sets the level and remediation for every rule.
    /// Defaults to transcript with `--message`, else document.
    #[arg(long, value_enum)]
    context: Option<ContextArg>,
    /// Ignore every osf-disable marker and report everything. Continuous
    /// integration uses this.
    #[arg(long)]
    no_suppress: bool,
    /// Error above this many words in a sentence. Overrides the config file.
    #[arg(long, default_value_t = 25)]
    max_sentence_words: usize,
    /// Warn above this many words in a sentence, under the error limit. Overrides the config file.
    #[arg(long)]
    warn_sentence_words: Option<usize>,
    /// Warn above this many numbers in one sentence. Overrides the config file.
    #[arg(long, default_value_t = 2)]
    max_numerals: usize,
    /// A text under this many words must not carry a heading. Overrides the config file.
    #[arg(long, default_value_t = 500)]
    short_text_words: usize,
    /// A path pattern to skip, on top of the configured list. Repeatable.
    /// For a person running the tool by hand; a gate run does not accept it.
    #[arg(long = "exclude", conflicts_with = "gate")]
    exclude: Vec<String>,
    /// Ignore the exclude list entirely and check everything. For a person
    /// running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    no_exclude: bool,
    /// Runs as a gate over a change that has not yet been approved: the exclude
    /// list is the compiled defaults only, never the config file or the
    /// environment, so that change cannot loosen this check by editing its
    /// own configuration.
    #[arg(long)]
    gate: bool,
}

#[derive(Subcommand)]
enum HookEvent {
    /// The agent wants to end its turn: lint the final message, refuse it on errors.
    Stop(StopArgs),
    /// The user submitted a new prompt: deliver any advice stored from the last turn.
    Prompt,
    /// A tool wrote a file: run the hook checkpoint on it, refuse it on errors.
    PostTool(PostToolArgs),
}

#[derive(Args)]
struct PostToolArgs {
    /// How long to let the hook checkpoint run before it reports skipped, in seconds.
    #[arg(long, default_value_t = 45)]
    timeout_secs: u64,
    /// How to report a refusal: `exit-code` or `decision-json`. Guessed from
    /// the event's key spelling when not given.
    #[arg(long, value_enum)]
    answer: Option<hook::Answer>,
}

#[derive(Args)]
struct StopArgs {
    /// Extra names that need no description, one per line.
    #[arg(long)]
    known_names: Option<PathBuf>,
    /// Refuse the stop at most this many times per turn, then let it through.
    #[arg(long, default_value_t = 2)]
    max_bounces: u32,
    /// How to report a refusal: `exit-code` or `decision-json`. Guessed from
    /// the event's key spelling when not given. An adapter that builds the
    /// event itself should always pass this.
    #[arg(long, value_enum)]
    answer: Option<hook::Answer>,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the values in force as TOML, and which layer set each one.
    Show,
}

fn main() -> ExitCode {
    let top_matches = Cli::command().get_matches();
    let cli = match Cli::from_arg_matches(&top_matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    match &cli.command {
        Command::Lint {
            kind: LintKind::Writing(args),
        } => {
            let sub = top_matches
                .subcommand_matches("lint")
                .and_then(|m| m.subcommand_matches("writing"));
            lint_writing(args, sub, cli.config.as_deref())
        }
        Command::Lint {
            kind: LintKind::Skill(args),
        } => lint_skill_cmd(args, cli.config.as_deref()),
        Command::Hook {
            event: HookEvent::Stop(args),
        } => {
            let loaded = match config::load(cli.config.as_deref(), &[], &[], false) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("osf: {e}");
                    return ExitCode::from(2);
                }
            };
            hook::stop(
                args.known_names.as_deref(),
                args.max_bounces,
                &loaded.config.writing,
                args.answer,
            )
        }
        Command::Hook {
            event: HookEvent::Prompt,
        } => hook::prompt(),
        Command::Hook {
            event: HookEvent::PostTool(args),
        } => hook::post_tool(
            std::time::Duration::from_secs(args.timeout_secs),
            args.answer,
        ),
        Command::Config {
            action: ConfigAction::Show,
        } => config_show(cli.config.as_deref()),
        Command::Explain { rule_id } => explain(rule_id),
        Command::Scan(args) => scan_cmd(args, cli.config.as_deref()),
        Command::Check(args) => check_cmd(args, cli.config.as_deref()),
        Command::Verify(args) => verify_cmd(args),
        Command::Changeset {
            action: ChangesetAction::Risk(args),
        } => changeset_risk_cmd(args),
        Command::Changeset {
            action: ChangesetAction::Tests(args),
        } => changeset_tests_cmd(args),
        Command::Review {
            action: ReviewAction::Post(args),
        } => review_post_cmd(args),
        Command::Review {
            action: ReviewAction::Run(args),
        } => review_run_cmd(args),
        Command::Review {
            action: ReviewAction::Reduce(args),
        } => review_reduce_cmd(args),
        Command::Review {
            action: ReviewAction::WorkItem(args),
        } => review_work_item_cmd(args),
        Command::Hooks {
            action: HooksAction::Install(args),
        } => hooks_install_cmd(args),
        Command::Pr { action } => pr_cmd(action),
        Command::Assets {
            action: AssetsAction::Publish(args),
        } => assets_publish_cmd(args),
        Command::Agents {
            action: AgentsAction::List(args),
        } => agents_list_cmd(args),
    }
}

fn pr_cmd(action: &PrAction) -> ExitCode {
    match action {
        PrAction::Section {
            action: SectionAction::Write(args),
        } => pr_section_write_cmd(args),
        PrAction::Status {
            action: StatusAction::Render(args),
        } => pr_status_render_cmd(args),
        PrAction::Status {
            action: StatusAction::Apply(args),
        } => pr_status_apply_cmd(args),
        PrAction::Status {
            action: StatusAction::Refresh(args),
        } => pr_status_refresh_cmd(args),
        PrAction::Tree {
            action: TreeAction::Render(args),
        } => pr_tree_render_cmd(args),
    }
}

/// One agent's row in `osf agents list`.
#[derive(serde::Serialize)]
struct AgentRow {
    name: &'static str,
    family: &'static str,
    enabled: bool,
    builder: bool,
    reviewer: bool,
    model: Option<String>,
    command: &'static [&'static str],
    hooks: &'static str,
}

fn agents_list_cmd(args: &AgentsListArgs) -> ExitCode {
    let selection = match agents::selection(Path::new(".")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("osf agents list: {e}");
            return ExitCode::from(2);
        }
    };
    let rows: Vec<AgentRow> = agents::AGENTS
        .iter()
        .map(|a| {
            let is = |list: &[&agents::Agent]| list.iter().any(|x| x.name == a.name);
            AgentRow {
                name: a.name,
                family: a
                    .family_for(selection.model(a))
                    .unwrap_or(osf::builder::UNKNOWN),
                enabled: is(&selection.enabled),
                builder: selection.builder.name == a.name,
                reviewer: is(&selection.reviewers),
                model: selection.model(a).map(str::to_string),
                command: a.command,
                hooks: a.hooks.file(),
            }
        })
        .collect();
    if args.json {
        match serde_json::to_string(&rows) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("osf agents list: {e}");
                return ExitCode::from(2);
            }
        }
        return ExitCode::SUCCESS;
    }
    let yes = |on: bool| if on { "yes" } else { "no" }.to_string();
    let mut table: Vec<Vec<String>> = vec![[
        "agent",
        "family",
        "enabled",
        "builder",
        "reviewer",
        "model",
        "hook settings",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()];
    for row in &rows {
        table.push(vec![
            row.name.to_string(),
            row.family.to_string(),
            yes(row.enabled),
            yes(row.builder),
            yes(row.reviewer),
            row.model.clone().unwrap_or_else(|| "-".to_string()),
            row.hooks.to_string(),
        ]);
    }
    let mut widths = vec![0; 7];
    for line in &table {
        for (width, cell) in widths.iter_mut().zip(line) {
            *width = (*width).max(cell.len());
        }
    }
    for line in &table {
        let cells: Vec<String> = line
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        println!("{}", cells.join("  ").trim_end());
    }
    ExitCode::SUCCESS
}
fn hooks_install_agents_cmd(root: &Path) -> ExitCode {
    let selection = match agents::selection(Path::new(".")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("osf hooks install: {e}");
            return ExitCode::from(2);
        }
    };
    match agents::write_hooks(root, &selection.enabled) {
        Ok(paths) => {
            for path in paths {
                println!("osf hooks install: wrote {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf hooks install: {e}");
            ExitCode::from(2)
        }
    }
}

fn hooks_install_cmd(args: &HooksInstallArgs) -> ExitCode {
    let dir = Path::new(".");
    if let Some(root) = args.root.as_deref().filter(|_| args.agents) {
        return hooks_install_agents_cmd(root);
    }
    if args.check {
        return hooks_check_cmd(dir);
    }
    match githooks::install(dir) {
        Ok(report) => {
            let names: Vec<&str> = report
                .scripts
                .iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                .collect();
            println!(
                "osf hooks install: wrote {} to {}",
                names.join(", "),
                report.hooks_dir.display()
            );
            println!(
                "osf hooks install: set core.hooksPath to {} in {}",
                report.hooks_dir.display(),
                report.repo_root.display()
            );
            match &report.osf_on_path {
                Some(path) => {
                    println!("osf hooks install: osf is on PATH at {}", path.display());
                }
                None => eprintln!(
                    "osf hooks install: warning: osf is not on PATH; the hook scripts call \
                     \"osf\", which will fail until it is"
                ),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

fn hooks_check_cmd(dir: &Path) -> ExitCode {
    match githooks::check(dir) {
        Ok(status) => {
            println!("osf hooks install --check: {}", status.message());
            if status.is_installed() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

fn explain(rule_id: &str) -> ExitCode {
    let Some(meta) = lints::rule_meta(rule_id).or_else(|| scan::rule_meta(rule_id)) else {
        eprintln!("osf: no such rule: {rule_id}");
        return ExitCode::from(2);
    };
    println!(
        "{} (class: {}, group: {}, citation: {})\n",
        meta.id, meta.class, meta.group, meta.citation
    );
    if lints::policy::disabled_by_default(rule_id) {
        println!("Enforcement: disabled by default following the rule audit. The detector and its purpose are retained for evaluation and future improvement. Every command applies this compiled policy last, so a configuration cannot re-enable the rule.\n");
    }
    println!("{}", meta.doc);
    ExitCode::SUCCESS
}

/// Builds the flag layer from the fields the user actually passed on the
/// command line, told apart from a flag's own clap default by
/// [`ArgMatches::value_source`].
fn flags_overlay(
    sub: Option<&ArgMatches>,
    args: &WritingArgs,
) -> Vec<(&'static [&'static str], toml::Value)> {
    let Some(m) = sub else {
        return Vec::new();
    };
    let passed = |id: &str| m.value_source(id) == Some(ValueSource::CommandLine);
    let as_int = |n: usize| toml::Value::Integer(i64::try_from(n).unwrap_or(i64::MAX));
    let mut overlay: Vec<(&'static [&'static str], toml::Value)> = Vec::new();
    if passed("max_sentence_words") {
        overlay.push((
            &["writing", "max_sentence_words"],
            as_int(args.max_sentence_words),
        ));
    }
    if let Some(warn) = args.warn_sentence_words {
        overlay.push((&["writing", "warn_sentence_words"], as_int(warn)));
    }
    if passed("max_numerals") {
        overlay.push((&["writing", "max_numerals"], as_int(args.max_numerals)));
    }
    if passed("short_text_words") {
        overlay.push((
            &["writing", "short_text_words"],
            as_int(args.short_text_words),
        ));
    }
    overlay
}

fn config_show(config_flag: Option<&std::path::Path>) -> ExitCode {
    let loaded = match config::load(config_flag, &[], &[], false) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    match &loaded.file {
        Some(p) => println!("# file: {}", p.display()),
        None => println!("# file: none; compiled defaults only"),
    }
    for (path, layer) in &loaded.sources {
        let value = osf_lint_core::lookup(&loaded.tree, path)
            .map_or_else(|| "?".to_string(), ToString::to_string);
        println!("{path} = {value}  # {layer}");
    }
    ExitCode::SUCCESS
}

/// Files named on the command line, or standard input when none are given.
fn read_inputs(paths: &[PathBuf]) -> Result<Vec<(String, String)>, ExitCode> {
    if paths.is_empty() {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| {
            eprintln!("osf: cannot read standard input: {e}");
            ExitCode::from(2)
        })?;
        return Ok(vec![("<stdin>".to_string(), text)]);
    }
    paths
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .map(|t| (p.display().to_string(), t))
                .map_err(|e| {
                    eprintln!("osf: cannot read {}: {e}", p.display());
                    ExitCode::from(2)
                })
        })
        .collect()
}

/// How many findings landed at each level, across every input; how many
/// candidate inputs the exclude list dropped before they were checked; and
/// how many files were checked against a declared expectation instead.
#[derive(Default)]
struct Tally {
    errors: usize,
    warnings: usize,
    infos: usize,
    suppressed: usize,
    excluded: usize,
    declared: usize,
}

impl Tally {
    fn count(&mut self, findings: &[lints::Finding]) {
        for f in findings {
            if f.suppressed.is_some() {
                self.suppressed += 1;
            } else {
                match f.level {
                    lints::Level::Error => self.errors += 1,
                    lints::Level::Warning => self.warnings += 1,
                    lints::Level::Info => self.infos += 1,
                }
            }
        }
    }
}

/// Builds the exclude matcher for one run: nothing at all with
/// `--no-exclude`, otherwise the resolved config list, which already
/// carries any `--exclude` flags added on top of the compiled defaults.
fn build_excluder(configured: &[String], no_exclude: bool) -> Result<exclude::Excluder, String> {
    if no_exclude {
        return Ok(exclude::Excluder::none());
    }
    exclude::Excluder::build(configured)
}

/// Turns a declared fixture's mismatch into findings: one error per rule
/// id it promised but did not produce, one per rule id it produced but did
/// not promise. Fixed at error, never run through `cfg.levels`: a config
/// file must not be able to turn off the one check that catches a rule
/// that silently stopped firing.
fn expectation_findings(mismatch: &lints::Mismatch) -> Vec<lints::Finding> {
    let missing = mismatch.missing.iter().map(|id| {
        lints::Finding::new(
            "expectation-missing",
            lints::Level::Error,
            1,
            format!("declared rule '{id}' did not fire; it may have stopped working"),
            id.clone(),
        )
    });
    let unexpected = mismatch.unexpected.iter().map(|id| {
        lints::Finding::new(
            "expectation-unexpected",
            lints::Level::Error,
            1,
            format!("rule '{id}' fired but this file did not declare it"),
            id.clone(),
        )
    });
    missing.chain(unexpected).collect()
}

/// A warning that an `osf-expect` marker outside a `tests/fixtures` path
/// has no effect: the file is still linted normally, findings and all.
fn outside_fixtures_warning() -> lints::Finding {
    lints::Finding::new(
        "expectation-outside-fixtures",
        lints::Level::Warning,
        1,
        "an osf-expect marker only applies under a tests/fixtures path; ignoring it here"
            .to_string(),
        "osf-expect".to_string(),
    )
}

/// One error per declared id that names a scan rule: a scan finding must
/// never be silenced by anything inside the repository, a fixture's own
/// declaration included.
fn forbidden_scan_rule_findings(ids: &[&String]) -> Vec<lints::Finding> {
    ids.iter()
        .map(|id| {
            lints::Finding::new(
                "expectation-forbidden-scan-rule",
                lints::Level::Error,
                1,
                format!("a scan finding cannot be declared expected: '{id}'"),
                (*id).clone(),
            )
        })
        .collect()
}

/// Runs a declared fixture's assertion: `raw` must fire exactly the rule
/// ids `expected` names. Prints a one-line note on a match; on a mismatch,
/// returns the missing and unexpected findings for the normal pipeline.
fn check_declaration(
    name: &str,
    expected: &std::collections::BTreeSet<String>,
    raw: &[lints::Finding],
    format: Format,
) -> Vec<lints::Finding> {
    let mismatch = lints::check_expectation(expected, raw);
    if mismatch.is_empty() {
        if format == Format::Human {
            let ids: Vec<&str> = expected.iter().map(String::as_str).collect();
            println!(
                "{name}: declaration matched ({} rule(s): {})",
                ids.len(),
                ids.join(", ")
            );
        }
        return Vec::new();
    }
    expectation_findings(&mismatch)
}

/// Applies `--strict`, tallies, and prints `findings` for one input, in
/// every format except sarif, which the caller batches.
fn finish_lint_one(
    name: &str,
    text: &str,
    mut findings: Vec<lints::Finding>,
    args: &WritingArgs,
    format: Format,
    tally: &mut Tally,
) -> Vec<lints::Finding> {
    if args.strict {
        for f in &mut findings {
            if f.suppressed.is_none() && f.level == lints::Level::Warning {
                f.level = lints::Level::Error;
            }
        }
    }
    tally.count(&findings);
    let visible: Vec<lints::Finding> = findings
        .iter()
        .filter(|f| f.suppressed.is_none())
        .cloned()
        .collect();
    match format {
        Format::Human if !visible.is_empty() => {
            println!("{}", osf_lint_core::render_human(name, text, &visible));
        }
        Format::Human | Format::Sarif => {}
        Format::Json => {
            for f in &visible {
                println!("{}", f.to_json(name, f.level));
            }
        }
    }
    findings
}

/// Lints one named input. A file under `tests/fixtures` that declares an
/// `osf-expect` marker is checked against that declaration instead of
/// against the usual level rules; every other file is linted as before.
fn lint_one(
    name: &str,
    text: &str,
    args: &WritingArgs,
    known: &lints::KnownNames,
    cfg: &config::WritingConfig,
    format: Format,
    tally: &mut Tally,
) -> Vec<lints::Finding> {
    let context = resolve_context(args.context, args.message);
    let raw = lints::writing::lint_writing(text, known, cfg, context, false, args.no_suppress);
    if let Some(expected) = lints::parse_expectation(text) {
        if lints::is_fixture_path(name) {
            tally.declared += 1;
            let forbidden: Vec<&String> = expected
                .iter()
                .filter(|id| lints::is_scan_rule(id))
                .collect();
            let findings = if forbidden.is_empty() {
                check_declaration(name, &expected, &raw, format)
            } else {
                forbidden_scan_rule_findings(&forbidden)
            };
            return finish_lint_one(name, text, findings, args, format, tally);
        }
        let mut findings = osf_lint_core::apply_level_overrides(raw, &cfg.levels);
        findings.push(outside_fixtures_warning());
        return finish_lint_one(name, text, findings, args, format, tally);
    }
    let findings = osf_lint_core::apply_level_overrides(raw, &cfg.levels);
    finish_lint_one(name, text, findings, args, format, tally)
}

fn render_sarif(sarif_files: &[(String, Vec<lints::Finding>)]) -> Result<String, ExitCode> {
    let tool = osf_lint_core::ToolInfo {
        name: "osf",
        version: env!("CARGO_PKG_VERSION"),
        information_uri: INFORMATION_URI,
    };
    let report = osf_lint_core::to_sarif(sarif_files, &tool);
    serde_json::to_string_pretty(&report).map_err(|e| {
        eprintln!("osf: cannot render sarif: {e}");
        ExitCode::from(2)
    })
}

fn print_sarif(sarif_files: &[(String, Vec<lints::Finding>)]) -> Result<(), ExitCode> {
    let text = render_sarif(sarif_files)?;
    println!("{text}");
    Ok(())
}

/// Writes `sarif_files` as SARIF 2.1.0 to `path`, creating its parent
/// folders first, so a checkpoint task can point `--sarif-out` anywhere.
fn write_sarif_out(
    path: &Path,
    sarif_files: &[(String, Vec<lints::Finding>)],
) -> Result<(), ExitCode> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                eprintln!("osf: cannot create {}: {e}", parent.display());
                ExitCode::from(2)
            })?;
        }
    }
    let text = render_sarif(sarif_files)?;
    std::fs::write(path, text).map_err(|e| {
        eprintln!("osf: cannot write {}: {e}", path.display());
        ExitCode::from(2)
    })
}

/// The files a check runs over: `FILES` when given, else every non-blank
/// line in the file named by `OSF_FILES_FROM`, else none. A backslash path
/// is normalised to the forward-slash, repository-relative form every
/// other path in this tool already uses.
fn resolve_check_files(cli_files: &[String]) -> Result<Vec<String>, ExitCode> {
    if !cli_files.is_empty() {
        return Ok(cli_files.iter().map(|p| p.replace('\\', "/")).collect());
    }
    let Some(list_path) = std::env::var_os("OSF_FILES_FROM") else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(&list_path).map_err(|e| {
        eprintln!(
            "osf: cannot read {}: {e}",
            PathBuf::from(&list_path).display()
        );
        ExitCode::from(2)
    })?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.replace('\\', "/"))
        .collect())
}

/// `--checkpoint` is missing: exits 2, naming every value it accepts, so a
/// broken moon task fails loudly instead of silently reading the wrong
/// checkpoint's content.
fn missing_checkpoint_exit() -> ExitCode {
    let values: Vec<&str> = checkpoint::Checkpoint::value_variants()
        .iter()
        .map(|c| c.label())
        .collect();
    eprintln!("osf check: --checkpoint is required: {}", values.join(", "));
    ExitCode::from(2)
}

fn check_cmd(args: &CheckArgs, config_flag: Option<&std::path::Path>) -> ExitCode {
    let Some(checkpoint) = args.checkpoint else {
        return missing_checkpoint_exit();
    };
    let loaded = match config::load(config_flag, &[], &[], args.gate) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let excluder = match exclude::Excluder::build(&loaded.config.exclude) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let files = match resolve_check_files(&args.files) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let name = args.name.label();
    if args.name != check::CheckName::ScanCommits && files.is_empty() {
        println!("osf check {name}: nothing to check");
        if let Some(path) = &args.sarif_out {
            if let Err(code) = write_sarif_out(path, &[]) {
                return code;
            }
        }
        return ExitCode::SUCCESS;
    }
    let base = args.base.clone().or_else(|| std::env::var("OSF_BASE").ok());
    let opts = verify::Options {
        dir: Path::new("."),
        base,
        message_file: None,
        config: &loaded.config,
        excluder: &excluder,
        checkpoint,
    };
    match args.name {
        check::CheckName::LintWriting => eprintln!(
            "osf check writing policy: {}",
            lints::policy::coverage(&loaded.config.writing.levels)
        ),
        check::CheckName::LintSkill => {
            eprintln!(
                "osf skill-script-unpinned {}",
                lints::script_pins::coverage()
            );
            eprintln!(
                "osf check skill policy: {}",
                lints::policy::coverage(&lints::policy::skill_levels(
                    &loaded.config.writing.levels,
                    &loaded.config.skill.levels
                ))
            );
        }
        _ => {}
    }
    let findings = match check::run_check(args.name, &opts, &files, args.gate) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;
    let mut sarif_files: Vec<(String, Vec<lints::Finding>)> = Vec::new();
    for (path, finding) in &findings {
        if finding.suppressed.is_none() {
            println!("{}", finding.render(path, finding.level));
            match finding.level {
                lints::Level::Error => errors += 1,
                lints::Level::Warning => warnings += 1,
                lints::Level::Info => infos += 1,
            }
        }
        sarif_files.push((path.clone(), vec![finding.clone()]));
    }
    println!("osf check {name}: {errors} error(s), {warnings} warning(s), {infos} info");
    if let Some(path) = &args.sarif_out {
        if let Err(code) = write_sarif_out(path, &sarif_files) {
            return code;
        }
    }
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn lint_writing(
    args: &WritingArgs,
    sub: Option<&ArgMatches>,
    config_flag: Option<&std::path::Path>,
) -> ExitCode {
    let overlay = flags_overlay(sub, args);
    let loaded = match config::load(config_flag, &overlay, &args.exclude, args.gate) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let cfg = &loaded.config.writing;
    eprintln!(
        "osf lint writing policy: {}",
        lints::policy::coverage(&cfg.levels)
    );
    let known = match lints::load_known_names(&cfg.known_names, args.known_names.as_deref()) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let excluder = match build_excluder(&loaded.config.exclude, args.no_exclude) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let mut tally = Tally::default();
    let paths = if args.paths.is_empty() {
        args.paths.clone()
    } else {
        let path_strings: Vec<String> =
            args.paths.iter().map(|p| p.display().to_string()).collect();
        let (kept, dropped) = excluder.partition(path_strings);
        tally.excluded = dropped;
        kept.into_iter().map(PathBuf::from).collect()
    };
    let inputs = match read_inputs(&paths) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let format = resolve_format(args.format, args.json);
    let mut sarif_files: Vec<(String, Vec<lints::Finding>)> = Vec::new();
    for (name, text) in &inputs {
        let findings = lint_one(name, text, args, &known, cfg, format, &mut tally);
        if format == Format::Sarif {
            sarif_files.push((name.clone(), findings));
        }
    }
    if format == Format::Sarif {
        if let Err(code) = print_sarif(&sarif_files) {
            return code;
        }
    } else if format == Format::Human {
        println!(
            "osf lint writing: {} error(s), {} warning(s), {} info, {} suppressed, {} excluded, {} declared",
            tally.errors, tally.warnings, tally.infos, tally.suppressed, tally.excluded, tally.declared
        );
    }
    if tally.errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Builds the flag layer from the size-budget overrides the user actually
/// passed, told apart from "not passed" by `Option::is_none`, since neither
/// flag carries a clap default.
fn skill_flags_overlay(args: &SkillLintArgs) -> Vec<(&'static [&'static str], toml::Value)> {
    let as_int = |n: usize| toml::Value::Integer(i64::try_from(n).unwrap_or(i64::MAX));
    let mut overlay: Vec<(&'static [&'static str], toml::Value)> = Vec::new();
    if let Some(n) = args.overview_max_paragraphs {
        overlay.push((&["skill", "overview_max_paragraphs"], as_int(n)));
    }
    if let Some(n) = args.overview_max_words {
        overlay.push((&["skill", "overview_max_words"], as_int(n)));
    }
    overlay
}

fn lint_skill_cmd(args: &SkillLintArgs, config_flag: Option<&std::path::Path>) -> ExitCode {
    let overlay = skill_flags_overlay(args);
    let loaded = match config::load(config_flag, &overlay, &[], args.gate) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let cfg = &loaded.config.skill;
    let writing_cfg = &loaded.config.writing;
    let levels = lints::policy::skill_levels(&writing_cfg.levels, &cfg.levels);
    eprintln!(
        "osf skill-script-unpinned {}",
        lints::script_pins::coverage()
    );
    eprintln!(
        "osf lint skill policy: {}",
        lints::policy::coverage(&levels)
    );
    let known = match lints::load_known_names(&writing_cfg.known_names, args.known_names.as_deref())
    {
        Ok(k) => k,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    if args.paths.is_empty() {
        eprintln!("osf: lint skill needs at least one skill folder");
        return ExitCode::from(2);
    }
    let format = resolve_format(args.format, false);
    let mut tally = Tally::default();
    let mut sarif_files: Vec<(String, Vec<lints::Finding>)> = Vec::new();
    for dir in &args.paths {
        let label = dir.to_string_lossy().replace('\\', "/");
        let skill_findings =
            match lints::skill::lint_skill_checked(dir, &label, cfg, &known, writing_cfg) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("osf: {e}");
                    return ExitCode::from(2);
                }
            };
        let mut by_file: std::collections::BTreeMap<String, Vec<lints::Finding>> =
            std::collections::BTreeMap::new();
        for sf in skill_findings {
            by_file.entry(sf.file).or_default().push(sf.finding);
        }
        for (file, raw_findings) in by_file {
            let name = format!("{}/{file}", dir.display());
            let mut findings = osf_lint_core::apply_level_overrides(raw_findings, &levels);
            if args.strict {
                for f in &mut findings {
                    if f.suppressed.is_none() && f.level == lints::Level::Warning {
                        f.level = lints::Level::Error;
                    }
                }
            }
            tally.count(&findings);
            let visible: Vec<lints::Finding> = findings
                .iter()
                .filter(|f| f.suppressed.is_none())
                .cloned()
                .collect();
            match format {
                Format::Human => {
                    for f in &visible {
                        println!("{}", f.render(&name, f.level));
                    }
                }
                Format::Sarif => {}
                Format::Json => {
                    for f in &visible {
                        println!("{}", f.to_json(&name, f.level));
                    }
                }
            }
            if format == Format::Sarif {
                sarif_files.push((name, findings));
            }
        }
    }
    if format == Format::Sarif {
        if let Err(code) = print_sarif(&sarif_files) {
            return code;
        }
    } else if format == Format::Human {
        println!(
            "osf lint skill: {} error(s), {} warning(s), {} info, {} suppressed",
            tally.errors, tally.warnings, tally.infos, tally.suppressed
        );
        println!("{}", lints::agnix::checked_note());
    }
    if tally.errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn scan_cmd(args: &ScanArgs, config_flag: Option<&std::path::Path>) -> ExitCode {
    let loaded = match config::load(config_flag, &[], &args.exclude, args.gate) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let dir = Path::new(".");
    let rules = match scan::Rules::build(dir, &loaded.config.scan) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    for note in rules.notes() {
        eprintln!("osf scan: {note}");
    }
    let excluder = match build_excluder(&loaded.config.exclude, args.no_exclude) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let mut tally = Tally::default();
    let results = if let Some(range) = &args.commits {
        match scan::scan_commits(dir, range, &rules) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        match scan::scan_paths(dir, &args.paths, &rules, &excluder, args.no_suppress) {
            Ok(outcome) => {
                tally.excluded = outcome.excluded;
                outcome.files
            }
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        }
    };

    let format = resolve_format(args.format, false);
    let mut sarif_files: Vec<(String, Vec<lints::Finding>)> = Vec::new();
    for (name, raw_findings) in results {
        let findings =
            osf_lint_core::apply_level_overrides(raw_findings, &loaded.config.scan.levels);
        tally.count(&findings);
        let visible: Vec<lints::Finding> = findings
            .iter()
            .filter(|f| f.suppressed.is_none())
            .cloned()
            .collect();
        match format {
            Format::Human => {
                for f in &visible {
                    println!("{}", f.render(&name, f.level));
                }
            }
            Format::Sarif => {}
            Format::Json => {
                for f in &visible {
                    println!("{}", f.to_json(&name, f.level));
                }
            }
        }
        if format == Format::Sarif {
            sarif_files.push((name, findings));
        }
    }
    if format == Format::Sarif {
        if let Err(code) = print_sarif(&sarif_files) {
            return code;
        }
    } else if format == Format::Human {
        println!(
            "osf scan: {} error(s), {} warning(s), {} info, {} suppressed, {} excluded",
            tally.errors, tally.warnings, tally.infos, tally.suppressed, tally.excluded
        );
    }
    if tally.errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `--checkpoint` when given, else `--stage` mapped onto it. One of the two
/// is required.
fn resolve_checkpoint(args: &VerifyArgs) -> Result<checkpoint::Checkpoint, ExitCode> {
    if let Some(c) = args.checkpoint {
        return Ok(c);
    }
    if let Some(stage) = args.stage {
        return Ok(stage.into());
    }
    eprintln!("osf verify: needs --checkpoint or --stage");
    Err(ExitCode::from(2))
}

/// The hook checkpoint's file list, one repository-relative path per line
/// on standard input, normalised to forward slashes.
fn read_hook_files() -> Result<Vec<String>, ExitCode> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).map_err(|e| {
        eprintln!("osf verify: cannot read standard input: {e}");
        ExitCode::from(2)
    })?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.replace('\\', "/"))
        .collect())
}

fn verify_cmd(args: &VerifyArgs) -> ExitCode {
    match checkpoint::detect_adoption(Path::new(".")) {
        checkpoint::Adoption::Adopted(_) => {}
        checkpoint::Adoption::NotAdopted(path, reason) => {
            println!("osf verify: not adopted at {}: {reason}", path.display());
            return ExitCode::SUCCESS;
        }
        checkpoint::Adoption::AdoptedButBroken(root, missing) => {
            eprintln!(
                "osf verify: osf.toml at {} asks for osf, but {missing} is missing",
                root.display()
            );
            return ExitCode::from(2);
        }
        checkpoint::Adoption::CouldNotRun(reason) => {
            eprintln!("osf verify: {reason}");
            return ExitCode::from(2);
        }
    }
    let checkpoint = match resolve_checkpoint(args) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let files = if checkpoint == checkpoint::Checkpoint::Hook && args.files_from_stdin {
        match read_hook_files() {
            Ok(f) => Some(f),
            Err(code) => return code,
        }
    } else {
        None
    };
    let state_dir = match journal::state_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let req = checkpoint::Request {
        root: Path::new("."),
        checkpoint,
        base: args.base.clone(),
        files,
        timeout: args.timeout_secs.map(std::time::Duration::from_secs),
        remote: args.hook_args.first().cloned(),
    };
    let summary = checkpoint::run(&req, &state_dir);
    for line in &summary.lines {
        println!("{line}");
    }
    for finding in &summary.error_findings {
        eprintln!("osf verify: {finding}");
    }
    if let Some(err) = &summary.journal_error {
        eprintln!("osf verify: {err}");
    }
    ExitCode::from(checkpoint::exit_code(&summary, checkpoint))
}

/// `osf review run`'s own exit code, ignoring `--warn-only`: 0 pass, 1 fail,
/// 2 could-not-run or misconfigured. Kept separate from [`review_run_cmd`]
/// so `--warn-only` has one place to override the result, rather than a
/// forced zero threaded through every early return below.
fn review_run_exit_code(args: &ReviewRunArgs) -> u8 {
    let root = Path::new(".");
    let config_root = args
        .config_root
        .clone()
        .unwrap_or_else(|| root.to_path_buf());
    if args.if_enabled {
        let roster = match reviewers::roster(&config_root) {
            Ok(roster) => roster,
            Err(e) => {
                eprintln!("osf review run: {e}");
                return 2;
            }
        };
        let selected = match &args.reviewer {
            Some(name) => roster.iter().any(|r| &r.name == name),
            None => !roster.is_empty(),
        };
        if !selected {
            return review_slot_off(args);
        }
    }
    let Some(base) = args.base.clone().or_else(|| std::env::var("OSF_BASE").ok()) else {
        eprintln!("osf review run: a base is required: pass --base or set OSF_BASE");
        return 2;
    };
    let pull_request = match args.pull_request.as_deref().map(load_pull_request) {
        Some(Ok(pr)) => Some(pr),
        Some(Err(e)) => {
            eprintln!("osf review run: {e}");
            return 2;
        }
        None => None,
    };
    let binding = if args.reviewer.is_some() {
        match args
            .binding
            .binding(root, &base, pull_request.as_ref().map(|pr| pr.number))
        {
            Ok(binding) => Some(binding),
            Err(e) => {
                eprintln!("osf review run: {e}");
                return 2;
            }
        }
    } else {
        None
    };
    let req = review_run::Request {
        root,
        config_root: &config_root,
        base: &base,
        work_item: args.work_item.as_deref(),
        pull_request: pull_request.as_ref(),
        builder_family_overrides: &args.builder_family,
        binding: binding.as_ref(),
    };
    if let (Some(name), Some(out)) = (&args.reviewer, &args.out) {
        return match review_run::run_reviewer(&req, name).and_then(|run| run.save(out)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("osf review run: {e}");
                2
            }
        };
    }
    let state_dir = match journal::state_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("osf: {e}");
            return 2;
        }
    };
    match review_run::run(&req, &state_dir) {
        Ok(outcome) => finish_review(&outcome, args.sarif_out.as_deref(), args.post_to.as_deref()),
        Err(e) => {
            eprintln!("osf review run: {e}");
            2
        }
    }
}

/// The `--if-enabled` exit when nothing is selected: one line, an empty
/// SARIF file or an empty reviewer file when asked for, and exit 0.
fn review_slot_off(args: &ReviewRunArgs) -> u8 {
    if let (Some(name), Some(out)) = (&args.reviewer, &args.out) {
        println!("review: slot off, reviewer {name} is not selected");
        let empty = review_run::ReviewerRun {
            reviewer: name.clone(),
            lenses: Vec::new(),
            binding: None,
        };
        return match empty.save(out) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("osf review run: {e}");
                2
            }
        };
    }
    println!("review: slot off, no reviewer enabled");
    if let Some(path) = &args.sarif_out {
        if write_sarif_out(path, &[]).is_err() {
            return 2;
        }
    }
    0
}

/// The pull request's number, title and body, from the JSON file at `path`.
fn load_pull_request(path: &Path) -> Result<osf::review_context::PullRequest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints `outcome`, writes SARIF and posts as asked, and returns the exit
/// code: 0 pass, 1 fail, 2 could-not-run. A journal that could not record
/// what happened, or a post that never reached the pull request, leaves no
/// evidence behind the verdict: the run counts as could-not-run rather than
/// reporting a verdict nothing backs up.
fn finish_review(
    outcome: &review_run::RunOutcome,
    sarif_out: Option<&Path>,
    post_to: Option<&str>,
) -> u8 {
    for line in &outcome.lines {
        println!("{line}");
    }
    for note in &outcome.notes {
        println!("note: {note}");
    }
    if let Some(err) = &outcome.journal_error {
        eprintln!("osf review run: {err}");
    }
    if let Some(path) = sarif_out {
        let sarif_files = review_sarif_files(&outcome.findings);
        if write_sarif_out(path, &sarif_files).is_err() {
            return 2;
        }
    }
    let posted = post_to.is_none_or(|post_to| post_run_outcome(outcome, post_to));
    if outcome.journal_error.is_some() || !posted {
        return 2;
    }
    match outcome.verdict {
        reducer::Verdict::Pass => 0,
        reducer::Verdict::Fail => 1,
        reducer::Verdict::CouldNotRun => 2,
    }
}

/// `osf review reduce`'s own exit code, ignoring `--warn-only`; the same
/// codes as [`review_run_exit_code`].
fn review_reduce_exit_code(args: &ReviewReduceArgs) -> u8 {
    let root = Path::new(".");
    let config_root = args
        .config_root
        .clone()
        .unwrap_or_else(|| root.to_path_buf());
    let Some(base) = args.base.clone().or_else(|| std::env::var("OSF_BASE").ok()) else {
        eprintln!("osf review reduce: a base is required: pass --base or set OSF_BASE");
        return 2;
    };
    let mut runs = Vec::with_capacity(args.files.len());
    for file in &args.files {
        let name = file
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        match review_run::ReviewerRun::load(file) {
            Ok(run) => runs.push(review_run::SavedRun { name, run }),
            Err(e) => {
                eprintln!("osf review reduce: {e}");
                return 2;
            }
        }
    }
    let state_dir = match journal::state_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("osf: {e}");
            return 2;
        }
    };
    let binding = match args.binding.binding(root, &base, None) {
        Ok(binding) => binding,
        Err(e) => {
            eprintln!("osf review reduce: {e}");
            return 2;
        }
    };
    let req = review_run::Request {
        root,
        config_root: &config_root,
        base: &base,
        work_item: None,
        pull_request: None,
        builder_family_overrides: &args.builder_family,
        binding: Some(&binding),
    };
    match review_run::reduce_saved(&req, &runs, &state_dir) {
        Ok(outcome) => finish_review(&outcome, args.sarif_out.as_deref(), args.post_to.as_deref()),
        Err(e) => {
            eprintln!("osf review reduce: {e}");
            2
        }
    }
}

/// `osf review work-item`: saves the pull request's work item, or the reason
/// it has none. Exits 2 only when the inputs are bad or the code host fails.
fn review_work_item_cmd(args: &ReviewWorkItemArgs) -> ExitCode {
    let Some(repository) = args
        .repository
        .clone()
        .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
    else {
        eprintln!("osf review work-item: a repository is required: pass --repository or set GITHUB_REPOSITORY");
        return ExitCode::from(2);
    };
    let pull_request = match load_pull_request(&args.pull_request) {
        Ok(pr) => pr,
        Err(e) => {
            eprintln!("osf review work-item: {e}");
            return ExitCode::from(2);
        }
    };
    let item = match osf::work_item::find(
        &osf::work_item::GhIssues,
        &repository,
        pull_request.body.as_deref().unwrap_or_default(),
        &args.head,
    ) {
        Ok(item) => item,
        Err(e) => {
            eprintln!("osf review work-item: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::write(&args.out, item.to_json()) {
        eprintln!("osf review work-item: {}: {e}", args.out.display());
        return ExitCode::from(2);
    }
    match &item {
        osf::work_item::WorkItem::Found { reference, .. } => {
            println!("work item: {reference}");
        }
        osf::work_item::WorkItem::Missing(reason) => println!("work item: none, {reason}"),
    }
    ExitCode::SUCCESS
}

fn review_reduce_cmd(args: &ReviewReduceArgs) -> ExitCode {
    let code = review_reduce_exit_code(args);
    ExitCode::from(if args.warn_only { 0 } else { code })
}

fn review_run_cmd(args: &ReviewRunArgs) -> ExitCode {
    let code = review_run_exit_code(args);
    ExitCode::from(if args.warn_only { 0 } else { code })
}

/// Splits `--post-to`'s `owner/repo#N` into the repository and the pull
/// request number.
///
/// # Errors
/// Names the reason when there is no `#`, the number after it does not
/// parse, or the repository half is empty.
fn parse_post_to(raw: &str) -> Result<(String, u64), String> {
    let (repo, number) = raw
        .rsplit_once('#')
        .ok_or_else(|| format!("{raw:?} is not owner/repo#N"))?;
    if repo.is_empty() {
        return Err(format!("{raw:?} names no repository before '#'"));
    }
    let pr = number
        .parse::<u64>()
        .map_err(|e| format!("{raw:?}: the pull request number: {e}"))?;
    Ok((repo.to_string(), pr))
}

/// `finding`'s severity, as `osf review post` and the review answer schema
/// both spell it.
fn severity_str(severity: answer::Severity) -> &'static str {
    match severity {
        answer::Severity::Blocker => "blocker",
        answer::Severity::Major => "major",
        answer::Severity::Minor => "minor",
    }
}

/// `finding`'s action, as `osf review post`'s own block list spells it.
fn action_str(action: answer::Action) -> &'static str {
    match action {
        answer::Action::MustFix => "must-fix",
        answer::Action::ShouldFix => "should-fix",
        answer::Action::MaybeFix => "maybe-fix",
        answer::Action::Justify => "justify",
        answer::Action::Defer => "defer",
        answer::Action::Dismiss => "dismiss",
    }
}

/// A run's kept findings, as `review::Finding`s `osf review post`'s own
/// planning already knows how to turn into inline comments. Each finding's
/// id is its lens name and its place within that lens, one-based, since a
/// review run names no id of its own.
fn review_findings_for_post(findings: &[review_run::KeptFinding]) -> Vec<review::Finding> {
    let mut per_lens: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    findings
        .iter()
        .map(|kept| {
            let n = per_lens.entry(kept.lens.as_str()).or_insert(0);
            *n += 1;
            review::Finding {
                id: format!("{}-{n}", kept.lens),
                path: kept.finding.path.clone(),
                line: Some(kept.finding.line),
                severity: severity_str(kept.finding.severity).to_string(),
                action: action_str(kept.finding.action).to_string(),
                body: kept.finding.body.clone(),
            }
        })
        .collect()
}

/// The review's summary body for a posted review: one line per lens, then
/// the run's own verdict line, exactly as printed to standard output.
fn review_run_summary(outcome: &review_run::RunOutcome) -> String {
    let mut summary = format!("osf review run\n\n{}", outcome.lines.join("\n"));
    for note in &outcome.notes {
        let _ = write!(summary, "\nnote: {note}");
    }
    summary
}

/// `--post-to`'s whole job: build the findings and the summary from
/// `outcome`, then post through the forge. Returns whether the post landed:
/// a caller that cannot post has no review evidence on the pull request,
/// whatever the run's own verdict was.
fn post_run_outcome(outcome: &review_run::RunOutcome, post_to: &str) -> bool {
    let (repo, pr) = match parse_post_to(post_to) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("osf review run --post-to: {e}");
            return false;
        }
    };
    let findings = review_findings_for_post(&outcome.findings);
    let summary = review_run_summary(outcome);
    let block_on = resolve_block_on(None);
    let plan = match review::plan_review(&findings, &summary, &block_on) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("osf review run --post-to: {e}");
            return false;
        }
    };
    let github = github_adapter();
    let head_sha = match github.head_commit(&repo, pr) {
        Ok(sha) => sha,
        Err(e) => {
            eprintln!("osf review run --post-to: {e}");
            return false;
        }
    };
    let pull_request = forge::PullRequestId {
        repo: repo.clone(),
        pr: pr.to_string(),
    };
    match post_plan(&github, &pull_request, &plan, &head_sha) {
        Ok(forge::PostedReview {
            verdict,
            advisory: false,
            inline,
            id,
            state,
            url,
        }) => {
            println!("posted review {id}: {state}, {url}");
            println!(
                "osf review run --post-to: {} with {inline} inline comment(s) on {repo}#{pr}",
                verdict.as_event()
            );
            true
        }
        Ok(forge::PostedReview {
            verdict,
            advisory: true,
            inline,
            id,
            state,
            url,
        }) => {
            println!("posted comment {id}: {state}, {url}");
            println!(
                "osf review run --post-to: COMMENT (advisory {}) with {inline} inline \
                 comment(s) on {repo}#{pr}",
                verdict.as_event()
            );
            true
        }
        Err(e) => {
            eprintln!("osf review run --post-to: {e}");
            false
        }
    }
}

/// A review run's kept findings, grouped by file, as SARIF-ready findings.
/// A lens name is a fixed, bounded vocabulary, exactly the case
/// `osf_lint_core::intern` exists for, so it stands in as the rule id.
fn review_sarif_files(findings: &[review_run::KeptFinding]) -> Vec<(String, Vec<lints::Finding>)> {
    let mut by_path: Vec<(String, Vec<lints::Finding>)> = Vec::new();
    for kept in findings {
        let rule = osf_lint_core::intern(&kept.lens);
        let level = match kept.finding.severity {
            answer::Severity::Blocker => lints::Level::Error,
            answer::Severity::Major => lints::Level::Warning,
            answer::Severity::Minor => lints::Level::Info,
        };
        let line = usize::try_from(kept.finding.line).unwrap_or(usize::MAX);
        let finding = lints::Finding {
            rule,
            level,
            line,
            message: kept.finding.body.clone(),
            excerpt: kept.finding.quote.clone(),
            source: rule,
            evidence: lints::Evidence::Statistical,
            remediation: lints::Remediation::default(),
            suppressed: None,
        };
        match by_path
            .iter_mut()
            .find(|(path, _)| *path == kept.finding.path)
        {
            Some((_, list)) => list.push(finding),
            None => by_path.push((kept.finding.path.clone(), vec![finding])),
        }
    }
    by_path
}

fn changeset_risk_cmd(args: &RiskArgs) -> ExitCode {
    let format = args.format.unwrap_or(Format::Human);
    if format == Format::Sarif {
        eprintln!("osf changeset risk: format 'sarif' is not supported; use human or json");
        return ExitCode::from(2);
    }
    let dir = Path::new(".");
    let report = match changeset_risk::assess(dir, dir, &args.base) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("osf changeset risk: {e}");
            return ExitCode::from(2);
        }
    };
    match format {
        Format::Human => println!("{}", report.render_human()),
        Format::Json => println!("{}", report.to_json()),
        Format::Sarif => unreachable!("rejected above"),
    }
    ExitCode::SUCCESS
}

fn read_to_string_or_exit(path: &Path) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("osf: cannot read {}: {e}", path.display());
        ExitCode::from(2)
    })
}

/// The only place that builds the GitHub adapter.
fn github_adapter() -> github::GitHub<github::RealGh> {
    github::GitHub::real()
}

fn pr_status_render_cmd(args: &StatusRenderArgs) -> ExitCode {
    let tier_text = match read_to_string_or_exit(&args.tier_json) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let review_text = if let Some(path) = &args.review_json {
        match read_to_string_or_exit(path) {
            Ok(t) => t,
            Err(code) => return code,
        }
    } else {
        let (Some(repo), Some(pr)) = (&args.repo, &args.pr) else {
            eprintln!("osf pr status render: needs --repo and --pr, or --review-json");
            return ExitCode::from(2);
        };
        match github_adapter().client().view_review(repo, pr) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        }
    };
    let tests_text = match &args.base {
        Some(base) => match changeset_tests::summarize(Path::new("."), base, "HEAD") {
            Ok(summary) => Some(changeset_tests::render(&summary)),
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        },
        None => None,
    };
    let head = match &args.head {
        Some(h) => h.clone(),
        None => match git::head_sha(Path::new(".")) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let input = pr_status::RenderInput {
        tier_json: &tier_text,
        gates: &args.gates,
        review_json: &review_text,
        head: &head,
        tests: tests_text.as_deref(),
        automated_review: args.automated_review.as_deref(),
        human_review: args.human_review.as_deref(),
    };
    match pr_status::render(&input) {
        Ok(block) => {
            print!("{block}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

/// The block list `osf review post` uses: `--block-on`, else
/// `REVIEW_BLOCK_ON`, else the compiled default.
fn resolve_block_on(flag: Option<&str>) -> Vec<String> {
    if let Some(raw) = flag {
        return review::parse_block_on(raw);
    }
    if let Ok(raw) = std::env::var("REVIEW_BLOCK_ON") {
        return review::parse_block_on(&raw);
    }
    review::parse_block_on("must-fix,should-fix")
}

/// The forge verdict matching a review plan's verdict.
fn to_forge_verdict(verdict: review::Verdict) -> forge::Verdict {
    match verdict {
        review::Verdict::Approve => forge::Verdict::Approve,
        review::Verdict::RequestChanges => forge::Verdict::RequestChanges,
    }
}

/// The forge inline comment matching a review plan's comment.
fn to_forge_comment(comment: &review::Comment) -> forge::ReviewComment {
    forge::ReviewComment {
        path: comment.path.clone(),
        line: comment.line,
        body: comment.body.clone(),
    }
}

/// Posts `plan` through `forge`; the adapter prints the self-review warning.
fn post_plan(
    forge: &dyn forge::Forge,
    pull_request: &forge::PullRequestId,
    plan: &review::Plan,
    head_sha: &str,
) -> Result<forge::PostedReview, forge::ForgeError> {
    let request = forge::NewReview {
        pull_request: pull_request.clone(),
        head_sha: head_sha.to_string(),
        verdict: to_forge_verdict(plan.verdict),
        body: plan.body.clone(),
        comments: plan.comments.iter().map(to_forge_comment).collect(),
    };
    forge.post_review(&request)
}

/// Prints the dry-run preview of `plan`: the verdict it would send and the
/// payload GitHub would receive. Never touches the network.
fn print_dry_run(args: &ReviewPostArgs, plan: &review::Plan, head_sha: &str) -> ExitCode {
    println!(
        "osf review post: dry run — {} with {} inline comment(s) on {}#{} at {head_sha}",
        plan.verdict.as_event(),
        plan.comments.len(),
        args.repo,
        args.pr
    );
    let payload = review::payload(
        head_sha,
        &plan.body,
        plan.verdict.as_event(),
        &plan.comments,
    );
    match serde_json::to_string_pretty(&payload) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: cannot render the review as JSON: {e}");
            ExitCode::from(2)
        }
    }
}

fn changeset_tests_cmd(args: &ChangesetTestsArgs) -> ExitCode {
    match changeset_tests::summarize(Path::new("."), &args.base, &args.head) {
        Ok(summary) => {
            println!("{}", changeset_tests::render(&summary));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

fn pr_status_apply_cmd(args: &StatusApplyArgs) -> ExitCode {
    let block_text = match read_to_string_or_exit(&args.block) {
        Ok(t) => t,
        Err(code) => return code,
    };

    if args.body_file.is_some() || args.out.is_some() {
        let (Some(body_path), Some(out_path)) = (&args.body_file, &args.out) else {
            eprintln!("osf pr status apply: --body-file needs --out");
            return ExitCode::from(2);
        };
        let body = match std::fs::read_to_string(body_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "osf: cannot read {}: {e}; nothing was changed",
                    body_path.display()
                );
                return ExitCode::from(2);
            }
        };
        return match pr_status::apply(&body, &block_text) {
            Ok(new_body) => match std::fs::write(out_path, &new_body) {
                Ok(()) => {
                    println!("osf pr status apply: wrote {}", out_path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("osf: cannot write {}: {e}", out_path.display());
                    ExitCode::from(2)
                }
            },
            Err(e) => {
                eprintln!("osf: {e}");
                ExitCode::from(2)
            }
        };
    }

    let (Some(repo), Some(pr)) = (&args.repo, &args.pr) else {
        eprintln!("osf pr status apply: needs --repo and --pr, or --body-file and --out");
        return ExitCode::from(2);
    };
    if args.dry_run {
        let github = github_adapter();
        let body = match github.client().view_body(repo, pr) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        };
        let new_body = match pr_status::apply(&body, &block_text) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        };
        print!("{new_body}");
        return ExitCode::SUCCESS;
    }
    let pull_request = forge::PullRequestId {
        repo: repo.clone(),
        pr: pr.clone(),
    };
    let github = github_adapter();
    let forge: &dyn forge::Forge = &github;
    match forge.write_status_block(&pull_request, &block_text) {
        Ok(()) => {
            println!("osf pr status apply: applied to {repo}#{pr}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

/// Fetches one ref from `remote` so a fresh, shallow-on-history checkout
/// has it to diff against.
///
/// # Errors
/// Returns an error when git cannot run or exits non-zero.
fn git_fetch(remote: &str, ref_name: &str) -> Result<(), String> {
    let mut command = std::process::Command::new("git");
    command.args(["fetch", remote, ref_name]);
    osf::scrub_git_env_for_dir(&mut command, Path::new("."));
    let output = command
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git fetch {remote} {ref_name} failed: {}",
            stderr.trim()
        ));
    }
    Ok(())
}

/// The ref to assess risk against: `base` verbatim when given, else
/// `origin/<base_ref>` after fetching it so a fresh checkout has it.
fn pr_status_refresh_base(base: Option<&String>, base_ref: &str) -> Result<String, ExitCode> {
    if let Some(b) = base {
        return Ok(b.clone());
    }
    git_fetch("origin", base_ref).map_err(|e| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    })?;
    Ok(format!("origin/{base_ref}"))
}

/// The gate spec for `render` from a forge check-status read.
fn gates_from_forge_checks(status: &forge::CheckStatus) -> String {
    let checks: Vec<(String, String, String)> = status
        .checks
        .iter()
        .map(|c| (c.name.clone(), c.state.clone(), c.bucket.clone()))
        .collect();
    pr_status::gates_from_checks(&checks, STATUS_CHECK_NAME)
}

/// The gate spec for `render` from `checks_json` or the forge's check-status read.
fn pr_status_refresh_gates(
    forge: &dyn forge::Forge,
    pull_request: &forge::PullRequestId,
    checks_json: Option<&PathBuf>,
) -> Result<String, ExitCode> {
    if let Some(path) = checks_json {
        let checks_text = read_to_string_or_exit(path)?;
        return pr_status::gates_from_checks_json(&checks_text, STATUS_CHECK_NAME).map_err(|e| {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        });
    }
    match forge.read_check_status(pull_request) {
        forge::ReadOutcome::Found(status) => Ok(gates_from_forge_checks(&status)),
        forge::ReadOutcome::Empty => Ok(String::new()),
        forge::ReadOutcome::Unknown(reason) => {
            eprintln!("osf: {reason}");
            Err(ExitCode::from(2))
        }
    }
}

/// The review JSON for `render`: from `review_json` when given, else from `gh pr view`.
fn pr_status_refresh_review(
    client: &dyn pr_status::GhClient,
    repo: &str,
    pr: &str,
    review_json: Option<&PathBuf>,
) -> Result<String, ExitCode> {
    if let Some(path) = review_json {
        read_to_string_or_exit(path)
    } else {
        client.view_review(repo, pr).map_err(|e| {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        })
    }
}

fn pr_status_refresh_cmd(args: &StatusRefreshArgs) -> ExitCode {
    let github = github_adapter();
    pr_status_refresh_run(github.client(), &github, args).unwrap_or_else(|code| code)
}

fn pr_status_refresh_run(
    client: &dyn pr_status::GhClient,
    forge: &dyn forge::Forge,
    args: &StatusRefreshArgs,
) -> Result<ExitCode, ExitCode> {
    let to_exit = |e: pr_status::StatusError| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    };

    let pr_info_text = client.view_pr_info(&args.repo, &args.pr).map_err(to_exit)?;
    let pr_info = pr_status::parse_pr_info(&pr_info_text).map_err(to_exit)?;

    let head = git::head_sha(Path::new(".")).map_err(|e| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    })?;
    let base = pr_status_refresh_base(args.base.as_ref(), &pr_info.base_ref)?;
    let report = changeset_risk::assess(Path::new("."), Path::new("."), &base).map_err(|e| {
        eprintln!("osf changeset risk: {e}");
        ExitCode::from(2)
    })?;
    let tier_text = report.to_json().to_string();
    let pull_request = forge::PullRequestId {
        repo: args.repo.clone(),
        pr: args.pr.clone(),
    };
    let gates = pr_status_refresh_gates(forge, &pull_request, args.checks_json.as_ref())?;
    let review_text =
        pr_status_refresh_review(client, &args.repo, &args.pr, args.review_json.as_ref())?;
    let tests_summary = changeset_tests::summarize(Path::new("."), &base, "HEAD").map_err(|e| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    })?;
    let tests_text = changeset_tests::render(&tests_summary);

    let input = pr_status::RenderInput {
        tier_json: &tier_text,
        gates: &gates,
        review_json: &review_text,
        head: &head,
        tests: Some(&tests_text),
        automated_review: None,
        human_review: None,
    };
    let block = pr_status::render(&input).map_err(to_exit)?;
    let unchanged = pr_status::is_unchanged(&pr_info.body, &block).map_err(to_exit)?;

    if args.dry_run {
        print!("{block}");
        println!(
            "osf pr status refresh: {}",
            if unchanged {
                "unchanged"
            } else {
                "would update"
            }
        );
        return Ok(ExitCode::SUCCESS);
    }
    if unchanged {
        println!("osf pr status refresh: unchanged");
        return Ok(ExitCode::SUCCESS);
    }

    forge
        .write_status_block(&pull_request, &block)
        .map_err(|e| {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        })?;
    println!("osf pr status refresh: updated");
    Ok(ExitCode::SUCCESS)
}

/// Reports a forge review post's result as output and an exit code.
fn report_outcome(
    outcome: Result<forge::PostedReview, forge::ForgeError>,
    args: &ReviewPostArgs,
    head_sha: &str,
) -> ExitCode {
    match outcome {
        Ok(forge::PostedReview {
            verdict,
            advisory: false,
            inline,
            id,
            state,
            url,
        }) => {
            println!("posted review {id}: {state}, {url}");
            println!(
                "osf review post: {} with {inline} inline comment(s) on {}#{} at {head_sha}",
                verdict.as_event(),
                args.repo,
                args.pr
            );
            ExitCode::SUCCESS
        }
        Ok(forge::PostedReview {
            verdict,
            advisory: true,
            inline,
            id,
            state,
            url,
        }) => {
            println!("posted comment {id}: {state}, {url}");
            println!(
                "osf review post: COMMENT (advisory {}) with {inline} inline comment(s) on \
                 {}#{} at {head_sha}",
                verdict.as_event(),
                args.repo,
                args.pr
            );
            ExitCode::SUCCESS
        }
        Err(forge::ForgeError::Rejected(e)) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
        Err(forge::ForgeError::Failed(e)) => {
            eprintln!("osf: {e}");
            ExitCode::from(1)
        }
    }
}

fn review_post_cmd(args: &ReviewPostArgs) -> ExitCode {
    let findings = match review::load_findings(&args.findings) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let summary = match std::fs::read_to_string(&args.summary) {
        // A summary written on Windows arrives with carriage returns.
        // They would travel into the posted body and show up as stray
        // characters, so the text is normalised on the way in. The
        // trailing newline goes too: the body is joined to other text,
        // and a blank line there is an accident, not a choice.
        Ok(s) => s.replace("\r\n", "\n").trim_end().to_string(),
        Err(e) => {
            eprintln!("osf: cannot read {}: {e}", args.summary.display());
            return ExitCode::from(2);
        }
    };
    let block_on = resolve_block_on(args.block_on.as_deref());
    let plan = match review::plan_review(&findings, &summary, &block_on) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let github = github_adapter();
    let head_sha = match github.head_commit(&args.repo, args.pr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };

    if args.dry_run {
        return print_dry_run(args, &plan, &head_sha);
    }
    let pull_request = forge::PullRequestId {
        repo: args.repo.clone(),
        pr: args.pr.to_string(),
    };
    let outcome = post_plan(&github, &pull_request, &plan, &head_sha);
    report_outcome(outcome, args, &head_sha)
}

fn pr_tree_render_cmd(args: &TreeRenderArgs) -> ExitCode {
    match pr_tree::collect(Path::new("."), &args.base, &args.head) {
        Ok(changes) if changes.is_empty() => {
            eprintln!(
                "osf: no files differ between {} and {}",
                args.base, args.head
            );
            ExitCode::from(2)
        }
        Ok(changes) => {
            print!("{}", pr_tree::render(&changes));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

fn pr_section_write_cmd(args: &SectionWriteArgs) -> ExitCode {
    let content = match std::fs::read_to_string(&args.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("osf: cannot read {}: {e}", args.file.display());
            return ExitCode::from(2);
        }
    };
    let repo = args.repo.as_deref();
    let body = match section::fetch_body(repo, args.pr) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let head = match args.head.as_deref().filter(|h| !h.is_empty()) {
        Some(h) => h.to_string(),
        None => match section::fetch_head_sha(repo, args.pr) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let new_body = match section::apply(&body, &args.name, &head, &content) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    match section::write_body(repo, args.pr, &new_body) {
        Ok(()) => {
            println!(
                "osf pr section write: wrote '{}' on pull request #{}",
                args.name, args.pr
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
    }
}

/// The remote to clone from and push to, and the `owner/name` label for the
/// printed raw content address: `--repo` and `--remote` verbatim when
/// given, else both read from the current directory's `origin` remote.
fn resolve_assets_target(
    repo: Option<&str>,
    remote: Option<&str>,
) -> Result<(String, String), String> {
    if let (Some(repo), Some(remote)) = (repo, remote) {
        return Ok((repo.to_string(), remote.to_string()));
    }
    let origin = git::remote(Path::new("."))
        .map_err(|e| format!("cannot read the origin remote to fill in --repo or --remote: {e}"))?;
    let resolved_repo = repo.map_or_else(
        || format!("{}/{}", origin.owner, origin.name),
        str::to_string,
    );
    let resolved_remote = remote.map_or_else(
        || {
            format!(
                "https://{}/{}/{}.git",
                origin.host, origin.owner, origin.name
            )
        },
        str::to_string,
    );
    Ok((resolved_repo, resolved_remote))
}

fn assets_publish_cmd(args: &AssetsPublishArgs) -> ExitCode {
    let (repo, remote) = match resolve_assets_target(args.repo.as_deref(), args.remote.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok());
    let publish_args = assets::PublishArgs {
        remote: &remote,
        repo: &repo,
        branch: &args.branch,
        path: &args.path,
        dir: &args.dir,
        token: token.as_deref(),
        max_attempts: args.max_attempts,
    };
    match assets::publish(&publish_args) {
        Ok(url) => {
            println!("{url}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_PATH: &str = "crates/osf/tests/fixtures/bad.md";
    const REAL_PATH: &str = "docs/real.md";
    const TWO_RULE_TEXT: &str =
        "A thing — another thing. It ran fine; it passed the whole suite.\n";

    fn writing_args() -> WritingArgs {
        WritingArgs {
            paths: Vec::new(),
            format: None,
            json: false,
            strict: false,
            known_names: None,
            message: false,
            context: None,
            no_suppress: false,
            max_sentence_words: 25,
            warn_sentence_words: None,
            max_numerals: 2,
            short_text_words: 500,
            exclude: Vec::new(),
            no_exclude: false,
            gate: false,
        }
    }

    fn known() -> lints::KnownNames {
        lints::load_known_names(&[], None).expect("built-in names load")
    }

    fn lint_it(name: &str, text: &str) -> (Vec<lints::Finding>, Tally) {
        let cfg = config::WritingConfig::default();
        let args = writing_args();
        let mut tally = Tally::default();
        let findings = lint_one(name, text, &args, &known(), &cfg, Format::Sarif, &mut tally);
        (findings, tally)
    }

    #[test]
    fn a_fixture_matching_its_declaration_produces_no_findings() {
        let text = format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\nsemicolon\n-->\n");
        let (findings, tally) = lint_it(FIXTURE_PATH, &text);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(tally.declared, 1);
        assert_eq!(tally.errors, 0);
        assert_eq!(tally.warnings, 0);
    }

    #[test]
    fn a_declaration_naming_a_scan_rule_is_refused() {
        let text =
            format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\nsemicolon\nscan-denied-name\n-->\n");
        let (findings, tally) = lint_it(FIXTURE_PATH, &text);
        assert_eq!(tally.declared, 1);
        assert_eq!(tally.errors, 1);
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.rule, "expectation-forbidden-scan-rule");
        assert_eq!(finding.excerpt, "scan-denied-name");
        assert!(finding.message.contains("cannot be declared expected"));
    }

    #[test]
    fn a_declaration_naming_only_ordinary_rules_still_works() {
        let text = format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\nsemicolon\n-->\n");
        let (findings, tally) = lint_it(FIXTURE_PATH, &text);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(tally.declared, 1);
    }

    #[test]
    fn a_fixture_missing_a_declared_rule_fails() {
        let text =
            format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\nsemicolon\nbare-reference\n-->\n");
        let (findings, tally) = lint_it(FIXTURE_PATH, &text);
        assert_eq!(tally.declared, 1);
        assert_eq!(tally.errors, 1);
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.rule, "expectation-missing");
        assert_eq!(finding.excerpt, "bare-reference");
    }

    #[test]
    fn a_fixture_with_an_undeclared_finding_fails() {
        let text = format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\n-->\n");
        let (findings, tally) = lint_it(FIXTURE_PATH, &text);
        assert_eq!(tally.declared, 1);
        assert_eq!(tally.errors, 1);
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.rule, "expectation-unexpected");
        assert_eq!(finding.excerpt, "semicolon");
    }

    #[test]
    fn a_file_with_no_declaration_behaves_as_before() {
        let (findings, tally) = lint_it(REAL_PATH, TWO_RULE_TEXT);
        assert_eq!(tally.declared, 0);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.rule == "em-dash"));
        assert!(findings.iter().any(|f| f.rule == "semicolon"));
    }

    #[test]
    fn a_declaration_outside_a_fixtures_path_is_inert() {
        let text = format!("{TWO_RULE_TEXT}<!-- osf-expect\nem-dash\n-->\n");
        let (findings, tally) = lint_it(REAL_PATH, &text);
        assert_eq!(tally.declared, 0, "not eligible, so not counted as checked");
        assert!(findings.iter().any(|f| f.rule == "em-dash"));
        assert!(findings.iter().any(|f| f.rule == "semicolon"));
        assert!(findings
            .iter()
            .any(|f| f.rule == "expectation-outside-fixtures"));
    }

    /// A skill fixture's `SKILL.md` carries an `osf-expect-skill` marker,
    /// never a plain `osf-expect` one. The writing lint, run over that same
    /// file directly the way `osf lint writing` does over every changed
    /// Markdown file, must not read that marker as its own: a skill rule
    /// id such as `skill-description-no-trigger` never fires as a writing
    /// rule, so treating it as a writing declaration would always report it
    /// missing. The two checkers must stay blind to each other's marker.
    #[test]
    fn a_skill_declaration_is_invisible_to_the_writing_lint() {
        let skill_text = "---\nname: no-trigger\ndescription: Checks a folder for common problems before a release.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing and note any file over ten megabytes.\n2. Check that a license file exists.\n3. Check that a readme file exists.\n4. Write one line per problem found, with the file path.\n\nStop when every check has run once, whether or not it found a problem.\n\n<!-- osf-expect-skill\nskill-description-no-trigger\n-->\n";
        assert!(lints::parse_expectation(skill_text).is_none());
        let (findings, tally) = lint_it(
            "crates/osf/tests/fixtures/skills/no-trigger/SKILL.md",
            skill_text,
        );
        assert_eq!(
            tally.declared, 0,
            "an osf-expect-skill marker is not a writing declaration"
        );
        assert!(
            findings.iter().all(|f| !f.rule.starts_with("expectation-")),
            "{findings:?}"
        );
    }
}
