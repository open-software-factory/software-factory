use osf::{config, exclude, hook, lint, scan, verify};

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

/// Where the text lives, for the CLI: mirrors [`lint::Context`], since that
/// type lives in the config-agnostic core crate and cannot derive `clap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ContextArg {
    Transcript,
    Commit,
    Document,
    Skill,
}

impl From<ContextArg> for lint::Context {
    fn from(value: ContextArg) -> Self {
        match value {
            ContextArg::Transcript => lint::Context::Transcript,
            ContextArg::Commit => lint::Context::Commit,
            ContextArg::Document => lint::Context::Document,
            ContextArg::Skill => lint::Context::Skill,
        }
    }
}

/// `--context` wins when given. Otherwise `--message` means transcript,
/// and a file with neither flag is a document.
fn resolve_context(context: Option<ContextArg>, message: bool) -> lint::Context {
    match context {
        Some(c) => c.into(),
        None if message => lint::Context::Transcript,
        None => lint::Context::Document,
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
    /// Run every check one gate needs, in one entry point every gate calls.
    Verify(VerifyArgs),
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
    /// Runs as a gate over a change nobody has approved yet: the exclude
    /// list is the compiled defaults only, never the config file or the
    /// environment, so that change cannot loosen this check by editing its
    /// own configuration.
    #[arg(long)]
    gate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum StageArg {
    PreCommit,
    PrePush,
    Ci,
}

impl From<StageArg> for verify::Stage {
    fn from(value: StageArg) -> Self {
        match value {
            StageArg::PreCommit => verify::Stage::PreCommit,
            StageArg::PrePush => verify::Stage::PrePush,
            StageArg::Ci => verify::Stage::Ci,
        }
    }
}

impl StageArg {
    const fn label(self) -> &'static str {
        match self {
            StageArg::PreCommit => "pre-commit",
            StageArg::PrePush => "pre-push",
            StageArg::Ci => "ci",
        }
    }
}

#[derive(Args)]
struct VerifyArgs {
    /// Which gate is calling: pre-commit, pre-push, or continuous integration.
    #[arg(long, value_enum)]
    stage: StageArg,
    /// What to diff changed files against. Defaults to the repository's default branch.
    #[arg(long)]
    base: Option<String>,
    /// A file holding the commit message, for the pre-commit stage.
    #[arg(long)]
    message_file: Option<PathBuf>,
    /// How to print findings: human, sarif, or json.
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// A path pattern to skip, on top of the configured list. Repeatable.
    /// For a person running the tool by hand; a gate run does not accept it.
    #[arg(long = "exclude", conflicts_with = "gate")]
    exclude: Vec<String>,
    /// Ignore the exclude list entirely and check everything. For a person
    /// running the tool by hand; a gate run does not accept it.
    #[arg(long, conflicts_with = "gate")]
    no_exclude: bool,
    /// Runs as a gate over a change nobody has approved yet: the exclude
    /// list is the compiled defaults only, never the config file or the
    /// environment, so that change cannot loosen this check by editing its
    /// own configuration.
    #[arg(long)]
    gate: bool,
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
    /// Runs as a gate over a change nobody has approved yet: the exclude
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
        Command::Config {
            action: ConfigAction::Show,
        } => config_show(cli.config.as_deref()),
        Command::Explain { rule_id } => explain(rule_id),
        Command::Scan(args) => scan_cmd(args, cli.config.as_deref()),
        Command::Verify(args) => verify_cmd(args, cli.config.as_deref()),
    }
}

fn explain(rule_id: &str) -> ExitCode {
    let Some(meta) = lint::rule_meta(rule_id).or_else(|| scan::rule_meta(rule_id)) else {
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
    fn count(&mut self, findings: &[lint::Finding]) {
        for f in findings {
            if f.suppressed.is_some() {
                self.suppressed += 1;
            } else {
                match f.level {
                    lint::Level::Error => self.errors += 1,
                    lint::Level::Warning => self.warnings += 1,
                    lint::Level::Info => self.infos += 1,
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
fn expectation_findings(mismatch: &lint::Mismatch) -> Vec<lint::Finding> {
    let missing = mismatch.missing.iter().map(|id| {
        lint::Finding::new(
            "expectation-missing",
            lint::Level::Error,
            1,
            format!("declared rule '{id}' did not fire; it may have stopped working"),
            id.clone(),
        )
    });
    let unexpected = mismatch.unexpected.iter().map(|id| {
        lint::Finding::new(
            "expectation-unexpected",
            lint::Level::Error,
            1,
            format!("rule '{id}' fired but this file did not declare it"),
            id.clone(),
        )
    });
    missing.chain(unexpected).collect()
}

/// A warning that an `osf-expect` marker outside a `tests/fixtures` path
/// has no effect: the file is still linted normally, findings and all.
fn outside_fixtures_warning() -> lint::Finding {
    lint::Finding::new(
        "expectation-outside-fixtures",
        lint::Level::Warning,
        1,
        "an osf-expect marker only applies under a tests/fixtures path; ignoring it here"
            .to_string(),
        "osf-expect".to_string(),
    )
}

/// One error per declared id that names a scan rule: a scan finding must
/// never be silenced by anything inside the repository, a fixture's own
/// declaration included.
fn forbidden_scan_rule_findings(ids: &[&String]) -> Vec<lint::Finding> {
    ids.iter()
        .map(|id| {
            lint::Finding::new(
                "expectation-forbidden-scan-rule",
                lint::Level::Error,
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
    raw: &[lint::Finding],
    format: Format,
) -> Vec<lint::Finding> {
    let mismatch = lint::check_expectation(expected, raw);
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
    mut findings: Vec<lint::Finding>,
    args: &WritingArgs,
    format: Format,
    tally: &mut Tally,
) -> Vec<lint::Finding> {
    if args.strict {
        for f in &mut findings {
            if f.suppressed.is_none() && f.level == lint::Level::Warning {
                f.level = lint::Level::Error;
            }
        }
    }
    tally.count(&findings);
    let visible: Vec<lint::Finding> = findings
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
    known: &lint::KnownNames,
    cfg: &config::WritingConfig,
    format: Format,
    tally: &mut Tally,
) -> Vec<lint::Finding> {
    let context = resolve_context(args.context, args.message);
    let raw = lint::lint_writing(text, known, cfg, context, false, args.no_suppress);
    if let Some(expected) = lint::parse_expectation(text) {
        if lint::is_fixture_path(name) {
            tally.declared += 1;
            let forbidden: Vec<&String> = expected
                .iter()
                .filter(|id| lint::is_scan_rule(id))
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

fn print_sarif(sarif_files: &[(String, Vec<lint::Finding>)]) -> Result<(), ExitCode> {
    let tool = osf_lint_core::ToolInfo {
        name: "osf",
        version: env!("CARGO_PKG_VERSION"),
        information_uri: INFORMATION_URI,
    };
    let report = osf_lint_core::to_sarif(sarif_files, &tool);
    let text = serde_json::to_string_pretty(&report).map_err(|e| {
        eprintln!("osf: cannot render sarif: {e}");
        ExitCode::from(2)
    })?;
    println!("{text}");
    Ok(())
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
    let known = match lint::load_known_names(&cfg.known_names, args.known_names.as_deref()) {
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
    let mut sarif_files: Vec<(String, Vec<lint::Finding>)> = Vec::new();
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
    let known = match lint::load_known_names(&writing_cfg.known_names, args.known_names.as_deref())
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
    let mut sarif_files: Vec<(String, Vec<lint::Finding>)> = Vec::new();
    for dir in &args.paths {
        let label = dir.to_string_lossy().replace('\\', "/");
        let skill_findings =
            match lint::skill::lint_skill_checked(dir, &label, cfg, &known, writing_cfg) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("osf: {e}");
                    return ExitCode::from(2);
                }
            };
        let mut by_file: std::collections::BTreeMap<String, Vec<lint::Finding>> =
            std::collections::BTreeMap::new();
        for sf in skill_findings {
            by_file.entry(sf.file).or_default().push(sf.finding);
        }
        for (file, raw_findings) in by_file {
            let name = format!("{}/{file}", dir.display());
            let mut findings = osf_lint_core::apply_level_overrides(raw_findings, &cfg.levels);
            if args.strict {
                for f in &mut findings {
                    if f.suppressed.is_none() && f.level == lint::Level::Warning {
                        f.level = lint::Level::Error;
                    }
                }
            }
            tally.count(&findings);
            let visible: Vec<lint::Finding> = findings
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
        println!("{}", lint::agnix::checked_note());
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
        match scan::scan_paths(dir, &args.paths, &rules, &excluder) {
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
    let mut sarif_files: Vec<(String, Vec<lint::Finding>)> = Vec::new();
    for (name, raw_findings) in results {
        let findings =
            osf_lint_core::apply_level_overrides(raw_findings, &loaded.config.scan.levels);
        tally.count(&findings);
        let visible: Vec<lint::Finding> = findings
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

fn verify_cmd(args: &VerifyArgs, config_flag: Option<&std::path::Path>) -> ExitCode {
    let loaded = match config::load(config_flag, &[], &args.exclude, args.gate) {
        Ok(l) => l,
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
    let opts = verify::Options {
        dir: Path::new("."),
        base: args.base.clone(),
        message_file: args.message_file.as_deref(),
        config: &loaded.config,
        excluder: &excluder,
    };
    let report = match verify::run(args.stage.into(), &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };

    let format = resolve_format(args.format, false);
    match format {
        Format::Human => {
            for (check, name, f) in report.findings() {
                if f.suppressed.is_none() {
                    println!("{}", f.render(&format!("{check}: {name}"), f.level));
                }
            }
            print!("{}", report.render_summary(args.stage.label()));
        }
        Format::Json => {
            for (check, name, f) in report.findings() {
                if f.suppressed.is_none() {
                    println!("{}", f.to_json(&format!("{check}: {name}"), f.level));
                }
            }
        }
        Format::Sarif => {
            let sarif_files: Vec<(String, Vec<lint::Finding>)> = report
                .findings()
                .map(|(check, name, f)| (format!("{check}: {name}"), vec![f.clone()]))
                .collect();
            if let Err(code) = print_sarif(&sarif_files) {
                return code;
            }
        }
    }
    if report.total_errors() > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_PATH: &str = "crates/osf/tests/fixtures/bad.md";
    const REAL_PATH: &str = "docs/real.md";
    const TWO_RULE_TEXT: &str = "A thing — another thing. It ran; it passed.\n";

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

    fn known() -> lint::KnownNames {
        lint::load_known_names(&[], None).expect("built-in names load")
    }

    fn lint_it(name: &str, text: &str) -> (Vec<lint::Finding>, Tally) {
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
}
