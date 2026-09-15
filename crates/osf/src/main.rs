mod hook;
mod lint;

use clap::{Args, Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

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
struct WritingArgs {
    /// Files to check. With no files, standard input is checked.
    paths: Vec<PathBuf>,
    /// Report findings as JSON lines.
    #[arg(long)]
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Lint {
            kind: LintKind::Writing(args),
        } => lint_writing(&args),
        Command::Hook {
            event: HookEvent::Stop(args),
        } => hook::stop(args.known_names.as_deref(), args.max_bounces),
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
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for (name, text) in &inputs {
        let kind = if args.message {
            lint::Kind::Message
        } else {
            lint::Kind::Document
        };
        let findings = lint::lint_writing(text, &known, kind);
        for f in &findings {
            let level = if args.strict {
                lint::Level::Error
            } else {
                f.level
            };
            match level {
                lint::Level::Error => errors += 1,
                lint::Level::Warning => warnings += 1,
            }
            if args.json {
                println!("{}", f.to_json(name, level));
            } else {
                println!("{}", f.render(name, level));
            }
        }
    }
    if !args.json {
        println!("osf lint writing: {errors} error(s), {warnings} warning(s)");
    }
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
