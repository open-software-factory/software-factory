use osf::status::GhClient;
use osf::{
    answer, check, checkpoint, config, exclude, githooks, hook, journal, lints, reducer, review,
    review_run, risk, scan, status, verify,
};

use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
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
    /// Report the blast radius of a change: low, normal, or high, with the reasons.
    Risk(RiskArgs),
    /// Render or apply the status block at the top of a pull request description.
    Status {
        #[command(subcommand)]
        action: StatusAction,
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
}

#[derive(Subcommand)]
enum HooksAction {
    /// Write osf's own hook scripts and point this repository at them.
    Install(HooksInstallArgs),
}

#[derive(Args)]
struct HooksInstallArgs {
    /// Report whether this repository's hooks point at osf's own folder,
    /// instead of installing anything. Exits non-zero when they do not.
    #[arg(long)]
    check: bool,
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
    /// One sentence describing the problem.
    #[arg(long)]
    problem: String,
    /// One sentence describing the approach.
    #[arg(long)]
    approach: String,
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
    /// One sentence describing the problem. Required only when the
    /// description carries no status block yet.
    #[arg(long)]
    problem: Option<String>,
    /// One sentence describing the approach. Required only when the
    /// description carries no status block yet.
    #[arg(long)]
    approach: Option<String>,
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
    /// answer and the decision.
    Run(ReviewRunArgs),
}

#[derive(Args)]
struct ReviewRunArgs {
    /// What to diff the change against.
    #[arg(long)]
    base: String,
    /// A file holding the work item body, for a lens that needs one.
    #[arg(long = "work-item")]
    work_item: Option<PathBuf>,
    /// Write the kept findings as SARIF to this path.
    #[arg(long = "sarif-out")]
    sarif_out: Option<PathBuf>,
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
    /// Extra positional arguments a git hook passes (pre-push's remote name and URL), swallowed so a real hook call is never refused.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    #[allow(dead_code)]
    hook_args: Vec<String>,
}

#[derive(Subcommand)]
enum LintKind {
    /// Check prose for references, names, sentence length, and filler.
    Writing(WritingArgs),
    /// Check a skill folder's SKILL.md against eleven structural and safety rules.
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
    #[arg(long)]
    overview_max_paragraphs: Option<usize>,
    /// A first section that is not step-shaped may hold this many words. Overrides the config file.
    #[arg(long)]
    overview_max_words: Option<usize>,
    /// Extra names that need no description, one per line.
    #[arg(long)]
    known_names: Option<PathBuf>,
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
        Command::Risk(args) => risk_cmd(args),
        Command::Status {
            action: StatusAction::Render(args),
        } => status_render_cmd(args),
        Command::Status {
            action: StatusAction::Apply(args),
        } => status_apply_cmd(args),
        Command::Status {
            action: StatusAction::Refresh(args),
        } => status_refresh_cmd(args),
        Command::Review {
            action: ReviewAction::Post(args),
        } => review_post_cmd(args),
        Command::Review {
            action: ReviewAction::Run(args),
        } => review_run_cmd(args),
        Command::Hooks {
            action: HooksAction::Install(args),
        } => hooks_install_cmd(args),
    }
}

fn hooks_install_cmd(args: &HooksInstallArgs) -> ExitCode {
    let dir = Path::new(".");
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
    let loaded = match config::load(config_flag, &overlay, &[], false) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let cfg = &loaded.config.skill;
    let writing_cfg = &loaded.config.writing;
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
            let mut findings = osf_lint_core::apply_level_overrides(raw_findings, &cfg.levels);
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

fn review_run_cmd(args: &ReviewRunArgs) -> ExitCode {
    let state_dir = match journal::state_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let req = review_run::Request {
        root: Path::new("."),
        base: &args.base,
        work_item: args.work_item.as_deref(),
    };
    let outcome = match review_run::run(&req, &state_dir) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("osf review run: {e}");
            return ExitCode::from(2);
        }
    };
    for line in &outcome.lines {
        println!("{line}");
    }
    if let Some(err) = &outcome.journal_error {
        eprintln!("osf review run: {err}");
    }
    if let Some(path) = &args.sarif_out {
        let sarif_files = review_sarif_files(&outcome.findings);
        if let Err(code) = write_sarif_out(path, &sarif_files) {
            return code;
        }
    }
    ExitCode::from(match outcome.verdict {
        reducer::Verdict::Pass => 0,
        reducer::Verdict::Fail => 1,
        reducer::Verdict::CouldNotRun => 2,
    })
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

fn risk_cmd(args: &RiskArgs) -> ExitCode {
    let format = args.format.unwrap_or(Format::Human);
    if format == Format::Sarif {
        eprintln!("osf risk: format 'sarif' is not supported; use human or json");
        return ExitCode::from(2);
    }
    let dir = Path::new(".");
    let report = match risk::assess(dir, &args.base) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("osf risk: {e}");
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

fn status_render_cmd(args: &StatusRenderArgs) -> ExitCode {
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
            eprintln!("osf status render: needs --repo and --pr, or --review-json");
            return ExitCode::from(2);
        };
        match status::RealGh.view_review(repo, pr) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("osf: {e}");
                return ExitCode::from(2);
            }
        }
    };
    let input = status::RenderInput {
        tier_json: &tier_text,
        gates: &args.gates,
        problem: &args.problem,
        approach: &args.approach,
        review_json: &review_text,
    };
    match status::render(&input) {
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

/// Posts `plan` to `repo`#`pr`, retrying once as an advisory comment when
/// GitHub refuses the formal review. Thin: the decision at each step lives
/// in [`review::after_first_attempt`] and [`review::after_fallback_attempt`].
fn post_plan(repo: &str, pr: u64, plan: &review::Plan, head_sha: &str) -> review::Outcome {
    let payload = review::payload(
        head_sha,
        &plan.body,
        plan.verdict.as_event(),
        &plan.comments,
    );
    let attempt = review::post_via_gh(repo, pr, &payload);
    match review::after_first_attempt(attempt, plan, head_sha) {
        review::NextStep::Done(outcome) => outcome,
        review::NextStep::RetryAsComment(advisory_payload) => {
            eprintln!(
                "osf review post: GitHub refuses {} from the pull request's own author; posting \
                 as COMMENT with the verdict marked advisory. reviewDecision stays empty until \
                 the reviewer has its own identity.",
                plan.verdict.as_event()
            );
            let second_attempt = review::post_via_gh(repo, pr, &advisory_payload);
            review::after_fallback_attempt(second_attempt, plan)
        }
    }
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

fn status_apply_cmd(args: &StatusApplyArgs) -> ExitCode {
    let block_text = match read_to_string_or_exit(&args.block) {
        Ok(t) => t,
        Err(code) => return code,
    };

    if args.body_file.is_some() || args.out.is_some() {
        let (Some(body_path), Some(out_path)) = (&args.body_file, &args.out) else {
            eprintln!("osf status apply: --body-file needs --out");
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
        return match status::apply(&body, &block_text) {
            Ok(new_body) => match std::fs::write(out_path, &new_body) {
                Ok(()) => {
                    println!("osf status apply: wrote {}", out_path.display());
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
        eprintln!("osf status apply: needs --repo and --pr, or --body-file and --out");
        return ExitCode::from(2);
    };
    let client = status::RealGh;
    let body = match client.view_body(repo, pr) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let new_body = match status::apply(&body, &block_text) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    if args.dry_run {
        print!("{new_body}");
        return ExitCode::SUCCESS;
    }
    match client.edit_body(repo, pr, &new_body) {
        Ok(()) => {
            println!("osf status apply: applied to {repo}#{pr}");
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

/// `Problem` and `Approach` for a refresh: read back from the description's
/// own block when it has one, else from `--problem`/`--approach`. With
/// neither, there is nothing to refresh, and that is `None`, never an
/// error: a pull request opts into the block by applying it once.
fn status_refresh_problem_approach(
    body: &str,
    problem: Option<&String>,
    approach: Option<&String>,
) -> Result<Option<(String, String)>, ExitCode> {
    match status::extract_problem_approach(body) {
        Ok(Some((p, a))) => Ok(Some((p, a))),
        Ok(None) => match (problem, approach) {
            (Some(p), Some(a)) => Ok(Some((p.clone(), a.clone()))),
            _ => Ok(None),
        },
        Err(e) => {
            eprintln!("osf: {e}");
            Err(ExitCode::from(2))
        }
    }
}

/// The ref to assess risk against: `base` verbatim when given, else
/// `origin/<base_ref>` after fetching it so a fresh checkout has it.
fn status_refresh_base(base: Option<&String>, base_ref: &str) -> Result<String, ExitCode> {
    if let Some(b) = base {
        return Ok(b.clone());
    }
    git_fetch("origin", base_ref).map_err(|e| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    })?;
    Ok(format!("origin/{base_ref}"))
}

/// The gate spec for `render`: from `checks_json` when given, else from
/// `gh pr checks`, with the check running this refresh left out.
fn status_refresh_gates(
    client: &dyn status::GhClient,
    repo: &str,
    pr: &str,
    checks_json: Option<&PathBuf>,
) -> Result<String, ExitCode> {
    let checks_text = if let Some(path) = checks_json {
        read_to_string_or_exit(path)?
    } else {
        client.view_checks(repo, pr).map_err(|e| {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        })?
    };
    status::gates_from_checks_json(&checks_text, STATUS_CHECK_NAME).map_err(|e| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    })
}

/// The review JSON for `render`: from `review_json` when given, else from `gh pr view`.
fn status_refresh_review(
    client: &dyn status::GhClient,
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

fn status_refresh_cmd(args: &StatusRefreshArgs) -> ExitCode {
    status_refresh_run(&status::RealGh, args).unwrap_or_else(|code| code)
}

fn status_refresh_run(
    client: &dyn status::GhClient,
    args: &StatusRefreshArgs,
) -> Result<ExitCode, ExitCode> {
    let to_exit = |e: status::StatusError| {
        eprintln!("osf: {e}");
        ExitCode::from(2)
    };

    let pr_info_text = client.view_pr_info(&args.repo, &args.pr).map_err(to_exit)?;
    let pr_info = status::parse_pr_info(&pr_info_text).map_err(to_exit)?;

    let Some((problem, approach)) = status_refresh_problem_approach(
        &pr_info.body,
        args.problem.as_ref(),
        args.approach.as_ref(),
    )?
    else {
        println!(
            "osf status refresh: no status block in the description, so nothing to refresh; run `osf status apply` once to start one"
        );
        return Ok(ExitCode::SUCCESS);
    };
    let base = status_refresh_base(args.base.as_ref(), &pr_info.base_ref)?;
    let report = risk::assess(Path::new("."), &base).map_err(|e| {
        eprintln!("osf risk: {e}");
        ExitCode::from(2)
    })?;
    let tier_text = report.to_json().to_string();
    let gates = status_refresh_gates(client, &args.repo, &args.pr, args.checks_json.as_ref())?;
    let review_text =
        status_refresh_review(client, &args.repo, &args.pr, args.review_json.as_ref())?;

    let input = status::RenderInput {
        tier_json: &tier_text,
        gates: &gates,
        problem: &problem,
        approach: &approach,
        review_json: &review_text,
    };
    let block = status::render(&input).map_err(to_exit)?;
    let unchanged = status::is_unchanged(&pr_info.body, &block).map_err(to_exit)?;

    if args.dry_run {
        print!("{block}");
        println!(
            "osf status refresh: {}",
            if unchanged {
                "unchanged"
            } else {
                "would update"
            }
        );
        return Ok(ExitCode::SUCCESS);
    }
    if unchanged {
        println!("osf status refresh: unchanged");
        return Ok(ExitCode::SUCCESS);
    }

    let new_body = status::apply(&pr_info.body, &block).map_err(to_exit)?;
    client
        .edit_body(&args.repo, &args.pr, &new_body)
        .map_err(to_exit)?;
    println!("osf status refresh: updated");
    Ok(ExitCode::SUCCESS)
}

/// Reports one of [`review::Outcome`]'s four cases: a formal review
/// landed, it landed as a fallback comment, or nothing landed and why.
fn report_outcome(outcome: review::Outcome, args: &ReviewPostArgs, head_sha: &str) -> ExitCode {
    match outcome {
        review::Outcome::Reviewed {
            verdict,
            n_inline,
            id,
            state,
            url,
        } => {
            println!("posted review {id}: {state}, {url}");
            println!(
                "osf review post: {} with {n_inline} inline comment(s) on {}#{} at {head_sha}",
                verdict.as_event(),
                args.repo,
                args.pr
            );
            ExitCode::SUCCESS
        }
        review::Outcome::FallbackComment {
            verdict,
            n_inline,
            id,
            state,
            url,
        } => {
            println!("posted comment {id}: {state}, {url}");
            println!(
                "osf review post: COMMENT (advisory {}) with {n_inline} inline comment(s) on \
                 {}#{} at {head_sha}",
                verdict.as_event(),
                args.repo,
                args.pr
            );
            ExitCode::SUCCESS
        }
        review::Outcome::Rejected(e) => {
            eprintln!("osf: {e}");
            ExitCode::from(2)
        }
        review::Outcome::PostFailed(e) => {
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
    let head_sha = match review::fetch_head_sha(&args.repo, args.pr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };

    if args.dry_run {
        return print_dry_run(args, &plan, &head_sha);
    }
    let outcome = post_plan(&args.repo, args.pr, &plan, &head_sha);
    report_outcome(outcome, args, &head_sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_PATH: &str = "crates/osf/tests/fixtures/bad.md";
    const REAL_PATH: &str = "docs/real.md";
    const TWO_RULE_TEXT: &str =
        "A thing — another thing. It ran fine; it passed the whole suite.\n";

    #[test]
    fn a_refresh_with_no_block_and_no_flags_has_nothing_to_do() {
        let none = status_refresh_problem_approach("just a description\n", None, None);
        assert!(matches!(none, Ok(None)));
        let p = "the problem".to_string();
        let a = "the approach".to_string();
        let some = status_refresh_problem_approach("just a description\n", Some(&p), Some(&a));
        assert_eq!(some.ok().flatten(), Some((p, a)));
    }

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
