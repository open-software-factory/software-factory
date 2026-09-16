mod hook;
mod lint;

use clap::{Args, Parser, Subcommand, ValueEnum};
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Lint {
            kind: LintKind::Writing(args),
        } => lint_writing(&args),
        Command::Hook {
            event: HookEvent::Stop(args),
        } => hook::stop(args.known_names.as_deref(), args.max_bounces, args.answer),
    }
}

fn lint_writing(args: &WritingArgs) -> ExitCode {
    let known = match lint::load_known_names(args.known_names.as_deref()) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("osf: {e}");
            return ExitCode::from(2);
        }
    };
    let mut inputs: Vec<(String, String)> = Vec::new();
    if args.paths.is_empty() {
        let mut text = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut text) {
            eprintln!("osf: cannot read standard input: {e}");
            return ExitCode::from(2);
        }
        inputs.push(("<stdin>".to_string(), text));
    } else {
        for p in &args.paths {
            match std::fs::read_to_string(p) {
                Ok(t) => inputs.push((p.display().to_string(), t)),
                Err(e) => {
                    eprintln!("osf: cannot read {}: {e}", p.display());
                    return ExitCode::from(2);
                }
            }
        }
    }
    let format = resolve_format(args.format, args.json);
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut suppressed = 0usize;
    let mut sarif_files: Vec<(String, Vec<lint::Finding>)> = Vec::new();
    for (name, text) in &inputs {
        let kind = if args.message {
            lint::Kind::Message
        } else {
            lint::Kind::Document
        };
        let mut findings = lint::lint_writing(text, &known, kind, false, args.no_suppress);
        if args.strict {
            for f in &mut findings {
                if f.suppressed.is_none() {
                    f.level = lint::Level::Error;
                }
            }
        }
        for f in &findings {
            if f.suppressed.is_some() {
                suppressed += 1;
                continue;
            }
            match f.level {
                lint::Level::Error => errors += 1,
                lint::Level::Warning => warnings += 1,
            }
        }
        let visible: Vec<lint::Finding> = findings
            .iter()
            .filter(|f| f.suppressed.is_none())
            .cloned()
            .collect();
        match format {
            Format::Human if !visible.is_empty() => {
                println!("{}", osf_lint_core::render_human(name, text, &visible));
            }
            Format::Human => {}
            Format::Json => {
                for f in &visible {
                    println!("{}", f.to_json(name, f.level));
                }
            }
            Format::Sarif => sarif_files.push((name.clone(), findings)),
        }
    }
    match format {
        Format::Sarif => {
            let tool = osf_lint_core::ToolInfo {
                name: "osf",
                version: env!("CARGO_PKG_VERSION"),
                information_uri: INFORMATION_URI,
            };
            let report = osf_lint_core::to_sarif(&sarif_files, &tool);
            match serde_json::to_string_pretty(&report) {
                Ok(text) => println!("{text}"),
                Err(e) => {
                    eprintln!("osf: cannot render sarif: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        Format::Human => println!(
            "osf lint writing: {errors} error(s), {warnings} warning(s), {suppressed} suppressed"
        ),
        Format::Json => {}
    }
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
