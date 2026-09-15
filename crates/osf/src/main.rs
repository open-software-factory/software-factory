mod config;
mod hook;
mod lint;

use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
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
}

#[derive(Subcommand)]
enum LintKind {
    /// Check prose for references, names, sentence length, and filler.
    Writing(WritingArgs),
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
    /// Extra names that need no description, one per line.
    #[arg(long)]
    known_names: Option<PathBuf>,
    /// The text is a reply to a person: a heading in a short text is an error.
    #[arg(long)]
    message: bool,
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
}

#[derive(Subcommand)]
enum HookEvent {
    /// The agent wants to end its turn: lint the final message, refuse it on errors.
    Stop(StopArgs),
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
        Command::Hook {
            event: HookEvent::Stop(args),
        } => {
            let loaded = match config::load(cli.config.as_deref(), &[]) {
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
        Command::Config {
            action: ConfigAction::Show,
        } => config_show(cli.config.as_deref()),
    }
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
    let loaded = match config::load(config_flag, &[]) {
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

/// How many findings landed at each level, across every input.
#[derive(Default)]
struct Tally {
    errors: usize,
    warnings: usize,
    suppressed: usize,
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
                }
            }
        }
    }
}

/// Lints one named input, applying level overrides and `--strict`, and
/// prints it in every format except sarif, which the caller batches.
fn lint_one(
    name: &str,
    text: &str,
    args: &WritingArgs,
    known: &lint::KnownNames,
    cfg: &config::WritingConfig,
    format: Format,
    tally: &mut Tally,
) -> Vec<lint::Finding> {
    let kind = if args.message {
        lint::Kind::Message
    } else {
        lint::Kind::Document
    };
    let findings = lint::lint_writing(text, known, cfg, kind, false, args.no_suppress);
    let mut findings = osf_lint_core::apply_level_overrides(findings, &cfg.levels);
    if args.strict {
        for f in &mut findings {
            if f.suppressed.is_none() {
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
    let loaded = match config::load(config_flag, &overlay) {
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
    let inputs = match read_inputs(&args.paths) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let format = resolve_format(args.format, args.json);
    let mut tally = Tally::default();
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
            "osf lint writing: {} error(s), {} warning(s), {} suppressed",
            tally.errors, tally.warnings, tally.suppressed
        );
    }
    if tally.errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
